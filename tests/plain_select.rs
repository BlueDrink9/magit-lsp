//! Plain presentation of the commit-selection log.

use magit_lsp_test_support as common;
use magit_lsp_test_support::fake_editor;

use common::stdio;
use common::{RepoFixture, TestSession, WIDE_VIEWPORT};
use fake_editor::{FakeEditor, Profile as EditorProfile};
use magit_lsp::file_uri::uri_from_path;
use magit_lsp::protocol::Cursor;
use magit_lsp::tree::NodeKind;
use serde_json::{json, Value};

/// Drain messages until the client applies a `workspace/applyEdit` request.
fn next_apply(server: &mut stdio::TestServer) -> Value {
    loop {
        let message = server.recv();
        if message["method"] == "workspace/applyEdit" {
            return message;
        }
    }
}

/// Open the repository's `.magit` status document as a Plain client.
fn open_plain(repo: &RepoFixture) -> (stdio::TestServer, FakeEditor, String) {
    open_plain_with_env(repo, &[])
}

/// Open the repository's `.magit` status document as a Plain client, spawning
/// the server with `env` overrides.
fn open_plain_with_env(
    repo: &RepoFixture,
    env: &[(&str, &str)],
) -> (stdio::TestServer, FakeEditor, String) {
    magit_lsp_test_support::cargo_server_binary!();
    let uri = uri_from_path(&repo.path().join(".magit"))
        .expect("native path has a file URI")
        .as_str()
        .to_owned();
    let mut server = stdio::TestServer::spawn(env);
    let mut init = stdio::initialize_message();
    init["params"]["capabilities"] = json!({
        "workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},
        "textDocument":{"codeAction":{"dynamicRegistration":true},"general":{"positionEncodings":["utf-16"]}}
    });
    init["params"]["workspaceFolders"] = json!([{"uri":uri_from_path(repo.path()).expect("native path has a file URI").as_str(),"name":"repo"}]);
    server.send(init);
    let initialized = server.response_with_id(1);
    assert_eq!(
        initialized["result"]["capabilities"]["codeActionProvider"],
        true
    );
    server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    let mut editor = FakeEditor::new(EditorProfile::Standard);
    server.send(editor.open(&uri, "old"));
    let edit = next_apply(&mut server);
    for frame in editor.dispatch(&edit) {
        server.send(frame);
    }
    (server, editor, uri)
}

/// The only offered action with `command` on `range`.
fn offered_for_range(
    server: &mut stdio::TestServer,
    uri: &str,
    range: Value,
    command: &str,
    id: i64,
) -> Value {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":range,"context":{"diagnostics":[]}}}));
    server.response_with_id(id)["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["command"]["arguments"][0]["command"] == command)
        .map(|action| action["command"]["arguments"][0].clone())
        .unwrap_or(Value::Null)
}

/// The offered `menu.run-option` action whose structured target is `item_key`.
fn offered_menu_option(
    server: &mut stdio::TestServer,
    uri: &str,
    line: u32,
    item_key: &str,
    id: i64,
) -> Value {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":line,"character":0},"end":{"line":line,"character":0}},"context":{"diagnostics":[]}}}));
    server.response_with_id(id)["result"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|action| action["command"]["arguments"][0]["command"] == "menu.run-option")
        .find(|action| action["command"]["arguments"][0]["target"]["itemKey"] == item_key)
        .map(|action| action["command"]["arguments"][0].clone())
        .unwrap_or(Value::Null)
}

/// Execute an offered Plain action, acknowledging any render edits it causes.
fn execute_offered(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    args: Value,
    id: i64,
) -> Value {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[args]}}));
    loop {
        let message = server.recv();
        if message["method"] == "workspace/applyEdit" {
            for frame in editor.dispatch(&message) {
                server.send(frame);
            }
            continue;
        }
        if message["id"] == json!(id) {
            return message;
        }
    }
}

/// The rendered row index of the line containing `needle`.
fn row_containing(text: &str, needle: &str) -> u32 {
    text.lines()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("missing {needle:?} row: {text}")) as u32
}

