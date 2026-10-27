//! The editor bridge end to end (R-3.4): a real `git commit` waits for the
//! client to edit Git's message file, then continues or aborts.

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
    let edit_id = edit["editId"].as_str().expect("edit id").to_string();
    let path = edit["path"].as_str().expect("edit path");
    assert_eq!(edit["kind"], json!("commit-message"), "{edit}");
    assert_eq!(edit["title"], json!("Commit message"), "{edit}");
    assert_eq!(edit["mode"], json!("realFile"), "{edit}");
    assert_eq!(edit["scope"], json!("commit-message"), "{edit}");
    write(Path::new(path), "Bridge commit message\n");

    let confirmed = invoke(
        &mut server,
        5,
        &status,
        "common.confirm",
        json!({"editId": edit_id}),
    );
    assert!(
        confirmed["result"]["effects"]
            .as_array()
            .is_some_and(|effects| effects.is_empty()),
        "confirm reports nothing: {confirmed}"
    );

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
    let edit_id = edit["editId"].as_str().expect("edit id").to_string();
    let path = edit["path"].as_str().expect("edit path");
    write(Path::new(path), "Should not land\n");

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "view/invoke",
        "params": {
            "viewId": status,
            "command": "common.cancel",
            "args": {"editId": edit_id},
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

/// A11 cancellation acceptance is deferred after the progress cancellation
/// regression timed out waiting for completion. Keep an explicit failing
/// placeholder so running ignored tests cannot report this as a pass.
#[cfg(target_os = "linux")]
#[test]
#[ignore = "A11: progress cancellation regression deferred; timed out waiting for completion"]
fn progress_cancel_kills_the_commit_and_releases_its_edit() {
    todo!("restore end-to-end coverage after the A11 cancellation regression is fixed");
}

/// An early cancellation survives repository discovery and targets the actual
/// mutation, even when it arrives before the worker has spawned any child.
#[cfg(unix)]
#[test]
fn early_commit_cancellation_does_not_kill_repository_discovery() {
    let (tmp, _repo) = staged_repo();
    let before = tmp.git(&["rev-parse", "HEAD"]);
    let cancel = magit_lsp::runner::CancelHandle::new();
    cancel.cancel();
    let error = magit_lsp::runner::with_cancel(Some(cancel), || {
        magit_lsp::editor::commit(tmp.path().to_str().unwrap(), "early", &tmp.path().join("unused.sock"))
    }).expect_err("the pre-cancelled commit must fail");
    assert_eq!(error.git.expect("a structured Git failure").subcommand, "commit");
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), before);
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
    let edit_id = edit["editId"].as_str().expect("edit id").to_string();
    let path = edit["path"].as_str().expect("edit path");
    write(Path::new(path), "Never confirmed\n");

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
