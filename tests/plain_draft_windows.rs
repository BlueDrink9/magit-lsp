//! Plain-mode commit drafts on Windows: the documented Unix-only refusal.
//!
//! `EditRegistry::create` is a platform stub on Windows (src/editor.rs:793), so
//! a commit-create action is answered with one `Cannot open commit draft:
//! managed commit drafts require Unix` showMessage and no draft frames follow
//! (src/server/view_execution.rs:1171-1175). This scenario pins that refusal —
//! request answered, message delivered, transport still serving — instead of
//! the suite-wide `recv_timeout` stall (tests/common/stdio.rs:223-262) that
//! tests/plain_draft.rs used to hit. Real Windows draft support belongs to the
//! deferred Windows phase, which replaces this expectation.
#![cfg(not(unix))]
#[path = "common/editor_model/mod.rs"]
mod editor_model;
#[path = "common/fake_editor.rs"]
mod fake_editor;
#[path = "common/mod.rs"]
mod fixtures;
use fixtures::stdio;

use fake_editor::{FakeEditor, Profile};
use magit_lsp::file_uri::uri_from_path;
use serde_json::{json, Value};
use std::path::Path;
use std::time::{Duration, Instant};

fn caps(show_document: bool) -> Value {
    let mut capabilities = json!({
        "workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},
        "textDocument":{"codeAction":{"disabledSupport":true}},
        "general":{"positionEncodings":["utf-8","utf-16","utf-32"]}
    });
    if show_document {
        capabilities["window"] = json!({"showDocument":{"support":true}});
    }
    capabilities
}

/// Open the repository's `.magit` status document with optional client identity.
fn open_plain_inner(
    repo: &fixtures::RepoFixture,
    capabilities: Value,
    extra_env: &[(&str, &str)],
    client_name: Option<&str>,
) -> (stdio::TestServer, FakeEditor, String) {
    let uri = uri_from_path(&repo.path().join(".magit"))
        .expect("native path has a file URI")
        .as_str()
        .to_owned();
    let mut server = stdio::TestServer::spawn(extra_env);
    let mut init = stdio::initialize_message();
    init["params"]["capabilities"] = capabilities;
    if let Some(client_name) = client_name {
        init["params"]["clientInfo"] = json!({"name":client_name});
    }
    init["params"]["workspaceFolders"] = json!([{"uri":uri_from_path(repo.path()).expect("native path has a file URI").as_str(),"name":"repo"}]);
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
    edit["documentChanges"].as_array().is_some_and(|changes| {
        changes
            .iter()
            .any(|change| change["textDocument"]["uri"] == draft_uri)
    }) || edit["changes"].get(draft_uri).is_some()
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

/// Wait for the status document to offer the commit menu.
///
/// Menu actions only exist once the server holds a retained render whose text
/// matches both the client's document and its own, and the menu render can be
/// delivered after the `executeCommand` response. Requesting actions for every
/// rendered line keeps the transport moving until that render is applied, so
/// the wait never blocks on a frame the server may not push unprompted.
fn wait_for_commit_menu(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    draft_uri: &str,
    seen: &mut Vec<Value>,
    uri: &str,
) -> Vec<Value> {
    let deadline = Instant::now() + Duration::from_secs(15);
    // Unique request IDs for this wait, clear of the fixed IDs used above.
    let mut id = 1000;
    loop {
        for line in 0..editor.text(uri).lines().count() as u32 {
            let offered = code_actions(server, editor, draft_uri, seen, uri, line, id);
            id += 1;
            let menu: Vec<Value> = offered
                .into_iter()
                .filter(|action| {
                    action["command"]["arguments"][0]["command"] == "menu.run-action"
                        && action["command"]["arguments"][0]["target"]["itemKey"].is_string()
                })
                .collect();
            if !menu.is_empty() {
                return menu;
            }
        }
        assert!(
            Instant::now() < deadline,
            "commit menu was not offered before the deadline; client version={:?}",
            editor.documents.get(uri).map(|doc| doc.version),
        );
    }
}

/// Poll code actions until `command` is offered for the `needle` row.
///
/// The status render that follows an `executeCommand` can reach the client
/// after its response, so one query may still see the previous document.
/// Polling keeps the transport moving until the synced document and the offered
/// actions agree, and fails on the deadline instead of looping forever.
fn wait_for_status_action(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    draft_uri: &str,
    seen: &mut Vec<Value>,
    uri: &str,
    needle: &str,
    command: &str,
    mut id: i64,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let line = editor
            .text(uri)
            .lines()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("status view shows {needle}: {}", editor.text(uri)))
            as u32;
        let offered = code_actions(server, editor, draft_uri, seen, uri, line, id);
        id += 1;
        if let Some(action) = offered
            .iter()
            .find(|action| action["command"]["arguments"][0]["command"] == command)
        {
            return action["command"]["arguments"][0].clone();
        }
        assert!(
            Instant::now() < deadline,
            "{command} was not offered for {needle} before the deadline: {offered:#?}"
        );
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
    let stage = wait_for_status_action(
        server,
        editor,
        draft_uri,
        seen,
        uri,
        "f.txt",
        "status.stage",
        90,
    );
    assert!(execute_action(server, editor, draft_uri, seen, stage, 91)["error"].is_null());

    let open_commit = wait_for_status_action(
        server,
        editor,
        draft_uri,
        seen,
        uri,
        "f.txt",
        "open.commit",
        92,
    );
    assert!(execute_action(server, editor, draft_uri, seen, open_commit, 93)["error"].is_null());

    wait_for_commit_menu(server, editor, draft_uri, seen, uri)
        .into_iter()
        .find(|action| {
            action["command"]["arguments"][0]["command"] == "menu.run-action"
                && action["command"]["arguments"][0]["target"]["itemKey"].is_string()
        })
        .expect("commit creation offered")["command"]["arguments"][0]
        .clone()
}

