//! Plain-mode commit draft delivery: replace an already-open buffer first.
// Managed commit drafts need the Unix helper/validated-fd boundary, so the
// whole suite builds only where that boundary exists. On Windows the editor
// registry's `EditRegistry::create` is the platform stub at src/editor.rs:793
// ("managed commit drafts require Unix"); `spawn_repository_operation` answers
// the create request with one `Cannot open commit draft: {error}` showMessage
// and returns None (src/server/view_execution.rs:1171-1175), so the draft runs
// that send the template applyEdit, showDocument and manual instructions
// (src/server/plain.rs:1300-1355) never start. Every scenario here waits for
// one of those frames, so all of them stalled in the bounded `recv_timeout`
// (crates/test-support/src/stdio.rs:223-262) instead of failing. Real Windows draft
// support belongs to the deferred Windows phase; tests/plain_draft_windows.rs
// pins the documented refusal in the meantime.
#![cfg(unix)]
use fixtures::stdio;
use magit_lsp_test_support as fixtures;
use magit_lsp_test_support::fake_editor;

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

/// Open the repository's `.magit` status document as a Plain client.
fn open_plain(
    repo: &fixtures::RepoFixture,
    capabilities: Value,
) -> (stdio::TestServer, FakeEditor, String) {
    open_plain_with_env(repo, capabilities, &[])
}

/// Open the repository's `.magit` status document with extra server environment.
fn open_plain_with_env(
    repo: &fixtures::RepoFixture,
    capabilities: Value,
    extra_env: &[(&str, &str)],
) -> (stdio::TestServer, FakeEditor, String) {
    open_plain_inner(repo, capabilities, extra_env, None)
}

/// Open the repository's `.magit` status document identifying as `client_name`.
fn open_plain_as(
    repo: &fixtures::RepoFixture,
    capabilities: Value,
    client_name: &str,
) -> (stdio::TestServer, FakeEditor, String) {
    open_plain_inner(repo, capabilities, &[], Some(client_name))
}