/// The document text with one row replaced by `replacement`.
///
/// The captured final-terminator convention is preserved, so the edited text
/// matches the server's serialized field document exactly.
fn replace_row(text: &str, row: usize, replacement: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    lines[row] = replacement.to_string();
    let mut edited = lines.join("\n");
    if text.ends_with('\n') {
        edited.push('\n');
    }
    edited
}

/// Open the status view's `log.select-and-copy` with `args`, using `id` for
/// the code-action request and the following id for the invocation.
fn open_log_select_with_args(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    uri: &str,
    args: Value,
    id: i64,
) -> String {
    server.send(
        json!({"jsonrpc":"2.0","id":id,"method":"textDocument/codeAction","params":{
        "textDocument":{"uri":uri},
        "range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},
        "context":{"diagnostics":[]}}}),
    );
    let status_actions = server.response_with_id(id)["result"]
        .as_array()
        .unwrap()
        .clone();
    let status_view = status_actions
        .iter()
        .find_map(|action| action["command"]["arguments"][0]["target"]["viewId"].as_str())
        .map(str::to_owned)
        .expect("a view target exposing the status view id");
    server.send(
        json!({"jsonrpc":"2.0","id":id + 1,"method":"view/invoke","params":{
        "viewId":status_view,"command":"log.select-and-copy","args":args}}),
    );
    let invoked = loop {
        let message = server.recv();
        if message["method"] == "workspace/applyEdit" {
            for frame in editor.dispatch(&message) {
                server.send(frame);
            }
            continue;
        }
        if message["id"] == json!(id + 1) {
            break message;
        }
    };
    assert!(
        invoked["error"].is_null(),
        "log.select-and-copy failed: {invoked}"
    );
    invoked["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == "pushView")
        .unwrap_or_else(|| panic!("log select view not pushed: {invoked}"))["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Open the status view's `log.select-and-copy` with free-text revisions.
fn open_free_text_log_select(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    uri: &str,
) -> String {
    open_log_select_with_args(server, editor, uri, json!({"allowFreeText":true}), 19)
}

/// Send a full-text `didChange` for `uri`.
fn send_full_text_change(server: &mut stdio::TestServer, uri: &str, version: i64, text: String) {
    server.send(
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":uri,"version":version},
        "contentChanges":[{"text":text}]}}),
    );
}

#[test]
fn plain_commit_menu_option_renders_the_log_select_document() {
    let (repo, _) = RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .commit("second", [("f.txt", "two\n")])
        .modify("f.txt", "changed\n")
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo);
    let file_row = row_containing(editor.text(&uri), "f.txt");
    let open = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":file_row,"character":0},"end":{"line":file_row,"character":0}}),
        "open.commit",
        10,
    );
    assert!(!open.is_null(), "open.commit not offered");
    let opened = execute_offered(&mut server, &mut editor, open, 11);
    assert!(opened["error"].is_null(), "open.commit failed: {opened}");

    let menu_text = editor.text(&uri).to_owned();
    let reuse = row_containing(&menu_text, "Reuse commit message");
    let option = offered_menu_option(&mut server, &uri, reuse, "=C", 12);
    assert!(!option.is_null(), "menu.run-option =C not offered");

    let staged = execute_offered(&mut server, &mut editor, option, 13);
    assert!(
        staged["error"].is_null(),
        "menu.run-option failed: {staged}"
    );

    let text = editor.text(&uri).to_owned();
    assert!(
        text.contains("Log — Reuse commit message (--reuse-message=)"),
        "log select header missing: {text}"
    );
    assert!(
        text.contains("Select a commit for --reuse-message= (or enter a revision)"),
        "log select prompt missing: {text}"
    );
    assert!(
        text.contains("initial") && text.contains("second"),
        "log select commit rows missing: {text}"
    );
    let pushed = staged["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == "pushView")
        .unwrap_or_else(|| panic!("log select view not pushed: {staged}"));
    assert_eq!(pushed["descriptor"]["scope"], "log.select");
    assert!(
        !server
            .sent
            .iter()
            .any(|message| message["method"] == "view/prompt"),
        "a Plain log select must not request a native prompt"
    );
    assert!(
        !server
            .sent
            .iter()
            .any(|message| message["method"] == "view/pick"),
        "a Plain log select must not request a native pick"
    );
}

