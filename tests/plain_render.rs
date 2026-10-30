//! Deterministic tests for the reusable Plain document synchronization controller.
#[path = "common/fake_editor.rs"]
mod fake_editor;

use fake_editor::{FakeEditor, Profile};
use magit_lsp::docstore::Encoding;
use magit_lsp::presentation::plain::{hash_text, text_edits, DocumentFormat, DocumentState, PlainDocument};
use serde_json::{json, Value};

#[path = "common/stdio.rs"]
mod stdio;
#[path = "common/mod.rs"]
mod fixtures;

fn controller(encoding: Encoding, versioned: bool) -> PlainDocument {
    PlainDocument::new("file:///status", encoding, versioned, DocumentFormat::from_text(""))
}

#[test]
fn format_serializes_empty_unicode_and_final_rows_exactly() {
    let tree = |lines: &[&str]| magit_lsp::tree::Tree { version: 0, lines: lines.iter().map(|s| (*s).to_owned()).collect(), nodes: vec![], line_nodes: vec![], tokens: vec![], symbols: vec![], links: vec![] };
    for (source, lines, expected) in [
        ("", vec![], ""), ("x\n", vec![], "\n"), ("", vec![""], ""),
        ("", vec!["α"], "α"), ("x\n", vec!["α", ""], "α\n\n"),
        ("x\r\n", vec!["α", "", "row"], "α\r\n\r\nrow\r\n"),
        ("x\r\ny", vec!["α", "row"], "α\r\nrow"), ("x", vec!["α", "row"], "α\nrow"),
    ] {
        assert_eq!(DocumentFormat::from_text(source).serialize(&tree(&lines)), expected);
    }
}

#[test]
fn retained_action_hash_uses_exact_serialized_eof_text() {
    let tree = magit_lsp::tree::Tree { version: 0, lines: vec!["α".into(), "row".into()], nodes: vec![], line_nodes: vec![], tokens: vec![], symbols: vec![], links: vec![] };
    let mut doc = PlainDocument::new("file:///status", Encoding::Utf16, true, DocumentFormat::from_text("stale\r\n"));
    doc.render_actions(tree, "view".into(), 7, "scope".into());
    let exact = "α\r\nrow\r\n";
    assert_eq!(doc.action_snapshot(exact).unwrap().2, 7);
    assert!(doc.action_snapshot("α\r\nrow").is_none());
    assert!(doc.action_snapshot("α\r\nrow\r\n\r\n").is_none());
}

#[test]
fn controller_serializes_unicode_last_line_growth_and_shrink_without_eof_retry() {
    for encoding in [Encoding::Utf8, Encoding::Utf16, Encoding::Utf32] {
        for source in ["stale", "stale\n"] {
            let uri = "file:///status";
            let mut editor = FakeEditor::new(Profile::Standard);
            editor.encoding = match encoding { Encoding::Utf8 => "utf-8", Encoding::Utf16 => "utf-16", Encoding::Utf32 => "utf-32" };
            editor.open(uri, source);
            let mut doc = PlainDocument::new(uri, encoding, true, DocumentFormat::from_text(source));
            assert!(doc.open(1, source).is_none());
            let tree = |last: &str, generation| magit_lsp::tree::Tree { version: generation, lines: vec!["header".into(), last.into()], nodes: vec![], line_nodes: vec![], tokens: vec![], symbols: vec![], links: vec![] };
            for (generation, last) in [(1, "α😀"), (2, "α😀 grew"), (3, "α😀")] {
                let target = tree(last, generation);
                let expected = DocumentFormat::from_text(source).serialize(&target);
                let request = doc.render_actions(target, "view".into(), generation, "scope".into()).unwrap();
                assert_eq!(editor.apply_edit(&request.edit)["applied"], true);
                let echoed = editor.text(uri).to_owned();
                assert_eq!(echoed, expected);
                assert!(doc.response(request.id, true).is_none());
                assert!(doc.change(generation as i32 + 1, echoed.clone()).is_none());
                assert_eq!(doc.state(), DocumentState::Ready);
                let (retained, _, retained_generation, _) = doc.action_snapshot(&echoed).unwrap();
                assert_eq!(retained.lines, vec!["header", last]);
                assert_eq!(retained_generation, generation);
                assert!(doc.action_snapshot(&(echoed.clone() + "\n")).is_none());
            }
        }
    }
}

