use crate::errors;
use crate::config::{ConfigService, LoadOptions};
use crate::git::{self, GitRepo};
use crate::protocol::*;
use crate::runner::{self, CancelHandle};
use crate::semantic_tokens;
use crate::session::{InvokeResult, Session};
use crate::transport;
use crate::tree::Tree;
use crate::ui::PromptUi;
use crate::view::Viewport;
use crate::{Error, Result};
use lsp_server::{ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::request::Request as LspRequest;
use lsp_types::{
    DocumentLinkOptions, FoldingRangeProviderCapability, OneOf, SemanticTokensFullOptions,
    SemanticTokensOptions, SemanticTokensResult, SemanticTokensServerCapabilities,
    ServerCapabilities,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc as std_mpsc, Arc, Mutex};
use tokio::io::{AsyncWrite, BufReader};
use tokio::sync::{mpsc, Mutex as AsyncMutex, Notify};
use tokio::task::JoinSet;

type RepoLocks = Arc<AsyncMutex<HashMap<String, Arc<AsyncMutex<()>>>>>;

/// FIFO tickets for one repository's mutations.
///
/// The async repo lock serialises mutations but does not decide which waiting
/// handler runs first. Tickets are allocated in the reader loop, so mutations
/// apply in the order the client sent them.
struct MutationGate {
    next: AtomicU64,
    served: AtomicU64,
    notify: Notify,
}

impl MutationGate {
    fn new() -> Self {
        Self {
            next: AtomicU64::new(0),
            served: AtomicU64::new(0),
            notify: Notify::new(),
        }
    }
}

/// One request's place in its repository's mutation order.
struct MutationTurn {
    gate: Arc<MutationGate>,
    ticket: u64,
    repo_path: String,
}

impl MutationTurn {
    /// Wait until every earlier mutation for this repository has finished.
    async fn wait(&self) {
        loop {
            let notified = self.gate.notify.notified();
            tokio::pin!(notified);
            // Register before re-checking, so a finish cannot slip between the
            // check and the await.
            notified.as_mut().enable();
            if self.gate.served.load(Ordering::SeqCst) == self.ticket {
                return;
            }
            notified.await;
        }
    }
}

impl Drop for MutationTurn {
    fn drop(&mut self) {
        self.gate.served.fetch_add(1, Ordering::SeqCst);
        self.gate.notify.notify_waiters();
    }
}

/// Startup options parsed from the command line.
#[derive(Debug, Clone, Default)]
pub struct ServerOptions {
    /// Explicit `--config <path>`; wins over `MAGIT_LSP_CONFIG`.
    pub config_path: Option<std::path::PathBuf>,
}

/// Client capabilities that affect key delivery (§3.9).
#[derive(Debug, Clone, Copy)]
struct ClientCaps {
    delivery: KeymapDelivery,
    key_modifiers: KeyModifiers,
}

impl Default for ClientCaps {
    fn default() -> Self {
        Self {
            delivery: KeymapDelivery::Dynamic,
            key_modifiers: KeyModifiers::Full,
        }
    }
}

/// Server state shared by concurrent request handlers.
#[derive(Clone)]
struct Shared {
    repo_locks: RepoLocks,
    /// Open views per repository root.
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    /// View id → repository root, so notifications can find their session.
    view_repos: Arc<Mutex<HashMap<String, String>>>,
    /// Global view-id counter; ids are unique across repositories.
    next_view: Arc<AtomicU64>,
    /// Configuration and resolved keymaps.
    config: Arc<ConfigService>,
    /// Key delivery settings declared by the client in `initialize`.
    client: Arc<Mutex<ClientCaps>>,
    /// Why the initial configuration load failed, if it did.
    startup_error: Option<String>,
    /// The writer channel, so a blocking command can send `view/prompt`.
    sender: mpsc::UnboundedSender<Message>,
    /// Prompts waiting for an answer, keyed by prompt id.
    prompts: Arc<Mutex<HashMap<String, std_mpsc::Sender<PromptValue>>>>,
    /// Global prompt-id counter; ids are unique across sessions.
    next_prompt: Arc<AtomicU64>,
    /// Running mutations keyed by their progress token, so a client cancel can
    /// kill the child on the blocking worker thread.
    progress: Arc<Mutex<HashMap<String, CancelHandle>>>,
    /// Global progress-token counter.
    next_progress: Arc<AtomicU64>,
    /// Per-repository mutation tickets, allocated in request order.
    mutation_gates: Arc<Mutex<HashMap<String, Arc<MutationGate>>>>,
}

impl Shared {
    fn new(
        config: Arc<ConfigService>,
        startup_error: Option<String>,
        sender: mpsc::UnboundedSender<Message>,
    ) -> Self {
        Self {
            repo_locks: Arc::new(AsyncMutex::new(HashMap::new())),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            view_repos: Arc::new(Mutex::new(HashMap::new())),
            next_view: Arc::new(AtomicU64::new(0)),
            config,
            client: Arc::new(Mutex::new(ClientCaps::default())),
            startup_error,
            sender,
            prompts: Arc::new(Mutex::new(HashMap::new())),
            next_prompt: Arc::new(AtomicU64::new(0)),
            progress: Arc::new(Mutex::new(HashMap::new())),
            next_progress: Arc::new(AtomicU64::new(0)),
            mutation_gates: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Reserve the next mutation slot for `repo_path`.
    fn reserve_mutation(&self, repo_path: String) -> MutationTurn {
        let gate = {
            let mut gates = self
                .mutation_gates
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            Arc::clone(
                gates
                    .entry(repo_path.clone())
                    .or_insert_with(|| Arc::new(MutationGate::new())),
            )
        };
        let ticket = gate.next.fetch_add(1, Ordering::SeqCst);
        MutationTurn {
            gate,
            ticket,
            repo_path,
        }
    }

    /// Allocate a prompt id, unique for this server process.
    fn next_prompt_id(&self) -> String {
        let next = self.next_prompt.fetch_add(1, Ordering::Relaxed) + 1;
        format!("p{next}")
    }

    /// Announce a mutation with the LSP work-done handshake.
    ///
    /// The standard handshake is a `window/workDoneProgress/create` request for
    /// the token, then `$/progress` with `begin`; `end` is sent when the
    /// mutation finishes. The returned handle kills the child on cancel.
    fn begin_progress(
        &self,
        sender: &mpsc::UnboundedSender<Message>,
        command: &str,
    ) -> (String, CancelHandle) {
        let next = self.next_progress.fetch_add(1, Ordering::Relaxed) + 1;
        let token = format!("magit-lsp/mutation/{next}");
        let handle = CancelHandle::new();
        self.progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(token.clone(), handle.clone());
        let _ = sender.send(Message::Request(Request::new(
            RequestId::from(format!("workDoneProgress/{next}")),
            "window/workDoneProgress/create".to_string(),
            serde_json::json!({ "token": token }),
        )));
        let _ = sender.send(Message::Notification(Notification::new(
            "$/progress".to_string(),
            serde_json::json!({
                "token": token,
                "value": { "kind": "begin", "title": "Git", "message": command },
            }),
        )));
        (token, handle)
    }

    /// Finish the progress sequence started by [`Self::begin_progress`].
    fn end_progress(&self, sender: &mpsc::UnboundedSender<Message>, token: &str) {
        self.progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(token);
        let _ = sender.send(Message::Notification(Notification::new(
            "$/progress".to_string(),
            serde_json::json!({ "token": token, "value": { "kind": "end" } }),
        )));
    }

    /// Cancel the mutation running under a `window/workDoneProgress/cancel`.
    fn cancel_progress(&self, params: &Value) {
        let Some(token) = params.get("token").and_then(progress_token) else {
            return;
        };
        let handle = self
            .progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&token)
            .cloned();
        if let Some(handle) = handle {
            handle.cancel();
        }
    }

    /// Send a prompt notification and block until the client answers it.
    ///
    /// Returning `None` means the user cancelled: `view/promptRespond` drops the
    /// sender without a value, so `recv` fails.
    fn ask(&self, request: &ViewPromptParams) -> Option<PromptValue> {
        let prompt_id = self.next_prompt_id();
        let params = ViewPromptParams {
            prompt_id: prompt_id.clone(),
            ..request.clone()
        };
        let (tx, rx) = std_mpsc::channel();
        self.prompts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(prompt_id.clone(), tx);

        let sent = self
            .sender
            .send(Message::Notification(Notification::new(
                VIEW_PROMPT.to_string(),
                params,
            )))
            .is_ok();

        let answer = if sent { rx.recv().ok() } else { None };

        self.prompts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&prompt_id);
        answer
    }

    /// Deliver a `view/promptRespond` answer to the waiting command.
    fn respond_prompt(
        &self,
        prompt_id: &str,
        cancelled: bool,
        value: Option<PromptValue>,
    ) -> Result<()> {
        let sender = self
            .prompts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(prompt_id)
            .ok_or_else(|| Error::new(format!("unknown prompt: {prompt_id}")))?;
        if !cancelled {
            if let Some(value) = value {
                let _ = sender.send(value);
            }
        }
        // Dropping the sender without a value is the cancellation signal.
        Ok(())
    }

    /// How the client wants keymaps delivered.
    fn delivery(&self) -> KeymapDelivery {
        self.client
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .delivery
    }

    /// The repository root that owns `uri`, found through its open view.
    fn repo_for_uri(&self, uri: &str) -> Option<String> {
        self.sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .find(|(_, session)| {
                session
                    .views()
                    .iter()
                    .any(|view| view.descriptor().uri == uri)
            })
            .map(|(repo, _)| repo.clone())
    }

    /// Allocate a view id, unique for this server process.
    fn next_view_id(&self) -> String {
        let next = self.next_view.fetch_add(1, Ordering::Relaxed) + 1;
        format!("v{next}")
    }

    /// The repository root that owns `view_id`.
    fn repo_for_view(&self, view_id: &str) -> Option<String> {
        self.view_repos
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(view_id)
            .cloned()
    }

    /// The last acknowledged tree of the view showing `uri`, if any.
    ///
    /// Positional requests resolve through the view only, so they cannot refer
    /// to a render the client has not applied yet.
    fn view_tree_for_uri(&self, uri: &str) -> Option<Tree> {
        let sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        sessions
            .values()
            .find_map(|session| session.acked_tree_for_uri(uri))
    }

    /// Whether the view showing `uri` is in text mode.
    fn is_text_uri(&self, uri: &str) -> bool {
        let sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        sessions.values().any(|session| session.is_text_uri(uri))
    }
}

/// The server's [`PromptUi`]: sends each question to the client and blocks the
/// calling worker thread until the matching `view/promptRespond` arrives.
struct ServerUi {
    shared: Shared,
}

impl ServerUi {
    fn new(shared: &Shared) -> Self {
        Self {
            shared: shared.clone(),
        }
    }

    /// Ask one question and convert the wire value to the expected kind.
    fn ask(&self, request: &ViewPromptParams) -> Result<Option<PromptValue>> {
        Ok(self.shared.ask(request))
    }
}

impl PromptUi for ServerUi {
    fn confirm(&self, request: &ViewPromptParams) -> Result<Option<bool>> {
        match self.ask(request)? {
            Some(PromptValue::Bool(value)) => Ok(Some(value)),
            Some(other) => Err(Error::new(format!(
                "expected a bool answer, got {other:?}"
            ))),
            None => Ok(None),
        }
    }

    fn text(&self, request: &ViewPromptParams) -> Result<Option<String>> {
        match self.ask(request)? {
            Some(PromptValue::Text(value)) => Ok(Some(value)),
            Some(other) => Err(Error::new(format!(
                "expected a text answer, got {other:?}"
            ))),
            None => Ok(None),
        }
    }

    fn choice(&self, request: &ViewPromptParams) -> Result<Option<PromptChoiceAnswer>> {
        match self.ask(request)? {
            Some(PromptValue::Choice(value)) => Ok(Some(value)),
            Some(other) => Err(Error::new(format!(
                "expected a choice answer, got {other:?}"
            ))),
            None => Ok(None),
        }
    }

    fn multi_choice(&self, request: &ViewPromptParams) -> Result<Option<Vec<String>>> {
        match self.ask(request)? {
            Some(PromptValue::Multi { ids }) => Ok(Some(ids)),
            Some(other) => Err(Error::new(format!(
                "expected a multiChoice answer, got {other:?}"
            ))),
            None => Ok(None),
        }
    }
}

/// Recompute and push `view/keymap` for every open view after a config reload.
fn refresh_view_keymaps(shared: &Shared, sender: &mpsc::UnboundedSender<Message>) {
    if shared.delivery() != KeymapDelivery::Dynamic {
        return;
    }
    let keymaps = shared.config.keymaps();
    let mut sessions = shared
        .sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for session in sessions.values_mut() {
        session.set_keymaps(keymaps.clone());
        let ids: Vec<String> = session
            .views()
            .iter()
            .map(|view| view.descriptor().id.clone())
            .collect();
        for id in ids {
            if let Some(keymap) = session.keymap_notice(&id) {
                let _ = sender.send(Message::Notification(Notification::new(
                    VIEW_KEYMAP.to_string(),
                    keymap,
                )));
            }
        }
    }
}

/// Run the LSP server on stdio until the client requests shutdown.
pub fn run() -> Result<()> {
    run_with(ServerOptions::default())
}

/// Run the LSP server with the given startup options.
pub fn run_with(options: ServerOptions) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(Error::from)?;

    let code = runtime.block_on(serve(options))?;
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}

