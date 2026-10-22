//! stdout is the LSP pipe: it must carry framed JSON-RPC and nothing else.

mod common;

use common::*;
use serde_json::{json, Value};
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};

/// A running server whose stdout is parsed frame by frame.
///
/// Any stray byte between frames makes the parser fail, which is exactly the
/// hygiene property this suite protects.
struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    messages: Vec<Value>,
}

impl Server {
    /// Spawn the real server, logging to stderr or `log_file`.
    fn spawn(log_file: Option<&Path>) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_magit-lsp"));
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env(magit_lsp::logging::LEVEL_ENV, "trace")
            .env_remove(magit_lsp::logging::FILE_ENV);
        if let Some(path) = log_file {
            command.env(magit_lsp::logging::FILE_ENV, path);
        }

        let mut child = command.spawn().expect("spawn magit-lsp");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        Self {
            child,
            stdin: Some(stdin),
            stdout,
            messages: Vec::new(),
        }
    }

    fn send(&mut self, message: Value) {
        let stdin = self.stdin.as_mut().expect("server stdin open");
        let body = serde_json::to_vec(&message).expect("serialise message");
        write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).expect("write header");
        stdin.write_all(&body).expect("write body");
        stdin.flush().expect("flush");
    }

    /// Read one frame, panicking on stray bytes, and remember it.
    fn recv(&mut self) -> Option<Value> {
        let mut content_length = None;
        let mut saw_header = false;
        loop {
            let mut line = String::new();
            let read = self.stdout.read_line(&mut line).expect("read header");
            if read == 0 {
                assert!(!saw_header, "EOF in the middle of a frame header");
                return None;
            }
            saw_header = true;
            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                if name.trim().eq_ignore_ascii_case("content-length") {
                    content_length =
                        Some(value.trim().parse::<usize>().expect("Content-Length"));
                }
            }
        }

        let length = content_length.expect("frame has no Content-Length header");
        let mut body = vec![0; length];
        self.stdout.read_exact(&mut body).expect("read frame body");
        let message: Value =
            serde_json::from_slice(&body).expect("frame body is well-formed JSON-RPC");
        self.messages.push(message.clone());
        Some(message)
    }

    /// Read until the response carrying `id`.
    fn response_with_id(&mut self, id: i64) -> Value {
        loop {
            let message = self.recv().expect("server closed stdout early");
            if message["id"] == json!(id) {
                return message;
            }
        }
    }

    /// Close stdin, drain stdout to EOF, and return the exit status and stderr.
    fn finish(mut self) -> (ExitStatus, String) {
        self.stdin.take();
        while self.recv().is_some() {}
        let mut stderr = String::new();
        if let Some(mut pipe) = self.child.stderr.take() {
            pipe.read_to_string(&mut stderr).expect("read stderr");
        }
        let status = self.child.wait().expect("wait for magit-lsp");
        (status, stderr)
    }
}

/// The protocol prefix every session shares.
fn initialize(server: &mut Server) {
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "processId": null,
            "rootUri": null,
            "capabilities": {},
        },
    }));
    server.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
}

fn open_status(server: &mut Server, id: i64, repo: &Path) {
    server.send(json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "view/open",
        "params": {
            "kind": "status",
            "args": {"repo_path": repo.to_string_lossy()},
            "viewport": {"cols": 80, "rows": 24},
        },
    }));
}

/// Send `shutdown` then `exit`, and drain the responses.
fn shutdown_and_exit(server: &mut Server, id: i64) {
    server.send(json!({"jsonrpc": "2.0", "id": id, "method": "shutdown", "params": null}));
    server.response_with_id(id);
    server.send(json!({"jsonrpc": "2.0", "method": "exit", "params": null}));
}

#[test]
fn stdout_carries_only_framed_json_rpc_when_logging_to_stderr() {
    let (tmp, _repo) = init_repo();
    write(&tmp.path().join("f.txt"), "line1\nCHANGED\nline3\n");
    // A stale index lock makes the staging mutation fail without writing.
    write(&tmp.path().join(".git/index.lock"), "");

    let mut server = Server::spawn(None);
    initialize(&mut server);
    open_status(&mut server, 2, tmp.path());
    server.response_with_id(2);

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "view/invoke",
        "params": {"viewId": "v1", "command": "status.stage-all"},
    }));
    let stage = server.response_with_id(3);
    let effects = stage["result"]["effects"]
        .as_array()
        .expect("mutation effects");
    assert_eq!(effects[0]["level"], "error", "failing git op: {stage}");

    shutdown_and_exit(&mut server, 4);
    let (status, stderr) = server.finish();

    assert!(status.success(), "server exited: {stderr}");
    // The failure is logged, but only on stderr.
    assert!(stderr.contains("git failed"), "stderr: {stderr}");
}

#[test]
fn log_file_receives_logs_and_stdout_stays_clean() {
    let (tmp, _repo) = init_repo();
    write(&tmp.path().join("f.txt"), "line1\nCHANGED\nline3\n");
    write(&tmp.path().join(".git/index.lock"), "");
    let logs = tempfile::TempDir::new().expect("log tempdir");
    let log_file = logs.path().join("magit-lsp.log");

    let mut server = Server::spawn(Some(&log_file));
    initialize(&mut server);
    open_status(&mut server, 2, tmp.path());
    server.response_with_id(2);
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "view/invoke",
        "params": {"viewId": "v1", "command": "status.stage-all"},
    }));
    server.response_with_id(3);

    shutdown_and_exit(&mut server, 4);
    let (status, stderr) = server.finish();

    assert!(status.success(), "server exited: {stderr}");
    assert!(stderr.trim().is_empty(), "logs leaked to stderr: {stderr}");

    let written = std::fs::read_to_string(&log_file).expect("read log file");
    assert!(written.contains("git failed"), "log file: {written}");
    assert!(written.contains("index.lock"), "log file: {written}");
}

#[test]
fn src_never_writes_to_stdout_directly() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rust_files(&src, &mut files);
    assert!(!files.is_empty(), "no Rust sources under {}", src.display());

    for file in files {
        let source = std::fs::read_to_string(&file).expect("read source");
        for name in ["println!", "print!", "dbg!"] {
            if let Some(offset) = first_macro_hit(&source, name) {
                let line = source[..offset].lines().count();
                panic!(
                    "{}:{line} uses {name}; stdout is the LSP pipe",
                    file.display()
                );
            }
        }
    }
}

fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read src dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension() == Some(OsStr::new("rs")) {
            files.push(path);
        }
    }
}

/// Byte offset of a real `name!` invocation, ignoring `eprintln!` and friends.
fn first_macro_hit(source: &str, name: &str) -> Option<usize> {
    let mut start = 0;
    while let Some(offset) = source[start..].find(name) {
        let index = start + offset;
        let qualified = source[..index]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_');
        if !qualified {
            return Some(index);
        }
        start = index + name.len();
    }
    None
}
