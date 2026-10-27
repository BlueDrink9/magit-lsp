//! The editor bridge (§3.10).
//!
//! Git cannot open an editor itself: it runs a helper process, which joins the
//! server over a Unix socket, forwards the file Git wants edited, and blocks
//! until the client answers `common.confirm` or `common.cancel`. All state
//! stays here in the server; the helper only connects, forwards and exits.
//!
//! Three parties share [`EditRegistry`] through a process-global map keyed by
//! `editId`: the worker that spawns Git (creates the entry), the socket
//! listener (takes the reply receiver, sends `editDocument`, answers the
//! helper) and the confirm/cancel command handlers (send the reply).

use crate::git::GitRepo;
use crate::protocol::{EditDocumentMode, Effect};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc as std_mpsc, Arc, Mutex, OnceLock};

/// Environment variable carrying the bridge socket path to Git and the helper.
pub const SOCKET_ENV: &str = "MAGIT_LSP_SOCKET";
/// Environment variable carrying the pending edit id to Git and the helper.
pub const EDIT_ID_ENV: &str = "MAGIT_LSP_EDIT_ID";
/// The kind the helper reports for Git's commit message file.
pub const COMMIT_MESSAGE_KIND: &str = "commit-message";

/// The helper's hello: one JSON line naming the file Git wants edited.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EditHello {
    edit_id: String,
    path: String,
    kind: String,
}

/// Git's askpass hello: one JSON line carrying the prompt to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct AskpassHello {
    prompt: String,
}

/// The first line a helper sends: either an edit or an askpass request.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
enum BridgeHello {
    Edit(EditHello),
    Askpass(AskpassHello),
}

/// The answer the helper waits for; `ok: false` (or `abort: true`) aborts Git.
#[derive(Debug, Deserialize)]
struct HelperReply {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    secret: Option<String>,
}

/// How a pending edit was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    Ok,
    Abort,
}

struct EditEntry {
    /// The repository (session) that spawned the blocking Git process.
    repo_path: String,
    /// The command thread answers through this; the listener holds the other end.
    replies: std_mpsc::Sender<Reply>,
    /// Taken by the listener once the helper joins, then blocked on.
    waiting: Option<std_mpsc::Receiver<Reply>>,
}

/// The process-global map of edits Git is waiting on.
pub struct EditRegistry {
    entries: Mutex<HashMap<String, EditEntry>>,
    next_id: AtomicU64,
}

impl EditRegistry {
    fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(0),
        }
    }

    /// Register a pending edit for `repo_path` and return its id.
    pub fn create(&self, repo_path: &str) -> String {
        let next = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let edit_id = format!("e{next}");
        let (replies, waiting) = std_mpsc::channel();
        self.lock().insert(
            edit_id.clone(),
            EditEntry {
                repo_path: repo_path.to_string(),
                replies,
                waiting: Some(waiting),
            },
        );
        edit_id
    }

    /// The owning repository plus the reply receiver, once the helper joins.
    pub fn take_waiter(&self, edit_id: &str) -> Option<(String, std_mpsc::Receiver<Reply>)> {
        let mut entries = self.lock();
        let entry = entries.get_mut(edit_id)?;
        let waiting = entry.waiting.take()?;
        Some((entry.repo_path.clone(), waiting))
    }

    /// Answer a pending edit and drop it; `false` when no such edit is pending.
    pub fn resolve(&self, edit_id: &str, reply: Reply) -> bool {
        let entry = self.lock().remove(edit_id);
        match entry {
            Some(entry) => {
                let _ = entry.replies.send(reply);
                true
            }
            None => false,
        }
    }

    /// Forget an edit that finished or will never reach a helper.
    pub fn remove(&self, edit_id: &str) {
        self.lock().remove(edit_id);
    }

    /// Abort every pending edit: the client is gone, so Git must never hang.
    ///
    /// A helper that already connected receives the abort reply and Git aborts
    /// (§3.10 step 7); an entry no helper connected to is dropped, so the
    /// helper fails fast once it finally connects.
    pub fn abort_all(&self) {
        let ids: Vec<String> = self.lock().keys().cloned().collect();
        for edit_id in ids {
            self.resolve(&edit_id, Reply::Abort);
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, EditEntry>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The edit registry every commit worker and confirm/cancel shares.
pub fn registry() -> &'static EditRegistry {
    static REGISTRY: OnceLock<EditRegistry> = OnceLock::new();
    REGISTRY.get_or_init(EditRegistry::new)
}

/// How the listener hands an effect to the server's writer.
pub type EffectEmitter = Arc<dyn Fn(Vec<Effect>) + Send + Sync>;

/// How the listener asks the client for an askpass answer; `None` cancels.
pub type AskpassPrompt = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

static SOCKET: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Bind the bridge socket and start accepting helpers, once per process.
pub fn start_listener(emitter: EffectEmitter, askpass: AskpassPrompt) -> Option<&'static Path> {
    SOCKET
        .get_or_init(|| match bind_listener(emitter, askpass) {
            Ok(path) => Some(path),
            Err(error) => {
                tracing::warn!(%error, "editor bridge unavailable");
                None
            }
        })
        .as_deref()
}

