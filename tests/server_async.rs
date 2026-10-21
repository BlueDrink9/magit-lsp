//! The real server over real pipes: framing, pipelining, concurrency, ordering.

mod common;

use common::*;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// A running server plus its framed stdout stream.
struct TestServer {
    child: Child,
    stdin: Option<ChildStdin>,
    responses: Receiver<Result<Value, String>>,
    reader: JoinHandle<()>,
}

impl TestServer {
    /// Spawn the real binary with logging on stderr and optional extra env vars.
    fn spawn(extra_env: &[(&str, &str)]) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_magit-lsp"));
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env(magit_lsp::logging::LEVEL_ENV, "trace")
            .env_remove(magit_lsp::logging::FILE_ENV);
        for (key, value) in extra_env {
            command.env(key, value);
        }

        let mut child = command.spawn().expect("spawn magit-lsp");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = child.stdout.take().expect("child stdout");

        let (tx, responses) = mpsc::channel();
        let reader = thread::spawn(move || read_loop(stdout, tx));

        Self {
            child,
            stdin: Some(stdin),
            responses,
            reader,
        }
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    fn send(&mut self, message: Value) {
        self.send_raw(&frame(message));
    }

    fn send_raw(&mut self, bytes: &[u8]) {
        let stdin = self.stdin.as_mut().expect("server stdin open");
        stdin.write_all(bytes).expect("write to server");
        stdin.flush().expect("flush server stdin");
    }

    fn recv(&mut self) -> Value {
        match self.responses.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(message)) => message,
            Ok(Err(error)) => panic!("stdout is not clean LSP framing: {error}"),
            Err(error) => panic!("no response from server: {error}"),
        }
    }

    /// Read until the response carrying `id`, failing if the stream ends.
    fn response_with_id(&mut self, id: i64) -> Value {
        loop {
            let message = self.recv();
            if message["id"] == json!(id) {
                return message;
            }
        }
    }

    fn initialize(&mut self) {
        self.send(initialize_message());
        self.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
        let response = self.response_with_id(1);
        assert_eq!(
            response["result"]["capabilities"]["foldingRangeProvider"],
            json!(true),
            "initialize response: {response}"
        );
    }

    fn close_stdin(&mut self) {
        self.stdin.take();
    }

    fn wait_timeout(&mut self, timeout: Duration) -> ExitStatus {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.child.try_wait().expect("poll server") {
                return status;
            }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                panic!("server did not exit within {timeout:?}");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn finish(mut self, timeout: Duration) -> ExitStatus {
        self.close_stdin();
        let status = self.wait_timeout(timeout);
        self.reader.join().expect("reader thread");
        status
    }
}

fn initialize_message() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "processId": null,
            "rootUri": null,
            "capabilities": {},
        },
    })
}

fn frame(message: Value) -> Vec<u8> {
    let body = serde_json::to_vec(&message).expect("serialise message");
    let mut bytes = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    bytes.extend_from_slice(&body);
    bytes
}

/// Read every frame from `stdout`, reporting stray or truncated bytes.
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

fn repo_param(path: &std::path::Path) -> Value {
    json!(path.to_string_lossy())
}

fn shutdown_and_exit(server: &mut TestServer) {
    server.send(json!({"jsonrpc": "2.0", "id": 99, "method": "shutdown", "params": null}));
    let response = server.response_with_id(99);
    assert_eq!(response["result"], Value::Null);
    server.send(json!({"jsonrpc": "2.0", "method": "exit", "params": null}));
}

fn process_exists(pid: u32) -> bool {
    let output = Command::new("ps")
        .args(["-o", "pid=", "-p", &pid.to_string()])
        .output()
        .expect("run ps");
    !String::from_utf8_lossy(&output.stdout).trim().is_empty()
}

#[test]
fn pipelined_and_split_messages_are_framed_and_matched_by_id() {
    let (tmp, _repo) = init_repo();
    let repo = repo_param(tmp.path());
    let mut server = TestServer::spawn(&[]);

    // Several messages in one write: initialize, initialized and a request.
    let mut batch = Vec::new();
    batch.extend(frame(initialize_message()));
    batch.extend(frame(
        json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}),
    ));
    batch.extend(frame(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "vcs/status",
        "params": {"repo_path": repo},
    })));
    server.send_raw(&batch);

    let initialize = server.response_with_id(1);
    assert_eq!(
        initialize["result"]["capabilities"]["foldingRangeProvider"],
        json!(true)
    );
    let status = server.response_with_id(2);
    assert_eq!(status["result"]["branch"], json!("main"));

    // One request split across two writes.
    let split = frame(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "vcs/status",
        "params": {"repo_path": repo},
    }));
    let midpoint = split.len() / 2;
    server.send_raw(&split[..midpoint]);
    thread::sleep(Duration::from_millis(50));
    server.send_raw(&split[midpoint..]);

    let status = server.response_with_id(3);
    assert_eq!(status["result"]["branch"], json!("main"));

    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
}

