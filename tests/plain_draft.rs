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
    let uri = format!("file://{}/.magit", repo.path().display());
    let mut server = stdio::TestServer::spawn(extra_env);
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

/// Start a commit run, wait for its showDocument and attach the draft document.
fn start_open_draft(
    repo: &fixtures::RepoFixture,
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    status_uri: &str,
    seen: &mut Vec<Value>,
) -> String {
    let expected_path = repo.path().join(".git/COMMIT_EDITMSG").to_string_lossy().to_string();
    let draft_uri = format!("file://{expected_path}");
    let create = prepare_commit(server, editor, &draft_uri, seen, status_uri);
    assert!(execute_action(server, editor, &draft_uri, seen, create, 95)["error"].is_null());
    let deadline = Instant::now() + Duration::from_secs(15);
    while !seen.iter().any(|message| {
        message["method"] == "window/showDocument" && message["params"]["uri"] == draft_uri
    }) {
        assert!(Instant::now() < deadline, "draft delivery timed out: seen {seen:#?}");
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
        assert!(Instant::now() < deadline, "status view timed out: seen {seen:#?}");
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
    while !seen.iter().any(|message| message["method"] == "workspace/applyEdit" && draft_edit(message, draft_uri))
        || !seen.iter().any(|message| message["method"] == "window/showDocument" && message["params"]["uri"] == draft_uri)
    {
        assert!(Instant::now() < deadline, "draft delivery timed out: seen {seen:#?}");
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

    let offered = code_actions(&mut server, &mut editor, &draft_uri, &mut seen, &draft_uri, 0, 200);
    let confirm = draft_action(&offered, "common.confirm");
    let current = confirm["version"].as_i64().expect("commit version");
    let mut stale = confirm.clone();
    stale["version"] = json!(current - 1);
    let refused = execute_action(&mut server, &mut editor, &draft_uri, &mut seen, stale, 201);
    assert_eq!(
        refused["error"]["message"],
        "Wait for the exact synchronized document version; full-text repair may be required",
    );

    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head, "a stale confirm must not commit");
    let still = code_actions(&mut server, &mut editor, &draft_uri, &mut seen, &draft_uri, 0, 202);
    draft_action(&still, "common.confirm");
    assert_eq!(editor.text(&draft_uri), "Uncommitted body\n", "the draft stays pending");
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
    let expected_path = repo.path().join(".git/COMMIT_EDITMSG").to_string_lossy().to_string();
    let head = repo.git(&["rev-parse", "HEAD"]);
    let (mut server, mut editor, uri) = open_plain(&repo, caps(true));
    let mut seen = Vec::new();
    let draft_uri = start_open_draft(&repo, &mut server, &mut editor, &uri, &mut seen);
    server.send(editor.edit(&draft_uri, "Aborted body\n"));

    let offered = code_actions(&mut server, &mut editor, &draft_uri, &mut seen, &draft_uri, 0, 210);
    let abort = offered
        .iter()
        .find(|action| action["title"] == "Abort commit")
        .expect("attached draft offers Abort")["command"]["arguments"][0]
        .clone();
    assert_eq!(abort["command"], "common.cancel");
    let aborted = execute_action(&mut server, &mut editor, &draft_uri, &mut seen, abort, 211);
    assert!(aborted["error"].is_null(), "Abort failed: {aborted}");
    assert_eq!(aborted["result"]["type"], "aborted");
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head, "Abort must not commit");
    assert!(repo.git(&["status", "--porcelain"]).contains("f.txt"), "Abort keeps staged work");

    wait_for_status_view(&mut server, &mut editor, &draft_uri, &mut seen, &uri, &expected_path);
    assert!(editor.text(&uri).contains("f.txt"), "the status view is shown again");
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
    let expected_path = repo.path().join(".git/COMMIT_EDITMSG").to_string_lossy().to_string();
    let head = repo.git(&["rev-parse", "HEAD"]);
    let (mut server, mut editor, uri) = open_plain(&repo, caps(true));
    let mut seen = Vec::new();
    let draft_uri = start_open_draft(&repo, &mut server, &mut editor, &uri, &mut seen);

    let offered = code_actions(&mut server, &mut editor, &draft_uri, &mut seen, &draft_uri, 0, 220);
    let abort = offered
        .iter()
        .find(|action| action["title"] == "Abort commit")
        .expect("attached draft offers Abort")["command"]["arguments"][0]
        .clone();
    assert!(execute_action(&mut server, &mut editor, &draft_uri, &mut seen, abort, 221)["error"].is_null());

    let before_close = seen.len();
    server.send(editor.close(&draft_uri));
    // The server stays responsive and the late close changes nothing.
    let after = code_actions(&mut server, &mut editor, &draft_uri, &mut seen, &uri, 0, 222);
    assert!(after.iter().all(|action| {
        action["command"]["arguments"][0]["command"] != "common.confirm"
    }));
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head, "a close after abort must not commit");
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
    assert_ne!(repo.git(&["rev-parse", "HEAD"]), head, "the close must commit");
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
    assert_eq!(std::fs::read_to_string(&path).unwrap(), saved, "the file keeps the saved text");
    server.send(editor.close(&draft_uri));

    wait_for_head_message(&repo, "Prior saved body");
    assert_ne!(repo.git(&["rev-parse", "HEAD"]), head, "the close must commit");
    let message = repo.git(&["log", "-1", "--format=%B"]);
    assert_eq!(message.trim(), "Prior saved body", "the prior saved text is committed");
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
fn unrelated_document_notifications_are_rejected_without_touching_the_draft() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let expected_path = repo.path().join(".git/COMMIT_EDITMSG").to_string_lossy().to_string();
    let (mut server, mut editor, uri) = open_plain(&repo, caps(true));
    let mut seen = Vec::new();
    let draft_uri = start_open_draft(&repo, &mut server, &mut editor, &uri, &mut seen);
    let edited = "Kept body\n";
    server.send(editor.edit(&draft_uri, edited));

    let unrelated = format!("file://{}/unrelated.txt", repo.path().display());
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":unrelated,"languageId":"plaintext","version":1,"text":"unrelated\n"}}}));
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":unrelated,"version":2},"contentChanges":[{"text":"changed\n"}]}}));
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":unrelated}}}));

    let offered = code_actions(&mut server, &mut editor, &draft_uri, &mut seen, &draft_uri, 0, 230);
    let confirm = draft_action(&offered, "common.confirm");
    let confirmed = execute_action(&mut server, &mut editor, &draft_uri, &mut seen, confirm, 231);
    assert!(confirmed["error"].is_null(), "draft confirm failed: {confirmed}");

    wait_for_status_view(&mut server, &mut editor, &draft_uri, &mut seen, &uri, &expected_path);
    assert_eq!(repo.git(&["log", "-1", "--format=%B"]), format!("{edited}\n"));
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
    let line = text.lines().position(|line| line.contains("Staged")).unwrap_or(0) as u32;
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
fn unopened_plain_commit_draft_expires_and_disables_commit_commands() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let (mut server, mut editor, uri) = open_plain_with_env(
        &repo,
        caps(true),
        &[("MAGIT_LSP_DRAFT_OPEN_TIMEOUT_MS", "300")],
    );
    let expected_path = repo.path().join(".git/COMMIT_EDITMSG").to_string_lossy().to_string();
    let expected_uri = format!("file://{expected_path}");

    let mut seen = Vec::new();
    let create = prepare_commit(&mut server, &mut editor, &expected_uri, &mut seen, &uri);
    assert!(execute_action(&mut server, &mut editor, &expected_uri, &mut seen, create, 95)["error"].is_null());

    // The client never opens the draft, so the run expires and reports it.
    let deadline = Instant::now() + Duration::from_secs(15);
    while !seen.iter().any(|message| {
        message["method"] == "window/showMessage"
            && message["params"]["message"]
                .as_str()
                .is_some_and(|text| text.contains("not opened in time"))
    }) {
        assert!(Instant::now() < deadline, "draft expiry timed out: seen {seen:#?}");
        pump(&mut server, &mut editor, &expected_uri, &mut seen);
    }

    let actions = commit_command_actions(&mut server, &mut editor, &expected_uri, &mut seen, &uri);
    assert_eq!(actions.len(), 4, "all commit commands offered: {actions:#?}");
    for action in &actions {
        assert!(
            action["disabled"]["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("not opened in time")),
            "commit command must be unavailable: {action:#?}"
        );
        assert!(action["command"].is_null(), "disabled rows carry no command: {action:#?}");
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
    let expected_path = repo.path().join(".git/COMMIT_EDITMSG").to_string_lossy().to_string();
    let expected_uri = format!("file://{expected_path}");

    let mut seen = Vec::new();
    let create = prepare_commit(&mut server, &mut editor, &expected_uri, &mut seen, &uri);
    assert!(execute_action(&mut server, &mut editor, &expected_uri, &mut seen, create, 95)["error"].is_null());

    // The client opens the draft, so the run must survive the deadline.
    let deadline = Instant::now() + Duration::from_secs(15);
    while !seen.iter().any(|message| {
        message["method"] == "window/showDocument" && message["params"]["uri"] == expected_uri
    }) {
        assert!(Instant::now() < deadline, "draft delivery timed out: seen {seen:#?}");
        pump(&mut server, &mut editor, &expected_uri, &mut seen);
    }
    server.send(editor.open(&expected_uri, &std::fs::read_to_string(&expected_path).unwrap()));
    std::thread::sleep(Duration::from_millis(800));

    let actions = commit_command_actions(&mut server, &mut editor, &expected_uri, &mut seen, &uri);
    assert_eq!(actions.len(), 4, "all commit commands offered: {actions:#?}");
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