/// Read framed messages until `exit`, then return the process exit code.
async fn serve(options: ServerOptions) -> Result<i32> {
    let mut reader = BufReader::new(tokio::io::stdin());
    let (sender, receiver) = mpsc::unbounded_channel();
    let writer = tokio::spawn(writer_loop(tokio::io::stdout(), receiver));

    // The config is loaded before the client connects, so a broken file in the
    // default location cannot prevent startup. Its error is shown on initialize.
    let load_options = LoadOptions {
        config_path: options.config_path.clone(),
        ..Default::default()
    };
    let (config, startup_error) = match ConfigService::load(load_options.clone()) {
        Ok(service) => (Arc::new(service), None),
        Err(error) => {
            tracing::warn!(%error, "configuration failed to load; using defaults");
            let message = error.to_string();
            (
                Arc::new(ConfigService::with_defaults(load_options, error)),
                Some(message),
            )
        }
    };
    let shared = Shared::new(config, startup_error, sender.clone());

    // Poll the loaded files for changes. A 150 ms interval doubles as the
    // debounce, so a burst of writes reloads once.
    let watcher = {
        let config = shared.config.clone();
        let sender = sender.clone();
        let shared = shared.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                let reload = config.clone();
                let outcome = tokio::task::spawn_blocking(move || reload.reload(false))
                    .await
                    .unwrap_or_default();
                let changed = outcome.changed;
                let had_errors = !outcome.errors.is_empty();
                for error in outcome.errors {
                    let _ = sender.send(show_message(1, format!("magit-lsp config: {error}")));
                }
                if changed && !had_errors {
                    refresh_view_keymaps(&shared, &sender);
                    tracing::debug!("configuration reloaded");
                }
            }
        })
    };

    let mut handlers = JoinSet::new();
    let mut shutdown_requested = false;

    tracing::debug!("magit-lsp listening on stdio");

    let exit_code = loop {
        match transport::read_message(&mut reader).await {
            Ok(Some(Message::Request(request))) => {
                if request.method == "shutdown" {
                    shutdown_requested = true;
                    // Earlier requests must finish (and answer) before the
                    // shutdown response, even if the client pipelined them.
                    while handlers.join_next().await.is_some() {}
                    let response = Response::new_ok(request.id, Value::Null);
                    let _ = sender.send(Message::Response(response));
                    continue;
                }
                tracing::debug!(method = %request.method, "request");
                let turn = mutation_repo_path(&shared, &request.method, &request.params)
                    .map(|repo_path| shared.reserve_mutation(repo_path));
                handlers.spawn(handle_request(
                    request,
                    shared.clone(),
                    sender.clone(),
                    turn,
                ));
            }
            Ok(Some(Message::Notification(notification))) => match notification.method.as_str() {
                "exit" => break if shutdown_requested { 0 } else { 1 },
                // Acks and resizes update session state; they never answer.
                "view/rendered" | "view/resize" => {
                    for message in handle_view_notification(&shared, &notification) {
                        let _ = sender.send(message);
                    }
                }
                // The client's settings changed; reload and report failures.
                "workspace/didChangeConfiguration" => {
                    let config = shared.config.clone();
                    let outcome = tokio::task::spawn_blocking(move || config.reload(true))
                        .await
                        .unwrap_or_default();
                    let had_errors = !outcome.errors.is_empty();
                    for error in outcome.errors {
                        let _ =
                            sender.send(show_message(1, format!("magit-lsp config: {error}")));
                    }
                    if !had_errors {
                        refresh_view_keymaps(&shared, &sender);
                    }
                    tracing::debug!("configuration reloaded on client request");
                }
                // The client aborts the mutation running under this token.
                "window/workDoneProgress/cancel" => {
                    shared.cancel_progress(&notification.params);
                }
                // The server owns the document; other client notifications are
                // not needed to keep view state in sync.
                _ => {}
            },
            Ok(Some(Message::Response(_))) => {}
            Ok(None) => break 0,
            Err(error) => {
                return Err(Error::new(format!("failed to read LSP message: {error}")));
            }
        }
    };

    // In-flight work must not keep the process alive after `exit`; queued
    // responses are already on the writer channel.
    watcher.abort();
    let _ = watcher.await;
    handlers.abort_all();
    while handlers.join_next().await.is_some() {}
    // `Shared` also holds a sender clone, so it must go before the writer
    // channel can close.
    drop(shared);
    drop(sender);

    match writer.await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => return Err(Error::from(error)),
        Err(error) => return Err(Error::new(error.to_string())),
    }
    Ok(exit_code)
}

