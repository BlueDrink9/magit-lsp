//! Shared bounded real-pipe JSON-RPC test transport.

use serde_json::{json, Value};
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Environment variable naming the one server executable real-pipe tests run.
///
/// When set it is authoritative: the path must be absolute and name an
/// executable file, and the server Cargo built for the test harness is never
/// used as a fallback. `make test-rust-artifact` sets it for the artifact lane;
/// ordinary source tests leave it unset.
pub const SERVER_BINARY_ENV: &str = "MAGIT_LSP_BIN";

/// Server path registered by [`cargo_server_binary!`] from a test crate.
///
/// First write wins, so a parallel sibling's registration can never replace an
/// earlier one.
static CARGO_SERVER_BINARY: OnceLock<PathBuf> = OnceLock::new();

/// Record the server path Cargo guaranteed for the invoking test crate.
///
/// [`cargo_server_binary!`] calls this with the compile-time
/// `CARGO_BIN_EXE_magit-lsp` value; the first registration wins and returns the
/// stored path. [`server_binary`] prefers it over its walk fallback.
pub fn register_cargo_server_binary(path: &'static str) -> PathBuf {
    CARGO_SERVER_BINARY
        .get_or_init(|| PathBuf::from(path))
        .clone()
}

/// Register the Cargo-built server executable from inside a test crate.
///
/// `CARGO_BIN_EXE_magit-lsp` is defined only while compiling the root package's
/// test crates, so this helper crate cannot reference it itself. A spawning test
/// invokes this macro before its first spawn; the body expands in the invoking
/// crate, where the variable exists, and registers the value with
/// [`register_cargo_server_binary`].
#[macro_export]
macro_rules! cargo_server_binary {
    () => {
        $crate::stdio::register_cargo_server_binary(env!("CARGO_BIN_EXE_magit-lsp"))
    };
}

/// Resolve the server executable a real-pipe spawn must run.
///
/// An explicit [`SERVER_BINARY_ENV`] selection is used verbatim; only without
/// one does the shared transport use the path registered by
/// [`cargo_server_binary!`], or the server Cargo built alongside the test
/// harness when no test in the binary registered one. An invalid explicit
/// selection panics instead of falling back.
pub fn server_binary() -> PathBuf {
    match std::env::var_os(SERVER_BINARY_ENV) {
        Some(value) => explicit_server_binary(&value)
            .unwrap_or_else(|error| panic!("{SERVER_BINARY_ENV} {error}")),
        None => match CARGO_SERVER_BINARY.get() {
            Some(path) => path.clone(),
            None => {
                // This crate compiles as a dependency of the integration-test
                // binaries, where Cargo's `CARGO_BIN_EXE_magit-lsp` is not
                // defined. A suite that does not register it (see
                // [`cargo_server_binary!`]) still resolves the server through
                // this walk: the harness executable sits in the profile's
                // `deps` directory, so its parent's parent holds the server
                // Cargo built alongside it.
                let harness = std::env::current_exe().expect("resolve the test harness executable");
                harness
                    .parent()
                    .and_then(std::path::Path::parent)
                    .expect("the test harness lives in the Cargo profile directory")
                    .join(format!("magit-lsp{}", std::env::consts::EXE_SUFFIX))
            }
        },
    }
}

/// Validate one explicit server path and return it unchanged.
///
/// The path must be absolute, name an existing regular file and, on Unix, carry
/// an execute bit. Errors are readable fragments prefixed with
/// [`SERVER_BINARY_ENV`] by [`server_binary`].
pub fn explicit_server_binary(value: &OsStr) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    let shown = path.display();
    if value.is_empty() {
        return Err("is set but empty; pass the absolute path to the verified server".to_string());
    }
    if !path.is_absolute() {
        return Err(format!("must be an absolute path, got {shown}"));
    }
    let metadata = std::fs::metadata(&path)
        .map_err(|error| format!("does not name a readable file: {shown}: {error}"))?;
    if !metadata.is_file() {
        return Err(format!("does not name a regular file: {shown}"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(format!("is not executable: {shown}"));
        }
    }
    Ok(path)
}