fn next_edit(server: &mut stdio::TestServer) -> Value {
    loop {
        let frame = server.recv_raw();
        assert_ne!(frame["method"], "view/render");
        if frame["method"] == "workspace/applyEdit" { return frame; }
    }
}

fn plain_server(root: &std::path::Path, apply: bool, versioned: bool) -> stdio::TestServer {
    let mut server = stdio::TestServer::spawn(&[]);
    let mut init = stdio::initialize_message();
    init["params"]["capabilities"] = json!({"workspace":{"applyEdit":apply,"workspaceEdit":{"documentChanges":versioned},"executeCommand":{"dynamicRegistration":true}},"textDocument":{"codeAction":{"dynamicRegistration":true}}});
    init["params"]["workspaceFolders"] = json!([{"uri":format!("file://{}",root.display()),"name":"repo"}]);
    server.send(init);
    assert!(server.response_with_id(1)["result"].is_object());
    server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    server
}

#[test]
fn binary_profiles_watch_and_repair_divergence_in_both_response_orders() {
    for profile in [Profile::Standard, Profile::DecomposedChanges, Profile::DebouncedDidChange,
        Profile::ReorderEcho, Profile::NoDocumentChanges, Profile::NoShowDocument,
        Profile::CrlfNormalise, Profile::Utf16] {
        for response_first in [true, false] {
            let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f.txt", fixtures::TEN_LINES)]).build();
            let path = repo.path().join(".magit");
            fixtures::write(&path, "disk sentinel\n");
            let uri = format!("file://{}", path.display());
            let mut server = plain_server(repo.path(), true, profile != Profile::NoDocumentChanges);
            let mut editor = FakeEditor::new(profile);
            server.send(editor.open(&uri, "stale"));
            let first = next_edit(&mut server);
            assert_eq!(editor.apply_edit(&first["params"]["edit"])["applied"], true);
            let expected = editor.text(&uri).to_owned();
            let response = json!({"jsonrpc":"2.0","id":first["id"],"result":{"applied":true}});
            let mut change = editor.edit(&uri, "divergent user text");
            // The initial server edit has deliberately not been echoed, so
            // this divergent snapshot replaces the still-stale accepted text.
            change["params"]["contentChanges"] = json!([{"text":"divergent user text"}]);
            if response_first { server.send(response.clone()); }
            if profile == Profile::DebouncedDidChange {
                for frame in editor.flush() { server.send(frame); }
            } else { server.send(change); }
            if !response_first { server.send(response); }
            let repair = next_edit(&mut server);
            for frame in editor.dispatch(&repair) { server.send(frame); }
            for frame in editor.flush() { server.send(frame); }
            assert_eq!(editor.text(&uri), expected);
            // A real worktree write drives the native watcher, not a custom refresh.
            fixtures::write(&repo.path().join("watcher-only.txt"), "watcher bytes\n");
            let refreshed = next_edit(&mut server);
            for frame in editor.dispatch(&refreshed) { server.send(frame); }
            for frame in editor.flush() { server.send(frame); }
            assert!(editor.text(&uri).contains("watcher-only.txt"));
            assert_eq!(std::fs::read(&path).unwrap(), b"disk sentinel\n");
            assert_eq!(std::fs::read(repo.path().join("watcher-only.txt")).unwrap(), b"watcher bytes\n");
            server.shutdown_and_exit();
            let (status, frames) = server.finish_with_messages(std::time::Duration::from_secs(10));
            assert!(status.success());
            assert!(frames.iter().all(|f| f["method"] != "view/render"));
        }
    }
}

