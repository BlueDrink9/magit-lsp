//! The editor bridge end to end (R-3.4): a real `git commit` waits for the
//! client to synchronize/checkpoint a managed draft, then continues or aborts.

mod common;

use common::stdio::*;
use common::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

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
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":edit["uri"],"version":version},"contentChanges":[{"text":text}]
    }}));
}

/// Checkpoint and finish using correlated exact-version requests, never a file.
fn finish_draft(server: &mut TestServer, id: i64, edit: &Value) -> Value {
    draft_request(server, id, "draft/finish", edit, 2, Some("confirm"))
}

/// Issue a typed draft barrier without any message text or owner/repository.
fn draft_request(server: &mut TestServer, id: i64, method: &str, edit: &Value, version: i32, reason: Option<&str>) -> Value {
    let mut params = json!({"runId":edit["runId"],"uri":edit["uri"],"version":version});
    if method == "draft/finish" {
        params["command"] = json!("common.confirm");
        params["source"] = json!(if reason == Some("confirm") { "synchronized" } else { "saved-file" });
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[params]}}));
    } else {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
    }
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
    eprintln!("editor-bridge phase=start-commit/send view={status}");
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "view/invoke",
        "params": {"viewId": status, "command": "commit.create"},
    }));
    let mut response = None;
    let mut edit = None;
    while response.is_none() || edit.is_none() {
        eprintln!("editor-bridge phase=start-commit/wait response={} edit={}", response.is_some(), edit.is_some());
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

#[cfg(target_os = "linux")]
#[test]
fn r_edit_2_eof_during_precommit_cancels_owned_hook() {
    use std::os::unix::fs::PermissionsExt;

    fn start_time(pid: i32) -> Option<u64> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        stat.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()
    }
    fn live(pid: i32) -> bool {
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok().and_then(|stat| stat.rsplit_once(')').map(|(_, tail)| {
                !matches!(tail.trim_start().chars().next(), Some('Z' | 'X'))
            })).unwrap_or(false)
    }

    let (tmp, _repo) = staged_repo();
    let marker = tmp.path().join(".git/hook-ready");
    let pidfile = tmp.path().join(".git/hook-pid");
    let hook = tmp.path().join(".git/hooks/pre-commit");
    write(&hook, &format!("#!/bin/sh\nsh -c 'trap \"\" TERM; echo $$ > \"$1\"; touch \"$2\"; exec sleep 60' sh '{}' '{}' </dev/null >/dev/null 2>&1 &\nwait\n", pidfile.display(), marker.display()));
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let staged = tmp.git(&["ls-files", "--stage", "-z"]);
    let tree = tmp.git(&["write-tree"]);
    let patch = tmp.git(&["diff", "--cached", "--binary"]);
    let worktree = read(&tmp.path().join("f.txt"));

    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(20);
    server.initialize(json!({}));
    let repo = json!(tmp.path().to_string_lossy());
    let status = open_status(&mut server, 2, &repo);
    server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/invoke","params":{"viewId":status,"command":"commit.create"}}));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !marker.exists() {
        assert!(Instant::now() < deadline, "pre-commit readiness marker missing");
        thread::sleep(Duration::from_millis(10));
    }
    let pid: i32 = read(&pidfile).trim().parse().unwrap();
    let identity = start_time(pid).expect("hook descendant identity");
    struct Cleanup(i32, u64);
    impl Drop for Cleanup { fn drop(&mut self) { if start_time_for_cleanup(self.0) == Some(self.1) && process_live_for_cleanup(self.0) { unsafe { libc::kill(self.0, libc::SIGKILL); } } } }
    fn start_time_for_cleanup(pid:i32)->Option<u64>{ let s=std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?; s.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok() }
    fn process_live_for_cleanup(pid:i32)->bool{ std::fs::read_to_string(format!("/proc/{pid}/stat")).ok().and_then(|s|s.rsplit_once(')').map(|(_,t)|!matches!(t.trim_start().chars().next(),Some('Z'|'X')))).unwrap_or(false) }
    let _owned = Cleanup(pid, identity);
    server.close_stdin();
    let exit = server.finish(Duration::from_secs(8));
    assert!(exit.success(), "server exited: {exit:?}");
    let deadline = Instant::now() + Duration::from_secs(3);
    while live(pid) && Instant::now() < deadline { thread::sleep(Duration::from_millis(10)); }
    assert!(!live(pid), "owned hook descendant survived EOF");
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), staged);
    assert_eq!(tmp.git(&["write-tree"]), tree);
    assert_eq!(tmp.git(&["diff", "--cached", "--binary"]), patch);
    assert_eq!(read(&tmp.path().join("f.txt")), worktree);
    std::fs::remove_file(hook).unwrap();
    tmp.git(&["commit", "-m", "follow-up"]);
}

#[cfg(unix)]
#[test]
fn bridge_directory_is_private_with_permissive_umask() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = TempDir::new().expect("private temp root");
    let root = tmp.path().to_string_lossy().into_owned();
    let mut server = TestServer::spawn_with_umask(&[("TMPDIR", &root)], "000");
    server.recv_timeout = Duration::from_secs(20);
    let initialized = server.initialize(json!({}));
    assert!(initialized["result"]["capabilities"].is_object());
    let socket = server.socket_path();
    let dir = PathBuf::from(&root).join(format!("magit-lsp-{}", server.pid()));
    assert_eq!(socket, dir.join("editor.sock"));
    assert_eq!(
        std::fs::metadata(&dir)
            .expect("bridge dir")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let output = answer_askpass(&mut server, &socket, ASKPASS_QUESTION, ASKPASS_ANSWER);
    assert_helper_kept_the_answer(&output, ASKPASS_ANSWER);
    server.shutdown_and_exit();
    let (exit, log) = server.finish_with_stderr(Duration::from_secs(20));
    assert!(exit.success());
    assert_log_kept_the_answer("stderr", &log, ASKPASS_ANSWER);
}