/// Build the panic detail for a failed server spawn.
///
/// It names the attempted program and arguments, the harness executable that
/// resolved a fallback, and the [`SERVER_BINARY_ENV`] selection when one was
/// set, so a spawn failure says which executable the test meant to run instead
/// of only that spawning failed. `harness_exe` and `env_value` are parameters so
/// a unit test never has to touch the process environment.
pub fn spawn_failure_detail(
    command: &Command,
    harness_exe: Option<&Path>,
    env_value: Option<&OsStr>,
) -> String {
    let program = command.get_program().to_string_lossy();
    let args = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ");
    let harness = match harness_exe {
        Some(path) => path.display().to_string(),
        None => "(current_exe unavailable)".to_string(),
    };
    let selection = match env_value {
        Some(value) => format!("{SERVER_BINARY_ENV}={}", value.to_string_lossy()),
        None => format!("{SERVER_BINARY_ENV} is unset"),
    };
    format!(
        "attempted program: {program}\nattempted args: [{args}]\n\
         test harness: {harness}\nserver selection: {selection}"
    )
}

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
    /// Frames actually written through the JSON transport, in emission order.
    pub sent: Vec<Value>,
    /// Maximum wait for each incoming frame.
    pub recv_timeout: Duration,
    /// Automatically answer progress-create requests when explicitly enabled.
    pub auto_respond_progress_create: bool,
    /// Step the current bounded receive awaits, reported by a timeout panic.
    ///
    /// A caller sets this before [`TestServer::recv_raw`]; a completed receive
    /// clears it, so the label always describes the wait that timed out.
    pub await_step: Option<String>,
    /// Recently received frames (method and id), oldest first and bounded.
    received: Vec<String>,
}

/// How many received frames a timeout panic reports.
const RECEIVED_FRAME_HISTORY: usize = 8;

/// Watcher debounce, in milliseconds, every spawned test server gets unless
/// `extra_env` names `MAGIT_LSP_WATCH_DEBOUNCE_MS` (explicit values always win).
///
/// The production default is short enough that an arm-time rescan can fire a
/// refresh inside a test's offer-to-execute window; pinning it past the test's
/// lifetime makes plain-server tests deterministic. Tests that exercise watcher
/// timing pass their own small value, and opt-outs pass one too.
const DEFAULT_WATCH_DEBOUNCE_MS: &str = "60000";

impl TestServer {
    /// Request shutdown, assert its null result, and send exit.
    pub fn shutdown_and_exit(&mut self) {
        self.send(json!({"jsonrpc":"2.0","id":99,"method":"shutdown","params":null}));
        let response = self.response_with_id(99);
        assert_eq!(response["result"], Value::Null);
        self.send(json!({"jsonrpc":"2.0","method":"exit","params":null}));
    }
    /// Spawn the real binary (see [`server_binary`]) with logging on stderr and
    /// optional extra env vars.
    ///
    /// The watcher debounce defaults to [`DEFAULT_WATCH_DEBOUNCE_MS`]; any entry
    /// for `MAGIT_LSP_WATCH_DEBOUNCE_MS` in `extra_env` takes precedence.
    pub fn spawn(extra_env: &[(&str, &str)]) -> Self {
        Self::from_command(Command::new(server_binary()), extra_env)
    }

    /// Spawn the real binary with explicit server arguments, such as
    /// `--config <path>` or `--open-promise-ms <ms>`.
    pub fn spawn_with_args(args: &[&str], extra_env: &[(&str, &str)]) -> Self {
        let mut command = Command::new(server_binary());
        command.args(args);
        Self::from_command(command, extra_env)
    }