#[test]
fn binary_open_sentinel_converges_after_true_response_before_faithful_eof_echo() {
    for (source, expected_ending) in [("sentinel\n", "\n"), ("sentinel", ""), ("sentinel\r\n", "\r\n"), ("sentinel\r\nstale", "")] {
        let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f.txt", fixtures::TEN_LINES)]).build();
        let changed_file = "changed\n".to_owned() + fixtures::TEN_LINES;
        fixtures::write(&repo.path().join("f.txt"), &changed_file);
        let path = repo.path().join(".magit");
        fixtures::write(&path, "disk sentinel\n");
        let uri = format!("file://{}", path.display());
        let mut server = plain_server(repo.path(), true, true);
        let mut editor = FakeEditor::new(Profile::Standard);
        server.send(editor.open(&uri, source));
        let first = next_edit(&mut server);
        assert_eq!(editor.apply_edit(&first["params"]["edit"])["applied"], true);
        let rendered = editor.text(&uri).to_owned();
        server.send(json!({"jsonrpc":"2.0","id":first["id"],"result":{"applied":true}}));
        server.send(editor.edit(&uri, &rendered));
        // The immutable didOpen format controls every serialization, independent
        // of the sentinel the editor initially supplied.
        assert!(editor.text(&uri).ends_with(expected_ending));
        assert!(!editor.text(&uri).ends_with("\n\n"));
        let file_row = editor.text(&uri).lines().position(|line| line.contains("f.txt")).unwrap() as u32;
        server.send(json!({"jsonrpc":"2.0","id":30,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":file_row,"character":0},"end":{"line":file_row,"character":0}},"context":{"diagnostics":[]}}}));
        let actions = server.response_with_id(30)["result"].as_array().unwrap().clone();
        let content = actions.iter().find(|action| action["command"]["arguments"][0]["target"]["node_id"].is_string()).unwrap_or_else(|| panic!("file row action source {source:?}: {actions:?}"));
        assert!(content["command"]["arguments"][0]["viewGeneration"].is_number());
        let phantom = editor.text(&uri).lines().count() as u32;
        server.send(json!({"jsonrpc":"2.0","id":31,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":phantom,"character":0},"end":{"line":phantom,"character":0}},"context":{"diagnostics":[]}}}));
        let phantom_response = server.response_with_id(31);
        let phantom_actions = phantom_response["result"].as_array().unwrap();
        assert!(phantom_actions.iter().all(|action| !action["command"]["arguments"][0]["target"]["node_id"].is_string()), "phantom row {phantom} actions {phantom_actions:?}");
        server.shutdown_and_exit();
        let (status, frames) = server.finish_with_messages(std::time::Duration::from_secs(10));
        assert!(status.success());
        assert!(frames.iter().all(|frame| frame["method"] != "view/render"));
        assert_eq!(std::fs::read(&path).unwrap(), b"disk sentinel\n");
    }
}

#[test]
fn binary_close_reopen_ignores_pending_reply_and_disconnect_terminates_pending_edit() {
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f.txt", fixtures::TEN_LINES)]).build();
    let path = repo.path().join(".magit");
    fixtures::write(&path, "disk sentinel");
    let uri = format!("file://{}", path.display());
    let mut server = plain_server(repo.path(), true, true);
    let mut editor = FakeEditor::new(Profile::Standard);
    server.send(editor.open(&uri, "old incarnation"));
    let stale = next_edit(&mut server);
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri}}}));
    server.send(editor.open(&uri, "new incarnation"));
    server.send(json!({"jsonrpc":"2.0","id":stale["id"],"result":{"applied":true}}));
    let current = next_edit(&mut server);
    assert_ne!(stale["id"], current["id"]);
    assert_eq!(current["params"]["edit"]["documentChanges"][0]["textDocument"]["version"], 1);
    assert_eq!(editor.apply_edit(&current["params"]["edit"])["applied"], true);
    assert!(!editor.text(&uri).contains("incarnation"));
    let (status, frames) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
    assert!(frames.iter().all(|f| f["method"] != "view/render" && f["method"] != "workspace/applyEdit"));
    assert_eq!(std::fs::read(&path).unwrap(), b"disk sentinel");
}

#[test]
fn binary_absent_apply_edit_reports_readable_unsupported_without_custom_delivery() {
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f.txt", fixtures::TEN_LINES)]).build();
    let path = repo.path().join(".magit");
    fixtures::write(&path, "untouched");
    let uri = format!("file://{}", path.display());
    let mut server = plain_server(repo.path(), false, true);
    let mut editor = FakeEditor::new(Profile::Standard);
    server.send(editor.open(&uri, "untouched"));
    loop {
        let frame = server.recv_raw();
        assert_ne!(frame["method"], "view/render");
        assert_ne!(frame["method"], "workspace/applyEdit");
        if frame["method"] == "window/showMessage" {
            assert!(frame["params"]["message"].as_str().unwrap().contains("workspace/applyEdit"));
            break;
        }
    }
    server.shutdown_and_exit();
    assert!(server.finish(std::time::Duration::from_secs(10)).success());
    assert_eq!(editor.text(&uri), "untouched");
    assert_eq!(std::fs::read(&path).unwrap(), b"untouched");
}

#[test]
fn minimal_edits_use_utf8_utf16_and_utf32_positions_with_crlf() {
    let old = "head\r\n😀old\r\ntail";
    let new = "head\r\n😀new\r\ntail";
    for (encoding, column) in [(Encoding::Utf8, 4), (Encoding::Utf16, 2), (Encoding::Utf32, 1)] {
        let edits = text_edits(old, new, encoding);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0]["range"]["start"], json!({"line":1,"character":column}));
        assert_eq!(edits[0]["newText"], "new");
    }
    assert!(text_edits(new, new, Encoding::Utf16).is_empty());
    let newline_edit = text_edits("a\r\nb", "a\nb", Encoding::Utf16);
    assert_eq!(newline_edit[0]["range"]["start"], json!({"line":0,"character":1}));
    assert_eq!(newline_edit[0]["range"]["end"], json!({"line":1,"character":0}));
    assert_eq!(newline_edit[0]["newText"], "\n");
}

