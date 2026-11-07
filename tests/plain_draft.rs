//! Plain-mode commit draft delivery: replace an already-open buffer first.
#[path = "common/fake_editor.rs"]
mod fake_editor;
#[path = "common/editor_model/mod.rs"]
mod editor_model;
#[path = "common/mod.rs"]
mod fixtures;
#[path = "common/stdio.rs"]
mod stdio;

use fake_editor::{FakeEditor, Profile};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

fn caps(show_document: bool) -> Value {
    let mut capabilities = json!({
        "workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},
        "textDocument":{"codeAction":{}},
        "general":{"positionEncodings":["utf-8","utf-16","utf-32"]}
    });
    if show_document {
        capabilities["window"] = json!({"showDocument":{"support":true}});
    }
    capabilities
}

/// Open the repository's `.magit` status document as a Plain client.
fn open_plain(
    repo: &fixtures::RepoFixture,
    capabilities: Value,
) -> (stdio::TestServer, FakeEditor, String) {
    let uri = format!("file://{}/.magit", repo.path().display());
    let mut server = stdio::TestServer::spawn(&[]);
    let mut init = stdio::initialize_message();
    init["params"]["capabilities"] = capabilities;
    init["params"]["workspaceFolders"] =
        json!([{"uri":format!("file://{}",repo.path().display()),"name":"repo"}]);
    server.send(init);
    let initialized = server.response_with_id(1);
    assert!(
        initialized["result"]["capabilities"]["codeActionProvider"] == true,
        "Plain initialize failed: {initialized}"
    );
    server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    let mut editor = FakeEditor::new(Profile::Standard);
    server.send(editor.open(&uri, "stale\n"));
    loop {
        let message = server.recv();
        if message["method"] == "workspace/applyEdit" {
            for frame in editor.dispatch(&message) {
                server.send(frame);
            }
            break;
        }
    }
    (server, editor, uri)
}

/// Whether an apply edit targets the pending commit draft URI.
fn draft_edit(message: &Value, draft_uri: &str) -> bool {
    let edit = &message["params"]["edit"];
    edit["documentChanges"]
        .as_array()
        .is_some_and(|changes| changes.iter().any(|change| change["textDocument"]["uri"] == draft_uri))
        || edit["changes"].get(draft_uri).is_some()
}

/// Receive one frame, answering requests; a draft edit is applied without an echo.
fn pump(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    draft_uri: &str,
    seen: &mut Vec<Value>,
) -> Value {
    let message = server.recv();
    seen.push(message.clone());
    if message["method"] == "workspace/applyEdit" {
        if draft_edit(&message, draft_uri) {
            let result = editor.apply_edit(&message["params"]["edit"]);
            server.send(json!({"jsonrpc":"2.0","id":message["id"],"result":result}));
        } else {
            for frame in editor.dispatch(&message) {
                server.send(frame);
            }
        }
    } else if message["method"].is_string() && !message["id"].is_null() {
        for frame in editor.dispatch(&message) {
            server.send(frame);
        }
    }
    message
}

fn code_actions(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    draft_uri: &str,
    seen: &mut Vec<Value>,
    uri: &str,
    line: u32,
    id: i64,
) -> Vec<Value> {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":line,"character":0},"end":{"line":line,"character":0}},"context":{"diagnostics":[]}}}));
    loop {
        let message = pump(server, editor, draft_uri, seen);
        if message["id"] == json!(id) && message["method"].is_null() {
            return message["result"].as_array().unwrap().clone();
        }
    }
}

fn execute_action(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    draft_uri: &str,
    seen: &mut Vec<Value>,
    args: Value,
    id: i64,
) -> Value {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[args]}}));
    loop {
        let message = pump(server, editor, draft_uri, seen);
        if message["id"] == json!(id) && message["method"].is_null() {
            return message;
        }
    }
}

/// Stage `f.txt` and return the offered `menu.run-action` commit creation.
fn prepare_commit(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    draft_uri: &str,
    seen: &mut Vec<Value>,
    uri: &str,
) -> Value {
    let text = editor.text(uri).to_owned();
    let line = text.lines().position(|line| line.contains("f.txt")).unwrap_or(0) as u32;
    let stage = code_actions(server, editor, draft_uri, seen, uri, line, 90)
        .into_iter()
        .find(|action| action["command"]["arguments"][0]["command"] == "status.stage")
        .expect("status.stage offered")["command"]["arguments"][0]
        .clone();
    assert!(execute_action(server, editor, draft_uri, seen, stage, 91)["error"].is_null());

    let text = editor.text(uri).to_owned();
    let line = text.lines().position(|line| line.contains("f.txt")).unwrap_or(0) as u32;
    let open_commit = code_actions(server, editor, draft_uri, seen, uri, line, 92)
        .into_iter()
        .find(|action| action["command"]["arguments"][0]["command"] == "open.commit")
        .expect("open.commit offered")["command"]["arguments"][0]
        .clone();
    assert!(execute_action(server, editor, draft_uri, seen, open_commit, 93)["error"].is_null());

    let menu = editor.text(uri).to_owned();
    let line = menu.lines().position(|line| line.contains("Commit")).unwrap_or(0) as u32;
    code_actions(server, editor, draft_uri, seen, uri, line, 94)
        .into_iter()
        .find(|action| {
            action["command"]["arguments"][0]["command"] == "menu.run-action"
                && action["command"]["arguments"][0]["target"]["itemKey"].is_string()
        })
        .expect("commit creation offered")["command"]["arguments"][0]
        .clone()
}

