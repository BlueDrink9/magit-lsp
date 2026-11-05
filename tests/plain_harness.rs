//! Plain editor conformance self-tests and real-binary protocol smoke test.
#[path = "common/fake_editor.rs"]
mod fake_editor;
#[path = "common/mod.rs"]
mod fixtures;
#[path = "common/stdio.rs"]
mod stdio;
#[path = "common/client_pump.rs"]
mod client_pump;
use client_pump::ClientPump;
use fake_editor::{FakeEditor, Profile};
use serde_json::{json, Value};
use std::fs;
use std::time::Duration;

fn caps() -> Value { json!({"window":{"workDoneProgress":true},"general":{"positionEncodings":["utf-8","utf-16","utf-32"]}}) }

#[test]
fn real_binary_lifecycle_and_current_unsupported_methods_are_honest() {
    let mut client = ClientPump::new(stdio::TestServer::spawn(&[]), Profile::Standard, caps());
    let uri = "file:///tmp/plain-harness.txt";
    let frame = client.editor.open(uri, "hello\n");
    client.send(frame);
    let frame = client.editor.edit(uri, "edited\n");
    client.send(frame);
    let frame = client.editor.save(uri);
    client.send(frame);
    assert_eq!(client.editor.text(uri), "edited\n");
    let frame = client.editor.code_action(
        uri,
        json!({"start":{"line":0,"character":0},"end":{"line":0,"character":1}}),
    );
    let ca_id = client.send(frame);
    let frame = client.editor.execute_command("magit.command", json!([]));
    let ec_id = client.send(frame);
    // Await the second request first: the pump must retain the first response.
    let ec = client.until_response(ec_id);
    assert!(ec["error"].is_object());
    let ca = client.until_response(ca_id);
    assert!(ca.get("result").is_some() || ca["error"].is_object());
    let frame = client.editor.close(uri);
    client.send(frame);
    assert!(!client.editor.documents.contains_key(uri));
    client.finish();
}

#[test]
fn plain_did_open_apply_edit_is_pumped_and_echoed() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", fixtures::TEN_LINES)])
        .modify_standard()
        .build();
    let path = repo.path().join(".magit");
    fixtures::write(&path, "disk sentinel\n");
    let uri = format!("file://{}", path.display());
    let capabilities = json!({
        "workspace": {"applyEdit": true},
        "textDocument": {"codeAction": {}}
    });
    let mut client = ClientPump::new(
        stdio::TestServer::spawn(&[]),
        Profile::Standard,
        capabilities,
    );
    let open = client.editor.open(&uri, "stale\n");
    client.send(open);
    client.until(|client| client.editor.text(&uri).contains("f.txt"));

    let requests: Vec<_> = client
        .editor
        .outbound
        .iter()
        .filter(|message| message["method"] == "workspace/applyEdit")
        .collect();
    assert!(!requests.is_empty(), "server sent no workspace/applyEdit");
    assert!(client.server.sent.iter().any(|message| {
        message["id"] == requests[0]["id"] && message["result"]["applied"] == true
    }));
    assert!(client.server.sent.iter().any(|message| {
        message["method"] == "textDocument/didChange"
            && message["params"]["textDocument"]["uri"] == uri
    }));
    assert_eq!(
        client
            .server
            .sent
            .iter()
            .filter(|message| message["method"] == "textDocument/didOpen")
            .count(),
        1
    );
    assert!(client
        .server
        .sent
        .iter()
        .all(|message| message["method"] != "view/render"));
    assert!(client
        .server
        .notifications
        .iter()
        .all(|message| message["method"] != "view/render"));
    assert_eq!(fs::read(&path).unwrap(), b"disk sentinel\n");
    client.finish();
}