#[test]
fn plain_log_select_offers_commit_code_actions() {
    let (repo, _) = RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .commit("second", [("f.txt", "two\n")])
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo);
    let head = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();

    // A Plain client learns the status view id from the offered actions.
    server.send(
        json!({"jsonrpc":"2.0","id":19,"method":"textDocument/codeAction","params":{
        "textDocument":{"uri":uri},
        "range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},
        "context":{"diagnostics":[]}}}),
    );
    let status_actions = server.response_with_id(19)["result"]
        .as_array()
        .unwrap()
        .clone();
    let status_view = status_actions
        .iter()
        .find_map(|action| action["command"]["arguments"][0]["target"]["viewId"].as_str())
        .map(str::to_owned)
        .expect("a view target exposing the status view id");
    server.send(
        json!({"jsonrpc":"2.0","id":20,"method":"view/invoke","params":{
        "viewId":status_view,"command":"log.select-and-copy","args":{"multi":true}}}),
    );
    let invoked = loop {
        let message = server.recv();
        if message["method"] == "workspace/applyEdit" {
            for frame in editor.dispatch(&message) {
                server.send(frame);
            }
            continue;
        }
        if message["id"] == json!(20) {
            break message;
        }
    };
    assert!(
        invoked["error"].is_null(),
        "log.select-and-copy failed: {invoked}"
    );
    let pushed = invoked["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == "pushView")
        .unwrap_or_else(|| panic!("log select view not pushed: {invoked}"));
    assert_eq!(pushed["descriptor"]["scope"], "log.select");

    let text = editor.text(&uri).to_owned();
    let row = row_containing(&text, "second");
    let range = json!({"start":{"line":row,"character":0},"end":{"line":row,"character":0}});
    server.send(
        json!({"jsonrpc":"2.0","id":21,"method":"textDocument/codeAction","params":{
        "textDocument":{"uri":uri},"range":range,"context":{"diagnostics":[]}}}),
    );
    let actions = server.response_with_id(21)["result"]
        .as_array()
        .unwrap()
        .clone();
    let args_for = |command: &str| -> Value {
        actions
            .iter()
            .find(|action| action["command"]["arguments"][0]["command"] == command)
            .map(|action| action["command"]["arguments"][0].clone())
            .unwrap_or_else(|| panic!("{command} not offered: {actions:?}"))
    };

    let confirm = args_for("common.confirm");
    assert_eq!(
        confirm["target"]["nodeId"],
        format!("log:commit:{head}"),
        "confirm must target the structured commit node"
    );
    assert!(
        !confirm["target"]["mutationHash"]
            .as_str()
            .unwrap_or_default()
            .is_empty(),
        "confirm must carry a mutation hash"
    );
    let cancel = args_for("common.cancel");
    assert!(
        cancel["target"]["viewId"].is_string(),
        "cancel must be a view action: {cancel}"
    );
    let toggle = args_for("select.toggle-mark");
    assert_eq!(toggle["target"]["nodeId"], format!("log:commit:{head}"));

    let confirmed = execute_offered(&mut server, &mut editor, confirm, 22);
    assert!(
        confirmed["error"].is_null(),
        "common.confirm failed: {confirmed}"
    );
    let clipboard = confirmed["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == "clipboard")
        .unwrap_or_else(|| panic!("no clipboard effect: {confirmed}"));
    assert_eq!(clipboard["text"], head);
}

#[test]
fn plain_already_set_commit_option_clears_and_restores_the_menu() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .commit("second", [("f.txt", "two\n")])
        .build();
    let (mut test, _) =
        TestSession::open_status_at(&tmp, magit_lsp::config::Profile::Standard, WIDE_VIEWPORT);
    test.ack();
    test.invoke("open.commit");
    let menu_view = test.view_id().to_string();
    let unset_menu = test.text_of(&menu_view).join("\n");

    // Stage the option through the normal selection path: open the log select,
    // confirm a structured commit node and return to the menu.
    test.press("=C");
    let log_view = test.view_id().to_string();
    assert_ne!(
        log_view, menu_view,
        "the commit option opens the log select"
    );
    test.ack();
    let line = test
        .tree_of(&log_view)
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Commit)
        .expect("a structured commit node")
        .start_line;
    test.invoke_at("common.confirm", Cursor { line, column: 0 });
    match test
        .menu(&menu_view)
        .and_then(|menu| menu.item("=C").cloned())
    {
        Some(magit_lsp::menu::Item::Option { value: Some(_), .. }) => {}
        other => panic!("commit option was not set: {other:?}"),
    }

    let views_before = test.view_ids();
    let generation = test.version_of(&menu_view);
    let step = test
        .session_mut()
        .start_plain_menu_option(&menu_view, generation, "commit", "=C")
        .expect("start plain menu option");
    let result = match step {
        magit_lsp::session::Execution::Done(result) => result,
        _ => panic!("an already-set option must not prompt or pick again"),
    };
    match test
        .menu(&menu_view)
        .and_then(|menu| menu.item("=C").cloned())
    {
        Some(magit_lsp::menu::Item::Option { value: None, .. }) => {}
        other => panic!("commit option was not cleared: {other:?}"),
    }
    assert_eq!(
        test.text_of(&menu_view).join("\n"),
        unset_menu,
        "clearing must return the menu unchanged"
    );
    assert_eq!(
        test.view_ids(),
        views_before,
        "clearing must not push a log select view"
    );
    assert!(
        result.render.is_some(),
        "clearing must rerender the menu it changed"
    );
    assert!(
        !result
            .effects
            .iter()
            .any(|effect| matches!(effect, magit_lsp::protocol::Effect::PushView { .. })),
        "clearing must not push a view"
    );
}