#[cfg(unix)]
#[test]
fn bridge_refuses_preexisting_directory_and_symlink() {
    use std::os::unix::fs::{symlink, FileTypeExt, MetadataExt};
    use std::os::unix::net::UnixListener;

    for attack in ["directory", "socket", "symlink"] {
        let tmp = TempDir::new().expect("private temp root");
        let root = tmp.path().to_string_lossy().into_owned();
        // The shell blocks before exec. Prepare a real foreign socket in Rust,
        // then release the same PID into the normal server startup path.
        let mut server = TestServer::spawn_prepared(&[("TMPDIR", &root)], "read -r ready");
        let path = tmp.path().join(format!("magit-lsp-{}", server.pid()));
        let target = if attack == "symlink" {
            tmp.path().join("foreign")
        } else {
            path.clone()
        };
        std::fs::create_dir(&target).expect("foreign directory");
        std::fs::write(target.join("keep"), b"sentinel").expect("foreign sentinel");
        let foreign_socket = if attack != "directory" {
            Some(UnixListener::bind(target.join("editor.sock")).expect("foreign socket"))
        } else {
            None
        };
        if attack == "symlink" {
            symlink(&target, &path).expect("foreign symlink");
        }
        let original_dir = std::fs::symlink_metadata(&path).expect("original directory entry");
        let original_socket = foreign_socket
            .as_ref()
            .map(|_| std::fs::metadata(target.join("editor.sock")).expect("original socket"));

        server.send_raw(b"ready\n");
        let initialized = server.initialize(json!({}));
        assert!(initialized["result"]["capabilities"].is_object());
        server.shutdown_and_exit();
        let (exit, log) = server.finish_with_stderr(Duration::from_secs(20));
        // Bridge refusal is nonfatal: the LSP server remains usable.
        assert!(exit.success());
        assert!(log.contains("editor bridge unavailable"), "{attack}: {log}");
        assert!(log.contains("cannot exclusively create the editor bridge directory"));
        assert!(!log.contains("editor bridge listening"));
        let preserved_dir = std::fs::symlink_metadata(&path).expect("foreign entry remains");
        assert_eq!(preserved_dir.ino(), original_dir.ino());
        assert_eq!(preserved_dir.file_type(), original_dir.file_type());
        if attack == "symlink" {
            assert_eq!(std::fs::read_link(&path).expect("symlink remains"), target);
        }
        assert_eq!(
            std::fs::read(target.join("keep")).expect("sentinel remains"),
            b"sentinel"
        );
        if let Some(original_socket) = original_socket {
            let preserved = std::fs::metadata(target.join("editor.sock")).expect("socket remains");
            assert!(preserved.file_type().is_socket());
            assert_eq!(preserved.ino(), original_socket.ino());
        } else {
            assert!(!target.join("editor.sock").exists());
        }
        drop(foreign_socket);
    }
}

#[test]
fn a_commit_is_created_from_the_typed_message() {
    let (tmp, _repo) = staged_repo();
    assert!(
        hunk_count(&cached_diff(tmp.path())) >= 1,
        "something staged"
    );
    let repo = json!(tmp.path().to_string_lossy());
    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(20);
    let response = server.initialize(json!({}));
    assert!(
        response["result"]["capabilities"].is_object(),
        "initialize response: {response}"
    );

    let status = open_status(&mut server, 2, &repo);
    // The commit menu is where a user starts, even though this test invokes
    // the action directly.
    let _menu = invoke(&mut server, 3, &status, "open.commit", json!(null));

    let edit = start_commit(&mut server, &status);
    assert!(edit["runId"].is_string());
    assert!(edit.get("editId").is_none());
    assert!(edit.get("path").is_none());
    assert!(edit["uri"].as_str().unwrap().starts_with("file:///"));
    assert_eq!(edit["kind"], json!("commit-message"), "{edit}");
    assert_eq!(edit["title"], json!("Commit message"), "{edit}");
    assert_eq!(edit["mode"], json!("realFile"), "{edit}");
    assert_eq!(edit["scope"], json!("commit-message"), "{edit}");
    sync_draft(&mut server, &edit, "Bridge commit message\n");
    let confirmed = finish_draft(&mut server, 5, &edit);
    assert_eq!(confirmed["result"]["type"], "confirmed", "{confirmed}");

    wait_for_commit(&tmp, "Bridge commit message");
    assert_eq!(
        tmp.git(&["status", "--porcelain"]),
        "",
        "the commit leaves the worktree and index clean"
    );

    server.shutdown_and_exit();
    let exit = server.finish(Duration::from_secs(20));
    assert!(exit.success(), "server exited: {exit:?}");
}