#[test]
fn all_documented_profiles_have_assertable_behavior() {
    let profiles = [
        Profile::DecomposedChanges,
        Profile::DebouncedDidChange,
        Profile::ReorderEcho,
        Profile::NoDocumentChanges,
        Profile::NoShowDocument,
        Profile::CrlfNormalise,
        Profile::Utf16,
    ];
    for p in profiles {
        let mut e = FakeEditor::new(p);
        let uri = "file:///x";
        e.open(uri, "a\nb\n");
        match p {
            Profile::DecomposedChanges => {
                let n = e.edit(uri, "A\nB\n");
                let c = n["params"]["contentChanges"].as_array().unwrap();
                assert_eq!(c.len(), 3);
                assert!(c.iter().all(|v| v["range"].is_object()));
            }
            Profile::DebouncedDidChange => {
                e.edit(uri, "queued");
                assert!(e.has_pending());
                let sent = e.flush();
                assert!(!e.has_pending());
                assert_eq!(sent.len(), 1);
                assert_eq!(sent[0]["params"]["contentChanges"][0]["text"], "queued");
            }
            Profile::ReorderEcho => {
                let req = json!({"jsonrpc":"2.0","id":17,"method":"workspace/applyEdit","params":{"edit":{"changes":{"file:///x":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"z"}]}}}});
                let response = e.respond(&req);
                let echo = e.edit(uri, "echo");
                assert_eq!(response["id"], 17);
                assert_eq!(response["result"]["applied"], true);
                assert_eq!(echo["params"]["textDocument"]["version"], 3);
                assert_eq!(e.text(uri), "echo");
                assert_eq!(e.outbound[0]["method"], "workspace/applyEdit");
            }
            Profile::NoDocumentChanges => {
                assert_eq!(e.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":uri,"version":1},"edits":[]}]}))["applied"],false);
                assert_eq!(e.apply_edit(&json!({"changes":{"file:///x":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"z"}]}}))["applied"],true);
            }
            Profile::NoShowDocument => {
                assert_eq!(
                    e.respond(&json!({"id":5,"method":"window/showDocument"}))["result"]["success"],
                    false
                );
            }
            Profile::CrlfNormalise => {
                e.documents.get_mut(uri).unwrap().text = "a\r\nb".into();
                e.apply_edit(&json!({"changes":{"file:///x":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"z"}]}}));
                assert!(!e.text(uri).contains("\r\n"));
            }
            Profile::Utf16 => {
                e.documents.get_mut(uri).unwrap().text = "😀x".into();
                assert_eq!(e.apply_edit(&json!({"changes":{"file:///x":[{"range":{"start":{"line":0,"character":2},"end":{"line":0,"character":3}},"newText":"y"}]}}))["applied"],true);
                assert_eq!(e.text(uri), "😀y");
            }
            Profile::Standard => unreachable!(),
        }
    }
}