#[test]
fn plain_log_select_confirm_returns_the_caller_menu_with_the_chosen_commit() {
    let (repo, _) = RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .commit("second", [("f.txt", "two\n")])
        .modify("f.txt", "changed\n")
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo);
    let head = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let file_row = row_containing(editor.text(&uri), "f.txt");
    let open = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":file_row,"character":0},"end":{"line":file_row,"character":0}}),
        "open.commit",
        50,
    );
    let opened = execute_offered(&mut server, &mut editor, open, 51);
    assert!(opened["error"].is_null(), "open.commit failed: {opened}");
    let reuse = row_containing(editor.text(&uri), "Reuse commit message");
    let option = offered_menu_option(&mut server, &uri, reuse, "=C", 52);
    assert!(!option.is_null(), "menu.run-option =C not offered");

    let staged = execute_offered(&mut server, &mut editor, option, 53);
    assert!(
        staged["error"].is_null(),
        "menu.run-option failed: {staged}"
    );
    let log_view = staged["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == "pushView")
        .unwrap_or_else(|| panic!("log select view not pushed: {staged}"))["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let text = editor.text(&uri).to_owned();
    assert!(text.contains("Log — Reuse commit message"), "{text}");
    let commit_row = row_containing(&text, "second");
    let confirm = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":commit_row,"character":0},"end":{"line":commit_row,"character":0}}),
        "common.confirm",
        54,
    );
    assert!(!confirm.is_null(), "confirm not offered on the commit row");

    let confirmed = execute_offered(&mut server, &mut editor, confirm, 55);
    assert!(
        confirmed["error"].is_null(),
        "common.confirm failed: {confirmed}"
    );
    let effects = confirmed["result"]["effects"].as_array().unwrap();
    assert!(
        effects
            .iter()
            .any(|effect| effect["type"] == "popView" && effect["viewId"] == json!(log_view)),
        "confirm must pop the log select: {confirmed}"
    );

    let text = editor.text(&uri).to_owned();
    assert!(
        !text.contains("Log — Reuse commit message"),
        "no stale log select document remains: {text}"
    );
    assert!(
        text.contains(&format!("--reuse-message={head}")),
        "the chosen commit must reach the staged continuation: {text}"
    );
    let reuse = row_containing(&text, "Reuse commit message");
    let option = offered_menu_option(&mut server, &uri, reuse, "=C", 56);
    assert!(
        !option.is_null(),
        "the caller menu must be usable after confirm: {text}"
    );
}

