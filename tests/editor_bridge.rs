//! The editor bridge end to end (R-3.4): a real `git commit` waits for the
//! client to confirm the real commit-message file, then continues or aborts.

mod common;

use common::stdio::*;
use common::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

#[cfg(unix)]
#[test]
fn helper_draft_kind_checks_exact_target_and_todo_requires_explicit_intent() {
    use magit_lsp::editor::Reply;
    use magit_lsp::editor::{
        draft::{Draft, DraftContext},
        DraftKind, COMMIT_MESSAGE_KIND, REBASE_TODO_KIND,
    };
    use magit_lsp::protocol::DraftFinishReason;

    let dir = TempDir::new().unwrap();
    assert!(Command::new("git")
        .args(["init", "-q"])
        .arg(dir.path())
        .status()
        .unwrap()
        .success());
    std::fs::write(dir.path().join(".git/COMMIT_EDITMSG"), "message\n").unwrap();
    let repo = magit_lsp::git::GitRepo::open(dir.path()).unwrap();
    let todo = repo.rebase_todo_path().unwrap();
    std::fs::create_dir_all(todo.parent().unwrap()).unwrap();
    std::fs::write(&todo, "pick deadbeef subject\n").unwrap();
    let message = repo.commit_message_path().unwrap();
    let wrong = dir.path().join("wrong");
    std::fs::write(&wrong, "wrong\n").unwrap();

    for (kind, target, wrong_kind, expected_kind) in [
        (
            DraftKind::CommitMessage,
            message.as_path(),
            REBASE_TODO_KIND,
            COMMIT_MESSAGE_KIND,
        ),
        (
            DraftKind::RebaseTodo,
            todo.as_path(),
            COMMIT_MESSAGE_KIND,
            REBASE_TODO_KIND,
        ),
    ] {
        let mut draft = Draft::for_kind(&repo, "owner", kind).unwrap();
        let run = draft.run_id().to_owned();
        let uri = draft.uri().to_owned();
        let repository = draft.repository().to_owned();
        let context = DraftContext {
            owner: "owner",
            run_id: &run,
            uri: &uri,
            repository: &repository,
        };
        let token = draft.helper_token().to_owned();
        assert!(draft
            .accept_helper(context, &token, wrong_kind, target)
            .is_err());
        assert!(draft
            .accept_helper(context, &token, expected_kind, &wrong)
            .is_err());
        assert!(draft
            .accept_helper(context, &token, expected_kind, target)
            .is_ok());
        if kind == DraftKind::RebaseTodo {
            draft
                .did_open(context, 1, b"pick deadbeef subject\n")
                .unwrap();
            assert!(draft
                .finish(context, 1, DraftFinishReason::NormalClose)
                .is_err());
            assert_eq!(
                std::fs::read_to_string(&todo).unwrap(),
                "pick deadbeef subject\n"
            );
            draft
                .did_change(context, 2, b"drop deadbeef subject\n")
                .unwrap();
            assert_eq!(
                draft
                    .finish(context, 2, DraftFinishReason::Confirm)
                    .unwrap(),
                Reply::Ok
            );
            assert_eq!(
                std::fs::read_to_string(&todo).unwrap(),
                "drop deadbeef subject\n"
            );
        }
    }
    let mut cancel = Draft::for_kind(&repo, "owner", DraftKind::RebaseTodo).unwrap();
    let run = cancel.run_id().to_owned();
    let uri = cancel.uri().to_owned();
    let repository = cancel.repository().to_owned();
    let context = DraftContext {
        owner: "owner",
        run_id: &run,
        uri: &uri,
        repository: &repository,
    };
    assert_eq!(cancel.abort(context).unwrap(), Reply::Abort);
}

/// The answer the askpass tests type when Git asks for a secret.
const ASKPASS_ANSWER: &str = "s3cret";
/// The question Git asks in the askpass tests.
const ASKPASS_QUESTION: &str = "Password for 'git@example.com':";

impl TestServer {
    /// The bridge socket path, from the server's startup log.
    fn socket_path(&mut self) -> PathBuf {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let log = self.stderr_text();
            if let Some(path) = log.lines().find_map(|line| {
                line.split_once("editor bridge listening socket=")
                    .map(|(_, path)| path.trim())
            }) {
                return PathBuf::from(path);
            }
            assert!(
                Instant::now() < deadline,
                "the server did not report a bridge socket:\n{log}"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// The next `view/prompt` notification.
    fn prompt(&mut self) -> Value {
        loop {
            let message = self.recv();
            if message["method"] == json!("view/prompt") {
                return message;
            }
        }
    }

    /// Answer a prompt; `value` is the answer the server receives.
    fn respond_prompt(&mut self, id: i64, prompt_id: &str, cancelled: bool, value: Value) {
        self.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "view/promptRespond",
            "params": {
                "promptId": prompt_id,
                "cancelled": cancelled,
                "value": value,
            },
        }));
        let response = self.response_with_id(id);
        // Never print the response: it belongs to the exchange that carried
        // the (possibly secret) answer.
        assert!(
            response.get("error").is_none(),
            "the prompt answer was rejected"
        );
        assert!(
            response["result"].is_null(),
            "the prompt answer did not return a null result"
        );
    }
}