/// Own stdout: serialise every response through one channel so bytes never
/// interleave, and flush each frame before reading the next.
async fn writer_loop<W>(
    mut writer: W,
    mut receiver: mpsc::UnboundedReceiver<Message>,
) -> std::io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    while let Some(message) = receiver.recv().await {
        transport::write_message(&mut writer, &message).await?;
    }
    Ok(())
}

async fn handle_request(
    request: Request,
    shared: Shared,
    sender: mpsc::UnboundedSender<Message>,
    turn: Option<MutationTurn>,
) {
    let id = request.id.clone();
    let method = request.method.clone();
    let turn = turn;

    let (response, notifications) = if !is_request_method(&method) {
        (
            Response::new_err(
                id,
                ErrorCode::MethodNotFound as i32,
                format!("unknown method: {method}"),
            ),
            Vec::new(),
        )
    } else if let Some(active) = turn.as_ref() {
        active.wait().await;
        let lock = repository_lock(&shared.repo_locks, &active.repo_path).await;
        let _guard = lock.lock().await;
        maybe_test_delay().await;
        let (token, cancel) = shared.begin_progress(&sender, &method);
        let outcome = dispatch_response(request, &shared, Some(cancel)).await;
        shared.end_progress(&sender, &token);
        outcome
    } else {
        dispatch_response(request, &shared, None).await
    };

    // The response creates or identifies the view, so it goes first; render
    // notifications then refer to something the client already knows.
    let _ = sender.send(Message::Response(response));
    for notification in notifications {
        let _ = sender.send(notification);
    }
    // Release the mutation slot only after this request's messages are queued,
    // so responses for one repository keep their request order.
    drop(turn);
}