#[test]
#[cfg(debug_assertions)]
fn a_slow_mutation_does_not_delay_a_later_read() {
    let (tmp, _repo) = init_repo();
    write(&tmp.path().join("f.txt"), "line1\nCHANGED\nline3\n");
    let repo = repo_param(tmp.path());

    // The delay hook is compiled only into debug builds; see `server::maybe_test_delay`.
    let mut server = TestServer::spawn(&[("MAGIT_LSP_TEST_DELAY_MS", "500")]);
    server.initialize();

    let mut batch = Vec::new();
    batch.extend(frame(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "vcs/stageFile",
        "params": {"repo_path": repo, "path": "f.txt"},
    })));
    batch.extend(frame(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "vcs/status",
        "params": {"repo_path": repo},
    })));
    server.send_raw(&batch);

    let first = server.recv();
    assert_eq!(
        first["id"],
        json!(3),
        "the slow mutation blocked a later read: {first}"
    );
    let second = server.recv();
    assert_eq!(second["id"], json!(2));

    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
}

#[test]
fn mutations_for_one_repository_apply_in_request_order() {
    let (tmp, _repo) = init_repo();
    write(&tmp.path().join("f.txt"), "line1\nCHANGED\nline3\n");
    let repo = repo_param(tmp.path());

    let mut server = TestServer::spawn(&[]);
    server.initialize();

    let mut batch = Vec::new();
    batch.extend(frame(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "vcs/stageFile",
        "params": {"repo_path": repo, "path": "f.txt"},
    })));
    batch.extend(frame(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "vcs/unstageFile",
        "params": {"repo_path": repo, "path": "f.txt"},
    })));
    server.send_raw(&batch);

    let staged = server.response_with_id(2);
    assert!(
        staged["result"]["status"]["staged"]
            .as_array()
            .expect("staged list")
            .iter()
            .any(|entry| entry["path"] == json!("f.txt")),
        "stageFile did not stage the file: {staged}"
    );
    let unstaged = server.response_with_id(3);
    assert!(
        unstaged["result"]["status"]["unstaged"]
            .as_array()
            .expect("unstaged list")
            .iter()
            .any(|entry| entry["path"] == json!("f.txt")),
        "unstageFile did not run after stageFile: {unstaged}"
    );

    assert_eq!(cached_diff(tmp.path()), "");
    assert!(read(&tmp.path().join("f.txt")).contains("CHANGED"));

    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
}

#[test]
fn shutdown_then_exit_terminates_zero_without_an_orphan() {
    let mut server = TestServer::spawn(&[]);
    server.initialize();
    let pid = server.pid();

    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert_eq!(status.code(), Some(0), "server exited: {status:?}");
    assert!(!process_exists(pid), "magit-lsp {pid} is still running");
}

#[test]
fn exit_without_shutdown_terminates_with_code_one() {
    let mut server = TestServer::spawn(&[]);
    server.initialize();

    server.send(json!({"jsonrpc": "2.0", "method": "exit", "params": null}));
    let status = server.finish(Duration::from_secs(10));
    assert_eq!(status.code(), Some(1), "server exited: {status:?}");
}

#[test]
fn unknown_requests_reply_method_not_found() {
    let mut server = TestServer::spawn(&[]);
    server.initialize();

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "vcs/nope",
        "params": {},
    }));
    let response = server.response_with_id(7);
    assert_eq!(response["error"]["code"], json!(-32601), "{response}");

    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
}

#[test]
fn view_open_then_invoke_stage_round_trips_over_stdio() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let repo = repo_param(tmp.path());
    let mut server = TestServer::spawn(&[]);
    server.initialize();

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "view/open",
        "params": {
            "kind": "status",
            "args": {"repo_path": repo},
            "viewport": {"cols": 80, "rows": 24},
        },
    }));
    let opened = server.response_with_id(2);
    let view_id = opened["result"]["id"]
        .as_str()
        .expect("view id")
        .to_string();
    assert_eq!(opened["result"]["kind"], json!("status"));
    assert_eq!(
        opened["result"]["uri"],
        json!(format!(
            "magit://status/{}",
            tmp.path().to_string_lossy().trim_start_matches('/')
        ))
    );

    let initial = server.recv();
    assert_eq!(initial["method"], json!("view/render"), "{initial}");
    assert_eq!(initial["params"]["viewId"], json!(view_id));
    assert_eq!(initial["params"]["version"], json!(1));
    assert!(initial["params"]["baseVersion"].is_null());
    let text = initial["params"]["text"]
        .as_array()
        .expect("initial full text");
    let line = text
        .iter()
        .position(|line| {
            line.as_str()
                .is_some_and(|line| line.trim_start().starts_with('+'))
        })
        .expect("a '+' line in the rendered status");

    server.send(json!({
        "jsonrpc": "2.0",
        "method": "view/rendered",
        "params": {"viewId": view_id, "version": 1},
    }));
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "view/invoke",
        "params": {
            "viewId": view_id,
            "command": "status.stage",
            "cursor": {"line": line, "column": 0},
        },
    }));

    let mut response = None;
    let mut render = None;
    while response.is_none() || render.is_none() {
        let message = server.recv();
        if message["id"] == json!(3) {
            response = Some(message);
        } else if message["method"] == json!("view/render") {
            render = Some(message);
        }
    }
    let response = response.expect("invoke response");
    let render = render.expect("render notification");
    assert_eq!(response["result"]["effects"], json!([]));
    assert_eq!(render["params"]["viewId"], json!(view_id));
    assert_eq!(render["params"]["version"], json!(2));
    assert_eq!(render["params"]["baseVersion"], json!(1));
    assert!(render["params"]["text"].is_null());
    assert!(
        !render["params"]["edits"]
            .as_array()
            .expect("edits")
            .is_empty()
    );
    assert!(!render["params"]["cursor"].is_null());
    assert!(hunk_count(&cached_diff(tmp.path())) >= 1);

    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
}