/// Open the repository's `.magit` status document with optional client identity.
fn open_plain_inner(
    repo: &fixtures::RepoFixture,
    capabilities: Value,
    extra_env: &[(&str, &str)],
    client_name: Option<&str>,
) -> (stdio::TestServer, FakeEditor, String) {
    magit_lsp_test_support::cargo_server_binary!();
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

/// Receive one frame, rejecting the draft template edit instead of applying it.
fn pump_rejecting_draft(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    draft_uri: &str,
    seen: &mut Vec<Value>,
) -> Value {
    let message = server.recv();
    seen.push(message.clone());
    if message["method"] == "workspace/applyEdit" && draft_edit(&message, draft_uri) {
        server.send(json!({"jsonrpc":"2.0","id":message["id"],"result":{"applied":false}}));
    } else if message["method"].is_string() && !message["id"].is_null() {
        for frame in editor.dispatch(&message) {
            server.send(frame);
        }
    }
    message
}

/// Receive frames until the draft template edit arrives, leaving it unanswered.
///
/// The held request lets a test deliver its outcome after the client has
/// already acted on the template, or after a replacement delivery.
fn hold_draft_edit(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    draft_uri: &str,
    seen: &mut Vec<Value>,
) -> Value {
    loop {
        let message = server.recv();
        seen.push(message.clone());
        if message["method"] == "workspace/applyEdit" && draft_edit(&message, draft_uri) {
            return message;
        }
        if message["method"].is_string() && !message["id"].is_null() {
            for frame in editor.dispatch(&message) {
                server.send(frame);
            }
        }
    }
}

/// Answer a held draft template edit with the named delivery outcome.
fn fail_delivery(server: &mut stdio::TestServer, held: &Value, outcome: &str) {
    let response = match outcome {
        "rejection" => {
            json!({"jsonrpc":"2.0","id":held["id"],"result":{"applied":false,"failureReason":"unsupported template"}})
        }
        "error" => {
            json!({"jsonrpc":"2.0","id":held["id"],"error":{"code":-32603,"message":"template delivery failed"}})
        }
        "timeout" => return,
        other => panic!("unknown delivery outcome {other}"),
    };
    server.send(response);
}

/// Whether a readable draft-delivery failure was reported to the client.
fn reported_delivery_failure(seen: &[Value]) -> bool {
    seen.iter().any(|message| {
        message["method"] == "window/showMessage"
            && message["params"]["message"]
                .as_str()
                .is_some_and(|text| text.contains("could not be applied"))
    })
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

/// Drain frames until the offered Confirm carries the client's draft version.
fn wait_for_draft_confirm(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    draft_uri: &str,
    seen: &mut Vec<Value>,
    mut id: i64,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let offered = code_actions(server, editor, draft_uri, seen, draft_uri, 0, id);
        let client = editor
            .documents
            .get(draft_uri)
            .expect("open draft document")
            .version;
        let confirm = offered.iter().find_map(|action| {
            let args = &action["command"]["arguments"][0];
            (args["command"] == "common.confirm").then(|| args.clone())
        });
        if let Some(confirm) = confirm {
            if confirm["version"].as_i64() == Some(client) {
                return confirm;
            }
        }
        assert!(
            Instant::now() < deadline,
            "draft confirm did not catch up with the client: {offered:#?}"
        );
        id += 1;
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

/// Start a commit run, wait for its showDocument and attach the draft document.
fn start_open_draft(
    repo: &fixtures::RepoFixture,
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    status_uri: &str,
    seen: &mut Vec<Value>,
) -> String {
    let expected_path = repo
        .path()
        .join(".git/COMMIT_EDITMSG")
        .to_string_lossy()
        .to_string();
    let draft_uri = uri_from_path(Path::new(&expected_path))
        .expect("native path has a file URI")
        .as_str()
        .to_owned();
    let create = prepare_commit(server, editor, &draft_uri, seen, status_uri);
    assert!(execute_action(server, editor, &draft_uri, seen, create, 95)["error"].is_null());
    let deadline = Instant::now() + Duration::from_secs(15);
    while !seen.iter().any(|message| {
        message["method"] == "window/showDocument" && message["params"]["uri"] == draft_uri
    }) {
        assert!(
            Instant::now() < deadline,
            "draft delivery timed out: seen {seen:#?}"
        );
        pump(server, editor, &draft_uri, seen);
    }
    let template = std::fs::read_to_string(&expected_path).expect("commit template");
    server.send(editor.open(&draft_uri, &template));
    draft_uri
}

/// Return the semantic arguments of the offered draft action for a command.
fn draft_action(actions: &[Value], command: &str) -> Value {
    actions
        .iter()
        .find(|action| action["command"]["arguments"][0]["command"] == command)
        .unwrap_or_else(|| panic!("{command} offered: {actions:#?}"))["command"]["arguments"][0]
        .clone()
}

/// Drain frames until the status view replaces the pending draft path.
fn wait_for_status_view(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    draft_uri: &str,
    seen: &mut Vec<Value>,
    status_uri: &str,
    expected_path: &str,
) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while editor.text(status_uri).contains(expected_path) {
        assert!(
            Instant::now() < deadline,
            "status view timed out: seen {seen:#?}"
        );
        pump(server, editor, draft_uri, seen);
    }
}

fn wait_for_draft_messages(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    draft_uri: &str,
    seen: &mut Vec<Value>,
) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !seen
        .iter()
        .any(|message| message["method"] == "workspace/applyEdit" && draft_edit(message, draft_uri))
        || !seen.iter().any(|message| {
            message["method"] == "window/showDocument" && message["params"]["uri"] == draft_uri
        })
    {
        assert!(
            Instant::now() < deadline,
            "draft delivery timed out: seen {seen:#?}"
        );
        pump(server, editor, draft_uri, seen);
    }
}

/// Poll Git until `HEAD` carries exactly `message`, failing after the deadline.
fn wait_for_head_message(repo: &fixtures::RepoFixture, message: &str) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let last = repo.git(&["log", "-1", "--format=%B"]);
        if last.trim() == message {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "no commit with {message:?}; last message: {last:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Poll Git until `HEAD` moves away from `before`, failing after the deadline.
///
/// The server confirms a draft before the helper-driven Git commit runs, so a
/// status render or an `executeCommand` response is not proof that the commit
/// exists yet.
fn wait_for_head_change(repo: &fixtures::RepoFixture, before: &str) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let head = repo.git(&["rev-parse", "HEAD"]);
        if head != before {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "HEAD did not move past {before:?}; last: {head:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn open_plain_commit_buffer_is_replaced_before_show_document() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo, caps(true));
    let expected_path = repo
        .path()
        .join(".git/COMMIT_EDITMSG")
        .to_string_lossy()
        .to_string();
    let expected_uri = uri_from_path(Path::new(&expected_path))
        .expect("native path has a file URI")
        .as_str()
        .to_owned();
    let stale = "Previous commit message\n";
    // The client opened this buffer before the new run issued its template.
    server.send(editor.open(&expected_uri, stale));

    let mut seen = Vec::new();
    let create = prepare_commit(&mut server, &mut editor, &expected_uri, &mut seen, &uri);
    assert!(execute_action(
        &mut server,
        &mut editor,
        &expected_uri,
        &mut seen,
        create,
        95
    )["error"]
        .is_null());
    wait_for_draft_messages(&mut server, &mut editor, &expected_uri, &mut seen);

    let apply_index = seen
        .iter()
        .position(|message| {
            message["method"] == "workspace/applyEdit" && draft_edit(message, &expected_uri)
        })
        .expect("open buffer receives the new template");
    let show_index = seen
        .iter()
        .position(|message| {
            message["method"] == "window/showDocument" && message["params"]["uri"] == expected_uri
        })
        .expect("the draft is shown");
    assert!(
        apply_index < show_index,
        "applyEdit must precede showDocument: {seen:#?}"
    );

    let apply = &seen[apply_index];
    let change = &apply["params"]["edit"]["documentChanges"][0];
    assert_eq!(change["textDocument"]["uri"], expected_uri);
    assert_eq!(
        change["textDocument"]["version"], 1,
        "the store's current version: {apply}"
    );
    let edits = change["edits"].as_array().unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(
        edits[0]["range"],
        json!({"start":{"line":0,"character":0},"end":{"line":1,"character":0}})
    );
    let template = fixtures::read(std::path::Path::new(&expected_path));
    assert_eq!(edits[0]["newText"], template.as_str());
    assert_ne!(edits[0]["newText"], stale);
    assert_eq!(
        editor.text(&expected_uri),
        template,
        "the client applied the replacement"
    );

    let show = &seen[show_index];
    assert_eq!(
        show["params"],
        json!({"uri":expected_uri,"external":false,"takeFocus":true})
    );
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn rejected_draft_template_edit_is_reported_and_cancels_the_run() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let head = repo.git(&["rev-parse", "HEAD"]);
    let (mut server, mut editor, uri) = open_plain(&repo, caps(true));
    let expected_path = repo
        .path()
        .join(".git/COMMIT_EDITMSG")
        .to_string_lossy()
        .to_string();
    let draft_uri = uri_from_path(Path::new(&expected_path))
        .expect("native path has a file URI")
        .as_str()
        .to_owned();
    let stale = "Previous commit message\n";
    // The client opened this buffer before the new run issued its template.
    server.send(editor.open(&draft_uri, stale));

    let mut seen = Vec::new();
    let create = prepare_commit(&mut server, &mut editor, &draft_uri, &mut seen, &uri);
    server.send(json!({"jsonrpc":"2.0","id":95,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[create]}}));

    // The client refuses the replacement, so no template ever reaches the buffer.
    let deadline = Instant::now() + Duration::from_secs(15);
    while !seen.iter().any(|message| {
        message["method"] == "window/showMessage"
            && message["params"]["message"]
                .as_str()
                .is_some_and(|text| text.contains("could not be applied"))
    }) {
        assert!(
            Instant::now() < deadline,
            "rejection report timed out: seen {seen:#?}"
        );
        pump_rejecting_draft(&mut server, &mut editor, &draft_uri, &mut seen);
    }
    assert!(
        seen.iter()
            .any(|message| message["method"] == "workspace/applyEdit"
                && draft_edit(message, &draft_uri)),
        "the rejected template edit was offered: {seen:#?}"
    );
    assert_eq!(
        editor.text(&draft_uri),
        stale,
        "the rejection leaves the buffer as it was"
    );
    assert_eq!(
        repo.git(&["rev-parse", "HEAD"]),
        head,
        "a rejected draft must not commit"
    );

    // The canceled run leaves no pending draft to confirm or abort.
    let offered = code_actions(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &draft_uri,
        0,
        96,
    );
    assert!(
        offered.iter().all(|action| {
            let command = action["command"]["arguments"][0]["command"].as_str();
            command != Some("common.confirm") && command != Some("common.cancel")
        }),
        "a canceled draft offers no Confirm/Abort: {offered:#?}"
    );

    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn stale_explicit_confirm_is_refused_and_keeps_the_draft_pending() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let head = repo.git(&["rev-parse", "HEAD"]);
    let (mut server, mut editor, uri) = open_plain(&repo, caps(true));
    let mut seen = Vec::new();
    let draft_uri = start_open_draft(&repo, &mut server, &mut editor, &uri, &mut seen);
    server.send(editor.edit(&draft_uri, "Uncommitted body\n"));

    let offered = code_actions(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &draft_uri,
        0,
        200,
    );
    let confirm = draft_action(&offered, "common.confirm");
    let current = confirm["version"].as_i64().expect("commit version");
    let mut stale = confirm.clone();
    stale["version"] = json!(current - 1);
    let refused = execute_action(&mut server, &mut editor, &draft_uri, &mut seen, stale, 201);
    assert_eq!(
        refused["error"]["message"],
        "Wait for the exact synchronized document version; full-text repair may be required",
    );

    assert_eq!(
        repo.git(&["rev-parse", "HEAD"]),
        head,
        "a stale confirm must not commit"
    );
    let still = code_actions(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &draft_uri,
        0,
        202,
    );
    draft_action(&still, "common.confirm");
    assert_eq!(
        editor.text(&draft_uri),
        "Uncommitted body\n",
        "the draft stays pending"
    );
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn explicit_abort_creates_no_commit_and_restores_the_status_view() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let expected_path = repo
        .path()
        .join(".git/COMMIT_EDITMSG")
        .to_string_lossy()
        .to_string();
    let head = repo.git(&["rev-parse", "HEAD"]);
    let (mut server, mut editor, uri) = open_plain(&repo, caps(true));
    let mut seen = Vec::new();
    let draft_uri = start_open_draft(&repo, &mut server, &mut editor, &uri, &mut seen);
    server.send(editor.edit(&draft_uri, "Aborted body\n"));

    let offered = code_actions(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &draft_uri,
        0,
        210,
    );
    let abort = offered
        .iter()
        .find(|action| action["title"] == "Abort commit")
        .expect("attached draft offers Abort")["command"]["arguments"][0]
        .clone();
    assert_eq!(abort["command"], "common.cancel");
    let aborted = execute_action(&mut server, &mut editor, &draft_uri, &mut seen, abort, 211);
    assert!(aborted["error"].is_null(), "Abort failed: {aborted}");
    assert_eq!(aborted["result"]["type"], "aborted");
    assert_eq!(
        repo.git(&["rev-parse", "HEAD"]),
        head,
        "Abort must not commit"
    );
    assert!(
        repo.git(&["status", "--porcelain"]).contains("f.txt"),
        "Abort keeps staged work"
    );

    wait_for_status_view(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &uri,
        &expected_path,
    );
    assert!(
        editor.text(&uri).contains("f.txt"),
        "the status view is shown again"
    );
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn effective_plain_mutations_are_refused_during_registered_commit_wait() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n"), ("g.txt", "unchanged\n")])
        .modify_standard()
        .modify("g.txt", "changed\n")
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo, caps(true));
    let mut seen = Vec::new();
    let draft_uri = start_open_draft(&repo, &mut server, &mut editor, &uri, &mut seen);
    let head = repo.git(&["rev-parse", "HEAD"]);
    let index = repo.git(&["write-tree"]);
    let worktree = repo.git(&["status", "--porcelain=v1"]);
    let diff = repo.git(&["diff", "--binary"]);

    // Opening the draft queued a status render. The first query drains its
    // applyEdit/echo; the second obtains the acknowledged snapshot, not a retry.
    let _ = wait_for_status_action(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &uri,
        "g.txt",
        "status.discard",
        205,
    );
    let discard = wait_for_status_action(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &uri,
        "g.txt",
        "status.discard",
        207,
    );
    let refused = execute_action(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        discard,
        206,
    );
    assert!(
        refused["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == "message"
                && effect["text"] == "Wait for the current Git operation to finish"),
        "{refused}"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(repo.git(&["write-tree"]), index);
    assert_eq!(repo.git(&["status", "--porcelain=v1"]), worktree);
    assert_eq!(repo.git(&["diff", "--binary"]), diff);
    assert!(!repo.path().join(".git/rebase-merge").exists());
    assert!(!repo.path().join(".git/rebase-apply").exists());

    let _ = wait_for_status_action(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &uri,
        "g.txt",
        "status.stage",
        210,
    );
    let stage = wait_for_status_action(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &uri,
        "g.txt",
        "status.stage",
        220,
    );
    let refused = execute_action(&mut server, &mut editor, &draft_uri, &mut seen, stage, 230);
    assert!(
        refused["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == "message"
                && effect["text"] == "Wait for the current Git operation to finish"),
        "{refused}"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(repo.git(&["write-tree"]), index);
    assert_eq!(repo.git(&["status", "--porcelain=v1"]), worktree);

    let open = wait_for_status_action(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &uri,
        "g.txt",
        "open.commit",
        231,
    );
    let opened = execute_action(&mut server, &mut editor, &draft_uri, &mut seen, open, 232);
    assert!(opened["error"].is_null(), "{opened}");
    assert!(
        opened["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == "pushView"
                && effect["descriptor"]["scope"] == "menu.commit"),
        "{opened}"
    );
    // The first query drains the menu edit/echo; the second observes its acknowledged snapshot.
    let _ = code_actions(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &uri,
        0,
        233,
    );
    let offered = code_actions(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &uri,
        0,
        234,
    );
    let create = offered
        .iter()
        .find(|action| action["command"]["arguments"][0]["target"]["itemKey"] == "c")
        .expect("fresh commit creation menu action")["command"]["arguments"][0]
        .clone();
    let refused = execute_action(&mut server, &mut editor, &draft_uri, &mut seen, create, 235);
    assert!(
        refused["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == "message"
                && effect["text"] == "Wait for the current Git operation to finish"),
        "{refused}"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(repo.git(&["write-tree"]), index);
    assert_eq!(repo.git(&["diff", "--binary"]), diff);
    assert_eq!(repo.git(&["status", "--porcelain=v1"]), worktree);
    assert!(!repo.path().join(".git/rebase-merge").exists());
    assert!(!repo.path().join(".git/rebase-apply").exists());

    let offered = code_actions(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &uri,
        0,
        236,
    );
    let cancel = draft_action(&offered, "common.cancel");
    let cancelled = execute_action(&mut server, &mut editor, &draft_uri, &mut seen, cancel, 237);
    assert!(cancelled["error"].is_null(), "{cancelled}");
    assert!(
        cancelled["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == "popView"),
        "{cancelled}"
    );

    let offered = code_actions(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &draft_uri,
        0,
        240,
    );
    let abort = draft_action(&offered, "common.cancel");
    assert!(
        execute_action(&mut server, &mut editor, &draft_uri, &mut seen, abort, 250)["error"]
            .is_null()
    );
    let expected_path = repo
        .path()
        .join(".git/COMMIT_EDITMSG")
        .to_string_lossy()
        .to_string();
    wait_for_status_view(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &uri,
        &expected_path,
    );
    server.send(editor.close(&draft_uri));
    let discard = wait_for_status_action(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &uri,
        "g.txt",
        "status.discard",
        260,
    );
    let result = execute_action(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        discard,
        270,
    );
    assert!(result["error"].is_null(), "{result}");
    let _ = code_actions(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &uri,
        0,
        271,
    );
    // Text locates only the cursor; the offered opaque target owns identity.
    let yes = editor
        .text(&uri)
        .lines()
        .position(|line| line.trim() == "Yes")
        .expect("discard confirmation") as u32;
    let offered = code_actions(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &uri,
        yes,
        272,
    );
    let answer = draft_action(&offered, "prompt.answer");
    assert!(
        execute_action(&mut server, &mut editor, &draft_uri, &mut seen, answer, 273)["error"]
            .is_null()
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("g.txt")).unwrap(),
        "unchanged\n"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(repo.git(&["write-tree"]), index);

    let open = wait_for_status_action(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &uri,
        "f.txt",
        "open.commit",
        280,
    );
    assert!(
        execute_action(&mut server, &mut editor, &draft_uri, &mut seen, open, 281)["error"]
            .is_null()
    );
    let create = wait_for_commit_menu(&mut server, &mut editor, &draft_uri, &mut seen, &uri)
        .into_iter()
        .find(|action| action["command"]["arguments"][0]["target"]["itemKey"] == "c")
        .expect("same commit creation after release")["command"]["arguments"][0]
        .clone();
    let start = seen.len();
    assert!(
        execute_action(&mut server, &mut editor, &draft_uri, &mut seen, create, 282)["error"]
            .is_null()
    );
    while !seen[start..].iter().any(|message| {
        message["method"] == "window/showDocument" && message["params"]["uri"] == draft_uri
    }) {
        pump(&mut server, &mut editor, &draft_uri, &mut seen);
    }
    let template = std::fs::read_to_string(&expected_path).expect("second commit template");
    server.send(editor.open(&draft_uri, &template));
    server.send(editor.edit(&draft_uri, "Commit after deferred ownership release\n"));
    let confirm = wait_for_draft_confirm(&mut server, &mut editor, &draft_uri, &mut seen, 300);
    assert!(execute_action(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        confirm,
        301
    )["error"]
        .is_null());
    wait_for_head_change(&repo, &head);
    assert_eq!(
        repo.git(&["log", "-1", "--format=%B"]),
        "Commit after deferred ownership release\n\n"
    );
    assert_eq!(
        repo.git(&["show", "HEAD:f.txt"]),
        fixtures::standard_changes()
    );
    assert_eq!(repo.git(&["status", "--porcelain=v1"]), "");
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn close_after_abort_is_ignored() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let head = repo.git(&["rev-parse", "HEAD"]);
    let (mut server, mut editor, uri) = open_plain(&repo, caps(true));
    let mut seen = Vec::new();
    let draft_uri = start_open_draft(&repo, &mut server, &mut editor, &uri, &mut seen);

    let offered = code_actions(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &draft_uri,
        0,
        220,
    );
    let abort = offered
        .iter()
        .find(|action| action["title"] == "Abort commit")
        .expect("attached draft offers Abort")["command"]["arguments"][0]
        .clone();
    assert!(
        execute_action(&mut server, &mut editor, &draft_uri, &mut seen, abort, 221)["error"]
            .is_null()
    );

    let before_close = seen.len();
    server.send(editor.close(&draft_uri));
    // The server stays responsive and the late close changes nothing.
    let after = code_actions(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &uri,
        0,
        222,
    );
    assert!(after
        .iter()
        .all(|action| { action["command"]["arguments"][0]["command"] != "common.confirm" }));
    assert_eq!(
        repo.git(&["rev-parse", "HEAD"]),
        head,
        "a close after abort must not commit"
    );
    assert!(
        !seen[before_close..].iter().any(|message| {
            message["method"] == "window/showMessage"
                && message["params"]["message"]
                    .as_str()
                    .is_some_and(|text| text.contains("Commit draft"))
        }),
        "a close after abort must not report an error: {:#?}",
        &seen[before_close..],
    );
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn saved_file_close_commits_the_disk_text() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let path = magit_lsp::git::GitRepo::open(repo.path())
        .expect("open repository")
        .commit_message_path()
        .expect("commit message path");
    let head = repo.git(&["rev-parse", "HEAD"]);
    let (mut server, mut editor, uri) = open_plain(&repo, caps(true));
    let mut seen = Vec::new();
    let draft_uri = start_open_draft(&repo, &mut server, &mut editor, &uri, &mut seen);

    // The client writes the buffer to disk, reports the save, then closes.
    let saved = "Saved disk body\n";
    server.send(editor.edit(&draft_uri, saved));
    std::fs::write(&path, saved).expect("write saved commit message");
    server.send(editor.save(&draft_uri));
    server.send(editor.close(&draft_uri));

    wait_for_head_message(&repo, "Saved disk body");
    assert_ne!(
        repo.git(&["rev-parse", "HEAD"]),
        head,
        "the close must commit"
    );
    assert_eq!(
        repo.git(&["log", "-1", "--format=%B"]).trim(),
        "Saved disk body",
        "the commit message is the saved disk text"
    );
    assert_eq!(
        repo.git(&["show", "HEAD:f.txt"]),
        fixtures::standard_changes(),
        "the staged change is committed"
    );
    assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]), "2\n");
    assert_eq!(repo.git(&["status", "--porcelain"]), "");

    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn empty_saved_message_is_refused_by_git_and_reported() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let path = magit_lsp::git::GitRepo::open(repo.path())
        .expect("open repository")
        .commit_message_path()
        .expect("commit message path");
    let head = repo.git(&["rev-parse", "HEAD"]);
    let (mut server, mut editor, uri) = open_plain(&repo, caps(true));
    let mut seen = Vec::new();
    let draft_uri = start_open_draft(&repo, &mut server, &mut editor, &uri, &mut seen);

    // The client saves and closes an empty message, so Git itself refuses the
    // commit while the server keeps the staged change.
    server.send(editor.edit(&draft_uri, ""));
    std::fs::write(&path, "").expect("write empty commit message");
    server.send(editor.save(&draft_uri));
    server.send(editor.close(&draft_uri));

    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if seen.iter().any(|message| {
            message["method"] == "window/showMessage"
                && message["params"]["type"] == 3
                && message["params"]["message"]
                    .as_str()
                    .is_some_and(|text| text.contains("empty message"))
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "no empty-message report: {seen:#?}"
        );
        pump(&mut server, &mut editor, &draft_uri, &mut seen);
    }

    assert_eq!(
        repo.git(&["rev-parse", "HEAD"]),
        head,
        "an empty message must not commit"
    );
    assert_eq!(
        repo.git(&["rev-list", "--count", "HEAD"]),
        "1\n",
        "no commit is created"
    );
    assert!(
        repo.git(&["status", "--porcelain"]).contains("f.txt"),
        "the staged change survives the refusal"
    );

    // The server stays responsive after the refused commit.
    let _ = code_actions(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &uri,
        0,
        500,
    );
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn dirty_close_commits_the_prior_saved_disk_text() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let path = magit_lsp::git::GitRepo::open(repo.path())
        .expect("open repository")
        .commit_message_path()
        .expect("commit message path");
    let head = repo.git(&["rev-parse", "HEAD"]);
    let (mut server, mut editor, uri) = open_plain(&repo, caps(true));
    let mut seen = Vec::new();
    let draft_uri = start_open_draft(&repo, &mut server, &mut editor, &uri, &mut seen);

    // A saved checkpoint reaches disk, then the buffer becomes dirty again.
    let saved = "Prior saved body\n";
    let dirty = "Unsaved dirty body\n";
    server.send(editor.edit(&draft_uri, saved));
    std::fs::write(&path, saved).expect("write saved commit message");
    server.send(editor.save(&draft_uri));
    server.send(editor.edit(&draft_uri, dirty));
    assert_eq!(editor.text(&draft_uri), dirty, "the buffer is dirty");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        saved,
        "the file keeps the saved text"
    );
    server.send(editor.close(&draft_uri));

    wait_for_head_message(&repo, "Prior saved body");
    assert_ne!(
        repo.git(&["rev-parse", "HEAD"]),
        head,
        "the close must commit"
    );
    let message = repo.git(&["log", "-1", "--format=%B"]);
    assert_eq!(
        message.trim(),
        "Prior saved body",
        "the prior saved text is committed"
    );
    assert!(
        !message.contains("Unsaved dirty body"),
        "the dirty buffer must not be committed: {message:?}"
    );
    assert_eq!(
        repo.git(&["show", "HEAD:f.txt"]),
        fixtures::standard_changes(),
        "the staged change is committed"
    );
    assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]), "2\n");
    assert_eq!(repo.git(&["status", "--porcelain"]), "");

    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn dirty_save_creates_no_commit_and_keeps_the_draft_pending() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let path = magit_lsp::git::GitRepo::open(repo.path())
        .expect("open repository")
        .commit_message_path()
        .expect("commit message path");
    let head = repo.git(&["rev-parse", "HEAD"]);
    let (mut server, mut editor, uri) = open_plain(&repo, caps(true));
    let mut seen = Vec::new();
    let draft_uri = start_open_draft(&repo, &mut server, &mut editor, &uri, &mut seen);

    // The live text differs from the saved snapshot until the native save
    // writes it to disk and reports didSave.
    let dirty = "Dirty saved body\n";
    server.send(editor.edit(&draft_uri, dirty));
    std::fs::write(&path, dirty).expect("write saved commit message");
    server.send(editor.save(&draft_uri));

    // A save checkpoints only: it never commits and never ends the run.
    assert_eq!(
        repo.git(&["rev-parse", "HEAD"]),
        head,
        "a save must not commit"
    );
    assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]), "1\n");
    let offered = code_actions(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &draft_uri,
        0,
        400,
    );
    draft_action(&offered, "common.confirm");
    assert_eq!(
        editor.text(&draft_uri),
        dirty,
        "the saved draft stays editable"
    );

    // Only a later close commits, and it commits the saved text.
    server.send(editor.close(&draft_uri));
    wait_for_head_message(&repo, "Dirty saved body");
    assert_ne!(
        repo.git(&["rev-parse", "HEAD"]),
        head,
        "the close commits the saved text"
    );
    assert_eq!(
        repo.git(&["log", "-1", "--format=%B"]).trim(),
        "Dirty saved body"
    );
    assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]), "2\n");
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn dirty_cancel_keeps_the_draft_pending_and_the_editor_open() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let expected_path = magit_lsp::git::GitRepo::open(repo.path())
        .expect("open repository")
        .commit_message_path()
        .expect("commit message path")
        .to_string_lossy()
        .to_string();
    let head = repo.git(&["rev-parse", "HEAD"]);
    // VSCode confirms only its own closed-tab event, so its didClose is
    // detach-only and never commits.
    let (mut server, mut editor, uri) = open_plain_as(&repo, caps(true), "Visual Studio Code");
    let mut seen = Vec::new();
    let draft_uri = start_open_draft(&repo, &mut server, &mut editor, &uri, &mut seen);

    let dirty = "Cancelled close body\n";
    server.send(editor.edit(&draft_uri, dirty));

    // The dirty-close dialog is cancelled, so the editor and run are retained
    // and Confirm is still offered without a commit.
    let offered = code_actions(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &draft_uri,
        0,
        410,
    );
    draft_action(&offered, "common.confirm");
    assert_eq!(
        editor.text(&draft_uri),
        dirty,
        "the dirty buffer stays open"
    );
    assert!(
        editor.text(&uri).contains("f.txt"),
        "the status view is still shown: {}",
        editor.text(&uri)
    );
    assert_eq!(
        repo.git(&["rev-parse", "HEAD"]),
        head,
        "a cancelled close must not commit"
    );
    assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]), "1\n");

    // The non-confirming close only detaches the transport: no commit, and the
    // retained run accepts the exact live text again.
    let version = editor
        .documents
        .get(&draft_uri)
        .expect("open draft document")
        .version;
    server.send(editor.close(&draft_uri));
    assert_eq!(
        repo.git(&["rev-parse", "HEAD"]),
        head,
        "a non-confirming close must not commit"
    );
    assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]), "1\n");
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":draft_uri,"languageId":"plaintext","version":version,"text":dirty}}}));
    editor.documents.insert(
        draft_uri.clone(),
        fake_editor::Document {
            text: dirty.into(),
            version,
        },
    );
    let offered = code_actions(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &draft_uri,
        0,
        411,
    );
    draft_action(&offered, "common.confirm");
    assert_eq!(
        editor.text(&draft_uri),
        dirty,
        "the pending run keeps the buffer open"
    );
    assert!(
        !std::path::Path::new(&expected_path).exists()
            || std::fs::read_to_string(&expected_path).unwrap() != dirty,
        "a cancelled close must not write the draft"
    );

    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn unrelated_document_notifications_are_rejected_without_touching_the_draft() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let expected_path = repo
        .path()
        .join(".git/COMMIT_EDITMSG")
        .to_string_lossy()
        .to_string();
    let head = repo.git(&["rev-parse", "HEAD"]);
    let (mut server, mut editor, uri) = open_plain(&repo, caps(true));
    let mut seen = Vec::new();
    let draft_uri = start_open_draft(&repo, &mut server, &mut editor, &uri, &mut seen);
    let edited = "Kept body\n";
    server.send(editor.edit(&draft_uri, edited));

    let unrelated = uri_from_path(&repo.path().join("unrelated.txt"))
        .expect("native path has a file URI")
        .as_str()
        .to_owned();
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":unrelated,"languageId":"plaintext","version":1,"text":"unrelated\n"}}}));
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":unrelated,"version":2},"contentChanges":[{"text":"changed\n"}]}}));
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":unrelated}}}));

    let confirm = wait_for_draft_confirm(&mut server, &mut editor, &draft_uri, &mut seen, 230);
    let confirmed = execute_action(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        confirm,
        240,
    );
    assert!(
        confirmed["error"].is_null(),
        "draft confirm failed: {confirmed}"
    );

    wait_for_status_view(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &uri,
        &expected_path,
    );
    wait_for_head_change(&repo, &head);
    assert_eq!(
        repo.git(&["log", "-1", "--format=%B"]),
        format!("{edited}\n")
    );
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
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
    let expected_path = repo
        .path()
        .join(".git/COMMIT_EDITMSG")
        .to_string_lossy()
        .to_string();
    let expected_uri = uri_from_path(Path::new(&expected_path))
        .expect("native path has a file URI")
        .as_str()
        .to_owned();

    let mut seen = Vec::new();
    let create = prepare_commit(&mut server, &mut editor, &expected_uri, &mut seen, &uri);
    assert!(execute_action(
        &mut server,
        &mut editor,
        &expected_uri,
        &mut seen,
        create,
        95
    )["error"]
        .is_null());
    let deadline = Instant::now() + Duration::from_secs(15);
    while !seen.iter().any(|message| {
        message["method"] == "window/showDocument" && message["params"]["uri"] == expected_uri
    }) {
        assert!(
            Instant::now() < deadline,
            "showDocument timed out: seen {seen:#?}"
        );
        pump(&mut server, &mut editor, &expected_uri, &mut seen);
    }
    assert!(
        !seen
            .iter()
            .any(|message| message["method"] == "workspace/applyEdit"
                && draft_edit(message, &expected_uri)),
        "a closed buffer must not receive applyEdit: {seen:#?}"
    );
    let show = seen
        .iter()
        .find(|message| {
            message["method"] == "window/showDocument" && message["params"]["uri"] == expected_uri
        })
        .unwrap();
    assert_eq!(
        show["params"],
        json!({"uri":expected_uri,"external":false,"takeFocus":true})
    );
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
    let expected_path = repo
        .path()
        .join(".git/COMMIT_EDITMSG")
        .to_string_lossy()
        .to_string();
    let expected_uri = uri_from_path(Path::new(&expected_path))
        .expect("native path has a file URI")
        .as_str()
        .to_owned();

    let mut seen = Vec::new();
    let create = prepare_commit(&mut server, &mut editor, &expected_uri, &mut seen, &uri);
    assert!(execute_action(
        &mut server,
        &mut editor,
        &expected_uri,
        &mut seen,
        create,
        95
    )["error"]
        .is_null());
    let deadline = Instant::now() + Duration::from_secs(15);
    while !seen.iter().any(|message| {
        message["method"] == "window/showMessage"
            && message["params"]["message"].as_str().is_some_and(|text| {
                text.contains(&expected_path) && text.contains("Confirm") && text.contains("Abort")
            })
    }) {
        assert!(
            Instant::now() < deadline,
            "manual instructions timed out: seen {seen:#?}"
        );
        pump(&mut server, &mut editor, &expected_uri, &mut seen);
    }
    assert!(
        !seen
            .iter()
            .any(|message| message["method"] == "workspace/applyEdit"
                && draft_edit(message, &expected_uri)),
        "the manual path must not apply edits: {seen:#?}"
    );
    assert!(
        !seen
            .iter()
            .any(|message| message["method"] == "window/showDocument"
                && message["params"]["uri"] == expected_uri),
        "the manual path must not request showDocument: {seen:#?}"
    );
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn manual_preopened_draft_attaches_on_exact_template_echo() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let head = repo.git(&["rev-parse", "HEAD"]);
    let (mut server, mut editor, uri) = open_plain_with_env(
        &repo,
        caps(false),
        &[("MAGIT_LSP_DRAFT_OPEN_TIMEOUT_MS", "800")],
    );
    let draft_uri = uri_from_path(&repo.path().join(".git/COMMIT_EDITMSG"))
        .expect("native path has a file URI")
        .as_str()
        .to_owned();
    server.send(editor.open(&draft_uri, "Previous message\n"));
    let mut seen = Vec::new();
    let create = prepare_commit(&mut server, &mut editor, &draft_uri, &mut seen, &uri);
    assert!(
        execute_action(&mut server, &mut editor, &draft_uri, &mut seen, create, 95)["error"]
            .is_null()
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !seen
        .iter()
        .any(|m| m["method"] == "workspace/applyEdit" && draft_edit(m, &draft_uri))
    {
        assert!(
            Instant::now() < deadline,
            "preopened manual draft needs template delivery: {seen:#?}"
        );
        // Querying ensures bounded progress even when no template is delivered.
        code_actions(
            &mut server,
            &mut editor,
            &draft_uri,
            &mut seen,
            &draft_uri,
            0,
            96,
        );
    }
    let template = editor.text(&draft_uri).to_owned();
    server.send(editor.edit(&draft_uri, &template));
    std::thread::sleep(Duration::from_millis(1100));
    server.send(editor.edit(&draft_uri, "Preopened manual commit\n"));
    let confirm = wait_for_draft_confirm(&mut server, &mut editor, &draft_uri, &mut seen, 1100);
    assert!(
        execute_action(&mut server, &mut editor, &draft_uri, &mut seen, confirm, 97)["error"]
            .is_null()
    );
    wait_for_head_change(&repo, &head);
    assert_eq!(
        repo.git(&["log", "-1", "--format=%B"]).trim(),
        "Preopened manual commit"
    );
    assert_eq!(
        repo.git(&["show", "HEAD:f.txt"]),
        fixtures::standard_changes()
    );
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
    server.shutdown_and_exit();
    assert!(server
        .finish_with_messages(Duration::from_secs(10))
        .0
        .success());
}