/// A successful request plus any notifications it produced.
struct Dispatched {
    result: Value,
    notifications: Vec<Message>,
}

impl Dispatched {
    fn value(result: Value) -> Self {
        Self {
            result,
            notifications: Vec::new(),
        }
    }

    fn with(result: Value, notifications: Vec<Message>) -> Self {
        Self {
            result,
            notifications,
        }
    }
}

/// Run the synchronous Git work off the runtime, keeping the reader free.
///
/// `cancel` is installed on the blocking thread so the Git child can register
/// itself with it and be killed by `window/workDoneProgress/cancel`.
async fn dispatch_response(
    request: Request,
    shared: &Shared,
    cancel: Option<CancelHandle>,
) -> (Response, Vec<Message>) {
    let id = request.id.clone();
    let method = request.method.clone();
    let shared = shared.clone();

    match tokio::task::spawn_blocking(move || {
        runner::with_cancel(cancel, || dispatch(&request, &shared))
    })
    .await
    {
        Ok(Ok(dispatched)) => (
            Response::new_ok(id, dispatched.result),
            dispatched.notifications,
        ),
        Ok(Err(error)) => {
            tracing::warn!(method = %method, error = %error, "request failed");
            (error_response(id, error), Vec::new())
        }
        Err(error) => (
            Response::new_err(
                id,
                ErrorCode::InternalError as i32,
                format!("request task failed: {error}"),
            ),
            Vec::new(),
        ),
    }
}