#[test]
fn managed_draft_stdio_sync_and_checkpoint_barriers_are_exact_and_recoverable() {
    let (tmp, _) = staged_repo();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let target = tmp.path().join(".git/COMMIT_EDITMSG");
    let mut server = TestServer::spawn(&[]);
    let initialized = server.initialize(json!({}));
    assert_eq!(initialized["result"]["capabilities"]["textDocumentSync"], json!({"openClose":true,"change":1}));
    let status = open_status(&mut server, 2, &json!(tmp.path().to_string_lossy()));
    let edit = start_commit(&mut server, &status);
    assert_eq!(edit["version"], 1);
    assert_eq!(read(&target), edit["text"].as_str().unwrap());
    sync_draft(&mut server, &edit, "Earlier live\n");
    change_draft(&mut server, &edit, 9, "Chosen checkpoint\n\n# preserved comment\n");
    let unapplied = draft_request(&mut server, 10, "draft/finish", &edit, 8, Some("confirm"));
    assert!(unapplied["error"].is_object());
    write(&target, "Chosen checkpoint\n\n# preserved comment\n");
    // Save and the old common.confirm route cannot release Git or write its file.
    let bypass = invoke(&mut server, 12, &status, "common.confirm", json!({"runId":edit["runId"]}));
    assert!(!bypass["result"]["effects"].as_array().unwrap().is_empty());
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(read(&target), "Chosen checkpoint\n\n# preserved comment\n");

    for version in [8, 9] { change_draft(&mut server, &edit, version, "Invalid stale text\n"); }
    change_draft(&mut server, &edit, 10, &"x".repeat(magit_lsp::editor::draft::MAX_DRAFT_BYTES + 1));
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":edit["uri"],"version":10},"contentChanges":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"text":"Not full sync"}]
    }}));
    assert!(draft_request(&mut server, 13, "draft/save", &edit, 10, None)["error"].is_object());
    let mut wrong = edit.clone();
    wrong["runId"] = json!("wrong-run");
    assert!(draft_request(&mut server, 14, "draft/save", &wrong, 9, None)["error"].is_object());
    wrong = edit.clone();
    wrong["uri"] = json!("magit-commit://wrong-run/message");
    assert!(draft_request(&mut server, 15, "draft/save", &wrong, 9, None)["error"].is_object());
    // Barrier payloads are not a second text channel.
    server.send(json!({"jsonrpc":"2.0","id":16,"method":"draft/save","params":{"runId":edit["runId"],"uri":edit["uri"],"version":9,"text":"Forbidden"}}));
    assert!(server.response_with_id(16)["error"].is_object());

    change_draft(&mut server, &edit, 11, "Unsaved newer text must not land\n");
    assert!(draft_request(&mut server, 17, "draft/finish", &edit, 9, Some("confirm"))["error"].is_object());
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head, "detach is not close intent");
    let closed = draft_request(&mut server, 19, "draft/finish", &edit, 9, Some("normal-close"));
    assert_eq!(closed["result"], json!({"type":"confirmed","runId":edit["runId"],"uri":edit["uri"],"version":9}));
    wait_for_commit(&tmp, "Chosen checkpoint");
    assert_eq!(read(&target), "Chosen checkpoint\n\n# preserved comment\n");
    assert!(draft_request(&mut server, 20, "draft/finish", &edit, 9, Some("normal-close"))["error"].is_object());
    change_draft(&mut server, &edit, 12, "Late terminal text\n");
    assert!(draft_request(&mut server, 21, "draft/save", &edit, 12, None)["error"].is_object());
    let errors: Vec<_> = server.notifications.iter().filter(|n| n["method"] == "draft/error").collect();
    assert!(errors.len() >= 4, "stale, duplicate, oversized and range errors: {errors:?}");
    assert!(errors.iter().all(|n| n["params"]["uri"] == edit["uri"] && n["params"]["recoverable"] == true));
    assert!(errors.iter().any(|n| n["params"]["version"] == 10));
    server.shutdown_and_exit();
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[cfg(unix)]
#[test]
fn managed_draft_stdio_failed_write_retains_helper_and_selected_checkpoint() {
    use std::os::unix::fs::symlink;
    let (tmp, _) = staged_repo();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let target = tmp.path().join(".git/COMMIT_EDITMSG");
    let held = tmp.path().join(".git/held-message");
    let neighbor = tmp.path().join("neighbor.txt");
    write(&neighbor, "Neighbor must survive\n");
    let mut server = TestServer::spawn(&[]);
    server.initialize(json!({}));
    let status = open_status(&mut server, 2, &json!(tmp.path().to_string_lossy()));
    let edit = start_commit(&mut server, &status);
    sync_draft(&mut server, &edit, "Recovered checkpoint\n");
    std::fs::rename(&target, &held).unwrap();
    symlink(&neighbor, &target).unwrap();
    let failed = draft_request(&mut server, 11, "draft/finish", &edit, 2, Some("confirm"));
    assert_eq!(failed["error"]["data"]["recoverable"], true);
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(read(&neighbor), "Neighbor must survive\n");
    assert_eq!(read(&held), edit["text"].as_str().unwrap());
    std::fs::remove_file(&target).unwrap();
    std::fs::rename(&held, &target).unwrap();
    let retried = draft_request(&mut server, 12, "draft/finish", &edit, 2, Some("confirm"));
    assert_eq!(retried["result"]["type"], "confirmed", "{retried}");
    wait_for_commit(&tmp, "Recovered checkpoint");
    assert_eq!(read(&neighbor), "Neighbor must survive\n");
    server.shutdown_and_exit();
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[cfg(unix)]
#[test]
fn managed_draft_stdio_normal_close_selects_initial_template_without_save() {
    use std::os::unix::fs::PermissionsExt;
    let (tmp, _) = staged_repo();
    let hook = tmp.path().join(".git/hooks/prepare-commit-msg");
    write(&hook, "#!/bin/sh\nprintf 'Prepared initial message\\n# comment\\n' > \"$1\"\n");
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut server = TestServer::spawn(&[]);
    server.initialize(json!({}));
    let status = open_status(&mut server, 2, &json!(tmp.path().to_string_lossy()));
    let edit = start_commit(&mut server, &status);
    assert!(edit["text"].as_str().unwrap().starts_with("Prepared initial message\n"));
    sync_draft(&mut server, &edit, "Discard this unsaved message\n");
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":edit["uri"]}}}));
    let closed = draft_request(&mut server, 10, "draft/finish", &edit, 1, Some("normal-close"));
    assert!(closed["error"].is_object(), "didClose already won terminal arbitration: {closed}");
    wait_for_commit(&tmp, "Prepared initial message");
    assert_eq!(read(&tmp.path().join(".git/COMMIT_EDITMSG")), edit["text"].as_str().unwrap());
    server.shutdown_and_exit();
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn managed_draft_stdio_multiroot_and_other_connection_cannot_cross_runs() {
    let (first, _) = staged_repo();
    let (second, _) = staged_repo();
    let first_head = first.git(&["rev-parse", "HEAD"]);
    let second_head = second.git(&["rev-parse", "HEAD"]);
    let mut server = TestServer::spawn(&[]);
    server.initialize(json!({}));
    let status_a = open_status(&mut server, 2, &json!(first.path().to_string_lossy()));
    let status_b = open_status(&mut server, 3, &json!(second.path().to_string_lossy()));
    let a = start_commit(&mut server, &status_a);
    let b = start_commit(&mut server, &status_b);
    assert_ne!(a["runId"], b["runId"]);
    assert_ne!(a["uri"], b["uri"]);
    sync_draft(&mut server, &a, "First repository\n");
    sync_draft(&mut server, &b, "Second repository\n");
    let mut crossed = a.clone();
    crossed["uri"] = b["uri"].clone();
    assert!(draft_request(&mut server, 10, "draft/save", &crossed, 2, None)["error"].is_object());
    assert!(draft_request(&mut server, 11, "draft/finish", &crossed, 1, Some("normal-close"))["error"].is_object());
    let mut foreign = TestServer::spawn(&[]);
    foreign.initialize(json!({}));
    change_draft(&mut foreign, &a, 99, "Foreign connection\n");
    assert!(draft_request(&mut foreign, 10, "draft/save", &a, 2, None)["error"].is_object());
    assert!(draft_request(&mut foreign, 11, "draft/finish", &a, 1, Some("normal-close"))["error"].is_object());
    assert!(!foreign.notifications.iter().any(|n| n["method"] == "draft/error" && n["params"]["uri"] == a["uri"]));
    foreign.shutdown_and_exit();
    assert!(foreign.finish(Duration::from_secs(10)).success());
    assert_eq!(first.git(&["rev-parse", "HEAD"]), first_head);
    assert_eq!(second.git(&["rev-parse", "HEAD"]), second_head);
    assert!(finish_draft(&mut server, 12, &b)["error"].is_null());
    wait_for_commit(&second, "Second repository");
    assert_eq!(first.git(&["rev-parse", "HEAD"]), first_head);
    assert!(finish_draft(&mut server, 13, &a)["error"].is_null());
    wait_for_commit(&first, "First repository");
    assert_eq!(read(&first.path().join(".git/COMMIT_EDITMSG")), "First repository\n");
    assert_eq!(read(&second.path().join(".git/COMMIT_EDITMSG")), "Second repository\n");
    server.shutdown_and_exit();
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[cfg(unix)]
#[test]
fn managed_draft_stdio_linked_worktree_writes_only_actual_gitdir() {
    let (main, _) = staged_repo();
    let main_head = main.git(&["rev-parse", "HEAD"]);
    let main_message = read(&main.path().join(".git/COMMIT_EDITMSG"));
    let linked_parent = TempDir::new().unwrap();
    let linked = linked_parent.path().join("linked");
    main.git(&["worktree", "add", "-b", "draft-linked", linked.to_str().unwrap()]);
    write(&linked.join("f.txt"), "Linked staged content\n");
    git(&linked, &["add", "f.txt"]);
    let mut server = TestServer::spawn(&[]);
    server.initialize(json!({}));
    let status = open_status(&mut server, 2, &json!(linked.to_string_lossy()));
    let edit = start_commit(&mut server, &status);
    sync_draft(&mut server, &edit, "Linked worktree message\n");
    assert_eq!(finish_draft(&mut server, 10, &edit)["result"]["type"], "confirmed");
    let actual = magit_lsp::git::GitRepo::open(&linked).unwrap().commit_message_path().unwrap();
    assert!(linked.join(".git").is_file());
    assert_ne!(actual, main.path().join(".git/COMMIT_EDITMSG"));
    assert_eq!(read(&actual), "Linked worktree message\n");
    server.shutdown_and_exit();
    assert!(server.finish(Duration::from_secs(10)).success());
    assert_eq!(git(&linked, &["log", "-1", "--format=%B"]).trim(), "Linked worktree message");
    assert_eq!(main.git(&["rev-parse", "HEAD"]), main_head);
    assert_eq!(read(&main.path().join(".git/COMMIT_EDITMSG")), main_message);
}

#[cfg(unix)]
#[test]
fn managed_draft_production_helper_rejects_spoofs_and_duplicate_join() {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::{fs::PermissionsExt, net::UnixStream};
    let (tmp, _) = staged_repo();
    let (other, _) = staged_repo();
    let neighbor = tmp.path().join(".git/neighbor");
    write(&neighbor, "Protected neighbor\n");
    let hook = tmp.path().join(".git/hooks/pre-commit");
    write(&hook, "#!/bin/sh\nprintf '%s' \"$MAGIT_LSP_EDIT_ID\" > .git/test-run\nprintf '%s' \"$MAGIT_LSP_HELPER_TOKEN\" > .git/test-token\ntouch .git/test-ready\nwhile ! test -e .git/test-release; do sleep 0.01; done\n");
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut server = TestServer::spawn(&[]);
    server.initialize(json!({}));
    let status = open_status(&mut server, 2, &json!(tmp.path().to_string_lossy()));
    let socket = server.socket_path();
    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/invoke","params":{"viewId":status,"command":"commit.create"}}));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !tmp.path().join(".git/test-ready").exists() {
        assert!(Instant::now() < deadline, "pre-commit helper test did not start");
        thread::sleep(Duration::from_millis(10));
    }
    let run = read(&tmp.path().join(".git/test-run"));
    let token = read(&tmp.path().join(".git/test-token"));
    let hello = json!({"editId":run,"token":token,"kind":"commit-message","path":tmp.path().join(".git/COMMIT_EDITMSG")});
    let rejected = |value: &Value| {
        let mut stream = UnixStream::connect(&socket).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        writeln!(stream, "{value}").unwrap();
        let mut reply = String::new();
        BufReader::new(stream).read_line(&mut reply).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&reply).unwrap()["ok"], false);
    };
    for (key, value) in [
        ("editId", json!("wrong-run")), ("token", json!("wrong-token")),
        ("kind", json!("merge-message")), ("path", json!(neighbor)),
        ("path", json!(other.path().join(".git/COMMIT_EDITMSG"))),
    ] {
        let mut invalid = hello.clone();
        invalid[key] = value;
        rejected(&invalid);
    }
    write(&tmp.path().join(".git/test-release"), "release\n");
    let mut response = false;
    let mut edit = None;
    while !response || edit.is_none() {
        let message = server.recv();
        if message["id"] == 4 { response = true; }
        if let Some(found) = edit_document(&message) { edit = Some(found); }
    }
    let edit = edit.unwrap();
    assert_eq!(edit["runId"], run);
    assert!(!edit.to_string().contains(&token), "private helper token leaked in announcement");
    rejected(&hello); // one-shot credential cannot join a second socket
    sync_draft(&mut server, &edit, "Validated real helper\n");
    assert_eq!(finish_draft(&mut server, 10, &edit)["result"]["type"], "confirmed");
    wait_for_commit(&tmp, "Validated real helper");
    assert_eq!(read(&neighbor), "Protected neighbor\n");
    server.shutdown_and_exit();
    let (exit, log) = server.finish_with_stderr(Duration::from_secs(10));
    assert!(exit.success());
    assert!(!log.contains(&token), "private helper token leaked in logs");
}