#[test]
fn render_coalesces_and_echo_before_or_after_response_reconciles_without_version_assumptions() {
    for echo_first in [true, false] {
        let uri = "file:///status";
        let mut c = controller(Encoding::Utf16, true);
        let mut editor = FakeEditor::new(Profile::Standard);
        editor.open(uri, "stale");
        c.render("authoritative\n");
        let first = c.open(1, "stale").unwrap();
        assert_eq!(c.state(), DocumentState::Applying);
        assert_eq!(first.edit["documentChanges"][0]["textDocument"]["version"], 1);
        assert_eq!(c.render("intermediate"), None);
        let second_render = c.render("latest\n");
        assert_eq!(second_render, None);
        let applied = editor.apply_edit(&first.edit);
        assert_eq!(applied["applied"], true);
        let echo_version = editor.documents[uri].version;
        if echo_first {
            assert!(c.change(echo_version as i32, editor.text(uri)).is_none());
            let next = c.response(first.id, true).unwrap();
            assert_eq!(next.edit["documentChanges"][0]["edits"][0]["newText"], "latest");
        } else {
            assert!(c.response(first.id, true).is_none());
            let next = c.change(echo_version as i32, editor.text(uri)).unwrap();
            assert_eq!(next.edit["documentChanges"][0]["edits"][0]["newText"], "latest");
        }
    }
}

#[test]
fn unversioned_changes_profile_and_stale_rejection_recompute_from_latest_snapshot() {
    let uri = "file:///status";
    let mut c = controller(Encoding::Utf8, false);
    let mut editor = FakeEditor::new(Profile::NoDocumentChanges);
    editor.open(uri, "old");
    c.render("new");
    let request = c.open(7, "old").unwrap();
    assert!(request.edit.get("changes").is_some());
    assert!(request.edit.get("documentChanges").is_none());
    editor.edit(uri, "user text");
    assert!(c.change(19, "user text").is_none());
    let retry = c.response(request.id, false).unwrap();
    assert_eq!(retry.edit["changes"][uri][0]["newText"], "new");
    assert_eq!(retry.edit["changes"][uri][0]["range"]["start"]["character"], 0);
}

#[test]
fn divergence_is_repaired_and_close_discards_late_responses_without_resurrection() {
    let mut c = controller(Encoding::Utf16, true);
    c.render("disk");
    let first = c.open(1, "old").unwrap();
    assert!(c.change(4, "formatted").is_none());
    let repair = c.response(first.id, true).unwrap();
    assert_eq!(repair.edit["documentChanges"][0]["textDocument"]["version"], 4);
    c.close();
    assert_eq!(c.state(), DocumentState::Detached);
    assert!(c.response(repair.id, true).is_none());
    assert!(c.open(22, "disk").is_none());
    assert_eq!(c.state(), DocumentState::Ready);
    assert_ne!(hash_text("disk"), hash_text("formatted"));
}