#[test]
fn plain_log_select_cancel_returns_the_caller_without_changes() {
    let (repo, _) = RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .commit("second", [("f.txt", "two\n")])
        .modify("f.txt", "changed\n")
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo);
    let file_row = row_containing(editor.text(&uri), "f.txt");
    let open = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":file_row,"character":0},"end":{"line":file_row,"character":0}}),
        "open.commit",
        60,
    );
    let opened = execute_offered(&mut server, &mut editor, open, 61);
    assert!(opened["error"].is_null(), "open.commit failed: {opened}");
    let menu_text = editor.text(&uri).to_owned();
    let reuse = row_containing(&menu_text, "Reuse commit message");
    let option = offered_menu_option(&mut server, &uri, reuse, "=C", 62);
    let staged = execute_offered(&mut server, &mut editor, option, 63);
    assert!(
        staged["error"].is_null(),
        "menu.run-option failed: {staged}"
    );
    let log_view = staged["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == "pushView")
        .unwrap_or_else(|| panic!("log select view not pushed: {staged}"))["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let status_before = repo.git(&["status", "--porcelain"]);
    let head_before = repo.git(&["rev-parse", "HEAD"]);

    let text = editor.text(&uri).to_owned();
    let cancel_row = row_containing(&text, "Log — Reuse commit message");
    let cancel_range =
        json!({"start":{"line":cancel_row,"character":0},"end":{"line":cancel_row,"character":0}});
    let cancel = {
        server.send(
            json!({"jsonrpc":"2.0","id":64,"method":"textDocument/codeAction","params":{
            "textDocument":{"uri":uri},"range":cancel_range,"context":{"diagnostics":[]}}}),
        );
        let actions = server.response_with_id(64)["result"]
            .as_array()
            .unwrap()
            .clone();
        actions
            .iter()
            .find(|action| action["command"]["arguments"][0]["command"] == "common.cancel")
            .map(|action| action["command"]["arguments"][0].clone())
            .unwrap_or_else(|| panic!("common.cancel not offered: {actions:?}"))
    };
    let cancelled = execute_offered(&mut server, &mut editor, cancel, 65);
    assert!(
        cancelled["error"].is_null(),
        "common.cancel failed: {cancelled}"
    );
    let effects = cancelled["result"]["effects"].as_array().unwrap();
    assert!(
        !effects.iter().any(|effect| effect["type"] == "clipboard"),
        "cancel must not copy: {cancelled}"
    );
    assert!(
        effects
            .iter()
            .any(|effect| effect["type"] == "popView" && effect["viewId"] == json!(log_view)),
        "cancel must pop the log select: {cancelled}"
    );

    let text = editor.text(&uri).to_owned();
    assert_eq!(
        text, menu_text,
        "cancel must restore the caller menu unchanged"
    );
    assert_eq!(
        repo.git(&["status", "--porcelain"]),
        status_before,
        "cancel must not mutate the repository"
    );
    assert_eq!(
        repo.git(&["rev-parse", "HEAD"]),
        head_before,
        "cancel must not move HEAD"
    );

    let reuse = row_containing(&text, "Reuse commit message");
    let option = offered_menu_option(&mut server, &uri, reuse, "=C", 66);
    assert!(
        !option.is_null(),
        "the caller menu must stay usable after cancel: {text}"
    );
    let again = execute_offered(&mut server, &mut editor, option, 67);
    assert!(again["error"].is_null(), "menu.run-option failed: {again}");
    assert_eq!(
        again["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|effect| effect["type"] == "pushView")
            .unwrap_or_else(|| panic!("log select view not pushed again: {again}"))["descriptor"]
            ["scope"],
        "log.select",
        "the caller menu must open the selection again"
    );
}

#[test]
fn plain_log_select_revision_field_drives_confirm() {
    let (repo, _) = RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .commit("second", [("f.txt", "two\n")])
        .commit("third", [("f.txt", "three\n")])
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo);
    let parent = repo.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let _view = open_free_text_log_select(&mut server, &mut editor, &uri);

    let text = editor.text(&uri).to_owned();
    let field = row_containing(&text, "Select commit(s) to copy") + 1;
    let field_range =
        json!({"start":{"line":field,"character":0},"end":{"line":field,"character":0}});
    let offered = offered_for_range(&mut server, &uri, field_range.clone(), "common.confirm", 30);
    assert!(
        !offered.is_null(),
        "confirm not offered on the revision field"
    );
    assert_eq!(
        offered["target"]["nodeId"], "log:field:rev",
        "the revision must be a structured field node: {offered}"
    );

    send_full_text_change(
        &mut server,
        &uri,
        100,
        replace_row(&text, field as usize, "HEAD~1"),
    );
    let offered = offered_for_range(&mut server, &uri, field_range, "common.confirm", 31);
    assert!(
        !offered.is_null(),
        "confirm not offered after the field edit"
    );
    assert_eq!(offered["target"]["nodeId"], "log:field:rev");

    let confirmed = execute_offered(&mut server, &mut editor, offered, 32);
    assert!(
        confirmed["error"].is_null(),
        "common.confirm failed: {confirmed}"
    );
    let clipboard = confirmed["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == "clipboard")
        .unwrap_or_else(|| panic!("no clipboard effect: {confirmed}"));
    assert_eq!(
        clipboard["text"], parent,
        "a symbolic revision must resolve to the full commit id"
    );
}

#[test]
fn plain_log_select_stale_generation_is_refused_without_changes() {
    let (repo, _) = RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .commit("second", [("f.txt", "two\n")])
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo);
    let _view = open_free_text_log_select(&mut server, &mut editor, &uri);
    let status_before = repo.git(&["status", "--porcelain"]);
    let head_before = repo.git(&["rev-parse", "HEAD"]);

    let text = editor.text(&uri).to_owned();
    let commit = row_containing(&text, "second");
    let offered = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":commit,"character":0},"end":{"line":commit,"character":0}}),
        "common.confirm",
        70,
    );
    assert!(!offered.is_null(), "confirm not offered on the commit row");
    let generation = offered["viewGeneration"]
        .as_u64()
        .expect("an offered action carries its view generation");
    let mut stale = offered.clone();
    stale["viewGeneration"] = json!(generation.checked_sub(1).unwrap_or(generation + 1));

    let refused = execute_offered(&mut server, &mut editor, stale, 71);
    assert!(
        refused["error"].is_null(),
        "a stale confirm must not be a JSON-RPC error: {refused}"
    );
    let effects = refused["result"]["effects"].as_array().unwrap();
    assert!(
        effects
            .iter()
            .any(|effect| effect["type"] == "message" && effect["level"] == "warn"),
        "a stale generation must report a readable warning: {refused}"
    );
    assert!(
        !effects.iter().any(|effect| effect["type"] == "clipboard"),
        "a stale generation must not copy: {refused}"
    );
    assert!(
        !effects.iter().any(|effect| effect["type"] == "popView"),
        "a stale generation must not pop the selection: {refused}"
    );

    let after = editor.text(&uri).to_owned();
    assert_eq!(after, text, "the log select document must not change");
    assert!(
        after.contains("Select commit(s) to copy"),
        "the log select must stay displayed: {after}"
    );
    assert_eq!(
        repo.git(&["status", "--porcelain"]),
        status_before,
        "a stale generation must not mutate the repository"
    );
    assert_eq!(
        repo.git(&["rev-parse", "HEAD"]),
        head_before,
        "a stale generation must not move HEAD"
    );
}