fn open_status(server: &mut TestServer, id: i64, repo: &Value) -> String {
    eprintln!("editor-bridge phase=open-status/send id={id}");
    server.send(json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "view/open",
        "params": {
            "kind": "status",
            "args": {"repo_path": repo},
            "viewport": {"cols": 80, "rows": 24},
        },
    }));
    let opened = server.response_with_id(id);
    eprintln!("editor-bridge phase=open-status/response id={id} frame={opened}");
    let view_id = opened["result"]["id"]
        .as_str()
        .expect("view id")
        .to_string();
    eprintln!("editor-bridge phase=open-status/wait-render id={id}");
    let render = server.recv();
    eprintln!("editor-bridge phase=open-status/render id={id} frame={render}");
    assert_eq!(render["method"], json!("view/render"), "{render}");
    eprintln!("editor-bridge phase=open-status/wait-keymap id={id}");
    let keymap = server.recv();
    eprintln!("editor-bridge phase=open-status/keymap id={id} frame={keymap}");
    assert_eq!(keymap["method"], json!("view/keymap"), "{keymap}");
    view_id
}

fn invoke(server: &mut TestServer, id: i64, view_id: &str, command: &str, args: Value) -> Value {
    server.send(json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "view/invoke",
        "params": {
            "viewId": view_id,
            "command": command,
            "args": args,
        },
    }));
    server.response_with_id(id)
}

/// Attach to the exact announced template, then synchronize the edited message.
fn sync_draft(server: &mut TestServer, edit: &Value, text: &str) {
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":edit["uri"],"languageId":"gitcommit","version":1,"text":edit["text"]}
    }}));
    change_draft(server, edit, 2, text);
}

/// Send the sole supported full-sync text channel.
fn change_draft(server: &mut TestServer, edit: &Value, version: i32, text: &str) {
    server.send(
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":edit["uri"],"version":version},"contentChanges":[{"text":text}]
        }}),
    );
}

/// Confirm through the registered workspace/executeCommand endpoint.
fn finish_draft(server: &mut TestServer, id: i64, edit: &Value) -> Value {
    command_confirm(server, id, edit, 2, "synchronized")
}

fn command_confirm(
    server: &mut TestServer,
    id: i64,
    edit: &Value,
    version: i32,
    source: &str,
) -> Value {
    let args = json!({"command":"common.confirm","runId":edit["runId"],"uri":edit["uri"],"version":version,"source":source});
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[args]}}));
    server.response_with_id(id)
}

/// The first `editDocument` effect in a `view/effects` notification.
fn edit_document(message: &Value) -> Option<Value> {
    if message["method"] != json!("view/effects") {
        return None;
    }
    message["params"]["effects"]
        .as_array()?
        .iter()
        .find(|effect| effect["type"] == json!("editDocument"))
        .cloned()
}

/// Start a commit from the status view and return its pending edit.
fn start_commit(server: &mut TestServer, status: &str) -> Value {
    start_commit_with_args(server, status, json!(null))
}

/// Start a real editor-backed commit with structured menu arguments.
fn start_commit_with_args(server: &mut TestServer, status: &str, args: Value) -> Value {
    start_head_operation(server, status, "commit.create", args)
}

fn start_head_operation(
    server: &mut TestServer,
    status: &str,
    command: &str,
    args: Value,
) -> Value {
    assert_eq!(
        magit_lsp::commands::required_effects(command),
        [magit_lsp::commands::EffectKind::EditDocument].as_slice()
    );
    eprintln!("editor-bridge phase=start-commit/send view={status}");
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "view/invoke",
        "params": {"viewId": status, "command": command, "args": args},
    }));
    let mut response = None;
    let mut edit = None;
    while response.is_none() || edit.is_none() {
        eprintln!(
            "editor-bridge phase=start-commit/wait response={} edit={}",
            response.is_some(),
            edit.is_some()
        );
        let message = server.recv();
        eprintln!("editor-bridge phase=start-commit/received frame={message}");
        if message["id"] == json!(4) {
            response = Some(message);
        } else if let Some(found) = edit_document(&message) {
            edit = Some(found);
        }
    }
    edit.expect("the editDocument effect")
}

fn wait_for_commit(repo: &RepoFixture, message: &str) {
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
        thread::sleep(Duration::from_millis(20));
    }
}

fn staged_repo() -> (RepoFixture, magit_lsp::git::GitRepo) {
    RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .staged_file("f.txt", standard_changes())
        .build()
}

/// Capabilities of a rich client that edits commit drafts in place.
fn rich_capabilities() -> Value {
    json!({"general":{"positionEncodings":["utf-8","utf-16"]},"window":{"workDoneProgress":true},"experimental":{"magit":{
        "v":1,"keys":"dynamic","modifiers":"full",
        "layout":["main","side","bottom","float"],
        "effects":["visitFile","editDocument","clipboard"],
        "prompts":["confirm","text","choice","multiChoice","secret"]
    }}})
}

#[path = "editor_bridge/additional.rs"]
mod additional;
#[path = "editor_bridge/cases.rs"]
mod cases;