/// A commit menu queued behind an in-flight status render stays reachable.
///
/// The client holds one status `workspace/applyEdit` reply and echo while the
/// commit run starts. `PlainDocument::prepare` refuses a second in-flight edit,
/// so the menu render is only delivered after the held render completes. This
/// is the schedule a single post-response action query misses; the structural
/// wait must still find the menu, and the resulting commit must be real.
#[test]
fn commit_menu_is_offered_after_a_queued_status_render_is_released() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    std::fs::write(repo.path().join("g.txt"), "new file\n").expect("write untracked g.txt");
    let head = repo.git(&["rev-parse", "HEAD"]);
    let (mut server, mut editor, uri) = open_plain(&repo, caps(true));
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

    // Stage f.txt and wait until the synced document offers open.commit.
    let stage_f = wait_for_status_action(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &uri,
        "f.txt",
        "status.stage",
        90,
    );
    assert!(
        execute_action(&mut server, &mut editor, &draft_uri, &mut seen, stage_f, 91)["error"]
            .is_null()
    );
    let open_commit = wait_for_status_action(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &uri,
        "f.txt",
        "open.commit",
        92,
    );

    // Stage the untracked file but keep its reply and echo: one status render
    // stays in flight when the commit run starts.
    let stage_g = wait_for_status_action(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &uri,
        "g.txt",
        "status.stage",
        93,
    );
    server.send(json!({"jsonrpc":"2.0","id":94,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[stage_g]}}));
    let held = loop {
        let message = server.recv();
        seen.push(message.clone());
        if message["method"] == "workspace/applyEdit" && !draft_edit(&message, &draft_uri) {
            break message;
        }
        if message["method"].is_string() && !message["id"].is_null() {
            for frame in editor.dispatch(&message) {
                server.send(frame);
            }
        }
    };

    // The menu render is retained, not delivered: the pre-release document
    // offers no menu, which is exactly what the old one-shot query observed.
    assert!(execute_action(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        open_commit,
        95
    )["error"]
        .is_null());
    let mut early = Vec::new();
    for line in 0..editor.text(&uri).lines().count() as u32 {
        early.extend(code_actions(
            &mut server,
            &mut editor,
            &draft_uri,
            &mut seen,
            &uri,
            line,
            1000 + i64::from(line),
        ));
    }
    assert!(
        early
            .iter()
            .all(|action| action["command"]["arguments"][0]["command"] != "menu.run-action"),
        "the menu must wait for the queued render: {early:#?}"
    );

    // Releasing the held echo and reply delivers the queued menu render.
    for frame in editor.dispatch(&held) {
        server.send(frame);
    }
    let create = wait_for_commit_menu(&mut server, &mut editor, &draft_uri, &mut seen, &uri)
        .into_iter()
        .find(|action| action["command"]["arguments"][0]["target"]["itemKey"].is_string())
        .expect("commit creation offered")["command"]["arguments"][0]
        .clone();
    assert!(
        execute_action(&mut server, &mut editor, &draft_uri, &mut seen, create, 96)["error"]
            .is_null()
    );

    let deadline = Instant::now() + Duration::from_secs(15);
    while !seen.iter().any(|message| {
        message["method"] == "window/showDocument" && message["params"]["uri"] == draft_uri
    }) {
        assert!(
            Instant::now() < deadline,
            "draft delivery timed out: seen {seen:#?}"
        );
        pump(&mut server, &mut editor, &draft_uri, &mut seen);
    }
    let template = std::fs::read_to_string(&expected_path).expect("commit template");
    server.send(editor.open(&draft_uri, &template));
    server.send(editor.edit(&draft_uri, "Delayed menu commit\n"));
    let confirm = wait_for_draft_confirm(&mut server, &mut editor, &draft_uri, &mut seen, 1100);
    assert!(
        execute_action(&mut server, &mut editor, &draft_uri, &mut seen, confirm, 97)["error"]
            .is_null()
    );

    wait_for_status_view(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        &uri,
        &expected_path,
    );
    wait_for_head_change(&repo, &head);
    assert_eq!(
        repo.git(&["log", "-1", "--format=%B"]).trim(),
        "Delayed menu commit"
    );
    assert_eq!(
        repo.git(&["show", "HEAD:f.txt"]),
        fixtures::standard_changes()
    );
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(Duration::from_secs(10));
    assert!(status.success());
}