fn wait_for_draft_messages(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    draft_uri: &str,
    seen: &mut Vec<Value>,
) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !seen.iter().any(|message| message["method"] == "workspace/applyEdit" && draft_edit(message, draft_uri))
        || !seen.iter().any(|message| message["method"] == "window/showDocument" && message["params"]["uri"] == draft_uri)
    {
        assert!(Instant::now() < deadline, "draft delivery timed out: seen {seen:#?}");
        pump(server, editor, draft_uri, seen);
    }
}

#[test]
fn open_plain_commit_buffer_is_replaced_before_show_document() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo, caps(true));
    let expected_path = repo.path().join(".git/COMMIT_EDITMSG").to_string_lossy().to_string();
    let expected_uri = format!("file://{expected_path}");
    let stale = "Previous commit message\n";
    // The client opened this buffer before the new run issued its template.
    server.send(editor.open(&expected_uri, stale));

    let mut seen = Vec::new();
    let create = prepare_commit(&mut server, &mut editor, &expected_uri, &mut seen, &uri);
    assert!(execute_action(&mut server, &mut editor, &expected_uri, &mut seen, create, 95)["error"].is_null());
    wait_for_draft_messages(&mut server, &mut editor, &expected_uri, &mut seen);

    let apply_index = seen
        .iter()
        .position(|message| message["method"] == "workspace/applyEdit" && draft_edit(message, &expected_uri))
        .expect("open buffer receives the new template");
    let show_index = seen
        .iter()
        .position(|message| message["method"] == "window/showDocument" && message["params"]["uri"] == expected_uri)
        .expect("the draft is shown");
    assert!(apply_index < show_index, "applyEdit must precede showDocument: {seen:#?}");

    let apply = &seen[apply_index];
    let change = &apply["params"]["edit"]["documentChanges"][0];
    assert_eq!(change["textDocument"]["uri"], expected_uri);
    assert_eq!(change["textDocument"]["version"], 1, "the store's current version: {apply}");
    let edits = change["edits"].as_array().unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0]["range"], json!({"start":{"line":0,"character":0},"end":{"line":1,"character":0}}));
    let template = fixtures::read(std::path::Path::new(&expected_path));
    assert_eq!(edits[0]["newText"], template.as_str());
    assert_ne!(edits[0]["newText"], stale);
    assert_eq!(editor.text(&expected_uri), template, "the client applied the replacement");

    let show = &seen[show_index];
    assert_eq!(show["params"], json!({"uri":expected_uri,"external":false,"takeFocus":true}));
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn unopened_plain_commit_buffer_is_only_shown() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo, caps(true));
    let expected_path = repo.path().join(".git/COMMIT_EDITMSG").to_string_lossy().to_string();
    let expected_uri = format!("file://{expected_path}");

    let mut seen = Vec::new();
    let create = prepare_commit(&mut server, &mut editor, &expected_uri, &mut seen, &uri);
    assert!(execute_action(&mut server, &mut editor, &expected_uri, &mut seen, create, 95)["error"].is_null());
    let deadline = Instant::now() + Duration::from_secs(15);
    while !seen.iter().any(|message| message["method"] == "window/showDocument" && message["params"]["uri"] == expected_uri) {
        assert!(Instant::now() < deadline, "showDocument timed out: seen {seen:#?}");
        pump(&mut server, &mut editor, &expected_uri, &mut seen);
    }
    assert!(
        !seen.iter().any(|message| message["method"] == "workspace/applyEdit" && draft_edit(message, &expected_uri)),
        "a closed buffer must not receive applyEdit: {seen:#?}"
    );
    let show = seen
        .iter()
        .find(|message| message["method"] == "window/showDocument" && message["params"]["uri"] == expected_uri)
        .unwrap();
    assert_eq!(show["params"], json!({"uri":expected_uri,"external":false,"takeFocus":true}));
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn manual_plain_commit_instructions_have_no_apply_edit() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo, caps(false));
    let expected_path = repo.path().join(".git/COMMIT_EDITMSG").to_string_lossy().to_string();
    let expected_uri = format!("file://{expected_path}");

    let mut seen = Vec::new();
    let create = prepare_commit(&mut server, &mut editor, &expected_uri, &mut seen, &uri);
    assert!(execute_action(&mut server, &mut editor, &expected_uri, &mut seen, create, 95)["error"].is_null());
    let deadline = Instant::now() + Duration::from_secs(15);
    while !seen.iter().any(|message| {
        message["method"] == "window/showMessage"
            && message["params"]["message"]
                .as_str()
                .is_some_and(|text| text.contains(&expected_path) && text.contains("Confirm") && text.contains("Abort"))
    }) {
        assert!(Instant::now() < deadline, "manual instructions timed out: seen {seen:#?}");
        pump(&mut server, &mut editor, &expected_uri, &mut seen);
    }
    assert!(
        !seen.iter().any(|message| message["method"] == "workspace/applyEdit" && draft_edit(message, &expected_uri)),
        "the manual path must not apply edits: {seen:#?}"
    );
    assert!(
        !seen.iter().any(|message| message["method"] == "window/showDocument" && message["params"]["uri"] == expected_uri),
        "the manual path must not request showDocument: {seen:#?}"
    );
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(Duration::from_secs(10));
    assert!(status.success());
}