#[test]
fn plain_log_select_tampered_target_is_refused_without_changes() {
    let (repo, _) = RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .commit("second", [("f.txt", "two\n")])
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo);
    let _view = open_free_text_log_select(&mut server, &mut editor, &uri);
    let status_before = repo.git(&["status", "--porcelain"]);
    let head_before = repo.git(&["rev-parse", "HEAD"]);

    let text = editor.text(&uri).to_owned();
    let commit = row_containing(&text, "second");
    let range = json!({"start":{"line":commit,"character":0},"end":{"line":commit,"character":0}});
    let offered = offered_for_range(&mut server, &uri, range.clone(), "common.confirm", 80);
    assert!(!offered.is_null(), "confirm not offered on the commit row");

    let mut tampered_hash = offered.clone();
    tampered_hash["target"]["mutationHash"] = json!("deadbeef");
    let refused = execute_offered(&mut server, &mut editor, tampered_hash, 81);
    assert!(
        refused["error"].is_null(),
        "a tampered hash must not be a JSON-RPC error: {refused}"
    );
    let effects = refused["result"]["effects"].as_array().unwrap();
    assert!(
        effects
            .iter()
            .any(|effect| effect["type"] == "message" && effect["level"] == "warn"),
        "a tampered mutation hash must report a readable warning: {refused}"
    );
    assert!(
        !effects.iter().any(|effect| effect["type"] == "clipboard"),
        "a tampered mutation hash must not copy: {refused}"
    );
    assert!(
        !effects.iter().any(|effect| effect["type"] == "popView"),
        "a tampered mutation hash must not pop the selection: {refused}"
    );

    let mut missing_node = offered.clone();
    missing_node["target"]["nodeId"] = json!("log:commit:does-not-exist");
    let refused = execute_offered(&mut server, &mut editor, missing_node, 82);
    assert!(
        refused["error"].is_null(),
        "a missing node must not be a JSON-RPC error: {refused}"
    );
    let effects = refused["result"]["effects"].as_array().unwrap();
    assert!(
        effects
            .iter()
            .any(|effect| effect["type"] == "message" && effect["level"] == "warn"),
        "a missing node must report a readable warning: {refused}"
    );
    assert!(
        !effects.iter().any(|effect| effect["type"] == "clipboard"),
        "a missing node must not copy: {refused}"
    );
    assert!(
        !effects.iter().any(|effect| effect["type"] == "popView"),
        "a missing node must not pop the selection: {refused}"
    );

    assert_eq!(
        editor.text(&uri),
        text,
        "a refused target must not change the log select document"
    );
    assert_eq!(
        repo.git(&["status", "--porcelain"]),
        status_before,
        "a refused target must not mutate the repository"
    );
    assert_eq!(
        repo.git(&["rev-parse", "HEAD"]),
        head_before,
        "a refused target must not move HEAD"
    );
}