/// Read the status view's code actions for the four commit commands.
///
/// Starting a commit run pops the commit menu, so the status view is the
/// document the Plain client sees while the draft is pending. The commit
/// commands are offered there and report their availability state.
fn commit_command_actions(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    draft_uri: &str,
    seen: &mut Vec<Value>,
    uri: &str,
) -> Vec<Value> {
    let text = editor.text(uri).to_owned();
    let line = text
        .lines()
        .position(|line| line.contains("Staged"))
        .unwrap_or(0) as u32;
    code_actions(server, editor, draft_uri, seen, uri, line, 120)
        .into_iter()
        .filter(|action| {
            let command = action["data"]["command"]
                .as_str()
                .or_else(|| action["command"]["arguments"][0]["command"].as_str());
            command.is_some_and(|command| {
                matches!(
                    command,
                    "commit.create" | "commit.amend" | "commit.extend" | "commit.reword"
                )
            })
        })
        .collect()
}

#[test]
fn unopened_plain_commit_draft_expires_and_allows_retry() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let (mut server, mut editor, uri) = open_plain_with_env(
        &repo,
        caps(true),
        &[("MAGIT_LSP_DRAFT_OPEN_TIMEOUT_MS", "300")],
    );
    let expected_path = repo
        .path()
        .join(".git/COMMIT_EDITMSG")
        .to_string_lossy()
        .to_string();
    let expected_uri = uri_from_path(Path::new(&expected_path))
        .expect("native path has a file URI")
        .as_str()
        .to_owned();

    let mut seen = Vec::new();
    let create = prepare_commit(&mut server, &mut editor, &expected_uri, &mut seen, &uri);
    assert!(execute_action(
        &mut server,
        &mut editor,
        &expected_uri,
        &mut seen,
        create,
        95
    )["error"]
        .is_null());

    // The client never opens the draft, so the run expires and reports it.
    let deadline = Instant::now() + Duration::from_secs(15);
    while !seen.iter().any(|message| {
        message["method"] == "window/showMessage"
            && message["params"]["message"]
                .as_str()
                .is_some_and(|text| text.contains("not opened in time"))
    }) {
        assert!(
            Instant::now() < deadline,
            "draft expiry timed out: seen {seen:#?}"
        );
        pump(&mut server, &mut editor, &expected_uri, &mut seen);
    }

    let actions = commit_command_actions(&mut server, &mut editor, &expected_uri, &mut seen, &uri);
    assert_eq!(
        actions.len(),
        4,
        "all commit commands offered: {actions:#?}"
    );
    for action in &actions {
        assert!(
            !action["disabled"]["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("not opened in time")),
            "expiry must not permanently disable retry: {action:#?}"
        );
    }
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn opened_plain_commit_draft_is_not_expired() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let (mut server, mut editor, uri) = open_plain_with_env(
        &repo,
        caps(true),
        &[("MAGIT_LSP_DRAFT_OPEN_TIMEOUT_MS", "300")],
    );
    let expected_path = repo
        .path()
        .join(".git/COMMIT_EDITMSG")
        .to_string_lossy()
        .to_string();
    let expected_uri = uri_from_path(Path::new(&expected_path))
        .expect("native path has a file URI")
        .as_str()
        .to_owned();

    let mut seen = Vec::new();
    let create = prepare_commit(&mut server, &mut editor, &expected_uri, &mut seen, &uri);
    assert!(execute_action(
        &mut server,
        &mut editor,
        &expected_uri,
        &mut seen,
        create,
        95
    )["error"]
        .is_null());

    // The client opens the draft, so the run must survive the deadline.
    let deadline = Instant::now() + Duration::from_secs(15);
    while !seen.iter().any(|message| {
        message["method"] == "window/showDocument" && message["params"]["uri"] == expected_uri
    }) {
        assert!(
            Instant::now() < deadline,
            "draft delivery timed out: seen {seen:#?}"
        );
        pump(&mut server, &mut editor, &expected_uri, &mut seen);
    }
    server.send(editor.open(
        &expected_uri,
        &std::fs::read_to_string(&expected_path).unwrap(),
    ));
    std::thread::sleep(Duration::from_millis(800));

    let actions = commit_command_actions(&mut server, &mut editor, &expected_uri, &mut seen, &uri);
    assert_eq!(
        actions.len(),
        4,
        "all commit commands offered: {actions:#?}"
    );
    assert!(
        !seen.iter().any(|message| {
            message["method"] == "window/showMessage"
                && message["params"]["message"]
                    .as_str()
                    .is_some_and(|text| text.contains("not opened in time"))
        }),
        "an opened draft must not expire: {seen:#?}"
    );
    for action in &actions {
        assert!(
            !action["disabled"]["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("not opened in time")),
            "an opened draft keeps the commit commands available: {action:#?}"
        );
    }
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn second_session_gets_no_draft_actions_and_cannot_forge_confirm() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let head = repo.git(&["rev-parse", "HEAD"]);

    // Session A owns the commit draft for the shared repository.
    let (mut server_a, mut editor_a, uri_a) = open_plain(&repo, caps(true));
    let mut seen_a = Vec::new();
    let draft_uri = start_open_draft(&repo, &mut server_a, &mut editor_a, &uri_a, &mut seen_a);
    let offered_a = code_actions(
        &mut server_a,
        &mut editor_a,
        &draft_uri,
        &mut seen_a,
        &draft_uri,
        0,
        300,
    );
    let confirm = draft_action(&offered_a, "common.confirm");
    let abort = draft_action(&offered_a, "common.cancel");

    // Session B is an independent connection pointed at the same repository.
    let (mut server_b, mut editor_b, uri_b) = open_plain(&repo, caps(true));
    assert_eq!(
        uri_b, uri_a,
        "both sessions address the same status document"
    );
    let mut seen_b = Vec::new();

    // A draft run belongs to the connection that created it, so B offers no
    // Confirm/Abort for A's draft URI.
    let offered_b = code_actions(
        &mut server_b,
        &mut editor_b,
        &draft_uri,
        &mut seen_b,
        &draft_uri,
        0,
        310,
    );
    assert!(
        offered_b.iter().all(|action| {
            let command = action["command"]["arguments"][0]["command"].as_str();
            command != Some("common.confirm") && command != Some("common.cancel")
        }),
        "a foreign session offers no draft actions: {offered_b:#?}"
    );

    // A forged Confirm/Abort carrying A's exact identity is refused without a
    // commit, because B's owner has no such pending run.
    let refused = execute_action(
        &mut server_b,
        &mut editor_b,
        &draft_uri,
        &mut seen_b,
        confirm,
        320,
    );
    assert_eq!(
        refused["error"]["message"],
        "Commit draft is no longer pending"
    );
    let refused = execute_action(
        &mut server_b,
        &mut editor_b,
        &draft_uri,
        &mut seen_b,
        abort,
        321,
    );
    assert_eq!(
        refused["error"]["message"],
        "Commit draft URI is not pending on this connection"
    );
    assert_eq!(
        repo.git(&["rev-parse", "HEAD"]),
        head,
        "a foreign confirm must not commit"
    );
    assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]), "1\n");

    // A's own run stays pending and still offers both actions.
    let still = code_actions(
        &mut server_a,
        &mut editor_a,
        &draft_uri,
        &mut seen_a,
        &draft_uri,
        0,
        330,
    );
    draft_action(&still, "common.confirm");
    draft_action(&still, "common.cancel");

    server_a.shutdown_and_exit();
    let (status_a, _) = server_a.finish_with_messages(Duration::from_secs(10));
    assert!(status_a.success());
    server_b.shutdown_and_exit();
    let (status_b, _) = server_b.finish_with_messages(Duration::from_secs(10));
    assert!(status_b.success());
}