    fn from_command(mut command: Command, extra_env: &[(&str, &str)]) -> Self {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env(magit_lsp::logging::LEVEL_ENV, "trace")
            .env_remove(magit_lsp::logging::FILE_ENV)
            .env_remove(SERVER_BINARY_ENV)
            // Pin the watcher debounce past any test's lifetime so an arm-time
            // rescan cannot land a refresh inside an offer-to-execute window
            // (docs/evidence/plain-fresh-bulk-macos-flake.md). An explicit value
            // in `extra_env` below overrides this, so watcher-semantics tests
            // keep their own small debounce and opt-outs still win.
            .env("MAGIT_LSP_WATCH_DEBOUNCE_MS", DEFAULT_WATCH_DEBOUNCE_MS);
        for (key, value) in extra_env {
            command.env(key, value);
        }

        let spawned = command.spawn();
        let mut child = match spawned {
            Ok(child) => child,
            Err(error) => panic!(
                "spawn magit-lsp: {error}\n{}",
                spawn_failure_detail(
                    &command,
                    std::env::current_exe().ok().as_deref(),
                    std::env::var_os(SERVER_BINARY_ENV).as_deref(),
                )
            ),
        };
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
            sent: Vec::new(),
            recv_timeout: Duration::from_secs(10),
            auto_respond_progress_create: false,
            await_step: None,
            received: Vec::new(),
            stderr: log,
            stderr_reader: Some(stderr_reader),
        }
    }

    /// Spawn the real server (see [`server_binary`]) with a child-only umask
    /// before exec.
    pub fn spawn_with_umask(extra_env: &[(&str, &str)], umask: &str) -> Self {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("umask \"$1\"; shift; exec \"$@\"")
            .arg("magit-lsp-test")
            .arg(umask)
            .arg(server_binary());
        Self::from_command(command, extra_env)
    }

    /// Spawn the server (see [`server_binary`]) after child-only shell setup,
    /// retaining the shell's PID.
    pub fn spawn_prepared(extra_env: &[(&str, &str)], setup: &str) -> Self {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(format!("set -e; {setup}; exec \"$1\""))
            .arg("magit-lsp-test")
            .arg(server_binary());
        Self::from_command(command, extra_env)
    }

    /// Return the server process ID.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Send one framed JSON-RPC message.
    pub fn send(&mut self, message: Value) {
        self.send_raw(&frame(message.clone()));
        self.sent.push(message);
    }

    /// Write bytes verbatim to stdin.
    pub fn send_raw(&mut self, bytes: &[u8]) {
        let stdin = self.stdin.as_mut().expect("server stdin open");
        stdin.write_all(bytes).expect("write to server");
        stdin.flush().expect("flush server stdin");
    }

    /// Receive the next frame with a bounded wait.
    #[track_caller]
    pub fn recv(&mut self) -> Value {
        let message = self.recv_raw();
        if self.auto_respond_progress_create
            && message["method"] == "window/workDoneProgress/create"
        {
            let response = json!({"jsonrpc":"2.0", "id":message["id"], "result":null});
            self.send(response);
        }
        message
    }

    /// Receive a frame without replying, even when automatic replies are enabled.
    ///
    /// A timeout panic reports the awaited step ([`TestServer::await_step`]),
    /// the configured `recv_timeout` and the most recent received frames, so a
    /// silent server names its wait instead of only the helper that timed out.
    #[track_caller]
    pub fn recv_raw(&mut self) -> Value {
        let message = match self.responses.recv_timeout(self.recv_timeout) {
            Ok(Ok(message)) => {
                self.record_received(&message);
                self.await_step = None;
                message
            }
            Ok(Err(error)) => panic!("stdout is not clean LSP framing: {error}"),
            Err(error) => {
                let tail = {
                    let log = self
                        .stderr
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    log.lines()
                        .rev()
                        .take(40)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                let status = self.child.try_wait();
                let step = self.await_step.as_deref().unwrap_or("<unlabelled receive>");
                let received = if self.received.is_empty() {
                    "(none)".to_string()
                } else {
                    self.received.join(", ")
                };
                panic!(
                    "no response from server within {:?}: {error}\nawaited step: {step}\nlast sent: {:?}\nreceived frames (oldest first): {received}\nlast observed notifications: {:?}\nserver exit: {status:?}\nserver stderr:\n{tail}",
                    self.recv_timeout,
                    self.sent.last(),
                    &self.notifications[self.notifications.len().saturating_sub(4)..],
                )
            }
        };
        message
    }

    /// Return the next frame if one arrives within `timeout`, else `None`.
    ///
    /// Unlike [`TestServer::recv_raw`] a silent server is not a failure, so a
    /// poll loop can watch the stream while it also checks another condition.
    /// Framing errors and a closed stream still panic: only silence is benign.
    #[track_caller]
    pub fn try_recv_within(&mut self, timeout: Duration) -> Option<Value> {
        match self.responses.recv_timeout(timeout) {
            Ok(Ok(message)) => {
                self.record_received(&message);
                self.await_step = None;
                Some(message)
            }
            Ok(Err(error)) => panic!("stdout is not clean LSP framing: {error}"),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                panic!("server stream closed while waiting for the next frame")
            }
        }
    }

    /// Remember one received frame for timeout diagnostics, bounded to
    /// [`RECEIVED_FRAME_HISTORY`] entries.
    fn record_received(&mut self, message: &Value) {
        if self.received.len() == RECEIVED_FRAME_HISTORY {
            self.received.remove(0);
        }
        self.received.push(match message["method"].as_str() {
            Some(method) => match message.get("id") {
                Some(id) => format!("{method} id={id}"),
                None => method.to_string(),
            },
            None => format!("response id={}", message["id"]),
        });
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

    /// Wait for a later render of a view, preserving unrelated incoming messages.
    /// The wire exposes render versions but no separate status-readiness flag.
    pub fn wait_for_view_render_after(&mut self, view_id: &str, version: u64) -> Value {
        loop {
            let message = self.recv();
            if message["method"] == "view/render"
                && message["params"]["viewId"] == view_id
                && message["params"]["version"]
                    .as_u64()
                    .is_some_and(|next| next > version)
            {
                return message;
            }
            self.notifications.push(message);
        }
    }

    /// Poll the server-owned render until a later version is available (Plain clients).
    pub fn wait_for_view_resync_after(&mut self, view_id: &str, request_id: i64, version: u64) {
        let deadline = Instant::now() + self.recv_timeout;
        let mut next_id = request_id;
        loop {
            assert!(
                Instant::now() < deadline,
                "view {view_id} did not publish a newer render"
            );
            self.send(json!({"jsonrpc":"2.0","id":next_id,"method":"view/resync","params":{"viewId":view_id}}));
            let render = self.response_with_id(next_id);
            if render["result"]["version"]
                .as_u64()
                .is_some_and(|next| next > version)
            {
                return;
            }
            next_id += 1;
        }
    }

    /// Initialize with the requested client capabilities.
    pub fn initialize(&mut self, capabilities: Value) -> Value {
        let mut message = initialize_message();
        message["params"]["capabilities"] = capabilities;
        self.initialize_params(message["params"].take())
    }

    /// Initialize with complete parameters, including optional workspace roots.
    pub fn initialize_params(&mut self, params: Value) -> Value {
        let mut message = initialize_message();
        message["params"] = params;
        self.send(message);
        self.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
        self.response_with_id(1)
    }

    /// Initialize with the bundled rich client's supported v1 declaration.
    pub fn initialize_rich(&mut self, mut capabilities: Value) -> Value {
        capabilities["experimental"]["magit"] = serde_json::json!({
            "v": 1, "keys": "dynamic", "modifiers": "full",
            "layout": ["main", "side", "bottom", "float"],
            "effects": ["visitFile", "editDocument", "clipboard"],
            "prompts": ["confirm", "text", "choice", "multiChoice", "secret"]
        });
        let response = self.initialize(capabilities);
        assert!(
            response["result"]["capabilities"].is_object(),
            "rich initialize negotiation failed: {response:?}"
        );
        response
    }

    /// Enable automatic responses for progress-create requests in this client.
    pub fn auto_respond_to_progress_create(&mut self) {
        self.auto_respond_progress_create = true;
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
    pub fn finish(self, timeout: Duration) -> ExitStatus {
        self.finish_with_messages(timeout).0
    }

    /// Finish and return every unread frame through EOF for terminal assertions.
    pub fn finish_with_messages(mut self, timeout: Duration) -> (ExitStatus, Vec<Value>) {
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
        let messages = self
            .responses
            .try_iter()
            .map(|message| message.expect("stdout is clean LSP framing through EOF"))
            .collect();
        (status, messages)
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
            "capabilities": {"window": {"workDoneProgress": true}},
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_failure_detail_names_program_args_harness_and_explicit_selection() {
        let mut command = Command::new("/nonexistent/magit-lsp");
        command.args(["--config", "/tmp/magit-lsp.toml"]);
        let harness = Path::new("/harness/target/debug/deps/server_async-abc123");
        let detail = spawn_failure_detail(
            &command,
            Some(harness),
            Some(OsStr::new("/opt/explicit/magit-lsp")),
        );
        assert!(
            detail.contains("attempted program: /nonexistent/magit-lsp"),
            "{detail}"
        );
        assert!(
            detail.contains("attempted args: [--config /tmp/magit-lsp.toml]"),
            "{detail}"
        );
        assert!(
            detail.contains("/harness/target/debug/deps/server_async-abc123"),
            "{detail}"
        );
        assert!(
            detail.contains("server selection: MAGIT_LSP_BIN=/opt/explicit/magit-lsp"),
            "{detail}"
        );
    }

    #[test]
    fn spawn_failure_detail_states_an_unset_selection_and_missing_harness() {
        let command = Command::new("/nonexistent/magit-lsp");
        let detail = spawn_failure_detail(&command, None, None);
        assert!(
            detail.contains("test harness: (current_exe unavailable)"),
            "{detail}"
        );
        assert!(
            detail.contains("server selection: MAGIT_LSP_BIN is unset"),
            "{detail}"
        );
    }
}