#[test]
fn a_cancel_creates_no_commit() {
    let (tmp, _repo) = staged_repo();
    let before = tmp.git(&["status", "--porcelain"]);
    let repo = json!(tmp.path().to_string_lossy());
    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(20);
    let response = server.initialize(json!({}));
    assert!(
        response["result"]["capabilities"].is_object(),
        "initialize response: {response}"
    );

    let status = open_status(&mut server, 2, &repo);
    let _menu = invoke(&mut server, 3, &status, "open.commit", json!(null));

    let edit = start_commit(&mut server, &status);
    let edit_id = edit["runId"].as_str().expect("run id").to_string();
    sync_draft(&mut server, &edit, "Should not land\n");

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "view/invoke",
        "params": {
            "viewId": status,
            "command": "common.cancel",
            "args": {"runId": edit_id},
        },
    }));

    // The worker reports the aborted Git command asynchronously; it may be
    // queued before the cancel response, so collect both.
    let mut cancelled = None;
    let mut aborted = None;
    while cancelled.is_none() || aborted.is_none() {
        let message = server.recv();
        if message["id"] == json!(5) {
            cancelled = Some(message);
        } else if message["method"] == json!("view/effects") {
            aborted = Some(message);
        }
    }
    let cancelled = cancelled.expect("cancel response");
    assert!(
        cancelled["result"]["effects"]
            .as_array()
            .is_some_and(|effects| effects.is_empty()),
        "cancel reports nothing: {cancelled}"
    );
    let aborted = aborted.expect("the aborted commit effect");
    let effects = aborted["params"]["effects"].as_array().expect("effects");
    assert!(
        effects.iter().any(|effect| {
            effect["type"] == json!("message")
                && effect["level"] == json!("error")
                && effect["text"].as_str().is_some_and(|text| !text.is_empty())
        }),
        "an aborted commit reports an error message: {aborted}"
    );

    assert_eq!(
        tmp.git(&["log", "-1", "--format=%B"]).trim(),
        "init",
        "no new commit was created"
    );
    assert_eq!(
        tmp.git(&["status", "--porcelain"]),
        before,
        "the staged change survives a cancel"
    );

    server.shutdown_and_exit();
    let exit = server.finish(Duration::from_secs(20));
    assert!(exit.success(), "server exited: {exit:?}");
}