#[test]
fn current_unopened_draft_delivery_failure_cancels_for_rejection_error_and_timeout() {
    for outcome in ["rejection", "error", "timeout"] {
        let (repo, _) = fixtures::RepoFixture::new()
            .commit("initial", [("f.txt", "before\n")])
            .modify_standard()
            .build();
        let head = repo.git(&["rev-parse", "HEAD"]);
        let (mut server, mut editor, uri) = open_plain_with_env(
            &repo,
            caps(true),
            &[("MAGIT_LSP_DRAFT_DELIVERY_TIMEOUT_MS", "300")],
        );
        let expected_path = repo
            .path()
            .join(".git/COMMIT_EDITMSG")
            .to_string_lossy()
            .to_string();
        let draft_uri = uri_from_path(Path::new(&expected_path))
            .expect("native path has a file URI")
            .as_str()
            .to_owned();
        server.send(editor.open(&draft_uri, "Previous commit message\n"));

        let mut seen = Vec::new();
        let create = prepare_commit(&mut server, &mut editor, &draft_uri, &mut seen, &uri);
        server.send(json!({"jsonrpc":"2.0","id":95,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[create]}}));
        let held = hold_draft_edit(&mut server, &mut editor, &draft_uri, &mut seen);
        fail_delivery(&mut server, &held, outcome);

        // The current delivery never reached the buffer, so the run is canceled
        // with a readable report and Git never commits.
        let deadline = Instant::now() + Duration::from_secs(15);
        while !reported_delivery_failure(&seen) {
            assert!(
                Instant::now() < deadline,
                "{outcome} report timed out: {seen:#?}"
            );
            pump(&mut server, &mut editor, &draft_uri, &mut seen);
        }
        assert_eq!(
            repo.git(&["rev-parse", "HEAD"]),
            head,
            "{outcome} must not commit"
        );
        let offered = code_actions(
            &mut server,
            &mut editor,
            &draft_uri,
            &mut seen,
            &draft_uri,
            0,
            96,
        );
        assert!(
            offered.iter().all(|action| {
                let command = action["command"]["arguments"][0]["command"].as_str();
                command != Some("common.confirm") && command != Some("common.cancel")
            }),
            "{outcome} leaves no pending draft: {offered:#?}"
        );

        server.shutdown_and_exit();
        let (status, _) = server.finish_with_messages(Duration::from_secs(10));
        assert!(status.success());
    }
}