#[test]
fn bounded_snapshots_are_keyed_by_exact_text_hash() {
    let mut c = controller(Encoding::Utf16, true);
    c.open(1, "a");
    let hash = hash_text("a");
    for i in 0..10 { c.render(format!("render {i}")); }
    assert!(c.snapshot(&hash).is_none());
    let retained = hash_text("render 9");
    assert_eq!(c.snapshot(&retained).unwrap().0, "render 9");
}

#[test]
fn every_fake_editor_profile_can_be_driven_without_assuming_next_version() {
    let profiles = [Profile::Standard, Profile::DecomposedChanges, Profile::DebouncedDidChange,
        Profile::ReorderEcho, Profile::NoDocumentChanges, Profile::NoShowDocument,
        Profile::CrlfNormalise, Profile::Utf16];
    for profile in profiles {
        let mut editor = FakeEditor::new(profile);
        editor.open("file:///status", "a\r\nb");
        let mut c = controller(Encoding::Utf16, profile != Profile::NoDocumentChanges);
        c.render("a\r\nB");
        let request = c.open(1, "a\r\nb").unwrap();
        let outcome: Value = editor.apply_edit(&request.edit);
        assert_eq!(outcome["applied"], true, "profile: {profile:?}");
        let actual_version = editor.documents["file:///status"].version;
        assert!(actual_version > 1);
        assert!(c.change(actual_version as i32, editor.text("file:///status")).is_none());
        let followup = c.response(request.id, true);
        if profile == Profile::CrlfNormalise {
            assert!(followup.is_none());
            assert!(c.take_format_warning());
            assert!(c.render("a\r\nb").is_none());
            assert!(c.change(actual_version as i32 + 1, "a\nb").is_none());
            assert!(!c.action_snapshot("a\r\nb").is_some());
            assert!(!c.take_format_warning());
            assert!(c.open(actual_version as i32 + 2, "a\nb").is_some());
            assert_eq!(c.state(), DocumentState::Applying);
        } else {
            assert!(followup.is_none());
            assert_eq!(c.state(), DocumentState::Ready);
        }
    }
}

#[test]
fn format_drift_suspends_once_in_either_response_echo_order_and_reopens() {
    for (authoritative, drifted) in [("authoritative\r\n", "authoritative\n"), ("authoritative\n", "authoritative")] {
        for response_first in [true, false] {
            let mut c = PlainDocument::new("file:///status", Encoding::Utf16, true, DocumentFormat::from_text(authoritative));
            c.render(authoritative);
            let edit = c.open(1, "stale").unwrap();
            if response_first { assert!(c.response(edit.id, true).is_none()); }
            assert!(c.change(2, drifted).is_none());
            if !response_first { assert!(c.response(edit.id, true).is_none()); }
            assert!(c.take_format_warning());
            assert!(!c.take_format_warning());
            assert!(c.render_actions(magit_lsp::tree::Tree { version: 0, lines: vec!["authoritative".into()], nodes: vec![], line_nodes: vec![], tokens: vec![], symbols: vec![], links: vec![] }, "view".into(), 1, "scope".into()).is_none());
            assert!(c.action_snapshot(authoritative).is_none());
            assert!(c.change(3, drifted).is_none());
            assert!(!c.take_format_warning());
            assert!(c.open(4, drifted).is_some());
            assert_eq!(c.state(), DocumentState::Applying);
        }
    }
}

#[test]
fn delayed_exact_echo_clears_intermediate_format_drift_diagnosis() {
    let mut c = PlainDocument::new("file:///status", Encoding::Utf16, true, DocumentFormat::from_text("old\n"));
    c.render("authoritative\n");
    let edit = c.open(1, "stale\n").unwrap();
    assert!(c.change(2, "authoritative\r\n").is_none());
    assert!(c.change(3, "authoritative\n").is_none());
    assert!(c.response(edit.id, true).is_none());
    assert!(!c.take_format_warning());
    assert_eq!(c.state(), DocumentState::Ready);

    let mut rows = PlainDocument::new("file:///status", Encoding::Utf16, true, DocumentFormat::from_text("old\n"));
    rows.render("authoritative\n\n");
    let edit = rows.open(1, "stale\n").unwrap();
    assert!(rows.response(edit.id, true).is_none());
    let repair = rows.change(2, "authoritative\n").expect("a missing structured blank row is ordinary divergence");
    assert!(!rows.take_format_warning());
    assert!(repair.edit.is_object());
}