/// Apply a client notification that changes session state.
fn handle_view_notification(shared: &Shared, notification: &Notification) -> Vec<Message> {
    match notification.method.as_str() {
        VIEW_RENDERED => {
            let Ok(params) =
                serde_json::from_value::<ViewRenderedParams>(notification.params.clone())
            else {
                tracing::warn!("ignoring malformed view/rendered");
                return Vec::new();
            };
            let Some(repo) = shared.repo_for_view(&params.view_id) else {
                return Vec::new();
            };
            let mut sessions = shared
                .sessions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(session) = sessions.get_mut(&repo) {
                if let Err(error) = session.ack(&params.view_id, params.version) {
                    tracing::warn!(%error, "failed to record view ack");
                }
            }
            Vec::new()
        }
        VIEW_RESIZE => {
            let Ok(params) =
                serde_json::from_value::<ViewResizeParams>(notification.params.clone())
            else {
                tracing::warn!("ignoring malformed view/resize");
                return Vec::new();
            };
            let Some(repo) = shared.repo_for_view(&params.view_id) else {
                return Vec::new();
            };
            let mut sessions = shared
                .sessions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let Some(session) = sessions.get_mut(&repo) else {
                return Vec::new();
            };
            let viewport = Viewport {
                cols: params.cols,
                rows: params.rows,
            };
            match session.resize(&params.view_id, viewport) {
                // A menu re-flows its columns for the new width.
                Ok(Some(render)) => vec![Message::Notification(Notification::new(
                    VIEW_RENDER.to_string(),
                    render,
                ))],
                Ok(None) => Vec::new(),
                Err(error) => {
                    tracing::warn!(%error, "failed to resize view");
                    Vec::new()
                }
            }
        }
        _ => Vec::new(),
    }
}

/// Serialise mutations for one repository while reads stay concurrent.
async fn repository_lock(locks: &RepoLocks, repo_path: &str) -> Arc<AsyncMutex<()>> {
    let mut locks = locks.lock().await;
    locks
        .entry(repo_path.to_string())
        .or_insert_with(|| Arc::new(AsyncMutex::new(())))
        .clone()
}

/// Debug-only delay so tests can prove reads are not blocked by mutations.
#[cfg(debug_assertions)]
async fn maybe_test_delay() {
    let Ok(millis) = std::env::var("MAGIT_LSP_TEST_DELAY_MS") else {
        return;
    };
    let Ok(millis) = millis.parse::<u64>() else {
        return;
    };
    if millis > 0 {
        let _ = tokio::task::spawn_blocking(move || {
            std::thread::sleep(std::time::Duration::from_millis(millis));
        })
        .await;
    }
}

/// Release builds ignore the test hook entirely.
#[cfg(not(debug_assertions))]
async fn maybe_test_delay() {}