#[test]
fn plain_log_select_invalid_revision_changes_nothing_and_keeps_the_picker() {
    let (repo, _) = RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .commit("second", [("f.txt", "two\n")])
        .commit("third", [("f.txt", "three\n")])
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo);
    let head = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let _view = open_free_text_log_select(&mut server, &mut editor, &uri);

    let text = editor.text(&uri).to_owned();
    let field = row_containing(&text, "Select commit(s) to copy") + 1;
    send_full_text_change(
        &mut server,
        &uri,
        100,
        replace_row(&text, field as usize, "no-such-revision"),
    );

    let field_range =
        json!({"start":{"line":field,"character":0},"end":{"line":field,"character":0}});
    let offered = offered_for_range(&mut server, &uri, field_range, "common.confirm", 40);
    assert!(
        !offered.is_null(),
        "confirm not offered on the revision field"
    );
    let rejected = execute_offered(&mut server, &mut editor, offered, 41);
    assert!(rejected["error"].is_null(), "confirm failed: {rejected}");
    assert!(
        !rejected["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == "clipboard"),
        "an invalid revision must not copy: {rejected}"
    );
    assert!(
        rejected
            .to_string()
            .contains("No valid commits were selected"),
        "an invalid revision must report a readable message: {rejected}"
    );

    let text = editor.text(&uri).to_owned();
    let commit = row_containing(&text, "third");
    let offered = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":commit,"character":0},"end":{"line":commit,"character":0}}),
        "common.confirm",
        42,
    );
    assert!(!offered.is_null(), "the picker must stay usable: {text}");
    let confirmed = execute_offered(&mut server, &mut editor, offered, 43);
    let clipboard = confirmed["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == "clipboard")
        .unwrap_or_else(|| panic!("no clipboard effect: {confirmed}"));
    assert_eq!(clipboard["text"], head);
}