/// Apply every frame the server already queued, answering requests in order.
///
/// A burst scheduled by an earlier write must settle before a gate is armed,
/// so the parked status read belongs to the deliberate burst.
#[cfg(unix)]
fn drain_pending(server: &mut stdio::TestServer, editor: &mut FakeEditor) {
    loop {
        let pending = server.drain_notifications();
        if pending.is_empty() {
            return;
        }
        for message in pending {
            if message["method"].is_string() && !message["id"].is_null() {
                for frame in editor.dispatch(&message) {
                    server.send(frame);
                }
            }
        }
    }
}

/// Wait until the watcher's arming rescan has applied (bounded).
///
/// Arming queues one synthetic full refresh. It is wire-silent on an unchanged
/// tree, so the server log is the only observable; waiting here keeps the
/// [`StatusGate`] for the burst the test deliberately schedules instead of the
/// rescan's own status read.
#[cfg(unix)]
fn wait_for_arm_rescan(server: &mut stdio::TestServer) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let log = server.stderr_text();
        if log
            .lines()
            .any(|line| line.contains("watcher burst settled") && line.contains("rescan=true"))
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the arming rescan never settled:\n{log}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A `git` PATH wrapper that parks the first `status` read after arming.
///
/// Unlike the single-use `GatedGit` fixture, arming happens after the server has
/// opened its view: every earlier read runs immediately, and the first read
/// after [`Self::arm_burst`] blocks until [`Self::release`]. That is exactly
/// where a scheduled watcher burst sits once it has passed its publication
/// window check and taken session ownership.
#[cfg(unix)]
struct StatusGate {
    dir: tempfile::TempDir,
    arm: std::path::PathBuf,
    entered: std::path::PathBuf,
    finished: std::path::PathBuf,
    release: std::path::PathBuf,
}

