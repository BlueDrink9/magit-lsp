//! Shared bounded real-pipe JSON-RPC test transport.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// A running server plus its framed stdout stream.
pub struct TestServer {
    child: Child,
    stdin: Option<ChildStdin>,
    responses: Receiver<Result<Value, String>>,
    reader: Option<JoinHandle<()>>,
    stderr_reader: Option<JoinHandle<()>>,
    stderr: Arc<Mutex<String>>,
    /// Messages observed while waiting for responses.
    pub notifications: Vec<Value>,
    /// Maximum wait for each incoming frame.
    pub recv_timeout: Duration,
}

impl TestServer {
    /// Request shutdown, assert its null result, and send exit.
    pub fn shutdown_and_exit(&mut self) {
        self.send(json!({"jsonrpc":"2.0","id":99,"method":"shutdown","params":null}));
        let response = self.response_with_id(99);
        assert_eq!(response["result"], Value::Null);
        self.send(json!({"jsonrpc":"2.0","method":"exit","params":null}));
    }
    /// Spawn the real binary with logging on stderr and optional extra env vars.
    pub fn spawn(extra_env: &[(&str, &str)]) -> Self {
        Self::from_command(Command::new(env!("CARGO_BIN_EXE_magit-lsp")), extra_env)
    }

    fn from_command(mut command: Command, extra_env: &[(&str, &str)]) -> Self {
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
        let stderr = child.stderr.take().expect("child stderr");

        let (tx, responses) = mpsc::channel();
        let reader = thread::spawn(move || read_loop(stdout, tx));
        let log = Arc::new(Mutex::new(String::new()));
        let stderr_reader = drain_stderr(stderr, Arc::clone(&log));

        Self {
            child,
            stdin: Some(stdin),
            responses,
            reader: Some(reader),
            notifications: Vec::new(),
            recv_timeout: Duration::from_secs(10),
            stderr: log,
            stderr_reader: Some(stderr_reader),
        }
    }

    /// Spawn the real server with a child-only umask before exec.
    pub fn spawn_with_umask(extra_env: &[(&str, &str)], umask: &str) -> Self {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("umask \"$1\"; shift; exec \"$@\"")
            .arg("magit-lsp-test")
            .arg(umask)
            .arg(env!("CARGO_BIN_EXE_magit-lsp"));
        Self::from_command(command, extra_env)
    }

    /// Spawn the server after child-only shell setup, retaining the shell's PID.
    pub fn spawn_prepared(extra_env: &[(&str, &str)], setup: &str) -> Self {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(format!("set -e; {setup}; exec \"$1\""))
            .arg("magit-lsp-test")
            .arg(env!("CARGO_BIN_EXE_magit-lsp"));
        Self::from_command(command, extra_env)
    }

    /// Return the server process ID.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Send one framed JSON-RPC message.
    pub fn send(&mut self, message: Value) {
        self.send_raw(&frame(message));
    }

    /// Write bytes verbatim to stdin.
    pub fn send_raw(&mut self, bytes: &[u8]) {
        let stdin = self.stdin.as_mut().expect("server stdin open");
        stdin.write_all(bytes).expect("write to server");
        stdin.flush().expect("flush server stdin");
    }

    /// Receive the next frame with a bounded wait.
    pub fn recv(&mut self) -> Value {
        match self.responses.recv_timeout(self.recv_timeout) {
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

    /// Read until the response carrying `id`, failing if the stream ends.
    pub fn response_with_id(&mut self, id: i64) -> Value {
        loop {
            let message = self.recv();
            if message["id"] == json!(id) {
                return message;
            }
            self.notifications.push(message);
        }
    }

    /// Initialize with the requested client capabilities.
    pub fn initialize(&mut self, capabilities: Value) -> Value {
        let mut message = initialize_message();
        message["params"]["capabilities"] = capabilities;
        self.send(message);
        self.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
        self.response_with_id(1)
    }

    /// Collect incoming notifications until a 400ms quiet window.
    pub fn drain_notifications(&mut self) -> Vec<Value> {
        let mut out = Vec::new();
        loop {
            match self.responses.recv_timeout(Duration::from_millis(400)) {
                Ok(Ok(message)) => {
                    self.notifications.push(message.clone());
                    out.push(message);
                }
                Ok(Err(error)) => panic!("stdout is not clean LSP framing: {error}"),
                Err(_) => return out,
            }
        }
    }

    /// Return captured stderr, including logs emitted so far.
    pub fn stderr_text(&self) -> String {
        self.stderr
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// Close the input pipe.
    pub fn close_stdin(&mut self) {
        self.stdin.take();
    }

    /// Poll for the server's exit without blocking.
    pub fn try_wait(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().expect("poll server")
    }

    /// Wait for exit, killing the process on timeout.
    pub fn wait_timeout(&mut self, timeout: Duration) -> ExitStatus {
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

    /// Finish and return the fully drained stderr log.
    pub fn finish_with_stderr(self, timeout: Duration) -> (ExitStatus, String) {
        let log = Arc::clone(&self.stderr);
        let status = self.finish(timeout);
        let text = log.lock().unwrap_or_else(|p| p.into_inner()).clone();
        (status, text)
    }

    /// Close input, wait within the deadline, and validate remaining frames.
    pub fn finish(mut self, timeout: Duration) -> ExitStatus {
        self.close_stdin();
        let status = self.wait_timeout(timeout);
        self.reader
            .take()
            .expect("reader")
            .join()
            .expect("reader thread");
        self.stderr_reader
            .take()
            .expect("stderr reader")
            .join()
            .expect("stderr thread");
        while let Ok(message) = self.responses.try_recv() {
            message.expect("stdout is clean LSP framing through EOF");
        }
        status
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stdin.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Build the standard initialize request.
pub fn initialize_message() -> Value {
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

/// Encode a JSON value as an LSP frame.
pub fn frame(message: Value) -> Vec<u8> {
    let body = serde_json::to_vec(&message).expect("serialise message");
    let mut bytes = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    bytes.extend_from_slice(&body);
    bytes
}

/// Drain the server's stderr so a full pipe can never block it, keeping the
/// tail for failure messages.
fn drain_stderr(mut stderr: std::process::ChildStderr, log: Arc<Mutex<String>>) -> JoinHandle<()> {
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
    })
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
