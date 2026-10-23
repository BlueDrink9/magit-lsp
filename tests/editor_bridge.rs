//! The editor bridge end to end (R-3.4): a real `git commit` waits for the
//! client to edit Git's message file, then continues or aborts.

mod common;

use common::*;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// A running server plus its framed stdout stream.
struct TestServer {
    child: Child,
    stdin: Option<ChildStdin>,
    responses: Receiver<Result<Value, String>>,
    reader: JoinHandle<()>,
    stderr: Arc<Mutex<String>>,
}

impl TestServer {
    fn spawn() -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_magit-lsp"));
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env(magit_lsp::logging::LEVEL_ENV, "trace")
            .env_remove(magit_lsp::logging::FILE_ENV);

        let mut child = command.spawn().expect("spawn magit-lsp");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = child.stdout.take().expect("child stdout");
        let stderr = child.stderr.take().expect("child stderr");

        let (tx, responses) = mpsc::channel();
        let reader = thread::spawn(move || read_loop(stdout, tx));
        let log = Arc::new(Mutex::new(String::new()));
        drain_stderr(stderr, Arc::clone(&log));

        Self {
            child,
            stdin: Some(stdin),
            responses,
            reader,
            stderr: log,
        }
    }

    fn send(&mut self, message: Value) {
        let stdin = self.stdin.as_mut().expect("server stdin open");
        stdin
            .write_all(&frame(message))
            .expect("write to server");
        stdin.flush().expect("flush server stdin");
    }

    fn recv(&mut self) -> Value {
        match self.responses.recv_timeout(Duration::from_secs(20)) {
            Ok(Ok(message)) => message,
            Ok(Err(error)) => panic!("stdout is not clean LSP framing: {error}"),
            Err(error) => {
                let log = self
                    .stderr
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let tail: Vec<&str> = log.lines().rev().take(40).collect();
                let status = self.child.try_wait();
                panic!(
                    "no response from server: {error}\nserver exit: {status:?}\nserver stderr:\n{}",
                    tail.into_iter().rev().collect::<Vec<_>>().join("\n")
                )
            }
        }
    }

    fn response_with_id(&mut self, id: i64) -> Value {
        loop {
            let message = self.recv();
            if message["id"] == json!(id) {
                return message;
            }
        }
    }

    fn initialize(&mut self) {
        self.send(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": null,
                "rootUri": null,
                "capabilities": {},
            },
        }));
        self.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
        let response = self.response_with_id(1);
        assert!(
            response["result"]["capabilities"].is_object(),
            "initialize response: {response}"
        );
    }

    fn shutdown_and_exit(&mut self) {
        self.send(json!({"jsonrpc": "2.0", "id": 99, "method": "shutdown", "params": null}));
        let response = self.response_with_id(99);
        assert_eq!(response["result"], Value::Null);
        self.send(json!({"jsonrpc": "2.0", "method": "exit", "params": null}));
    }

    fn finish(mut self, timeout: Duration) -> ExitStatus {
        self.stdin.take();
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.child.try_wait().expect("poll server") {
                self.reader.join().expect("reader thread");
                return status;
            }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                panic!("server did not exit within {timeout:?}");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// The bridge socket path, from the server's startup log.
    fn socket_path(&mut self) -> PathBuf {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let log = self
                .stderr
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
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
        assert!(
            response.get("error").is_none(),
            "the prompt answer was rejected: {response}"
        );
        assert_eq!(response["result"], Value::Null, "{response}");
    }
}

fn frame(message: Value) -> Vec<u8> {
    let body = serde_json::to_vec(&message).expect("serialise message");
    let mut bytes = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    bytes.extend_from_slice(&body);
    bytes
}

fn drain_stderr(mut stderr: std::process::ChildStderr, log: Arc<Mutex<String>>) {
    thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        loop {
            match stderr.read(&mut buffer) {
                Ok(0) | Err(_) => return,
                Ok(read) => {
                    let mut log = log.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    log.push_str(&String::from_utf8_lossy(&buffer[..read]));
                    if log.len() > 200_000 {
                        log.drain(..100_000);
                    }
                }
            }
        }
    });
}