#[cfg(unix)]
impl StatusGate {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::TempDir::new().expect("status gate tempdir");
        let arm = dir.path().join("arm");
        let claimed = dir.path().join("claimed");
        let entered = dir.path().join("entered");
        let finished = dir.path().join("finished");
        let release = dir.path().join("release");
        let script = dir.path().join("git");
        let real = std::process::Command::new("sh")
            .args(["-c", "command -v git"])
            .output()
            .expect("locate git");
        assert!(real.status.success(), "git is not on PATH");
        let real = String::from_utf8_lossy(&real.stdout).trim().to_string();
        fixtures::write(
            &script,
            &format!(
                "#!/bin/sh\n\
                 subcommand=\"$1\"\n\
                 if [ \"$subcommand\" = '--no-optional-locks' ]; then subcommand=\"$2\"; fi\n\
                 if [ \"$subcommand\" = 'status' ] && [ -f '{arm}' ] && [ ! -f '{claimed}' ]; then\n\
                   : > '{claimed}'\n\
                   echo \"$$\" > '{entered}'\n\
                   n=0\n\
                   while [ ! -f '{release}' ]; do\n\
                     sleep 0.05\n\
                     n=$((n+1))\n\
                     [ \"$n\" -ge 4000 ] && break\n\
                   done\n\
                   rm -f '{entered}'\n\
                   '{real}' \"$@\"\n\
                   status=$?\n\
                   : > '{finished}'\n\
                   exit $status\n\
                 fi\n\
                 exec '{real}' \"$@\"\n",
                arm = arm.display(),
                claimed = claimed.display(),
                entered = entered.display(),
                finished = finished.display(),
                release = release.display(),
                real = real,
            ),
        );
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .expect("chmod status gate");
        Self {
            dir,
            arm,
            entered,
            finished,
            release,
        }
    }

    /// Install the wrapper first on the spawned server's `PATH`.
    fn child_env(&self) -> Vec<(String, String)> {
        let path = format!(
            "{}:{}",
            self.dir.path().display(),
            std::env::var("PATH").unwrap_or_default()
        );
        vec![("PATH".to_string(), path)]
    }

    /// Make the next `status` read park; every earlier read stays untouched.
    fn arm_burst(&self) {
        std::fs::write(&self.arm, b"armed\n").expect("arm status gate");
    }

    /// Block until the parked read has entered the gate (bounded).
    fn wait_entered(&self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.entered.exists() {
            assert!(
                Instant::now() < deadline,
                "the watcher's status read never entered the gate"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Block until the parked read has completed its real Git call (bounded).
    fn wait_finished(&self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.finished.exists() {
            assert!(
                Instant::now() < deadline,
                "the parked status read never finished"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Let the parked read proceed (idempotent).
    fn release(&self) {
        let _ = std::fs::write(&self.release, b"release\n");
    }
}

#[cfg(unix)]
impl Drop for StatusGate {
    fn drop(&mut self) {
        // A failed assertion must never leave the watcher parked.
        let _ = std::fs::write(&self.release, b"release\n");
    }
}

/// A watcher burst that passed its publication-window check before a
/// commit-confirm opened the window must not publish the tree it read.
///
/// `Shared::wait_publication_window` is consulted before the watcher takes
/// session ownership, so a burst can pass the check and then read Git while the
/// confirm opens the window and resumes the commit. Publishing that read
/// interleaved an intermediate status edit between the confirm and its
/// `Committed:` outcome (recorded replay divergence at
/// `commit-edit-confirm.jsonl:41`). The gate below parks the burst's status read
/// after the check, inside `refresh_all`; the confirm then opens the window and
/// the commit-msg hook parks Git, so the read lands mid-commit. The client must
/// observe exactly one status transition — the settled tree reclaiming the
/// pending row — and the committed message after it.
#[cfg(unix)]
#[test]
fn a_watcher_refresh_inside_a_commit_window_does_not_publish_a_status_edit() {
    use std::os::unix::fs::PermissionsExt;

    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let ready = repo.path().join(".git").join("hook.ready");
    let hook_release = repo.path().join(".git").join("hook.release");
    let hook = repo.path().join(".git").join("hooks").join("commit-msg");
    fixtures::write(
        &hook,
        &format!(
            "#!/bin/sh\nprintf ok > '{ready}'\ni=0\nwhile [ ! -f '{release}' ] && [ \"$i\" -lt 300 ]; do i=$((i+1)); sleep 0.05; done\nexit 0\n",
            ready = ready.display(),
            release = hook_release.display(),
        ),
    );
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    // Release the hook even when an assertion fails, so Git cannot outlive it.
    struct ReleaseHook(std::path::PathBuf);
    impl Drop for ReleaseHook {
        fn drop(&mut self) {
            let _ = std::fs::write(&self.0, "release");
        }
    }
    let _hook = ReleaseHook(hook_release.clone());

    let gate = StatusGate::new();
    let mut env = gate.child_env();
    env.push(("MAGIT_LSP_WATCH_DEBOUNCE_MS".to_string(), "0".to_string()));
    let env: Vec<(&str, &str)> = env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let (mut server, mut editor, uri) = open_plain_with_env(&repo, caps(true), &env);
    // The watcher's arming rescan reads status too; it must finish before the
    // gate is armed or it claims that read instead of the deliberate burst.
    // Settle it before writing anything else, while the log is small enough
    // that the settled marker cannot have been rotated away.
    wait_for_arm_rescan(&mut server);
    let mut seen = Vec::new();
    let draft_uri = start_open_draft(&repo, &mut server, &mut editor, &uri, &mut seen);
    server.send(editor.edit(&draft_uri, "Gated publication\n"));
    let confirm = wait_for_draft_confirm(&mut server, &mut editor, &draft_uri, &mut seen, 300);

    // Settle any burst the draft template write scheduled, so the gate below
    // parks the deliberate burst and not a leftover refresh.
    drain_pending(&mut server, &mut editor);

    // This worktree write schedules the burst the gate parks: its refresh has
    // already passed the (still closed) publication window when it blocks.
    gate.arm_burst();
    fixtures::write(&repo.path().join("touched.txt"), "worktree noise\n");
    gate.wait_entered();

    // The confirm opens the publication window and resumes Git, which parks in
    // the commit-msg hook: the burst's read now runs inside the open window.
    assert!(execute_action(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        confirm,
        301
    )["error"]
        .is_null());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.exists() {
        assert!(
            Instant::now() < deadline,
            "commit-msg hook did not become ready"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    gate.release();
    gate.wait_finished();
    std::fs::write(&hook_release, "release").unwrap();

    // Apply every status transition until the settled tree and the outcome both
    // arrive; the applied text is asserted, not just the presence of a render.
    let mut transitions: Vec<String> = Vec::new();
    let mut committed = false;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let settled = !editor.text(&uri).contains("COMMIT_EDITMSG")
            && editor.text(&uri).contains("touched.txt");
        if committed && settled {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "no settled status and Committed outcome; transitions: {transitions:#?}"
        );
        let message = server.recv();
        seen.push(message.clone());
        if message["method"] == "workspace/applyEdit" && !draft_edit(&message, &draft_uri) {
            for frame in editor.dispatch(&message) {
                server.send(frame);
            }
            transitions.push(editor.text(&uri).to_owned());
        } else if message["method"] == "workspace/applyEdit" {
            let result = editor.apply_edit(&message["params"]["edit"]);
            server.send(json!({"jsonrpc":"2.0","id":message["id"],"result":result}));
        } else if message["method"].is_string() && !message["id"].is_null() {
            for frame in editor.dispatch(&message) {
                server.send(frame);
            }
        }
        if message["method"] == "window/showMessage"
            && message["params"]["message"]
                .as_str()
                .is_some_and(|text| text.contains("Committed: Gated publication"))
        {
            committed = true;
        }
    }

    assert_eq!(
        transitions.len(),
        1,
        "the confirm owns exactly one status transition, not an intermediate refresh: {transitions:#?}"
    );
    let settled = &transitions[0];
    assert!(
        settled.contains("Staged changes (0)") && settled.contains("Untracked files (1)"),
        "the single transition carries the settled committed tree: {settled}"
    );
    assert!(
        settled.contains("touched.txt") && !settled.contains("COMMIT_EDITMSG"),
        "the settled tree lists the touched file and reclaims the pending row: {settled}"
    );
    assert!(
        !settled.contains("Staged changes (1)"),
        "no pending staged row survives the commit: {settled}"
    );
    wait_for_head_message(&repo, "Gated publication");
    assert_eq!(
        repo.git(&["show", "HEAD:f.txt"]),
        fixtures::standard_changes()
    );
    server.shutdown_and_exit();
    assert!(server
        .finish_with_messages(Duration::from_secs(10))
        .0
        .success());
}

/// A settled watcher read withheld inside a commit window is still delivered
/// once, before the outcome message.
///
/// The counterpart to the degraded-read case: the burst passes the publication
/// window check, the commit finishes while its status read is parked, and the
/// read therefore sees the settled tree. Withholding it must not lose the
/// client's only copy of that render — the owning worker publishes the withheld
/// render when the settled session still carries it, ahead of `Committed:`.
#[cfg(unix)]
#[test]
fn a_settled_watcher_read_inside_a_commit_window_publishes_before_the_outcome() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let gate = StatusGate::new();
    let mut env = gate.child_env();
    env.push(("MAGIT_LSP_WATCH_DEBOUNCE_MS".to_string(), "0".to_string()));
    let env: Vec<(&str, &str)> = env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let (mut server, mut editor, uri) = open_plain_with_env(&repo, caps(true), &env);
    // The arming rescan must settle before the gate is armed, or it claims
    // that read instead of the burst this test parks. Waiting here keeps the
    // settled marker inside the log, before the draft flow floods it.
    wait_for_arm_rescan(&mut server);
    let mut seen = Vec::new();
    let draft_uri = start_open_draft(&repo, &mut server, &mut editor, &uri, &mut seen);
    server.send(editor.edit(&draft_uri, "Settled publication\n"));
    let confirm = wait_for_draft_confirm(&mut server, &mut editor, &draft_uri, &mut seen, 300);
    drain_pending(&mut server, &mut editor);
    let head = repo.git(&["rev-parse", "HEAD"]);

    // The parked burst's read runs after the commit has settled, while the
    // worker still cannot publish because the burst owns the session.
    gate.arm_burst();
    fixtures::write(&repo.path().join("touched.txt"), "worktree noise\n");
    gate.wait_entered();
    assert!(execute_action(
        &mut server,
        &mut editor,
        &draft_uri,
        &mut seen,
        confirm,
        301
    )["error"]
        .is_null());
    wait_for_head_change(&repo, &head);
    gate.release();
    gate.wait_finished();

    let mut transitions: Vec<String> = Vec::new();
    let mut committed_at = None;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let settled = !editor.text(&uri).contains("COMMIT_EDITMSG")
            && editor.text(&uri).contains("touched.txt");
        if committed_at.is_some() && settled {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "no settled status and Committed outcome; transitions: {transitions:#?}"
        );
        let message = server.recv();
        seen.push(message.clone());
        if message["method"] == "workspace/applyEdit" && !draft_edit(&message, &draft_uri) {
            for frame in editor.dispatch(&message) {
                server.send(frame);
            }
            transitions.push(editor.text(&uri).to_owned());
        } else if message["method"] == "workspace/applyEdit" {
            let result = editor.apply_edit(&message["params"]["edit"]);
            server.send(json!({"jsonrpc":"2.0","id":message["id"],"result":result}));
        } else if message["method"].is_string() && !message["id"].is_null() {
            for frame in editor.dispatch(&message) {
                server.send(frame);
            }
        }
        if message["method"] == "window/showMessage"
            && message["params"]["message"]
                .as_str()
                .is_some_and(|text| text.contains("Committed: Settled publication"))
        {
            committed_at = Some(transitions.len());
        }
    }

    assert_eq!(
        committed_at,
        Some(1),
        "the settled transition precedes the outcome and only one exists: {transitions:#?}"
    );
    let settled = &transitions[0];
    assert!(
        settled.contains("Staged changes (0)") && settled.contains("touched.txt"),
        "the withheld settled render reached the client: {settled}"
    );
    wait_for_head_message(&repo, "Settled publication");
    server.shutdown_and_exit();
    assert!(server
        .finish_with_messages(Duration::from_secs(10))
        .0
        .success());
}

#[test]
fn late_draft_delivery_failure_after_open_never_cancels_the_run() {
    for outcome in ["rejection", "error", "timeout"] {
        let (repo, _) = fixtures::RepoFixture::new()
            .commit("initial", [("f.txt", "before\n")])
            .modify_standard()
            .build();
        let (mut server, mut editor, uri) = open_plain_with_env(
            &repo,
            caps(true),
            &[("MAGIT_LSP_DRAFT_DELIVERY_TIMEOUT_MS", "300")],
        );
        let expected_path = repo
            .path()
            .join(".git/COMMIT_EDITMSG")
            .to_string_lossy()
            .to_string();
        let draft_uri = uri_from_path(Path::new(&expected_path))
            .expect("native path has a file URI")
            .as_str()
            .to_owned();
        server.send(editor.open(&draft_uri, "Previous commit message\n"));

        let mut seen = Vec::new();
        let create = prepare_commit(&mut server, &mut editor, &draft_uri, &mut seen, &uri);
        server.send(json!({"jsonrpc":"2.0","id":95,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[create]}}));
        let held = hold_draft_edit(&mut server, &mut editor, &draft_uri, &mut seen);
        let template = fixtures::read(std::path::Path::new(&expected_path));

        // The exact didOpen reaches the server before the delivery result does.
        server.send(editor.open(&draft_uri, &template));
        fail_delivery(&mut server, &held, outcome);
        server.send(editor.edit(&draft_uri, "Late delivery survives\n"));

        // The opened run is still the current one: it confirms and commits.
        let confirm = wait_for_draft_confirm(&mut server, &mut editor, &draft_uri, &mut seen, 1100);
        assert!(
            execute_action(&mut server, &mut editor, &draft_uri, &mut seen, confirm, 97)["error"]
                .is_null()
        );
        wait_for_head_message(&repo, "Late delivery survives");
        assert!(
            !reported_delivery_failure(&seen),
            "a {outcome} after didOpen must not cancel the run: {seen:#?}"
        );

        server.shutdown_and_exit();
        let (status, _) = server.finish_with_messages(Duration::from_secs(10));
        assert!(status.success());
    }
}