/// The bridge socket path, when the listener was started.
pub fn socket_path() -> Option<&'static Path> {
    SOCKET.get().and_then(|path| path.as_deref())
}

/// Run `git commit` with the helper as its editor and return the committed
/// message, read back from `git log -1 --format=%B`. Repository discovery is
/// not a cancellation target; only the commit inherits the caller's handle.
pub fn commit(repo_path: &str, edit_id: &str, socket: &Path) -> Result<String> {
    let repo = crate::runner::with_cancel(None, || GitRepo::open(repo_path))?;
    let editor = helper_invocation()?;
    let socket = socket.to_string_lossy();
    let env = [
        ("GIT_EDITOR", editor.as_str()),
        ("EDITOR", editor.as_str()),
        ("VISUAL", editor.as_str()),
        (SOCKET_ENV, socket.as_ref()),
        (EDIT_ID_ENV, edit_id),
    ];
    repo.commit(&[], &env)
}

/// The helper subcommand: forward the file Git passed, then wait for a reply.
///
/// Returns whether the client confirmed; the caller maps `false` to exit 1 so
/// Git aborts the operation.
pub fn run_helper(file: &str) -> Result<bool> {
    #[cfg(unix)]
    {
        unix_helper(file)
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        Err(Error::new(
            "the editor bridge needs a Unix socket; this build has none",
        ))
    }
}

/// The askpass helper: forward Git's prompt and return the secret, if any.
///
/// The secret travels only over stdout; callers must never log it.
pub fn run_askpass(prompt: &str) -> Result<Option<String>> {
    #[cfg(unix)]
    {
        unix_askpass(prompt)
    }
    #[cfg(not(unix))]
    {
        let _ = prompt;
        Err(Error::new(
            "the askpass bridge needs a Unix socket; this build has none",
        ))
    }
}

/// Our own executable followed by the helper subcommand, quoted for `sh`.
fn helper_invocation() -> Result<String> {
    invocation("--editor-helper")
}

/// Our own executable followed by `--askpass`, quoted for `sh`.
pub fn askpass_command() -> Result<String> {
    invocation("--askpass")
}

fn invocation(subcommand: &str) -> Result<String> {
    let exe = std::env::current_exe()
        .map_err(|error| Error::new(format!("cannot locate magit-lsp: {error}")))?;
    Ok(format!(
        "{} {subcommand}",
        shell_quote(&exe.to_string_lossy())
    ))
}

/// The askpass command every spawned Git process should use.
static ASKPASS: OnceLock<String> = OnceLock::new();

/// Remember how a spawned Git process should ask for a password.
///
/// Only the server process knows its own path, so only the server sets this.
/// A process that never runs the server (a test binary) leaves it unset and Git
/// keeps its normal behaviour.
pub fn set_askpass_invocation(invocation: String) {
    let _ = ASKPASS.set(invocation);
}

/// The askpass command for spawned Git processes, when the server runs.
pub fn askpass_invocation() -> Option<&'static str> {
    ASKPASS.get().map(String::as_str)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(unix)]