#[cfg(target_os = "linux")]
#[test]
fn r_edit_2_progress_cancel_during_editor_releases_once() {
    let (tmp, _repo) = staged_repo();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let staged = tmp.git(&["ls-files", "--stage", "-z"]);
    let tree = tmp.git(&["write-tree"]);
    let worktree = read(&tmp.path().join("f.txt"));
    let repo = json!(tmp.path().to_string_lossy());
    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(20);
    server.initialize(json!({}));
    let status = open_status(&mut server, 2, &repo);

    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/invoke","params":{"viewId":status,"command":"commit.create"}}));
    let mut token = None;
    let mut edit = None;
    while token.is_none() || edit.is_none() {
        let message = server.recv();
        if message["method"] == json!("$/progress") && message["params"]["value"]["kind"] == json!("begin") {
            token = Some(message["params"]["token"].clone());
        }
        if let Some(found) = edit_document(&message) { edit = Some(found); }
    }
    let edit = edit.unwrap();
    let edit_id = edit["runId"].as_str().unwrap().to_string();
    sync_draft(&mut server, &edit, "Must not commit\n");
    let socket = server.socket_path();
    let mut owned = wait_for_edit_processes(&edit_id, &socket);
    server.send(json!({"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":token.unwrap()}}));

    let mut ends = 0;
    let mut failed = false;
    while !failed || ends == 0 {
        let message = server.recv();
        if message["method"] == json!("$/progress") && message["params"]["value"]["kind"] == json!("end") { ends += 1; }
        if message["method"] == json!("view/effects") {
            failed |= message["params"]["effects"].as_array().is_some_and(|effects| effects.iter().any(|e| e["type"] == json!("message") && e["level"] == json!("error") && e["text"].as_str().is_some_and(|s| !s.is_empty())));
        }
    }
    owned.finished();
    assert_eq!(ends, 1, "progress must end exactly once");
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/invoke","params":{"viewId":status,"command":"common.confirm","args":{"runId":edit_id}}}));
    let late = server.response_with_id(5);
    assert!(late["error"].is_object() || late["result"]["effects"].as_array().is_some_and(|e| !e.is_empty()), "late confirm must be rejected: {late}");
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), staged);
    assert_eq!(tmp.git(&["write-tree"]), tree);
    assert_eq!(read(&tmp.path().join("f.txt")), worktree);

    let edit = start_commit(&mut server, &status);
    sync_draft(&mut server, &edit, "Follow-up commit\n");
    let confirmed = finish_draft(&mut server, 6, &edit);
    assert_eq!(confirmed["result"]["type"], "confirmed", "{confirmed}");
    wait_for_commit(&tmp, "Follow-up commit");
    server.shutdown_and_exit();
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[cfg(target_os = "linux")]
#[test]
fn r_edit_2_progress_cancel_during_commit_msg_hook() {
    let (tmp, _repo) = staged_repo();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let staged = tmp.git(&["ls-files", "--stage", "-z"]);
    let tree = tmp.git(&["write-tree"]);
    let worktree = read(&tmp.path().join("f.txt"));
    let ready = tmp.path().join("hook.ready");
    let child_pid = tmp.path().join("hook-child.pid");
    let hook = tmp.path().join(".git/hooks/commit-msg");
    std::fs::write(&hook, format!(
        "#!/bin/sh\ntrap '' TERM\n(sleep 1000) &\necho $! > '{}'\necho ready > '{}'\nwait\n",
        child_pid.display(), ready.display()
    )).expect("write commit-msg hook");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let repo = json!(tmp.path().to_string_lossy());
    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(20);
    server.initialize(json!({}));
    let status = open_status(&mut server, 2, &repo);
    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/invoke","params":{"viewId":status,"command":"commit.create"}}));
    let mut token = None;
    let mut edit = None;
    while token.is_none() || edit.is_none() {
        let message = server.recv();
        if message["method"] == json!("$/progress") && message["params"]["value"]["kind"] == json!("begin") { token = Some(message["params"]["token"].clone()); }
        if let Some(found) = edit_document(&message) { edit = Some(found); }
    }
    let edit = edit.unwrap();
    let edit_id = edit["runId"].as_str().unwrap().to_string();
    sync_draft(&mut server, &edit, "Must not commit\n");
    let confirmed = finish_draft(&mut server, 5, &edit);
    assert_eq!(confirmed["result"]["type"], "confirmed", "{confirmed}");
    let socket = server.socket_path();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.exists() { assert!(Instant::now() < deadline, "commit-msg hook did not become ready"); thread::sleep(Duration::from_millis(20)); }
    let processes = edit_processes(&edit_id, &socket).expect("/proc inspection");
    assert!(!processes.is_empty(), "commit processes carry edit identity");
    let mut owned = OwnedEdit { edit_id: edit_id.clone(), socket: socket.clone(), pids: processes.into_iter().map(|p| p.pid).collect() };
    let child: u32 = read(&child_pid).trim().parse().expect("hook child pid");
    let token = token.unwrap();
    let cancelled_at = Instant::now();
    server.send(json!({"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":token}}));
    server.recv_timeout = Duration::from_millis(500);
    loop {
        let message = server.recv();
        assert_ne!(message["params"]["value"]["kind"], "end", "acknowledgement must precede terminal cleanup");
        if message["method"] == "$/progress" && message["params"]["token"] == token && message["params"]["value"]["kind"] == "report" {
            assert_eq!(message["params"]["value"]["message"], "Cancelling…");
            assert_eq!(message["params"]["value"]["cancellable"], false);
            break;
        }
    }
    assert!(cancelled_at.elapsed() < Duration::from_millis(500), "cancellation intent was not promptly acknowledged");
    // Duplicate/foreign requests must not restart cleanup or release the repo.
    server.send(json!({"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":token}}));
    server.send(json!({"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":"not-a-run"}}));
    server.send(json!({"jsonrpc":"2.0","id":55,"method":"view/invoke","params":{"viewId":status,"command":"status.stage-all"}}));
    let refused = server.response_with_id(55);
    assert!(refused["result"]["effects"][0]["text"].as_str().is_some_and(|s| s.contains("Cancelling")), "mutation overlapped cleanup: {refused}");
    assert!(cancelled_at.elapsed() < Duration::from_secs(1), "refusal waited for cleanup");
    server.recv_timeout = Duration::from_secs(20);
    let mut ends = 0;
    let mut failed = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !failed || ends == 0 {
        assert!(Instant::now() < deadline, "cancellation did not finish promptly");
        let message = server.recv();
        if message["method"] == "$/progress" && message["params"]["token"] == token {
            assert_ne!(message["params"]["value"]["kind"], "report", "duplicate cancellation was acknowledged twice");
            if message["params"]["value"]["kind"] == "end" { ends += 1; }
        }
        if message["method"] == json!("view/effects") { failed |= message["params"]["effects"].as_array().is_some_and(|effects| effects.iter().any(|e| e["type"] == json!("message") && e["level"] == json!("error") && e["text"].as_str().is_some_and(|s| !s.is_empty()))); }
    }
    owned.finished();
    assert_eq!(ends, 1, "progress ends exactly once");
    assert!(cancelled_at.elapsed() >= Duration::from_secs(2), "resistant hook lost its full TERM grace");
    server.notifications.clear();
    server.send(json!({"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":token}}));
    server.send(json!({"jsonrpc":"2.0","id":56,"method":"view/resync","params":{"viewId":status}}));
    assert!(server.response_with_id(56).get("error").is_none());
    server.drain_notifications();
    assert!(!server.notifications.iter().any(|m| m["method"] == "$/progress" && m["params"]["token"] == token), "late cancellation resurrected an ended token");
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), staged);
    assert_eq!(tmp.git(&["write-tree"]), tree);
    assert_eq!(read(&tmp.path().join("f.txt")), worktree);
    let deadline = Instant::now() + Duration::from_secs(5);
    while Path::new(&format!("/proc/{child}/stat")).exists() { assert!(Instant::now() < deadline, "hook descendant survived cancellation"); thread::sleep(Duration::from_millis(20)); }
    std::fs::remove_file(&hook).expect("remove blocking commit-msg hook");
    let edit = start_commit(&mut server, &status);
    sync_draft(&mut server, &edit, "Follow-up commit\n");
    let confirmed = finish_draft(&mut server, 6, &edit);
    assert_eq!(confirmed["result"]["type"], "confirmed", "{confirmed}");
    wait_for_commit(&tmp, "Follow-up commit");
    server.shutdown_and_exit();
    assert!(server.finish(Duration::from_secs(10)).success());
}