fn dispatch(request: &Request, shared: &Shared) -> Result<Dispatched> {
    let params = request.params.clone();
    match request.method.as_str() {
        "initialize" => {
            let mut notifications = Vec::new();
            let (delivery, key_modifiers) = parse_client_capabilities(&params);
            {
                let mut client = shared
                    .client
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                client.delivery = delivery;
                client.key_modifiers = key_modifiers;
            }
            tracing::debug!(?delivery, ?key_modifiers, "client key capabilities");
            let init = params
                .get("initializationOptions")
                .cloned()
                .filter(|value| !value.is_null());
            if let Some(value) = init {
                let outcome = shared.config.set_initialization(Some(value));
                for error in &outcome.errors {
                    notifications.push(show_message(1, format!("magit-lsp config: {error}")));
                }
            }
            if let Some(error) = &shared.startup_error {
                notifications.push(show_message(1, format!("magit-lsp config: {error}")));
            }
            tracing::debug!(files = ?shared.config.files(), "configuration ready");
            Ok(Dispatched::with(
                serde_json::json!({ "capabilities": capabilities()? }),
                notifications,
            ))
        }
        "shutdown" => Ok(Dispatched::value(Value::Null)),
        // A client may ask us to prepare a progress token; there is no state to
        // create, so acknowledge it.
        "window/workDoneProgress/create" => Ok(Dispatched::value(Value::Null)),
        ViewOpenRequest::METHOD => open_view(shared, params),
        ViewCloseRequest::METHOD => close_view(shared, params),
        ViewInvokeRequest::METHOD => invoke_view(shared, params),
        ViewKeyRequest::METHOD => key_view(shared, params),
        ViewSetModeRequest::METHOD => set_mode_view(shared, params),
        ViewPromptRespondRequest::METHOD => {
            let p: ViewPromptRespondParams = from_params(params)?;
            shared.respond_prompt(&p.prompt_id, p.cancelled, p.value)?;
            Ok(Dispatched::value(Value::Null))
        }
        ViewResyncRequest::METHOD => {
            let p: ViewResyncParams = from_params(params)?;
            let repo = shared
                .repo_for_view(&p.view_id)
                .ok_or_else(|| Error::new(format!("unknown view: {}", p.view_id)))?;
            let sessions = shared
                .sessions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let session = sessions
                .get(&repo)
                .ok_or_else(|| Error::new(format!("unknown view: {}", p.view_id)))?;
            Ok(Dispatched::value(serde_json::to_value(
                session.resync(&p.view_id)?,
            )?))
        }
        "textDocument/documentSymbol" => {
            let p: lsp_types::DocumentSymbolParams = from_params(params)?;
            let uri = p.text_document.uri.as_str();
            if shared.is_text_uri(uri) {
                return Ok(Dispatched::value(Value::Null));
            }
            let symbols = tree_for_positional_request(shared, uri)
                .map(|tree| tree.document_symbols())
                .unwrap_or_default();
            Ok(Dispatched::value(serde_json::to_value(symbols)?))
        }
        "textDocument/definition" => {
            let p: lsp_types::GotoDefinitionParams = from_params(params)?;
            let position = p.text_document_position_params;
            let uri = position.text_document.uri.as_str();
            if shared.is_text_uri(uri) {
                return Ok(Dispatched::value(Value::Null));
            }
            let location = tree_for_positional_request(shared, uri).and_then(|tree| {
                let repo = shared.repo_for_uri(uri)?;
                tree.definition(&repo, position.position)
            });
            Ok(Dispatched::value(serde_json::to_value(location)?))
        }
        "textDocument/documentLink" => {
            let p: lsp_types::DocumentLinkParams = from_params(params)?;
            let uri = p.text_document.uri.as_str();
            if shared.is_text_uri(uri) {
                return Ok(Dispatched::value(Value::Null));
            }
            let links = tree_for_positional_request(shared, uri).and_then(|tree| {
                let repo = shared.repo_for_uri(uri)?;
                Some(tree.document_links(&repo))
            });
            Ok(Dispatched::value(serde_json::to_value(
                links.unwrap_or_default(),
            )?))
        }
        "textDocument/foldingRange" => {
            let p: lsp_types::FoldingRangeParams = from_params(params)?;
            let uri = p.text_document.uri.as_str();
            if shared.is_text_uri(uri) {
                return Ok(Dispatched::value(Value::Null));
            }
            let ranges = tree_for_positional_request(shared, uri)
                .map(|tree| tree.folding_ranges())
                .unwrap_or_default();
            Ok(Dispatched::value(serde_json::to_value(ranges)?))
        }
        "textDocument/semanticTokens/full" => {
            let p: lsp_types::SemanticTokensParams = from_params(params)?;
            let uri = p.text_document.uri.as_str();
            if shared.is_text_uri(uri) {
                return Ok(Dispatched::value(Value::Null));
            }
            let tokens = tree_for_positional_request(shared, uri)
                .map(|tree| semantic_tokens::encode(&tree))
                .unwrap_or_default();
            Ok(Dispatched::value(serde_json::to_value(
                SemanticTokensResult::Tokens(tokens),
            )?))
        }
        other => Err(Error::new(format!("unknown method: {other}"))),
    }
}

/// The last acked tree of the view showing `uri`.
fn tree_for_positional_request(shared: &Shared, uri: &str) -> Option<Tree> {
    shared.view_tree_for_uri(uri)
}

fn open_view(shared: &Shared, params: Value) -> Result<Dispatched> {
    let p: ViewOpenParams = from_params(params)?;
    let repo = p
        .repo_path()
        .ok_or_else(|| Error::new("view/open needs args.repo_path for a status view"))?
        .to_string();
    let id = shared.next_view_id();
    let keymaps = shared.config.keymaps();
    let mut sessions = shared
        .sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = sessions
        .entry(repo.clone())
        .or_insert_with(|| Session::new(&repo));
    session.set_keymaps(keymaps);
    let (descriptor, render) = session.open_with_args(
        id.clone(),
        p.kind,
        p.placement.unwrap_or_default(),
        p.viewport,
        p.args.as_ref(),
    )?;
    let keymap = session.keymap_notice(&id);
    drop(sessions);
    shared
        .view_repos
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(id, repo);

    let mut notifications = vec![Message::Notification(Notification::new(
        VIEW_RENDER.to_string(),
        render,
    ))];
    if shared.delivery() == KeymapDelivery::Dynamic {
        if let Some(keymap) = keymap {
            notifications.push(Message::Notification(Notification::new(
                VIEW_KEYMAP.to_string(),
                keymap,
            )));
        }
    }
    Ok(Dispatched::with(
        serde_json::to_value(descriptor)?,
        notifications,
    ))
}