#[test]
fn plain_log_select_fixup_target_survives_a_cancelled_second_run() {
    let (repo, _) = RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .commit("second", [("f.txt", "two\n")])
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo);
    let head = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let status_text = editor.text(&uri).to_owned();
    let repo_before = (
        repo.git(&["status", "--porcelain"]),
        repo.git(&["rev-parse", "HEAD"]),
    );

    // First run: choose the fixup target and copy its object id.
    let first_view = open_log_select_with_args(
        &mut server,
        &mut editor,
        &uri,
        json!({"purpose":"fixup","prompt":"Select commit to fixup"}),
        90,
    );
    let text = editor.text(&uri).to_owned();
    assert!(
        text.contains("Log — fixup"),
        "the selection must carry the fixup purpose: {text}"
    );
    let commit = row_containing(&text, "second");
    let confirm = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":commit,"character":0},"end":{"line":commit,"character":0}}),
        "common.confirm",
        92,
    );
    assert!(!confirm.is_null(), "confirm not offered on the commit row");
    let confirmed = execute_offered(&mut server, &mut editor, confirm, 93);
    assert!(
        confirmed["error"].is_null(),
        "common.confirm failed: {confirmed}"
    );
    let effects = confirmed["result"]["effects"].as_array().unwrap();
    let chosen = effects
        .iter()
        .find(|effect| effect["type"] == "clipboard")
        .unwrap_or_else(|| panic!("no clipboard effect: {confirmed}"))["text"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(chosen, head, "the fixup target must be the chosen commit");
    assert!(
        effects
            .iter()
            .any(|effect| effect["type"] == "popView" && effect["viewId"] == json!(first_view)),
        "confirm must pop the first run: {confirmed}"
    );
    assert_eq!(
        editor.text(&uri),
        status_text,
        "confirm must return the unaffected caller"
    );

    // Second run: open the same fixup selection and cancel it.
    let second_view = open_log_select_with_args(
        &mut server,
        &mut editor,
        &uri,
        json!({"purpose":"fixup","prompt":"Select commit to fixup"}),
        94,
    );
    // A Plain client reuses its single `log.select` document view id.
    let text = editor.text(&uri).to_owned();
    assert!(
        text.contains("Log — fixup"),
        "the second selection must carry the fixup purpose: {text}"
    );
    let header = row_containing(&text, "Log — fixup");
    let cancel = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":header,"character":0},"end":{"line":header,"character":0}}),
        "common.cancel",
        96,
    );
    assert!(!cancel.is_null(), "cancel not offered on the selection");
    let cancelled = execute_offered(&mut server, &mut editor, cancel, 97);
    assert!(
        cancelled["error"].is_null(),
        "common.cancel failed: {cancelled}"
    );
    let effects = cancelled["result"]["effects"].as_array().unwrap();
    assert!(
        !effects.iter().any(|effect| effect["type"] == "clipboard"),
        "the cancelled second run must not copy: {cancelled}"
    );
    assert!(
        effects
            .iter()
            .any(|effect| effect["type"] == "popView" && effect["viewId"] == json!(second_view)),
        "cancel must pop the second run: {cancelled}"
    );
    assert_eq!(
        editor.text(&uri),
        status_text,
        "the cancelled second run must leave the caller unchanged"
    );

    assert_eq!(
        chosen, head,
        "the first chosen commit id must stay unchanged"
    );
    assert_eq!(
        (
            repo.git(&["status", "--porcelain"]),
            repo.git(&["rev-parse", "HEAD"]),
        ),
        repo_before,
        "the cancelled second run must not mutate HEAD, the index or the worktree"
    );
}