/// An early cancellation survives repository discovery and targets the actual
/// mutation, even when it arrives before the worker has spawned any child.
#[cfg(unix)]
#[test]
fn early_commit_cancellation_does_not_kill_repository_discovery() {
    let (tmp, _repo) = staged_repo();
    let before = tmp.git(&["rev-parse", "HEAD"]);
    let cancel = magit_lsp::runner::CancelHandle::new();
    let run = magit_lsp::editor::registry().create(tmp.path().to_str().unwrap(), "early-test").unwrap();
    cancel.cancel();
    let error = magit_lsp::runner::with_cancel(Some(cancel), || {
        magit_lsp::editor::commit(tmp.path().to_str().unwrap(), &run, &tmp.path().join("unused.sock"))
    }).expect_err("the pre-cancelled commit must fail");
    magit_lsp::editor::registry().remove(&run);
    assert_eq!(error.git.expect("a structured Git failure").subcommand, "commit");
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), before);
}

#[cfg(target_os = "linux")]
#[test]
fn r_edit_2_recorded_commit_wins_finalizing_and_completed_cancellation() {
    use std::os::unix::fs::PermissionsExt;

    for finalizing in [true, false] {
        let (tmp, _) = staged_repo();
        let head = tmp.git(&["rev-parse", "HEAD"]);
        let tree = tmp.git(&["write-tree"]);
        let worktree = read(&tmp.path().join("f.txt"));
        let ready = tmp.path().join(".git/post-ready");
        let hook = tmp.path().join(".git/hooks/post-commit");
        if finalizing {
            write(&hook, &format!("#!/bin/sh\ntouch '{}'\nwhile :; do sleep 0.02; done\n", ready.display()));
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let mut server = TestServer::spawn(&[]);
        server.initialize(json!({}));
        let status = open_status(&mut server, 2, &json!(tmp.path().to_string_lossy()));
        server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/invoke","params":{"viewId":status,"command":"commit.create"}}));
        let mut token = None;
        let mut edit = None;
        while token.is_none() || edit.is_none() {
            let message = server.recv();
            if message["method"] == "$/progress" && message["params"]["value"]["kind"] == "begin" {
                token = Some(message["params"]["token"].clone());
            }
            if let Some(found) = edit_document(&message) { edit = Some(found); }
        }
        let token = token.unwrap();
        let edit = edit.unwrap();
        sync_draft(&mut server, &edit, "Recorded before cancel\n");
        let confirmed = finish_draft(&mut server, 5, &edit);
        assert!(confirmed.get("error").is_none(), "{confirmed}");
        let mut observed = std::mem::take(&mut server.notifications);
        if finalizing {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !ready.exists() {
                assert!(Instant::now() < deadline, "post-commit did not start");
                thread::sleep(Duration::from_millis(10));
            }
            assert_ne!(tmp.git(&["rev-parse", "HEAD"]), head);
            server.send(json!({"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":token}}));
        }
        while !observed.iter().any(|m| m["method"] == "$/progress" && m["params"]["token"] == token && m["params"]["value"]["kind"] == "end") {
            observed.push(server.recv());
        }
        // End is terminal: repeated confirms and cancellation cannot resurrect
        // the edit or its progress, nor overwrite the successful outcome.
        server.send(json!({"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":token}}));
        let late = draft_request(&mut server, 6, "draft/finish", &edit, 2, Some("confirm"));
        assert!(late["error"].is_object(), "ended edit accepted a duplicate finish: {late}");
        server.send(json!({"jsonrpc":"2.0","id":7,"method":"view/resync","params":{"viewId":status}}));
        assert!(server.response_with_id(7).get("error").is_none());
        observed.extend(std::mem::take(&mut server.notifications));
        observed.extend(server.drain_notifications());
        let progress: Vec<_> = observed.iter().filter(|m| m["method"] == "$/progress" && m["params"]["token"] == token).collect();
        assert_eq!(progress.iter().filter(|m| m["params"]["value"]["kind"] == "end").count(), 1);
        assert_eq!(progress.iter().filter(|m| m["params"]["value"]["kind"] == "report").count(), usize::from(finalizing));
        let effects: Vec<_> = observed.iter().filter(|m| m["method"] == "view/effects")
            .flat_map(|m| m["params"]["effects"].as_array().into_iter().flatten()).collect();
        assert_eq!(effects.iter().filter(|e| e["text"].as_str().is_some_and(|s| s.starts_with("Committed:"))).count(), 1, "exactly one truthful completion: {effects:?}");
        assert!(!effects.iter().any(|e| e["level"] == "error"), "recorded commit reported failure: {effects:?}");
        assert_eq!(tmp.git(&["rev-parse", "HEAD^"]), head);
        assert_eq!(tmp.git(&["rev-parse", "HEAD^{tree}"]), tree);
        assert_eq!(tmp.git(&["write-tree"]), tree);
        assert!(tmp.git(&["diff", "--cached", "--binary"]).is_empty());
        assert_eq!(read(&tmp.path().join("f.txt")), worktree);
        if finalizing { std::fs::remove_file(hook).unwrap(); }
        write(&tmp.path().join("f.txt"), "Follow-up\n");
        tmp.git(&["add", "f.txt"]);
        let next = start_commit(&mut server, &status);
        sync_draft(&mut server, &next, "Follow-up commit\n");
        assert!(finish_draft(&mut server, 8, &next).get("error").is_none());
        wait_for_commit(&tmp, "Follow-up commit");
        server.shutdown_and_exit();
        assert!(server.finish(Duration::from_secs(10)).success());
    }
}