#[test]
fn registry_models_drive_fake_editor_observable_behavior() {
    use magit_lsp::quirks::{self, Client, Resolution, REGISTRY};
    for quirk in REGISTRY.iter().filter(|q| q.model.is_some()) {
        let resolution = Resolution { enabled: [quirk.id].into_iter().collect() };
        let profile = if quirk.model == Some("neovim-accepts-undeclared-documentChanges") { Profile::NoDocumentChanges } else { Profile::Standard };
        let mut editor = FakeEditor::from_resolution(profile, &resolution);
        let uri = "file:///model";
                editor.open(uri, "old");
        match quirk.model.unwrap() {
            "neovim-accepts-undeclared-documentChanges" => {
                assert_eq!(editor.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":uri,"version":1},"edits":[]}]}))["applied"], true);
            }
            "silent-stale-apply" => {
                editor.edit(uri, "new");
                let result = editor.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":uri,"version":1},"edits":[replacement(0,3,"BAD")]}]}));
                assert_eq!(result["applied"], true);
                assert_eq!(editor.text(uri), "new");
            }
            "decomposed-changes" => {
                let notification = editor.edit(uri, "new");
                assert!(notification["params"]["contentChanges"][0]["range"].is_object());
            }
            other => panic!("unhandled registry model: {other}"),
        }
    }
    let empty = json!({});
    let neovim = quirks::resolve(Client { name: Some("Neovim"), version: Some("0.12.6"), capabilities: &empty }, None).unwrap();
    let mut editor = FakeEditor::from_resolution(Profile::NoDocumentChanges, &neovim);
    let uri = "file:///combined";
    editor.open(uri, "old");
    assert_eq!(editor.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":uri,"version":1},"edits":[]}]}))["applied"], true);
    editor.edit(uri, "new");
    editor.open("file:///later", "old");
    editor.edit("file:///later", "later-local");
    let stale = editor.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":uri,"version":1},"edits":[replacement(0,3,"FIRST") ]},{"textDocument":{"uri":"file:///later","version":1},"edits":[replacement(0,11,"LATER")]}]}));
    assert_eq!(stale["applied"], true);
    assert_eq!(editor.text(uri), "new");
    assert_eq!(editor.text("file:///later"), "LATER");
    let helix = quirks::resolve(Client { name: Some("helix"), version: Some("25.07.1"), capabilities: &empty }, None).unwrap();
    let mut editor = FakeEditor::from_resolution(Profile::Standard, &helix);
    editor.open(uri, "old");
    assert!(editor.edit(uri, "new")["params"]["contentChanges"][0]["range"].is_object());
}

#[test]
fn versioned_apply_edit_validates_before_mutating_and_supports_multiple_changes() {
    let mut e = FakeEditor::new(Profile::Standard);
    let u = "file:///d";
    e.open(u, "a\nb\n");
    let result=e.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":u,"version":1},"edits":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"A"},{"range":{"start":{"line":1,"character":0},"end":{"line":1,"character":1}},"newText":"B"}]}]}));
    assert_eq!(result["applied"], true);
    assert_eq!(e.text(u), "A\nB\n");
    let before = e.text(u).to_owned();
    let stale = e.apply_edit(
        &json!({"documentChanges":[{"textDocument":{"uri":u,"version":1},"edits":[]}]}),
    );
    assert_eq!(stale["applied"], false);
    assert!(stale["failureReason"].is_string());
    assert_eq!(e.text(u), before);
}

#[test]
fn overlapping_and_malformed_edits_reject_atomically_across_documents() {
    let mut e = FakeEditor::new(Profile::Standard);
    e.open("file:///a", "abcd");
    e.open("file:///b", "wxyz");
    let result=e.apply_edit(&json!({"changes":{"file:///a":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":2}},"newText":"A"},{"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":3}},"newText":"B"}],"file:///b":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"W"}]}}));
    assert_eq!(result["applied"], false);
    assert_eq!(e.text("file:///a"), "abcd");
    assert_eq!(e.text("file:///b"), "wxyz");
}

#[test]
fn utf16_surrogate_midpoint_is_rejected_and_crlf_lines_are_addressable() {
    let mut e = FakeEditor::new(Profile::Utf16);
    e.open("file:///u", "😀x\r\ny");
    assert_eq!(e.apply_edit(&json!({"changes":{"file:///u":[{"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":2}},"newText":"z"}]}}))["applied"],false);
    assert_eq!(e.apply_edit(&json!({"changes":{"file:///u":[{"range":{"start":{"line":1,"character":0},"end":{"line":1,"character":1}},"newText":"Y"}]}}))["applied"],true);
    assert_eq!(e.text("file:///u"), "😀x\r\nY");
}

fn replacement(start: u64, end: u64, text: &str) -> Value {
    json!({"range":{"start":{"line":0,"character":start},"end":{"line":0,"character":end}},"newText":text})
}

#[test]
fn original_snapshot_order_and_atomic_rejection_matrix() {
    let a = "file:///a";
    let b = "file:///b";
    let mut e = FakeEditor::new(Profile::Standard);
    e.open(a, "abcdef");
    e.open(b, "uvwxyz");
    // Deliberately reversed and length-changing: all offsets use the original.
    assert_eq!(e.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":a,"version":1},"edits":[replacement(4,6,"!"),replacement(0,1,"LONG")]},{"textDocument":{"uri":b,"version":1},"edits":[replacement(0,1,"U")]}]}))["applied"],true);
    assert_eq!(e.text(a), "LONGbcd!");
    assert_eq!(e.text(b), "Uvwxyz");
    let valid = json!({"textDocument":{"uri":a,"version":2},"edits":[replacement(0,1,"X")]});
    let invalids = vec![
        json!({"textDocument":{"uri":b,"version":1},"edits":[]}),
        json!({"textDocument":{"uri":b,"version":"2"},"edits":[]}),
        json!({"textDocument":{"uri":b},"edits":[]}),
        json!({"textDocument":{"uri":b,"version":2},"edits":{}}),
        json!({"textDocument":{"uri":b,"version":2},"edits":[replacement(0,99,"x")]}),
        json!({"textDocument":{"uri":b,"version":2},"edits":[replacement(3,1,"x")]}),
        json!({"textDocument":{"uri":b,"version":2},"edits":[replacement(0,3,"x"),replacement(2,4,"y")]}),
        json!({"textDocument":{"uri":a,"version":2},"edits":[]}),
        json!({"kind":"create","uri":"file:///new"}),
    ];
    for invalid in invalids {
        let result = e.apply_edit(&json!({"documentChanges":[valid.clone(),invalid]}));
        assert_eq!(result["applied"], false, "{result}");
        assert!(result["failureReason"].is_string());
        assert_eq!(e.text(a), "LONGbcd!");
        assert_eq!(e.text(b), "Uvwxyz");
        assert_eq!(e.documents[a].version, 2);
        assert_eq!(e.documents[b].version, 2);
    }
    for edit in [
        json!({"documentChanges":{}}),
        json!({"changes":{a:[replacement(0,1,"X")],b:{}}}),
        json!({"changes":{a:[replacement(0,1,"X")],"file:///missing":[]}}),
        json!({"documentChanges":[],"changes":{}}),
    ] {
        assert_eq!(e.apply_edit(&edit)["applied"], false);
        assert_eq!(e.text(a), "LONGbcd!");
        assert_eq!(e.documents[a].version, 2);
    }
}

#[test]
fn utf8_utf16_utf32_bounds_use_original_crlf_snapshots() {
    for (encoding, start, end, invalid) in [
        ("utf-8", 4, 5, Some(1)),
        ("utf-16", 2, 3, Some(1)),
        ("utf-32", 1, 2, None),
    ] {
        let mut e = FakeEditor::new(Profile::Standard);
        e.encoding = encoding;
        e.open("file:///u", "😀x\r\ny");
        if let Some(mid) = invalid {
            assert_eq!(
                e.apply_edit(&json!({"changes":{"file:///u":[replacement(mid,start,"bad")]}}))
                    ["applied"],
                false
            );
            assert_eq!(e.text("file:///u"), "😀x\r\ny");
        }
        assert_eq!(
            e.apply_edit(&json!({"changes":{"file:///u":[replacement(start,end,"Y")]}}))["applied"],
            true
        );
        assert_eq!(e.text("file:///u"), "😀Y\r\ny");
        assert_eq!(
            e.apply_edit(&json!({"changes":{"file:///u":[replacement(end+1,end+1,"bad")]}}))
                ["applied"],
            false
        );
    }
}

#[test]
fn many_decomposed_ranges_replay_sequentially_in_one_notification() {
    for encoding in ["utf-8", "utf-16", "utf-32"] {
        let uri = "file:///decomposed";
        let mut e = FakeEditor::new(Profile::DecomposedChanges);
        e.encoding = encoding;
        e.open(uri, "😀old\r\ntail");
        let expected = (0..25)
            .map(|i| format!("😀 line {i}\n"))
            .collect::<String>();
        let notification = e.edit(uri, &expected);
        let changes = notification["params"]["contentChanges"].as_array().unwrap();
        assert_eq!(changes.len(), 26);
        assert_eq!(notification["params"]["textDocument"]["version"], 2);
        let mut replay = FakeEditor::new(Profile::Standard);
        replay.encoding = encoding;
        replay.open(uri, "😀old\r\ntail");
        for change in changes {
            assert!(change["range"].is_object());
            assert_eq!(
                replay.apply_edit(
                    &json!({"changes":{uri:[{"range":change["range"],"newText":change["text"]}]}})
                )["applied"],
                true
            );
        }
        assert_eq!(replay.text(uri), expected);
        assert_eq!(e.text(uri), expected);
    }
}

#[test]
fn debounce_releases_no_echo_until_flush_on_actual_transport() {
    let mut client = ClientPump::new(stdio::TestServer::spawn(&[]), Profile::DebouncedDidChange, caps());
    let uri = "file:///debounce";
    let open = client.editor.open(uri, "a");
    client.send(open);
    let start = client.server.sent.len();
    client.dispatch(&json!({"jsonrpc":"2.0","id":"script/debounce","method":"workspace/applyEdit","params":{"edit":{"changes":{uri:[replacement(0,1,"b")]}}}}));
    assert_eq!(client.server.sent.len(), start + 1);
    assert!(client.editor.has_pending());
    assert_eq!(client.editor.text(uri), "b");
    for frame in client.editor.flush() {
        client.send(frame);
    }
    assert!(!client.editor.has_pending());
    assert_eq!(client.server.sent.len(), start + 2);
    assert_eq!(client.server.sent[start]["id"], "script/debounce");
    assert_eq!(
        client.server.sent[start + 1]["method"],
        "textDocument/didChange"
    );
    assert_eq!(
        client.server.sent[start + 1]["params"]["textDocument"]["version"],
        2
    );
    client.finish();
}

#[test]
fn shared_dispatch_captures_real_send_order_for_echo_profiles() {
    for (profile, expected) in [
        (Profile::Standard, ["textDocument/didChange", "response"]),
        (Profile::ReorderEcho, ["response", "textDocument/didChange"]),
    ] {
        let mut client = ClientPump::new(stdio::TestServer::spawn(&[]), profile, caps());
        let open = client.editor.open("file:///order", "a");
        client.send(open);
        // Scripted server request, but actual dispatcher and pipe writes, not production Plain acceptance.
        let request = json!({"jsonrpc":"2.0","id":"script/apply","method":"workspace/applyEdit","params":{"edit":{"documentChanges":[{"textDocument":{"uri":"file:///order","version":1},"edits":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"b"}]}]}}});
        let start = client.server.sent.len();
        client.dispatch(&request);
        let writes = &client.server.sent[start..];
        assert_eq!(
            writes
                .iter()
                .map(|m| m["method"].as_str().unwrap_or("response"))
                .collect::<Vec<_>>(),
            expected
        );
        assert!(writes.iter().any(|m| m["id"] == "script/apply"));
        assert_eq!(client.editor.text("file:///order"), "b");
        client.dispatch(&json!({"jsonrpc":"2.0","id":"script/show","method":"window/showDocument","params":{"uri":"file:///order"}}));
        assert_eq!(
            client.server.sent.last().unwrap()["result"]["success"],
            true
        );
        client.dispatch(&json!({"jsonrpc":"2.0","id":"script/unknown","method":"unknown"}));
        assert_eq!(client.server.sent.last().unwrap()["error"]["code"], -32601);
        client.finish();
    }
}

#[test]
fn live_binary_progress_requests_are_serviced_while_command_runs() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("init", [("f.txt", fixtures::TEN_LINES)])
        .modify_standard()
        .build();
    let head = repo.git(&["rev-parse", "HEAD"]);
    let mut client = ClientPump::new(stdio::TestServer::spawn(&[]), Profile::Standard, caps());
    let id=client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/open","params":{"kind":"status","args":{"repo_path":repo.path().to_string_lossy()},"viewport":{"cols":80,"rows":24}}}));
    let opened = client.until_response(id);
    let view = opened["result"]["id"]
        .as_str()
        .expect("opened view")
        .to_owned();
    let id=client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/invoke","params":{"viewId":view,"command":"status.stage-all"}}));
    let response = client.until_response(id);
    assert!(response.get("result").is_some(), "{response}");
    assert!(repo
        .git(&["diff", "--cached", "--name-only"])
        .contains("f.txt"));
    // Cancel only when the actual commit editor is waiting. Cancelling on begin
    // races the setup rev-parse, whose killed process returns an RPC error.
    client.cancel_on_edit = true;
    let id=client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/invoke","params":{"viewId":view,"command":"commit.create"}}));
    assert!(client.until_response(id).get("result").is_some());
    client.until(|c| {
        !c.cancel_on_edit
            && c.server
                .notifications
                .iter()
                .filter(|m| m["method"] == "$/progress" && m["params"]["value"]["kind"] == "end")
                .count()
                == 2
    });
    let requests: Vec<_> = client
        .editor
        .outbound
        .iter()
        .filter(|m| m["method"] == "window/workDoneProgress/create")
        .collect();
    assert_eq!(requests.len(), 2);
    assert_ne!(requests[0]["id"], requests[1]["id"]);
    for request in requests {
        assert!(client
            .server
            .sent
            .iter()
            .any(|m| m["id"] == request["id"] && m.get("result") == Some(&Value::Null)));
        let token = &request["params"]["token"];
        let phases: Vec<_> = client
            .server
            .notifications
            .iter()
            .filter(|m| m["method"] == "$/progress" && m["params"]["token"] == *token)
            .map(|m| m["params"]["value"]["kind"].as_str().unwrap())
            .collect();
        assert_eq!(phases.first(), Some(&"begin"));
        assert_eq!(phases.last(), Some(&"end"));
        assert!(phases[1..phases.len() - 1].iter().all(|p| *p == "report"));
    }
    assert!(client
        .server
        .sent
        .iter()
        .any(|m| m["method"] == "window/workDoneProgress/cancel"));
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    client.finish();
}