/// The commit-create action is refused with a readable message, not a stall.
///
/// Windows has no managed-draft boundary yet, so the create action must answer
/// error-free and deliver exactly one user-facing refusal while the server
/// keeps serving requests. The draft delivery frames that the Unix suite waits
/// for are deliberately absent, which is why that suite is Unix-gated.
#[test]
fn commit_create_reports_the_unix_only_refusal() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let (mut server, mut editor, uri) = open_plain_inner(&repo, caps(false), &[], None);
    let expected_path = repo
        .path()
        .join(".git/COMMIT_EDITMSG")
        .to_string_lossy()
        .to_string();
    let draft_uri = uri_from_path(Path::new(&expected_path))
        .expect("native path has a file URI")
        .as_str()
        .to_owned();
    let mut seen = Vec::new();
    let create = prepare_commit(&mut server, &mut editor, &draft_uri, &mut seen, &uri);

    assert!(
        execute_action(&mut server, &mut editor, &draft_uri, &mut seen, create, 95)["error"]
            .is_null(),
        "the refusal is a user-facing message, not a request error: {seen:#?}"
    );

    let deadline = Instant::now() + Duration::from_secs(15);
    while !seen.iter().any(|message| {
        message["method"] == "window/showMessage"
            && message["params"]["message"].as_str().is_some_and(|text| {
                text.contains("Cannot open commit draft") && text.contains("require Unix")
            })
    }) {
        assert!(
            Instant::now() < deadline,
            "the Unix-only refusal was not delivered: seen {seen:#?}"
        );
        pump(&mut server, &mut editor, &draft_uri, &mut seen);
    }

    assert!(
        !seen
            .iter()
            .any(|message| message["method"] == "workspace/applyEdit"
                && draft_edit(message, &draft_uri)),
        "no draft template may be delivered without a live draft run: {seen:#?}"
    );

    // The transport still serves: the refusal ends the run, not the session.
    let line = editor
        .text(&uri)
        .lines()
        .position(|line| line.contains("f.txt"))
        .expect("status view shows f.txt") as u32;
    let offered = code_actions(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &uri,
        line,
        96,
    );
    assert!(
        !offered.is_empty(),
        "the status view still offers actions after the refusal: {offered:#?}"
    );

    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(Duration::from_secs(10));
    assert!(status.success());
}