#[test]
fn actual_plain_did_open_applies_authoritative_status_without_writing_presentation_file() {
    use std::time::{Duration, Instant};
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", fixtures::TEN_LINES)])
        .modify_standard()
        .build();
    let path = repo.path().join(".magit");
    fixtures::write(&path, "stale disk text\n");
    let disk_before = std::fs::read(&path).unwrap();
    let uri = format!("file://{}", path.display());
    let root_uri = format!("file://{}", repo.path().display());
    let mut server = stdio::TestServer::spawn(&[]);
    let mut initialize = stdio::initialize_message();
    initialize["params"]["capabilities"] = json!({
        "workspace": {"applyEdit": true, "workspaceEdit": {"documentChanges": true}},
        "general": {"positionEncodings": ["utf-16"]}
    });
    initialize["params"]["workspaceFolders"] = json!([{"uri":root_uri,"name":"repo"}]);
    server.send(initialize);
    server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    assert!(server.response_with_id(1)["result"]["capabilities"].is_object());

    let mut editor = FakeEditor::new(Profile::Standard);
    server.send(editor.open(&uri, "stale editor text\n"));
    let request = loop {
        let message = server.recv_raw();
        if message["method"] == "workspace/applyEdit" { break message; }
        assert_ne!(message["method"], "view/render", "Plain must not receive custom renders");
    };
    assert!(request["params"]["edit"]["documentChanges"].is_array());
    let rendered = editor.dispatch(&request);
    assert!(rendered.iter().any(|frame| frame["method"] == "textDocument/didChange"));
    assert!(editor.text(&uri).contains("line3 CHANGED"));
    assert!(!editor.text(&uri).contains("stale"));
    for frame in rendered { server.send(frame); }
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        // The server's correlated response to applyEdit is observed through
        // the same live stream; echoes are sent above in real client order.
        if server.sent.iter().any(|frame| frame["method"] == "textDocument/didChange") { break; }
        std::thread::yield_now();
    }
    assert_eq!(std::fs::read(&path).unwrap(), disk_before);
    server.shutdown_and_exit();
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn binary_format_drift_warns_once_refuses_actions_and_recovers_on_reopen() {
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f.txt", fixtures::TEN_LINES)]).modify("f.txt", "changed\n").build();
    let path = repo.path().join(".magit");
    fixtures::write(&path, "disk\r\n");
    let uri = format!("file://{}", path.display());
    let mut server = plain_server(repo.path(), true, true);
    let mut editor = FakeEditor::new(Profile::CrlfNormalise);
    server.send(editor.open(&uri, "stale\r\n"));
    let first = next_edit(&mut server);
    let mut frames = editor.dispatch(&first);
    if frames[0]["result"]["applied"] == true { server.send(frames.remove(0)); }
    for frame in frames { server.send(frame); }
    let warning = loop {
        let frame = server.recv();
        if frame["method"] == "window/showMessage" { break frame; }
        assert_ne!(frame["method"], "workspace/applyEdit", "format drift must not trigger a second repair");
    };
    let warning_text = warning["params"]["message"].as_str().unwrap();
    assert!(warning_text.contains("line endings or final newline"));
    assert!(warning_text.contains("close and reopen"));

    for version in [3, 4] {
        server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":version},"contentChanges":[{"text":editor.text(&uri)}]}}));
    }
    server.send(json!({"jsonrpc":"2.0","id":41,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"context":{"diagnostics":[]}}}));
    assert!(server.response_with_id(41)["result"].as_array().unwrap().is_empty());
    assert!(!server.notifications.iter().any(|message| message["method"] == "workspace/applyEdit" || message["method"] == "window/showMessage"));

    server.send(editor.close(&uri));
    server.send(editor.open(&uri, "stale\n"));
    let reopened = next_edit(&mut server);
    assert_eq!(editor.apply_edit(&reopened["params"]["edit"])["applied"], true);
    server.send(json!({"jsonrpc":"2.0","id":reopened["id"],"result":{"applied":true}}));
    let reopened_text = editor.text(&uri).to_owned();
    server.send(editor.edit(&uri, &reopened_text));
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}