fn close_view(shared: &Shared, params: Value) -> Result<Dispatched> {
    let p: ViewCloseParams = from_params(params)?;
    let Some(repo) = shared.repo_for_view(&p.view_id) else {
        // The client may have closed the view before its own close landed.
        return Ok(Dispatched::value(Value::Null));
    };
    let mut sessions = shared
        .sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(session) = sessions.get_mut(&repo) {
        session.close(&p.view_id)?;
    }
    drop(sessions);
    shared
        .view_repos
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&p.view_id);
    Ok(Dispatched::value(Value::Null))
}

fn invoke_view(shared: &Shared, params: Value) -> Result<Dispatched> {
    let p: ViewInvokeParams = from_params(params)?;
    let repo = shared
        .repo_for_view(&p.view_id)
        .ok_or_else(|| Error::new(format!("unknown view: {}", p.view_id)))?;
    let mut sessions = shared
        .sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = sessions
        .get_mut(&repo)
        .ok_or_else(|| Error::new(format!("unknown view: {}", p.view_id)))?;
    let ui = ServerUi::new(shared);
    let outcome = session.invoke_ui(&p, &ui)?;
    drop(sessions);
    Ok(dispatched_outcome(shared, &repo, outcome)?)
}

/// Handle `view/key`: resolve one key step and answer like `view/invoke`.
fn key_view(shared: &Shared, params: Value) -> Result<Dispatched> {
    let p: ViewKeyParams = from_params(params)?;
    let repo = shared
        .repo_for_view(&p.view_id)
        .ok_or_else(|| Error::new(format!("unknown view: {}", p.view_id)))?;
    let keymaps = shared.config.keymaps();
    let mut sessions = shared
        .sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = sessions
        .get_mut(&repo)
        .ok_or_else(|| Error::new(format!("unknown view: {}", p.view_id)))?;
    session.set_keymaps(keymaps);
    let ui = ServerUi::new(shared);
    let outcome = session.key_ui(&p, &ui)?;
    drop(sessions);
    Ok(dispatched_outcome(shared, &repo, outcome)?)
}

/// Handle `view/setMode`: switch between interactive and text mode.
fn set_mode_view(shared: &Shared, params: Value) -> Result<Dispatched> {
    let p: ViewSetModeParams = from_params(params)?;
    let repo = shared
        .repo_for_view(&p.view_id)
        .ok_or_else(|| Error::new(format!("unknown view: {}", p.view_id)))?;
    let mut sessions = shared
        .sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = sessions
        .get_mut(&repo)
        .ok_or_else(|| Error::new(format!("unknown view: {}", p.view_id)))?;
    let ui = ServerUi::new(shared);
    let outcome = session.set_mode_ui(&p, &ui)?;
    drop(sessions);
    Ok(dispatched_outcome(shared, &repo, outcome)?)
}

/// Turn a command outcome into its response plus render/keymap notifications.
fn dispatched_outcome(shared: &Shared, repo: &str, outcome: InvokeResult) -> Result<Dispatched> {
    let mut notifications = Vec::new();
    if let Some(render) = outcome.render {
        notifications.push(Message::Notification(Notification::new(
            VIEW_RENDER.to_string(),
            render,
        )));
    }
    // A mode change replaces the binding set, so the client must see it even
    // when the scope's ordinary keymap would not change.
    if shared.delivery() == KeymapDelivery::Dynamic {
        if let Some(keymap) = outcome.keymap {
            notifications.push(Message::Notification(Notification::new(
                VIEW_KEYMAP.to_string(),
                keymap,
            )));
        }
    }

    // Views pushed or popped by an effect must be routable before the client
    // can send a key for them.
    {
        let mut view_repos = shared
            .view_repos
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for effect in &outcome.effects {
            match effect {
                Effect::OpenView { descriptor } | Effect::PushView { descriptor } => {
                    view_repos.insert(descriptor.id.clone(), repo.to_string());
                }
                Effect::PopView { view_id } => {
                    view_repos.remove(view_id);
                }
                Effect::Quit => {
                    view_repos.retain(|_, owner| owner != repo);
                }
                _ => {}
            }
        }
    }

    for (render, keymap) in outcome.opened {
        notifications.push(Message::Notification(Notification::new(
            VIEW_RENDER.to_string(),
            render,
        )));
        if shared.delivery() == KeymapDelivery::Dynamic {
            if let Some(keymap) = keymap {
                notifications.push(Message::Notification(Notification::new(
                    VIEW_KEYMAP.to_string(),
                    keymap,
                )));
            }
        }
    }

    Ok(Dispatched::with(
        serde_json::to_value(ViewInvokeResponse {
            effects: outcome.effects,
        })?,
        notifications,
    ))
}