/// The processes holding a pending edit: Git's commit and its editor helper.
///
/// Both carry the edit id the server injected into their environment, so they
/// are exactly the processes this test owns through its own server. Git may
/// also run the helper through `sh -c`, which execs and keeps the same pid.
struct OwnedEdit {
    edit_id: String,
    socket: PathBuf,
    pids: Vec<u32>,
}

impl OwnedEdit {
    /// Release the ownership once every process ended by itself.
    fn finished(&mut self) {
        self.pids.clear();
    }
}

impl Drop for OwnedEdit {
    fn drop(&mut self) {
        if self.pids.is_empty() {
            // Every process ended by itself before the assertions ran.
            return;
        }
        for pid in std::mem::take(&mut self.pids) {
            // Re-verify the identity before killing: a pid may have been
            // reused by an unrelated process since it was recorded.
            let environ = std::fs::read(format!("/proc/{pid}/environ"));
            if matches!(&environ, Ok(bytes) if owned_environ(bytes, &self.edit_id, &self.socket)) {
                kill_pid(pid);
            }
        }
        // A failed run must never leave a Git commit or a helper waiting on a
        // dead socket behind, so wait until every owned process is really gone.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match edit_processes(&self.edit_id, &self.socket) {
                Ok(processes) if processes.is_empty() => return,
                Ok(_) => {}
                Err(warning) => {
                    eprintln!("warning: {warning}");
                    return;
                }
            }
            if Instant::now() >= deadline {
                eprintln!(
                    "warning: owned processes for edit {:?} survived the cleanup guard",
                    self.edit_id
                );
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}

/// Kill one owned process; only this test's own Git and helper need it.
fn kill_pid(pid: u32) {
    let _ = Command::new("kill").arg(pid.to_string()).status();
}

/// The environments of the processes carrying the edit id and socket path.
///
/// The socket path is part of the match: edit ids restart at `e1` in every
/// server process, so an id alone cannot distinguish this test's Git and
/// helper processes from another server's. A missing or unreadable `/proc`
/// is a hard error, never a silent pass.
fn owned_environ(environ: &[u8], edit_id: &str, socket: &Path) -> bool {
    let id_marker = format!("MAGIT_LSP_EDIT_ID={edit_id}\0");
    let socket_marker = format!("MAGIT_LSP_SOCKET={}\0", socket.to_string_lossy());
    let (id_marker, socket_marker) = (id_marker.as_bytes(), socket_marker.as_bytes());
    let carries = |marker: &[u8]| environ.windows(marker.len()).any(|w| w == marker);
    carries(id_marker) && carries(socket_marker)
}

/// A live process carrying the markers: its pid and its command line.
struct OwnedProcess {
    pid: u32,
    command: String,
}

/// The processes whose environment carries the edit id and socket path.
fn edit_processes(edit_id: &str, socket: &Path) -> Result<Vec<OwnedProcess>, String> {
    let mut processes = Vec::new();
    let entries =
        std::fs::read_dir("/proc").map_err(|error| format!("cannot inspect /proc: {error}"))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(environ) = std::fs::read(entry.path().join("environ")) else {
            continue;
        };
        if owned_environ(&environ, edit_id, socket) {
            let command = std::fs::read_to_string(entry.path().join("cmdline"))
                .map(|command| command.replace('\0', " "))
                .unwrap_or_default();
            processes.push(OwnedProcess {
                pid: name.parse().expect("a numeric pid"),
                command,
            });
        }
    }
    Ok(processes)
}