fn unix_helper(file: &str) -> Result<bool> {
    use std::os::unix::net::UnixStream;

    let socket = std::env::var_os(SOCKET_ENV)
        .ok_or_else(|| Error::new(format!("{SOCKET_ENV} is not set")))?;
    let edit_id = std::env::var(EDIT_ID_ENV)
        .map_err(|_| Error::new(format!("{EDIT_ID_ENV} is not set")))?;
    let path = absolute(file)?;

    let mut stream = UnixStream::connect(Path::new(&socket)).map_err(|error| {
        Error::new(format!("cannot connect to the editor bridge: {error}"))
    })?;
    let hello = EditHello {
        edit_id,
        path: path.to_string_lossy().into_owned(),
        kind: COMMIT_MESSAGE_KIND.to_string(),
    };
    let mut line = serde_json::to_string(&hello)
        .map_err(|error| Error::new(format!("cannot encode the edit hello: {error}")))?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    stream.flush()?;

    let mut reply = String::new();
    BufReader::new(
        stream
            .try_clone()
            .map_err(|error| Error::new(format!("cannot read the bridge reply: {error}")))?,
    )
    .read_line(&mut reply)?;
    let reply: HelperReply = serde_json::from_str(reply.trim())
        .map_err(|error| Error::new(format!("bad bridge reply: {error}")))?;
    Ok(reply.ok)
}

#[cfg(unix)]
fn unix_askpass(prompt: &str) -> Result<Option<String>> {
    use std::os::unix::net::UnixStream;

    let socket = std::env::var_os(SOCKET_ENV)
        .ok_or_else(|| Error::new(format!("{SOCKET_ENV} is not set")))?;
    let mut stream = UnixStream::connect(Path::new(&socket)).map_err(|error| {
        Error::new(format!("cannot connect to the askpass bridge: {error}"))
    })?;
    let hello = AskpassHello {
        prompt: prompt.to_string(),
    };
    let mut line = serde_json::to_string(&hello)
        .map_err(|error| Error::new(format!("cannot encode the askpass hello: {error}")))?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    stream.flush()?;

    let mut reply = String::new();
    BufReader::new(
        stream
            .try_clone()
            .map_err(|error| Error::new(format!("cannot read the askpass reply: {error}")))?,
    )
    .read_line(&mut reply)?;
    let reply: HelperReply = serde_json::from_str(reply.trim())
        .map_err(|error| Error::new(format!("bad askpass reply: {error}")))?;
    if reply.ok {
        Ok(reply.secret)
    } else {
        Ok(None)
    }
}

/// Make Git's relative edit path absolute, so the client can open it.
#[cfg(unix)]
fn absolute(file: &str) -> Result<PathBuf> {
    let path = Path::new(file);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| Error::new(format!("cannot resolve the edit path: {error}")))?
            .join(path)
    };
    Ok(std::fs::canonicalize(&path).unwrap_or(path))
}

#[cfg(unix)]
fn bind_listener(emitter: EffectEmitter, askpass: AskpassPrompt) -> Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    use std::os::unix::net::UnixListener;

    let dir = std::env::temp_dir().join(format!("magit-lsp-{}", std::process::id()));
    let mut builder = std::fs::DirBuilder::new();
    builder.mode(0o700);
    builder.create(&dir).map_err(|error| {
        Error::new(format!(
            "cannot exclusively create the editor bridge directory: {error}"
        ))
    })?;
    let path = dir.join("editor.sock");
    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(error) => {
            let _ = std::fs::remove_dir(&dir);
            return Err(Error::new(format!("cannot bind the editor bridge: {error}")));
        }
    };
    let cleanup_path = path.clone();
    let cleanup_dir = dir.clone();
    std::thread::Builder::new()
        .name("magit-lsp-editor".to_string())
        .spawn(move || {
            accept_loop(listener, emitter, askpass);
            let _ = std::fs::remove_file(cleanup_path);
            let _ = std::fs::remove_dir(cleanup_dir);
        })
        .map_err(|error| {
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_dir(&dir);
            Error::new(format!("cannot start the editor bridge: {error}"))
        })?;
    Ok(path)
}