/// Run a mutation for `repo_path`, reporting Git failures as message effects.
///
/// A failed Git command still produces a successful result with the current
/// status; only internal faults become JSON-RPC errors.
pub fn mutation<F>(repo_path: &str, op: F) -> Result<(git::Status, Vec<Effect>)>
where
    F: FnOnce(&GitRepo) -> Result<bool>,
{
    let repo = match GitRepo::open(repo_path) {
        Ok(repo) => repo,
        Err(err) => {
            let effect = Effect::from_error(&err).ok_or(err)?;
            return Ok((git::Status::empty(repo_path), vec![effect]));
        }
    };

    let effects = errors::effects_for_result(op(&repo))?;
    match repo.status() {
        Ok(status) => Ok((status, effects)),
        Err(err) => match Effect::from_error(&err) {
            Some(effect) => {
                let mut effects = effects;
                effects.push(effect);
                Ok((git::Status::empty(repo_path), effects))
            }
            None => Err(err),
        },
    }
}

/// A `window/showMessage` notification: 1 error, 2 warning, 3 info, 4 log.
fn show_message(kind: u8, message: String) -> Message {
    Message::Notification(Notification::new(
        "window/showMessage".to_string(),
        serde_json::json!({ "type": kind, "message": message }),
    ))
}

fn capabilities() -> Result<Value> {
    let mut capabilities = serde_json::to_value(ServerCapabilities {
        document_symbol_provider: Some(OneOf::Left(true)),
        definition_provider: Some(OneOf::Left(true)),
        document_link_provider: Some(DocumentLinkOptions {
            resolve_provider: Some(false),
            work_done_progress_options: Default::default(),
        }),
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: semantic_tokens::legend(),
                full: Some(SemanticTokensFullOptions::Bool(true)),
                range: None,
                work_done_progress_options: Default::default(),
            },
        )),
        ..Default::default()
    })
    .map_err(|e| Error::new(e.to_string()))?;
    // `window.workDoneProgress` is a client capability in the spec, but the
    // server advertises the window features it drives so thin clients can
    // enable them without a second handshake.
    if let Some(object) = capabilities.as_object_mut() {
        object.insert(
            "window".to_string(),
            serde_json::json!({ "workDoneProgress": true }),
        );
    }
    Ok(capabilities)
}

fn is_request_method(method: &str) -> bool {
    matches!(
        method,
        "initialize"
            | "shutdown"
            | "textDocument/documentSymbol"
            | "textDocument/definition"
            | "textDocument/documentLink"
            | "textDocument/foldingRange"
            | "textDocument/semanticTokens/full"
            | ViewOpenRequest::METHOD
            | ViewCloseRequest::METHOD
            | ViewInvokeRequest::METHOD
            | ViewKeyRequest::METHOD
            | ViewSetModeRequest::METHOD
            | ViewPromptRespondRequest::METHOD
            | ViewResyncRequest::METHOD
            | "window/workDoneProgress/create"
    )
}

/// The repository a mutation must lock; only the staging commands write.
fn mutation_repo_path(shared: &Shared, method: &str, params: &Value) -> Option<String> {
    // Keys run any command, so serialise every key press with its repo's views.
    if method == ViewKeyRequest::METHOD {
        let view_id = params.get("viewId").and_then(Value::as_str)?;
        return shared.repo_for_view(view_id);
    }
    if method != ViewInvokeRequest::METHOD {
        return None;
    }
    let command = params.get("command").and_then(Value::as_str)?;
    if !matches!(
        command,
        "status.stage" | "status.unstage" | "status.stage-all" | "status.unstage-all"
    ) {
        return None;
    }
    let view_id = params.get("viewId").and_then(Value::as_str)?;
    shared.repo_for_view(view_id)
}

/// A progress token is an integer or a string on the wire.
fn progress_token(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_string)
        .or_else(|| value.as_u64().map(|number| number.to_string()))
}

/// Read the key-delivery capabilities a client declared in `initialize`.
fn parse_client_capabilities(params: &Value) -> (KeymapDelivery, KeyModifiers) {
    let caps = params.get("capabilities");
    let delivery = caps
        .and_then(|caps| caps.get("keymapDelivery"))
        .and_then(Value::as_str)
        .map(KeymapDelivery::parse)
        .unwrap_or_default();
    let modifiers = caps
        .and_then(|caps| caps.get("keyModifiers"))
        .and_then(Value::as_str)
        .map(KeyModifiers::parse)
        .unwrap_or_default();
    (delivery, modifiers)
}

fn error_response(id: lsp_server::RequestId, err: Error) -> Response {
    let mut response = Response::new_err(
        id,
        lsp_server::ErrorCode::InternalError as i32,
        err.to_string(),
    );
    if let Some(user) = err.user_error() {
        if let Err(error) = &mut response.response_result {
            error.message = user.summary.clone();
            error.data = serde_json::to_value(user).ok();
        }
    }
    response
}

fn from_params<T: serde::de::DeserializeOwned>(params: Value) -> Result<T> {
    serde_json::from_value(params).map_err(|e| Error::new(e.to_string()))
}