/// Wait until both the Git commit and its editor helper are running.
fn wait_for_edit_processes(edit_id: &str, socket: &Path) -> OwnedEdit {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let processes = edit_processes(edit_id, socket).expect("/proc inspection");
        let has_git = processes.iter().any(|process| {
            process.command.contains(" commit ") || process.command.ends_with(" commit")
        });
        let has_helper = processes
            .iter()
            .any(|process| process.command.contains("--editor-helper"));
        if has_git && has_helper {
            return OwnedEdit {
                edit_id: edit_id.to_string(),
                socket: socket.to_path_buf(),
                pids: processes.into_iter().map(|p| p.pid).collect(),
            };
        }
        assert!(
            Instant::now() < deadline,
            "no Git commit was running for the edit {edit_id:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

/// The client leaving (stdin closed) must abort the pending edit so Git never
/// hangs: the server ends on its own, the Git commit and its editor helper end
/// by themselves, no commit was created, and the staged change survives (§3.10
/// step 7).
///
/// `close_stdin` is the same EOF every real client produces, so this observes
/// the natural shutdown rather than a killed process. The owned Git commit and
/// its helper are watched by pid, so a server exit alone cannot count as the
/// abort, and the assertion happens before any cleanup could fake a pass.
#[test]
fn a_client_disconnect_aborts_the_edit_so_git_never_hangs() {
    let (tmp, _repo) = staged_repo();
    let before = tmp.git(&["status", "--porcelain"]);
    let before_head = tmp.git(&["rev-parse", "HEAD"]);
    let repo = json!(tmp.path().to_string_lossy());
    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(20);
    let response = server.initialize(json!({}));
    let socket = server.socket_path();
    assert!(
        response["result"]["capabilities"].is_object(),
        "initialize response: {response}"
    );

    let status = open_status(&mut server, 2, &repo);
    let _menu = invoke(&mut server, 3, &status, "open.commit", json!(null));

    let edit = start_commit(&mut server, &status);
    let edit_id = edit["runId"].as_str().expect("run id").to_string();
    sync_draft(&mut server, &edit, "Never confirmed\n");
    write(&tmp.path().join(".git/COMMIT_EDITMSG"), "Never confirmed\n");

    let mut owned = wait_for_edit_processes(&edit_id, &socket);
    assert!(
        !owned.pids.is_empty(),
        "no Git commit was running for the edit {edit_id:?}"
    );

    // The client is gone: close its input and let the server end by itself.
    server.close_stdin();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let server_gone = server.try_wait().is_some();
        let git_gone = edit_processes(&edit_id, &socket)
            .expect("/proc inspection")
            .is_empty();
        if server_gone && git_gone {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the Git commit or its editor helper outlived the disconnect"
        );
        thread::sleep(Duration::from_millis(20));
    }
    // Every process ended by itself; the guard must not kill anything now.
    owned.finished();

    assert_eq!(
        tmp.git(&["log", "-1", "--format=%B"]).trim(),
        "init",
        "no new commit was created"
    );
    assert_eq!(
        tmp.git(&["rev-parse", "HEAD"]),
        before_head,
        "HEAD did not move"
    );
    assert_eq!(
        tmp.git(&["status", "--porcelain"]),
        before,
        "the staged change survives the disconnect"
    );

    let exit = server.finish(Duration::from_secs(20));
    assert!(exit.success(), "server exited: {exit:?}");
}

/// The bridge socket path, from a server log file at `path`.
fn socket_path_in_log(path: &Path) -> PathBuf {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(log) = std::fs::read_to_string(path) {
            if let Some(socket) = log.lines().find_map(|line| {
                line.split_once("editor bridge listening socket=")
                    .map(|(_, socket)| socket.trim())
            }) {
                return PathBuf::from(socket);
            }
        }
        assert!(
            Instant::now() < deadline,
            "the server did not report a bridge socket in its log"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

/// Ask Git's question through the bridge and return the helper's output.
fn answer_askpass(server: &mut TestServer, socket: &Path, question: &str, answer: &str) -> Output {
    let helper = Command::new(env!("CARGO_BIN_EXE_magit-lsp"))
        .arg("--askpass")
        .arg(question)
        .env("MAGIT_LSP_SOCKET", socket)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the askpass helper");

    let prompt = server.prompt();
    let params = &prompt["params"];
    assert_eq!(params["kind"], json!("text"), "the prompt kind");
    assert_eq!(params["secret"], json!(true), "the prompt is secret");
    assert!(
        params["title"].as_str() == Some(question),
        "the prompt asks Git's question"
    );
    let prompt_id = params["promptId"].as_str().expect("prompt id").to_string();
    // A text answer is the bare string on the wire.
    server.respond_prompt(90, &prompt_id, false, json!(answer));

    helper.wait_with_output().expect("the helper finished")
}

/// Assert the helper printed exactly the answer and nothing to stderr.
///
/// The diagnostics never include the captured streams: after a successful
/// round trip the answer is in stdout by design.
fn assert_helper_kept_the_answer(output: &Output, answer: &str) {
    assert!(output.status.success(), "the askpass helper failed");
    assert!(
        String::from_utf8_lossy(&output.stdout).trim() == answer,
        "the askpass helper did not print the answered secret"
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains(answer),
        "the askpass helper wrote the answer to stderr"
    );
}

/// Assert `log` proves the traced askpass path ran and omits the answer.
fn assert_log_kept_the_answer(sink: &str, log: &str, answer: &str) {
    assert!(
        log.contains("editor bridge: askpass"),
        "the {sink} log is missing the askpass trace; logging was not active"
    );
    assert!(
        !log.contains(answer),
        "the {sink} log contains the askpass answer"
    );
}

/// Run the askpass helper against the running server's bridge.
///
/// The helper is what Git runs when it needs a password: it asks the server
/// and prints the secret on stdout. The socket comes from the server's
/// environment, so the helper finds the bridge the same way Git would.
#[test]
fn the_askpass_helper_returns_the_secret_to_whoever_asked_for_it() {
    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(20);
    let response = server.initialize(json!({}));
    assert!(
        response["result"]["capabilities"].is_object(),
        "the server did not initialize"
    );
    let socket = server.socket_path();

    let output = answer_askpass(&mut server, &socket, ASKPASS_QUESTION, ASKPASS_ANSWER);
    assert_helper_kept_the_answer(&output, ASKPASS_ANSWER);

    server.shutdown_and_exit();
    let exit = server.finish(Duration::from_secs(20));
    assert!(exit.success(), "server exited: {exit:?}");
}

/// The server's trace log on stderr must never receive the askpass answer.
#[test]
fn the_askpass_answer_is_never_logged_to_stderr() {
    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(20);
    let response = server.initialize(json!({}));
    assert!(
        response["result"]["capabilities"].is_object(),
        "the server did not initialize"
    );
    let socket = server.socket_path();

    let output = answer_askpass(&mut server, &socket, ASKPASS_QUESTION, ASKPASS_ANSWER);
    assert_helper_kept_the_answer(&output, ASKPASS_ANSWER);

    // Drain stderr fully: the trace that proves the round trip was logged may
    // still be in flight when the helper exits.
    server.shutdown_and_exit();
    let (exit, log) = server.finish_with_stderr(Duration::from_secs(20));
    assert!(exit.success(), "the server exited unsuccessfully");
    assert_log_kept_the_answer("stderr", &log, ASKPASS_ANSWER);
}

/// The same guard for trace logging redirected to `MAGIT_LSP_LOG_FILE`.
///
/// The socket comes from that file as well, so this exercises the file sink
/// end to end instead of relying on stderr.
#[test]
fn the_askpass_answer_is_never_logged_to_a_log_file() {
    let dir = TempDir::new().expect("temp dir for the log");
    let log_path = dir.path().join("magit-lsp.log");
    let log_path_arg = log_path.to_string_lossy().into_owned();
    let mut server = TestServer::spawn(&[(magit_lsp::logging::FILE_ENV, &log_path_arg)]);
    server.recv_timeout = Duration::from_secs(20);
    let response = server.initialize(json!({}));
    assert!(
        response["result"]["capabilities"].is_object(),
        "the server did not initialize"
    );
    let socket = socket_path_in_log(&log_path);

    let output = answer_askpass(&mut server, &socket, ASKPASS_QUESTION, ASKPASS_ANSWER);
    assert_helper_kept_the_answer(&output, ASKPASS_ANSWER);

    server.shutdown_and_exit();
    let (exit, stderr) = server.finish_with_stderr(Duration::from_secs(20));
    assert!(exit.success(), "the server exited unsuccessfully");

    let log = std::fs::read_to_string(&log_path).expect("read the server log");
    assert_log_kept_the_answer("file", &log, ASKPASS_ANSWER);
    assert!(
        !stderr.contains(ASKPASS_ANSWER),
        "the server wrote the askpass answer to stderr"
    );
}

#[test]
fn a_cancelled_askpass_prompt_makes_the_helper_fail() {
    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(20);
    let response = server.initialize(json!({}));
    assert!(
        response["result"]["capabilities"].is_object(),
        "initialize response: {response}"
    );
    let socket = server.socket_path();

    let helper = Command::new(env!("CARGO_BIN_EXE_magit-lsp"))
        .arg("--askpass")
        .arg("Password:")
        .env("MAGIT_LSP_SOCKET", &socket)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the askpass helper");

    let prompt = server.prompt();
    let prompt_id = prompt["params"]["promptId"]
        .as_str()
        .expect("prompt id")
        .to_string();
    server.respond_prompt(91, &prompt_id, true, Value::Null);

    let output = helper.wait_with_output().expect("the helper finished");
    assert!(
        !output.status.success(),
        "a cancelled prompt must fail the helper: {output:?}"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "",
        "a cancelled prompt prints no secret"
    );

    server.shutdown_and_exit();
    let exit = server.finish(Duration::from_secs(20));
    assert!(exit.success(), "server exited: {exit:?}");
}