#[cfg(not(unix))]
fn bind_listener(_emitter: EffectEmitter, _askpass: AskpassPrompt) -> Result<PathBuf> {
    Err(Error::new(
        "the editor bridge needs a Unix socket; this build has none",
    ))
}

#[cfg(unix)]
fn accept_loop(
    listener: std::os::unix::net::UnixListener,
    emitter: EffectEmitter,
    askpass: AskpassPrompt,
) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };
        let emitter = Arc::clone(&emitter);
        let askpass = Arc::clone(&askpass);
        let _ = std::thread::Builder::new()
            .name("magit-lsp-editor-edit".to_string())
            .spawn(move || {
                if let Err(error) = handle_connection(stream, emitter, askpass) {
                    tracing::debug!(%error, "editor bridge connection failed");
                }
            });
    }
}

#[cfg(unix)]
fn handle_connection(
    mut stream: std::os::unix::net::UnixStream,
    emitter: EffectEmitter,
    askpass: AskpassPrompt,
) -> Result<()> {
    let mut reader = BufReader::new(
        stream
            .try_clone()
            .map_err(|error| Error::new(format!("cannot read the bridge hello: {error}")))?,
    );
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let hello: BridgeHello = serde_json::from_str(line.trim())
        .map_err(|error| Error::new(format!("bad bridge hello: {error}")))?;

    match hello {
        BridgeHello::Edit(hello) => handle_edit(&mut stream, emitter, hello),
        BridgeHello::Askpass(hello) => {
            tracing::debug!("editor bridge: askpass");
            match askpass(&hello.prompt) {
                Some(secret) => write_reply(&mut stream, true, Some(&secret)),
                None => write_reply(&mut stream, false, None),
            }
        }
    }
}

#[cfg(unix)]
fn handle_edit(
    stream: &mut std::os::unix::net::UnixStream,
    emitter: EffectEmitter,
    hello: EditHello,
) -> Result<()> {
    let Some((repo_path, replies)) = registry().take_waiter(&hello.edit_id) else {
        // An edit nobody started (a stale helper, or a restarted server) must
        // fail fast instead of blocking Git forever.
        return write_reply(stream, false, None);
    };
    tracing::debug!(edit_id = %hello.edit_id, repo = %repo_path, "editor bridge: edit");
    let metadata = edit_document_metadata(&hello.kind);
    emitter(vec![Effect::EditDocument {
        edit_id: hello.edit_id.clone(),
        path: hello.path,
        kind: hello.kind,
        title: metadata.title.to_string(),
        mode: EditDocumentMode::RealFile,
        scope: metadata.scope.to_string(),
    }]);

    let reply = replies.recv().unwrap_or(Reply::Abort);
    registry().remove(&hello.edit_id);
    write_reply(stream, reply == Reply::Ok, None)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EditDocumentMetadata {
    title: &'static str,
    scope: &'static str,
}

/// Metadata for a Git-owned real file, including its command/keymap scope.
fn edit_document_metadata(kind: &str) -> EditDocumentMetadata {
    match kind {
        COMMIT_MESSAGE_KIND => EditDocumentMetadata {
            title: "Commit message",
            scope: "commit-message",
        },
        "merge-message" => EditDocumentMetadata {
            title: "Merge message",
            scope: "commit-message",
        },
        "tag-message" => EditDocumentMetadata {
            title: "Tag message",
            scope: "commit-message",
        },
        _ => EditDocumentMetadata {
            title: "Edit Git file",
            scope: "common",
        },
    }
}

#[cfg(unix)]
fn write_reply(
    stream: &mut std::os::unix::net::UnixStream,
    ok: bool,
    secret: Option<&str>,
) -> Result<()> {
    let mut value = serde_json::json!({ "ok": ok });
    if let Some(secret) = secret {
        value["secret"] = serde_json::Value::String(secret.to_string());
    }
    let mut line = value.to_string();
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    stream.flush()?;
    Ok(())
}