fn read_loop(stdout: ChildStdout, tx: mpsc::Sender<Result<Value, String>>) {
    let mut reader = BufReader::new(stdout);
    loop {
        match read_frame(&mut reader) {
            Ok(Some(message)) => {
                if tx.send(Ok(message)).is_err() {
                    return;
                }
            }
            Ok(None) => return,
            Err(error) => {
                let _ = tx.send(Err(error));
                return;
            }
        }
    }
}

fn read_frame(reader: &mut impl BufRead) -> Result<Option<Value>, String> {
    let mut content_length = None;
    let mut saw_header = false;
    loop {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .map_err(|error| format!("read header: {error}"))?;
        if read == 0 {
            return if saw_header {
                Err("EOF in the middle of a frame header".to_string())
            } else {
                Ok(None)
            };
        }
        saw_header = true;
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = Some(
                    value
                        .trim()
                        .parse::<usize>()
                        .map_err(|error| format!("bad Content-Length: {error}"))?,
                );
            }
        }
    }

    let length = content_length.ok_or("frame has no Content-Length header")?;
    let mut body = vec![0; length];
    reader
        .read_exact(&mut body)
        .map_err(|error| format!("read frame body: {error}"))?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|error| format!("frame body is not JSON: {error}"))
}

/// Open the status view and consume its initial render and keymap.
fn open_status(server: &mut TestServer, id: i64, repo: &Value) -> String {
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
    let view_id = opened["result"]["id"]
        .as_str()
        .expect("view id")
        .to_string();
    let render = server.recv();
    assert_eq!(render["method"], json!("view/render"), "{render}");
    let keymap = server.recv();
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
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "view/invoke",
        "params": {"viewId": status, "command": "commit.create"},
    }));
    let mut response = None;
    let mut edit = None;
    while response.is_none() || edit.is_none() {
        let message = server.recv();
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

#[test]
fn a_commit_is_created_from_the_typed_message() {
    let (tmp, _repo) = staged_repo();
    assert!(hunk_count(&cached_diff(tmp.path())) >= 1, "something staged");
    let repo = json!(tmp.path().to_string_lossy());
    let mut server = TestServer::spawn();
    server.initialize();

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
    let mut server = TestServer::spawn();
    server.initialize();

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

/// Run the askpass helper against the running server's bridge.
///
/// The helper is what Git runs when it needs a password: it asks the server
/// and prints the secret on stdout. The socket comes from the server's
/// environment, so the helper finds the bridge the same way Git would.
#[test]
fn the_askpass_helper_returns_the_secret_to_whoever_asked_for_it() {
    let mut server = TestServer::spawn();
    server.initialize();
    let socket = server.socket_path();

    let helper = Command::new(env!("CARGO_BIN_EXE_magit-lsp"))
        .arg("--askpass")
        .arg("Password for 'git@example.com':")
        .env("MAGIT_LSP_SOCKET", &socket)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the askpass helper");

    // The server turns Git's question into a secret text prompt.
    let prompt = server.prompt();
    let params = &prompt["params"];
    assert_eq!(params["kind"], json!("text"), "{prompt}");
    assert_eq!(params["secret"], json!(true), "{prompt}");
    assert_eq!(
        params["title"],
        json!("Password for 'git@example.com':"),
        "{prompt}"
    );

    let prompt_id = params["promptId"].as_str().expect("prompt id").to_string();
    // A text answer is the bare string on the wire.
    server.respond_prompt(90, &prompt_id, false, json!("s3cret"));

    let output = helper.wait_with_output().expect("the helper finished");
    assert!(output.status.success(), "the helper exited: {output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "s3cret",
        "the helper printed the secret"
    );

    server.shutdown_and_exit();
    let exit = server.finish(Duration::from_secs(20));
    assert!(exit.success(), "server exited: {exit:?}");
}

#[test]
fn a_cancelled_askpass_prompt_makes_the_helper_fail() {
    let mut server = TestServer::spawn();
    server.initialize();
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
