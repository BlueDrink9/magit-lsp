use crate::config::{ConfigService, LoadOptions};
use crate::editor;
use crate::errors;
use crate::protocol::*;
use crate::runner::{self, CancelHandle};
use crate::semantic_tokens;
use crate::session::{Execution, InvokeResult, PublishedViews, Session};
use crate::transport;
use crate::tree::Tree;
use crate::ui::PromptUi;
use crate::view::{ViewMode, Viewport};
use crate::watcher::{Refresh, RepoWatcher, WatcherConfig};
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
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    close_confirms: bool,
    work_done_progress: bool,
}

impl Default for ClientCaps {
    fn default() -> Self {
        Self {
            delivery: KeymapDelivery::Dynamic,
            close_confirms: true,
            work_done_progress: false,
        }
    }
}

/// Server state shared by concurrent request handlers.
#[derive(Clone)]
struct Shared {
    /// Trusted identity allocated for this transport, never read from client JSON.
    draft_owner: Arc<String>,
    repo_locks: RepoLocks,
    /// Open views per repository root.
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    /// Published view snapshots per repository root, readable without the
    /// session lock so a read never waits for a command running inside Git.
    published: Arc<Mutex<HashMap<String, PublishedViews>>>,
    /// View id → repository root, so notifications can find their session.
    view_repos: Arc<Mutex<HashMap<String, String>>>,
    /// Global view-id counter; ids are unique across repositories.
    next_view: Arc<AtomicU64>,
    /// Configuration and resolved keymaps.
    config: Arc<ConfigService>,
    /// Repository configuration associated with each pending bridge edit.
    edit_configs: Arc<Mutex<HashMap<String, Arc<ConfigService>>>>,
    /// Key delivery settings declared by the client in `initialize`.
    client: Arc<Mutex<ClientCaps>>,
    /// Why the initial configuration load failed, if it did.
    startup_error: Option<String>,
    /// The writer channel, so a blocking command can send `view/prompt`.
    sender: mpsc::UnboundedSender<Message>,
    /// Requests sent to the client, keyed by their JSON-RPC ID.
    outbound: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<Response>>>>,
    /// Unique IDs for server-initiated requests.
    next_outbound: Arc<AtomicU64>,
    /// The prompt transport, shared with the editor bridge.
    prompts: Arc<PromptBridge>,
    /// Running mutations keyed by their progress token, so a client cancel can
    /// kill the child on the blocking worker thread.
    progress: Arc<Mutex<HashMap<String, ProgressRun>>>,
    closing: Arc<AtomicBool>,
    progress_changed: Arc<Notify>,
    /// Global progress-token counter.
    next_progress: Arc<AtomicU64>,
    /// Per-repository mutation tickets, allocated in request order.
    mutation_gates: Arc<Mutex<HashMap<String, Arc<MutationGate>>>>,
    /// Running repository watchers, keyed by repository root.
    watchers: Arc<Mutex<HashMap<String, RepoWatcher>>>,
}

#[derive(Clone)]
struct ProgressRun {
    cancel: CancelHandle,
    edit_id: Option<String>,
    cancellation_requested: bool,
    repo: Option<String>,
    visible: bool,
}

/// Removes an outbound request if its waiter future is abandoned.
struct PendingOutbound {
    outbound: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<Response>>>>,
    key: String,
}

impl Drop for PendingOutbound {
    fn drop(&mut self) {
        self.outbound.lock().unwrap_or_else(|p| p.into_inner()).remove(&self.key);
    }
}

impl Shared {
    fn new(
        config: Arc<ConfigService>,
        startup_error: Option<String>,
        sender: mpsc::UnboundedSender<Message>,
    ) -> Self {
        static NEXT_CONNECTION: AtomicU64 = AtomicU64::new(0);
        Self {
            draft_owner: Arc::new(format!("connection-{}", NEXT_CONNECTION.fetch_add(1, Ordering::Relaxed))),
            repo_locks: Arc::new(AsyncMutex::new(HashMap::new())),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            published: Arc::new(Mutex::new(HashMap::new())),
            view_repos: Arc::new(Mutex::new(HashMap::new())),
            next_view: Arc::new(AtomicU64::new(0)),
            config,
            edit_configs: Arc::new(Mutex::new(HashMap::new())),
            client: Arc::new(Mutex::new(ClientCaps::default())),
            startup_error,
            sender: sender.clone(),
            outbound: Arc::new(Mutex::new(HashMap::new())),
            next_outbound: Arc::new(AtomicU64::new(0)),
            prompts: Arc::new(PromptBridge::new(&sender)),
            progress: Arc::new(Mutex::new(HashMap::new())),
            closing: Arc::new(AtomicBool::new(false)),
            progress_changed: Arc::new(Notify::new()),
            next_progress: Arc::new(AtomicU64::new(0)),
            mutation_gates: Arc::new(Mutex::new(HashMap::new())),
            watchers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register before sending so a fast response cannot arrive before its waiter exists.
    fn register_outbound(&self, method: &str, params: Value) -> Result<(PendingOutbound, tokio::sync::oneshot::Receiver<Response>)> {
        let next = self.next_outbound.fetch_add(1, Ordering::Relaxed) + 1;
        let id = RequestId::from(format!("magit-lsp/outbound/{next}"));
        let key = serde_json::to_string(&id).map_err(|e| Error::new(e.to_string()))?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut outbound = self.outbound.lock().unwrap_or_else(|p| p.into_inner());
        if self.closing.load(Ordering::SeqCst) {
            return Err(Error::new("client request transport is closed"));
        }
        outbound.insert(key.clone(), tx);
        if self.sender.send(Message::Request(Request::new(id, method.to_string(), params))).is_err() {
            outbound.remove(&key);
            return Err(Error::new("client request transport is closed"));
        }
        drop(outbound);
        Ok((PendingOutbound { outbound: self.outbound.clone(), key }, rx))
    }

    /// Await a registered request, removing it on response, timeout or cancellation.
    async fn await_outbound(&self, pending: PendingOutbound, rx: tokio::sync::oneshot::Receiver<Response>) -> Result<Response> {
        self.await_outbound_for(pending, rx, std::time::Duration::from_secs(30)).await
    }

    async fn await_outbound_for(&self, _pending: PendingOutbound, rx: tokio::sync::oneshot::Receiver<Response>, duration: std::time::Duration) -> Result<Response> {
        let response = tokio::time::timeout(duration, rx).await;
        match response {
            Ok(Ok(response)) => match &response.response_result {
                Ok(_) => Ok(response),
                Err(error) => Err(Error::new(error.message.clone())),
            },
            Ok(Err(_)) => Err(Error::new("client disconnected while awaiting response")),
            Err(_) => Err(Error::new("client request timed out")),
        }
    }

    /// Resolve a matching response; unknown and late IDs are ignored.
    fn resolve_outbound(&self, response: Response) {
        let Ok(key) = serde_json::to_string(&response.id) else { return; };
        if let Some(sender) = self.outbound.lock().unwrap_or_else(|p| p.into_inner()).remove(&key) {
            let _ = sender.send(response);
        }
    }

    /// Cancel every request still awaiting a client response.
    fn cancel_outbound(&self) {
        self.outbound.lock().unwrap_or_else(|p| p.into_inner()).clear();
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

    /// Start the repository watcher for `repo` unless one is already running
    /// (R-3.5).
    ///
    /// Watcher events are debounced by the watcher itself; the handler here
    /// routes each burst through the normal incremental-update path: it refreshes
    /// the repository's session (text-mode views queue instead of clobbering
    /// client edits) and sends every render as a `view/render` notification. A
    /// start failure only warns the user — watching is an enhancement, so the
    /// view still opens.
    fn ensure_watcher(&self, repo: &str) {
        let mut watchers = self
            .watchers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if watchers.contains_key(repo) {
            return;
        }
        let shared = self.clone();
        let repo_key = repo.to_string();
        let repo_for_handler = repo_key.clone();
        let result = RepoWatcher::start(
            Path::new(repo),
            WatcherConfig::default(),
            move |refresh: Refresh| match refresh {
                Refresh::Changed(_paths) => {
                    // The sessions lock also serialises request handlers, so a
                    // burst cannot race a server-owned mutation's session work.
                    let renders = {
                        let mut sessions = shared
                            .sessions
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        match sessions.get_mut(&repo_for_handler) {
                            Some(session) => session.refresh_all(),
                            None => Vec::new(),
                        }
                    };
                    for render in renders {
                        match serde_json::to_value(&render) {
                            Ok(value) => {
                                let _ = shared.sender.send(Message::Notification(
                                    Notification::new(VIEW_RENDER.to_string(), value),
                                ));
                            }
                            Err(error) => {
                                tracing::debug!(%error, "watcher render failed to serialise");
                            }
                        }
                    }
                }
                Refresh::IndexLocked => {
                    let _ = shared.sender.send(show_message(
                        2,
                        "magit-lsp: waiting for Git to release .git/index.lock before refreshing"
                            .to_string(),
                    ));
                }
            },
        );
        match result {
            Ok(watcher) => {
                watchers.insert(repo_key, watcher);
            }
            Err(error) => {
                tracing::debug!(%error, "repository watcher failed to start");
                let _ = self.sender.send(show_message(
                    2,
                    format!("magit-lsp: file watching unavailable ({error}); views refresh only on common.refresh"),
                ));
            }
        }
    }

    /// Stop and join the watcher for `repo` when its last view has closed.
    ///
    /// Views in other repositories keep their watcher; an idle repository no
    /// longer costs an inotify tree or a refresh thread.
    fn stop_watcher_if_idle(&self, repo: &str) {
        let mut watchers = self
            .watchers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let no_views_left = self
            .view_repos
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .values()
            .all(|owner| owner != repo);
        if no_views_left {
            if let Some(mut watcher) = watchers.remove(repo) {
                watcher.join();
            }
        }
    }

    /// Announce a mutation with the LSP work-done handshake.
    ///
    /// The standard handshake is a `window/workDoneProgress/create` request for
    /// the token, then `$/progress` with `begin`; `end` is sent when the
    /// mutation finishes. The returned handle enqueues cancellation for the
    /// child on cancel.
    fn begin_progress(
        &self,
        sender: &mpsc::UnboundedSender<Message>,
        command: &str,
        edit_id: Option<String>,
        repo: Option<&str>,
    ) -> (String, CancelHandle) {
        let next = self.next_progress.fetch_add(1, Ordering::Relaxed) + 1;
        let token = format!("magit-lsp/mutation/{next}");
        let handle = CancelHandle::new();
        let mut progress = self.progress.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let closing = self.closing.load(Ordering::SeqCst);
        progress.insert(token.clone(), ProgressRun { cancel: handle.clone(), edit_id: edit_id.clone(), cancellation_requested: false, repo: repo.map(str::to_string), visible: false });
        drop(progress);
        if closing {
            handle.cancel();
            if let Some(edit_id) = edit_id { editor::registry().resolve(&edit_id, editor::Reply::Abort); }
        }
        let enabled = self.client.lock().unwrap_or_else(|p| p.into_inner()).work_done_progress;
        if enabled {
            let shared = self.clone();
            let sender = sender.clone();
            let token = token.clone();
            let command = command.to_string();
            tokio::spawn(async move {
                shared.create_progress(&sender, &token, &command).await;
            });
        }
        (token, handle)
    }

    /// Publish progress only after successful creation, while its owner is live.
    async fn create_progress(&self, sender: &mpsc::UnboundedSender<Message>, token: &str, command: &str) {
        let Ok((pending, rx)) = self.register_outbound("window/workDoneProgress/create", serde_json::json!({ "token": token })) else { return; };
        if self.await_outbound(pending, rx).await.is_err() { return; }
        let mut runs = self.progress.lock().unwrap_or_else(|p| p.into_inner());
        if self.closing.load(Ordering::SeqCst) { return; }
        if let Some(run) = runs.get_mut(token) {
            run.visible = true;
            let _ = sender.send(Message::Notification(Notification::new("$/progress".to_string(), serde_json::json!({ "token": token, "value": { "kind": "begin", "title": "Git", "message": command, "cancellable": true } }))));
        }
    }

    /// Finish the progress sequence started by [`Self::begin_progress`].
    fn end_progress(&self, sender: &mpsc::UnboundedSender<Message>, token: &str) {
        // Serialize report/end sends with removal, so a racing cancel cannot
        // resurrect an ended token. Repeated terminal release is harmless.
        let mut progress = self.progress.lock().unwrap_or_else(|p| p.into_inner());
        let Some(run) = progress.remove(token) else { return; };
        if !run.visible { self.progress_changed.notify_waiters(); return; }
        let _ = sender.send(Message::Notification(Notification::new(
            "$/progress".to_string(),
            serde_json::json!({ "token": token, "value": { "kind": "end" } }),
        )));
        self.progress_changed.notify_waiters();
    }

    /// Stop new work and cancel every operation that can wait on the client.
    fn terminal_cleanup(&self) {
        let runs = {
            let progress = self.progress.lock().unwrap_or_else(|p| p.into_inner());
            // Serialize the terminal boundary with publication of begin.
            self.closing.store(true, Ordering::SeqCst);
            progress.values().cloned().collect::<Vec<_>>()
        };
        self.cancel_outbound();
        for run in runs {
            run.cancel.cancel();
            if let Some(edit_id) = run.edit_id { editor::registry().resolve(&edit_id, editor::Reply::Abort); }
        }
        editor::registry().abort_all();
        self.prompts.close();
    }

    /// Wait until each progress-owning worker has run its terminal release.
    async fn wait_progress(&self) {
        loop {
            let notified = self.progress_changed.notified();
            if self.progress.lock().unwrap_or_else(|p| p.into_inner()).is_empty() { return; }
            notified.await;
        }
    }

    /// Cancel the mutation running under a `window/workDoneProgress/cancel`.
    fn cancel_progress(&self, params: &Value) {
        let Some(token) = params.get("token").and_then(progress_token) else {
            return;
        };
        let mut progress = self
            .progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(run) = progress.get_mut(&token) {
            if run.cancellation_requested { return; }
            run.cancellation_requested = true;
            // Acknowledge intent, not termination or repository outcome. Keep
            // progress and the repository reservation until the owner finishes.
            if run.visible { let _ = self.sender.send(Message::Notification(Notification::new(
                "$/progress".to_string(),
                serde_json::json!({ "token": token, "value": {
                    "kind": "report", "message": "Cancelling…", "cancellable": false,
                } }),
            ))); }
            let _ = self.sender.send(Message::Notification(Notification::new(
                VIEW_EFFECTS.to_string(),
                ViewEffectsParams { effects: vec![Effect::message(MessageLevel::Info, "Cancelling…".to_string())] },
            )));
            run.cancel.cancel();
            if let Some(edit_id) = &run.edit_id {
                editor::registry().resolve(edit_id, editor::Reply::Abort);
            }
        }
    }

    /// Refuse new commands until a cancelled owner has finished cleanup.
    fn cancellation_pending(&self, repo: &str) -> bool {
        self.progress.lock().unwrap_or_else(|p| p.into_inner()).values()
            .any(|run| run.cancellation_requested && run.repo.as_deref() == Some(repo))
    }

    /// Deliver a `view/promptRespond` answer to the waiting command.
    fn respond_prompt(
        &self,
        prompt_id: &str,
        cancelled: bool,
        value: Option<PromptValue>,
    ) -> Result<()> {
        self.prompts.respond(prompt_id, cancelled, value)
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

/// Whether the client wants the server to push every view's keymap.
fn dynamic_delivery(client: &Arc<Mutex<ClientCaps>>) -> bool {
    client
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .delivery
        == KeymapDelivery::Dynamic
}

/// The prompt transport: a `view/prompt` notification and its answer.
///
/// The editor bridge's askpass prompt runs on a thread that outlives the
/// server, so the bridge holds a *weak* sender: it must never keep the writer
/// channel (and with it the server process) alive.
struct PromptBridge {
    closed: AtomicBool,
    sender: mpsc::WeakUnboundedSender<Message>,
    /// Questions waiting for an answer, keyed by prompt id.
    pending: Mutex<HashMap<String, std_mpsc::Sender<PromptValue>>>,
    /// Prompt-id counter; ids are unique across this server process.
    next_id: AtomicU64,
}

impl PromptBridge {
    fn new(sender: &mpsc::UnboundedSender<Message>) -> Self {
        Self {
            closed: AtomicBool::new(false),
            sender: sender.downgrade(),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(0),
        }
    }

    /// Send one question and block until the client answers it.
    ///
    /// `None` means the user cancelled: `view/promptRespond` drops the sender
    /// without a value, so `recv` fails. A prompt that cannot be delivered (the
    /// writer is gone) is cancelled as well.
    fn ask(&self, request: &ViewPromptParams) -> Option<PromptValue> {
        let prompt_id = format!("p{}", self.next_id.fetch_add(1, Ordering::Relaxed) + 1);
        let params = ViewPromptParams {
            prompt_id: prompt_id.clone(),
            ..request.clone()
        };
        let (tx, rx) = std_mpsc::channel();
        {
            let mut pending = self.lock();
            if self.closed.load(Ordering::SeqCst) {
                return None;
            }
            pending.insert(prompt_id.clone(), tx);
        }

        let sent = match self.sender.upgrade() {
            Some(sender) => sender
                .send(Message::Notification(Notification::new(
                    VIEW_PROMPT.to_string(),
                    params,
                )))
                .is_ok(),
            None => false,
        };

        let cancel = runner::current_cancel();
        let answer = if sent {
            loop {
                if cancel.as_ref().is_some_and(|handle| handle.is_cancelled()) {
                    break None;
                }
                match rx.recv_timeout(std::time::Duration::from_millis(50)) {
                    Ok(answer) => break Some(answer),
                    Err(std_mpsc::RecvTimeoutError::Disconnected) => break None,
                    Err(std_mpsc::RecvTimeoutError::Timeout) => {}
                }
            }
        } else {
            None
        };

        self.lock().remove(&prompt_id);
        answer
    }

    /// Deliver an answer to the waiting question.
    fn respond(&self, prompt_id: &str, cancelled: bool, value: Option<PromptValue>) -> Result<()> {
        let sender = self
            .lock()
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

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, std_mpsc::Sender<PromptValue>>> {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn close(&self) {
        let mut pending = self.lock();
        self.closed.store(true, Ordering::SeqCst);
        pending.clear();
    }
}

/// The server's [`PromptUi`]: sends each question to the client and blocks the
/// calling worker thread until the matching `view/promptRespond` arrives.
struct ServerUi {
    prompts: Arc<PromptBridge>,
}

impl ServerUi {
    fn new(shared: &Shared) -> Self {
        Self {
            prompts: shared.prompts.clone(),
        }
    }

    /// Ask one question and convert the wire value to the expected kind.
    fn ask(&self, request: &ViewPromptParams) -> Result<Option<PromptValue>> {
        Ok(self.prompts.ask(request))
    }
}

impl PromptUi for ServerUi {
    fn confirm(&self, request: &ViewPromptParams) -> Result<Option<bool>> {
        match self.ask(request)? {
            Some(PromptValue::Bool(value)) => Ok(Some(value)),
            Some(other) => Err(Error::new(format!("expected a bool answer, got {other:?}"))),
            None => Ok(None),
        }
    }

    fn text(&self, request: &ViewPromptParams) -> Result<Option<String>> {
        match self.ask(request)? {
            Some(PromptValue::Text(value)) => Ok(Some(value)),
            Some(other) => Err(Error::new(format!("expected a text answer, got {other:?}"))),
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
    let mut sessions = shared
        .sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for (root, session) in sessions.iter_mut() {
        session.set_keymaps(
            shared
                .config
                .for_repository(std::path::Path::new(root))
                .keymaps(),
        );
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
    if let Some(path) = editor::start_listener(
        {
            let sender = sender.downgrade();
            // The bridge's thread outlives the server, so it must not hold a
            // `Shared` (that would keep the writer channel alive for ever).
            let client = shared.client.clone();
            let configs = shared.edit_configs.clone();
            Arc::new(move |effects: Vec<Effect>| {
                let Some(sender) = sender.upgrade() else {
                    return;
                };
                let _ = sender.send(Message::Notification(Notification::new(
                    VIEW_EFFECTS.to_string(),
                    ViewEffectsParams {
                        effects: effects.clone(),
                    },
                )));
                // A dynamic client gets the edit's keymap as a normal
                // `view/keymap`, keyed by the edit id. Static clients use
                // their own bindings and ignore it.
                if dynamic_delivery(&client) {
                    for effect in &effects {
                        if let Effect::EditDocument { run_id, kind, .. } = effect {
                            let config = configs
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .get(run_id)
                                .cloned();
                            if let Some(keymap) =
                                config.and_then(|config| edit_keymap(&config, run_id, kind))
                            {
                                let _ = sender.send(Message::Notification(Notification::new(
                                    VIEW_KEYMAP.to_string(),
                                    keymap,
                                )));
                            }
                        }
                    }
                }
            })
        },
        {
            // The askpass prompt runs on the bridge's thread and outlives the
            // server, so it holds the prompt bridge (a weak sender), never the
            // whole `Shared`.
            let prompts = shared.prompts.clone();
            Arc::new(move |prompt: &str| {
                let ui = ServerUi {
                    prompts: prompts.clone(),
                };
                let request = ViewPromptParams {
                    prompt_id: String::new(),
                    kind: PromptKind::Text,
                    title: prompt.to_string(),
                    detail: None,
                    yes_label: None,
                    no_label: None,
                    destructive: None,
                    default: None,
                    placeholder: None,
                    secret: Some(true),
                    items: None,
                    allow_custom: None,
                    preselected: None,
                    min: None,
                    max: None,
                };
                match ui.text(&request) {
                    Ok(secret) => secret,
                    Err(error) => {
                        tracing::debug!(%error, "askpass prompt failed");
                        None
                    }
                }
            })
        },
    ) {
        tracing::debug!(socket = %path.display(), "editor bridge listening");
    }

    // Git asks for passwords through the same bridge: the askpass helper is
    // this very executable, so every spawned Git process inherits the command.
    match editor::askpass_command() {
        Ok(command) => editor::set_askpass_invocation(command),
        Err(error) => tracing::warn!(%error, "cannot locate the askpass helper"),
    }

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
                for error in outcome.errors {
                    let _ = sender.send(show_message(1, format!("magit-lsp config: {error}")));
                }
                if changed {
                    refresh_view_keymaps(&shared, &sender);
                    tracing::debug!("configuration reloaded");
                }
            }
        })
    };

    let mut handlers = JoinSet::new();
    let mut shutdown_requested = false;
    let mut read_error = None;

    tracing::debug!("magit-lsp listening on stdio");

    let exit_code = loop {
        match transport::read_message(&mut reader).await {
            Ok(Some(Message::Request(request))) => {
                if request.method == "shutdown" {
                    shutdown_requested = true;
                    shared.terminal_cleanup();
                    // Earlier requests must finish (and answer) before the
                    // shutdown response, even if the client pipelined them.
                    while handlers.join_next().await.is_some() {}
                    shared.wait_progress().await;
                    let response = Response::new_ok(request.id, Value::Null);
                    let _ = sender.send(Message::Response(response));
                    continue;
                }
                if shared.closing.load(Ordering::SeqCst) {
                    let _ = sender.send(Message::Response(Response::new_err(request.id, ErrorCode::InvalidRequest as i32, "server is shutting down".to_string())));
                    continue;
                }
                // Draft barriers and full-sync notifications execute on the
                // reader's ordered lane, never in unordered request tasks.
                if request.method == "workspace/executeCommand" {
                    let response = match draft_request(&shared, &request) {
                        Ok(result) => Response::new_ok(request.id, result),
                        Err(error) => {
                            let mut response = Response::new_err(request.id, ErrorCode::InvalidParams as i32, error.to_string());
                            if let Err(error) = &mut response.response_result {
                                let params = if request.method == "workspace/executeCommand" {
                                    request.params.pointer("/arguments/0").unwrap_or(&request.params)
                                } else { &request.params };
                                error.data = Some(serde_json::json!({
                                    "runId": params.get("runId"), "uri": params.get("uri"),
                                    "version": params.get("version"), "recoverable": true,
                                }));
                            }
                            response
                        }
                    };
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
                "exit" => {
                    // The client is going away: a pending edit can never be
                    // answered, so abort it and let the worker (and Git)
                    // finish instead of hanging (§3.10 step 7).
                    shared.terminal_cleanup();
                    break if shutdown_requested { 0 } else { 1 };
                }
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
                    for error in outcome.errors {
                        let _ = sender.send(show_message(1, format!("magit-lsp config: {error}")));
                    }
                    // Failed roots retain their last good maps; successful
                    // roots must still receive their independently reloaded maps.
                    refresh_view_keymaps(&shared, &sender);
                    tracing::debug!("configuration reloaded on client request");
                }
                // The client aborts the mutation running under this token.
                "window/workDoneProgress/cancel" => {
                    shared.cancel_progress(&notification.params);
                }
                "textDocument/didOpen" | "textDocument/didChange" | "textDocument/didClose" => {
                    let uri = notification.params.pointer("/textDocument/uri").and_then(Value::as_str).unwrap_or("");
                    if let Ok(run) = editor::registry().run_for_uri(&shared.draft_owner, uri) {
                        if let Err(error) = draft_notification(&shared, &notification) {
                            editor::registry().notification_failure(
                                &shared.draft_owner, &run, uri, error.to_string(),
                                notification.method == "textDocument/didClose", |effects| {
                                    let _ = sender.send(Message::Notification(Notification::new(
                                        VIEW_EFFECTS.to_string(), ViewEffectsParams { effects },
                                    )));
                                },
                            );
                        }
                    }
                }
                // The server owns the document; other client notifications are
                // not needed to keep view state in sync.
                _ => {}
            },
            Ok(Some(Message::Response(response))) => shared.resolve_outbound(response),
            Ok(None) => {
                // The client disconnected without a goodbye: every pending
                // edit must abort so Git never hangs (§3.10 step 7), and the
                // commit worker can then finish and release the writer.
                shared.terminal_cleanup();
                break 0;
            }
            Err(error) => {
                shared.terminal_cleanup();
                read_error = Some(Error::new(format!("failed to read LSP message: {error}")));
                break 0;
            }
        }
    };

    // In-flight work must not keep the process alive after `exit`; queued
    // responses are already on the writer channel.
    watcher.abort();
    let _ = watcher.await;
    while handlers.join_next().await.is_some() {}
    shared.wait_progress().await;
    // `Shared` also holds a sender clone, so it must go before the writer
    // channel can close.
    drop(shared);
    drop(sender);

    match writer.await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => return Err(Error::from(error)),
        Err(error) => return Err(Error::new(error.to_string())),
    }
    if let Some(error) = read_error { return Err(error); }
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
    let trace_id = trace_id_of(&request.params);
    if trace_id > 0 {
        runner::trace(
            trace_id,
            "server_received",
            serde_json::json!({ "method": method }),
        );
    }
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
        if trace_id > 0 {
            runner::trace(
                trace_id,
                "handler_start",
                serde_json::json!({ "method": method }),
            );
        }
        let (token, cancel) = shared.begin_progress(&sender, &method, None, Some(&active.repo_path));
        let outcome = dispatch_response(request, &shared, Some(cancel), trace_id).await;
        shared.end_progress(&sender, &token);
        if trace_id > 0 {
            runner::trace(
                trace_id,
                "handler_end",
                serde_json::json!({ "method": method }),
            );
        }
        outcome
    } else {
        if trace_id > 0 {
            runner::trace(
                trace_id,
                "handler_start",
                serde_json::json!({ "method": method }),
            );
        }
        let outcome = dispatch_response(request, &shared, None, trace_id).await;
        if trace_id > 0 {
            runner::trace(
                trace_id,
                "handler_end",
                serde_json::json!({ "method": method }),
            );
        }
        outcome
    };

    // The response creates or identifies the view, so it goes first; render
    // notifications then refer to something the client already knows.
    let _ = sender.send(Message::Response(response));
    for notification in notifications {
        if trace_id > 0 {
            if let Message::Notification(notification) = &notification {
                if notification.method == VIEW_RENDER {
                    let view = notification
                        .params
                        .get("viewId")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    runner::trace(trace_id, "render_sent", serde_json::json!({ "view": view }));
                }
            }
        }
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
    trace_id: u64,
) -> (Response, Vec<Message>) {
    let id = request.id.clone();
    let method = request.method.clone();
    let shared = shared.clone();

    match tokio::task::spawn_blocking(move || {
        runner::with_cancel(cancel, || {
            runner::with_trace_id(trace_id, || dispatch(&request, &shared))
        })
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

/// Draft barriers on the receive-order lane; JSON-RPC ids correlate results.
#[cfg(unix)]
fn draft_request(shared: &Shared, request: &Request) -> Result<Value> {
    let response = match request.method.as_str() {
        "workspace/executeCommand" => {
            let params: lsp_types::ExecuteCommandParams = from_params(request.params.clone())?;
            if params.command != "magit.command" || params.arguments.len() != 1 {
                return Err(Error::new("Expected magit.command with one semantic command"));
            }
            let command: EditConfirmParams = from_params(params.arguments[0].clone())?;
            if command.command != "common.confirm" {
                return Err(Error::new("Only common.confirm is supported here"));
            }
            let p = DraftFinishParams {
                run_id: command.run_id, uri: command.uri, version: command.version,
                reason: match command.source {
                    EditConfirmSource::Synchronized => DraftFinishReason::Confirm,
                    EditConfirmSource::SavedFile => DraftFinishReason::NormalClose,
                },
            };
            if let Some(warning) = editor::registry().finish(&shared.draft_owner, &p)? {
                let _ = shared.sender.send(Message::Notification(Notification::new(
                    VIEW_EFFECTS.to_string(), ViewEffectsParams {
                        effects: vec![Effect::message(MessageLevel::Warn, warning)],
                    },
                )));
            }
            EditConfirmation::Confirmed { run_id: p.run_id, uri: p.uri, version: p.version }
        }
        _ => return Err(Error::new("Unknown commit draft request")),
    };
    serde_json::to_value(response).map_err(|error| Error::new(error.to_string()))
}

/// Apply full-sync text; ordinary clients confirm saved disk text on didClose.
#[cfg(unix)]
fn draft_notification(shared: &Shared, notification: &Notification) -> Result<()> {
    use lsp_types::{DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams};
    if shared.closing.load(Ordering::SeqCst) { return Err(Error::new("Server is shutting down")); }
    match notification.method.as_str() {
        "textDocument/didOpen" => {
            let p: DidOpenTextDocumentParams = from_params(notification.params.clone())?;
            let doc = p.text_document;
            let uri = doc.uri.as_str();
            let run = editor::registry().run_for_uri(&shared.draft_owner, uri)?;
            editor::registry().apply(&shared.draft_owner, &run, uri,
                |draft, context| draft.did_open(context, doc.version, doc.text.as_bytes()))
        }
        "textDocument/didChange" => {
            let p: DidChangeTextDocumentParams = from_params(notification.params.clone())?;
            if p.content_changes.len() != 1 || p.content_changes[0].range.is_some() || p.content_changes[0].range_length.is_some() {
                return Err(Error::new("Commit drafts require exactly one full-sync text change"));
            }
            let uri = p.text_document.uri.as_str();
            let run = editor::registry().run_for_uri(&shared.draft_owner, uri)?;
            editor::registry().apply(&shared.draft_owner, &run, uri,
                |draft, context| draft.did_change(context, p.text_document.version, p.content_changes[0].text.as_bytes()))
        }
        "textDocument/didClose" => {
            let p: DidCloseTextDocumentParams = from_params(notification.params.clone())?;
            let uri = p.text_document.uri.as_str();
            let run = editor::registry().run_for_uri(&shared.draft_owner, uri)?;
            let close_confirms = shared.client.lock().unwrap_or_else(|p| p.into_inner()).close_confirms;
            if close_confirms {
                editor::registry().finish(&shared.draft_owner, &DraftFinishParams {
                    run_id: run, uri: uri.to_string(), version: 0, reason: DraftFinishReason::NormalClose,
                })?;
                Ok(())
            } else {
                editor::registry().apply(&shared.draft_owner, &run, uri,
                    |draft, context| draft.did_close(context))
            }
        }
        _ => Err(Error::new("Unknown commit draft notification")),
    }
}

/// Reject managed drafts on platforms without the validated helper boundary.
#[cfg(not(unix))]
fn draft_request(_shared: &Shared, _request: &Request) -> Result<Value> {
    Err(Error::new("managed commit drafts require Unix"))
}

/// Reject managed draft sync on platforms without Unix support.
#[cfg(not(unix))]
fn draft_notification(_shared: &Shared, _notification: &Notification) -> Result<()> {
    Err(Error::new("managed commit drafts require Unix"))
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
                client.work_done_progress = params.pointer("/capabilities/window/workDoneProgress").and_then(Value::as_bool) == Some(true);
                client.close_confirms = !params.pointer("/clientInfo/name").and_then(Value::as_str)
                    .is_some_and(|name| name.to_ascii_lowercase().contains("visual studio code") || name.to_ascii_lowercase().contains("codium"));
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
            let published = shared
                .published
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get(&repo)
                .cloned()
                .ok_or_else(|| Error::new(format!("unknown view: {}", p.view_id)))?;
            Ok(Dispatched::value(serde_json::to_value(
                published.resync(&p.view_id)?,
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
    let repo = crate::git::GitRepo::open(&repo)?
        .root()
        .canonicalize()?
        .to_string_lossy()
        .into_owned();
    let id = shared.next_view_id();
    let config = shared.config.for_repository(std::path::Path::new(&repo));
    let keymaps = config.keymaps();
    let mut sessions = shared
        .sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = sessions
        .entry(repo.clone())
        .or_insert_with(|| Session::new(&repo));
    shared
        .published
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .entry(repo.clone())
        .or_insert_with(|| session.published());
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
    let repo = repo.as_str();
    shared
        .view_repos
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(id, repo.to_string());
    shared.ensure_watcher(repo);

    let mut notifications = vec![Message::Notification(Notification::new(
        VIEW_RENDER.to_string(),
        render,
    ))];
    if let Some(error) = config.last_error() {
        notifications.push(show_message(1, format!("magit-lsp config: {error}")));
    }
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
    shared.stop_watcher_if_idle(&repo);
    Ok(Dispatched::value(Value::Null))
}

fn invoke_view(shared: &Shared, params: Value) -> Result<Dispatched> {
    let p: ViewInvokeParams = from_params(params)?;
    let repo = shared
        .repo_for_view(&p.view_id)
        .ok_or_else(|| Error::new(format!("unknown view: {}", p.view_id)))?;
    if shared.cancellation_pending(&repo) { return cancelling_response(); }
    let mut sessions = shared
        .sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = sessions
        .get_mut(&repo)
        .ok_or_else(|| Error::new(format!("unknown view: {}", p.view_id)))?;
    let step = session.start_invoke(&p)?;
    drop(sessions);
    let outcome = drive_execution(shared, &repo, step)?;
    Ok(dispatched_outcome(shared, &repo, outcome)?)
}

/// Handle `view/key`: resolve one key step and answer like `view/invoke`.
fn key_view(shared: &Shared, params: Value) -> Result<Dispatched> {
    let p: ViewKeyParams = from_params(params)?;
    let repo = shared
        .repo_for_view(&p.view_id)
        .ok_or_else(|| Error::new(format!("unknown view: {}", p.view_id)))?;
    if shared.cancellation_pending(&repo) { return cancelling_response(); }
    let keymaps = shared
        .config
        .for_repository(std::path::Path::new(&repo))
        .keymaps();
    let mut sessions = shared
        .sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = sessions
        .get_mut(&repo)
        .ok_or_else(|| Error::new(format!("unknown view: {}", p.view_id)))?;
    session.set_keymaps(keymaps);
    let step = session.start_key(&p)?;
    drop(sessions);
    let mut outcome = drive_execution(shared, &repo, step)?;
    if shared.delivery() == KeymapDelivery::ForwardAll
        && matches!(
            outcome.effects.as_slice(),
            [Effect::Message { text, .. }] if text.starts_with("Unbound key: ")
        )
    {
        outcome.effects = vec![Effect::PassThrough { key: p.key }];
    }
    Ok(dispatched_outcome(shared, &repo, outcome)?)
}

/// Readable refusal while the prior operation still owns repository cleanup.
fn cancelling_response() -> Result<Dispatched> {
    Ok(Dispatched::value(serde_json::json!({ "effects": [Effect::message(
        MessageLevel::Info, "Cancelling… wait for the current Git operation to finish".to_string(),
    )] })))
}

/// Wait with no session ownership; reacquire only to validate and apply an answer.
fn drive_execution(shared: &Shared, repo: &str, mut step: Execution) -> Result<InvokeResult> {
    loop {
        match step {
            Execution::Done(result) => return Ok(result),
            Execution::Prompt(request) => {
                let answer = crate::ui::answer(&ServerUi::new(shared), &request);
                let mut sessions = shared
                    .sessions
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let session = sessions
                    .get_mut(repo)
                    .ok_or_else(|| Error::new("Prompt session closed"))?;
                match answer {
                    Ok(answer) => {
                        let answer = if shared.prompts.closed.load(Ordering::SeqCst) {
                            None
                        } else {
                            answer
                        };
                        step = session.resume_prompt(&request.prompt_id, answer)?;
                    }
                    Err(error) => {
                        let _ = session.resume_prompt(&request.prompt_id, None);
                        return Err(error);
                    }
                }
            }
        }
    }
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
    if let Some(operation) = outcome.commit.filter(|_| !shared.closing.load(Ordering::SeqCst)) {
        // Git must not run while this request still holds any session or repo
        // lock: it waits for the user to edit and confirm.
        spawn_commit(shared.clone(), repo.to_string(), operation);
    }
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
    let mut repo_view_pushed = false;
    let mut repo_quit = false;
    {
        let mut view_repos = shared
            .view_repos
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for effect in &outcome.effects {
            match effect {
                Effect::OpenView { descriptor } | Effect::PushView { descriptor } => {
                    view_repos.insert(descriptor.id.clone(), repo.to_string());
                    repo_view_pushed = true;
                }
                Effect::PopView { view_id } => {
                    view_repos.remove(view_id);
                }
                Effect::Quit => {
                    view_repos.retain(|_, owner| owner != repo);
                    repo_quit = true;
                }
                _ => {}
            }
        }
    }
    // The watcher lifecycle rides the same effects: a new view arms the
    // watcher (idempotent), quitting the last view of the repo stops it.
    if repo_view_pushed {
        shared.ensure_watcher(repo);
    }
    if repo_quit {
        shared.stop_watcher_if_idle(repo);
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

/// The keymap a dynamic client applies to an edit buffer.
///
/// Only kinds with a registry scope get one; the server still owns the keys,
/// so a client never hard-codes confirm/cancel.
fn edit_keymap(config: &ConfigService, edit_id: &str, kind: &str) -> Option<ViewKeymapParams> {
    let scope = match kind {
        editor::COMMIT_MESSAGE_KIND | "merge-message" | "tag-message" => "commit-message",
        _ => return None,
    };
    let keymap = config.keymap(scope)?;
    Some(ViewKeymapParams {
        view_id: edit_id.to_string(),
        scope: scope.to_string(),
        version: 0,
        capture: false,
        bindings: keymap.bindings().to_vec(),
        mode: ViewMode::Interactive,
        pending: String::new(),
        live_keys: Vec::new(),
    })
}

/// Ends the commit progress token however the worker finishes.
///
/// This is the `finally` of the commit: an error, a failed hook, an editor
/// cancel or a Git process that never opens an editor all drop the guard.
struct ProgressEnd {
    shared: Shared,
    token: String,
}

impl Drop for ProgressEnd {
    fn drop(&mut self) {
        self.shared.end_progress(&self.shared.sender, &self.token);
    }
}

/// Own a commit's pending edit and repository-specific editor configuration.
struct CommitEdit {
    id: String,
    configs: Arc<Mutex<HashMap<String, Arc<ConfigService>>>>,
}

impl Drop for CommitEdit {
    fn drop(&mut self) {
        editor::registry().remove(&self.id);
        self.configs
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&self.id);
    }
}

/// Run `git commit` on a detached worker and refresh the views afterwards.
///
/// The worker owns the editor wait, so neither the sessions mutex nor the repo
/// lock is held while Git blocks on the helper. Only the final refresh takes
/// the session lock again, briefly.
fn spawn_commit(shared: Shared, repo: String, operation: crate::git::CommitOperation) {
    let sender = shared.sender.clone();
    // The begin is sent before the invoke response, so the client is not idle
    // until the editor (or the failure) has been answered.
    let edit_id = match editor::registry().create(&repo, &shared.draft_owner) {
        Ok(id) => id,
        Err(error) => {
            let _ = sender.send(show_message(1, format!("Cannot open commit draft: {error}")));
            return;
        }
    };
    shared
        .edit_configs
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(
            edit_id.clone(),
            shared.config.for_repository(std::path::Path::new(&repo)),
        );
    let edit = CommitEdit {
        id: edit_id.clone(),
        configs: shared.edit_configs.clone(),
    };
    let (token, cancel) = shared.begin_progress(&sender, "commit", Some(edit_id), Some(&repo));
    let progress = ProgressEnd {
        shared: shared.clone(),
        token,
    };
    let worker = std::thread::Builder::new()
        .name("magit-lsp-commit".to_string())
        .spawn(move || {
            let _progress = progress;
            let result = runner::with_cancel(Some(cancel), || match editor::socket_path() {
                Some(socket) => editor::commit(&repo, &edit.id, socket, &operation),
                None => Err(Error::new("the editor bridge is not running")),
            });
            // Release both resources before any outcome or progress-end reaches
            // the client, including when Git was killed before opening an editor.
            drop(edit);

            let mut messages = Vec::new();
            match result {
                Ok(outcome) => {
                    let renders: Vec<ViewRender> = if shared.closing.load(Ordering::SeqCst) { Vec::new() } else {
                        let mut sessions = shared
                            .sessions
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        let Some(session) = sessions.get_mut(&repo) else {
                            return;
                        };
                        let ids: Vec<String> = session
                            .views()
                            .iter()
                            .map(|view| view.descriptor().id.clone())
                            .collect();
                        ids.into_iter()
                            .filter_map(|id| session.refresh(&id).ok().flatten())
                            .collect()
                    };
                    messages.extend(renders.into_iter().map(|render| {
                        Message::Notification(Notification::new(VIEW_RENDER.to_string(), render))
                    }));
                    // Replace the "Committing…" feedback with the outcome.
                    let subject = outcome.message.as_deref().unwrap_or("").lines().next().unwrap_or("").trim().to_string();
                    let text = if subject.is_empty() {
                        "Committed".to_string()
                    } else {
                        format!("Committed: {subject}")
                    };
                    let mut effects = vec![Effect::message(MessageLevel::Info, text)];
                    if let Some(warning) = outcome.warning { effects.push(Effect::message(MessageLevel::Warn, warning)); }
                    messages.push(Message::Notification(Notification::new(
                        VIEW_EFFECTS.to_string(),
                        ViewEffectsParams {
                            effects,
                        },
                    )));
                }
                Err(error) => {
                    let effects = errors::effects_for_result(Err(error)).unwrap_or_else(|error| {
                        vec![Effect::message(MessageLevel::Error, error.to_string())]
                    });
                    messages.push(Message::Notification(Notification::new(
                        VIEW_EFFECTS.to_string(),
                        ViewEffectsParams { effects },
                    )));
                }
            }
            for message in messages {
                let _ = sender.send(message);
            }
        });
    if let Err(error) = worker {
        tracing::warn!(%error, "failed to start the commit worker");
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
        object.insert("executeCommandProvider".to_string(), serde_json::json!({ "commands": ["magit.command"] }));
        object.insert(
            "textDocumentSync".to_string(),
            serde_json::json!({ "openClose": true, "change": 1 }),
        );
        object.insert(
            "window".to_string(),
            serde_json::json!({ "workDoneProgress": true }),
        );
    }
    // The legend carries a suggested theme group per token type, so a client
    // never has to maintain a token-type table of its own.
    if let Some(legend) = capabilities.pointer_mut("/semanticTokensProvider/legend") {
        if let Some(object) = legend.as_object_mut() {
            object.insert(
                "tokenTypeGroups".to_string(),
                semantic_tokens::token_groups(),
            );
        }
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
    // Prompting work retains its ordering turn, but new commands must reach
    // the session's readable refusal rather than queue behind human input.
    if matches!(method, ViewKeyRequest::METHOD | ViewInvokeRequest::METHOD) {
        if let Some(repo) = params
            .get("viewId")
            .and_then(Value::as_str)
            .and_then(|id| shared.repo_for_view(id))
        {
            let sessions = shared
                .sessions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if sessions.get(&repo).is_some_and(Session::has_pending_prompt) {
                return None;
            }
        }
    }
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

/// The optional key trace id a client attaches to a request (§A.4).
fn trace_id_of(params: &Value) -> u64 {
    params.get("traceId").and_then(Value::as_u64).unwrap_or(0)
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

#[cfg(test)]
mod commit_tests {
    use super::*;

    fn test_shared() -> (Shared, mpsc::UnboundedReceiver<Message>) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "").unwrap();
        let config = ConfigService::load(LoadOptions { config_path: Some(path), ..Default::default() }).unwrap();
        let (sender, receiver) = mpsc::unbounded_channel();
        (Shared::new(Arc::new(config), None, sender), receiver)
    }

    async fn poll_pending<F: std::future::Future>(mut future: std::pin::Pin<&mut F>) {
        std::future::poll_fn(|cx| {
            assert!(future.as_mut().poll(cx).is_pending());
            std::task::Poll::Ready(())
        }).await;
    }

    fn request_id(receiver: &mut mpsc::UnboundedReceiver<Message>) -> RequestId {
        let Message::Request(request) = receiver.try_recv().expect("outbound request") else { panic!("expected request") };
        request.id
    }

    #[tokio::test]
    async fn outbound_resolves_out_of_order_and_ignores_unknown_duplicate_and_late_ids() {
        let (shared, mut messages) = test_shared();
        let (first, rx1) = shared.register_outbound("first", Value::Null).unwrap();
        let (second, rx2) = shared.register_outbound("second", Value::Null).unwrap();
        let id1 = request_id(&mut messages);
        let id2 = request_id(&mut messages);
        assert_ne!(id1, id2);
        shared.resolve_outbound(Response::new_ok(RequestId::from("unknown".to_string()), Value::Null));
        assert_eq!(shared.outbound.lock().unwrap().len(), 2);
        shared.resolve_outbound(Response::new_ok(id2.clone(), serde_json::json!("second result")));
        shared.resolve_outbound(Response::new_err(id2, -32000, "duplicate".into()));
        assert_eq!(shared.outbound.lock().unwrap().len(), 1);
        assert_eq!(shared.await_outbound(second, rx2).await.unwrap().response_result.unwrap(), serde_json::json!("second result"));
        shared.resolve_outbound(Response::new_err(id1.clone(), -32000, "declined".into()));
        assert!(shared.await_outbound(first, rx1).await.unwrap_err().to_string().contains("declined"));
        shared.resolve_outbound(Response::new_ok(id1, Value::Null));
        assert!(shared.outbound.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn outbound_timeout_and_unpolled_await_remove_registered_entries() {
        let (shared, mut messages) = test_shared();
        let (pending, rx) = shared.register_outbound("timeout", Value::Null).unwrap();
        let id = request_id(&mut messages);
        assert!(shared.await_outbound_for(pending, rx, std::time::Duration::ZERO).await.unwrap_err().to_string().contains("timed out"));
        assert!(shared.outbound.lock().unwrap().is_empty());
        shared.resolve_outbound(Response::new_ok(id, Value::Null));
        let (pending, rx) = shared.register_outbound("unpolled", Value::Null).unwrap();
        drop(shared.await_outbound(pending, rx));
        assert!(shared.outbound.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn aborted_outbound_await_removes_registered_entry() {
        let (shared, _messages) = test_shared();
        let worker = shared.clone();
        let (ready, started) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let (pending, rx) = worker.register_outbound("aborted", Value::Null).unwrap();
            ready.send(()).unwrap();
            worker.await_outbound(pending, rx).await
        });
        started.await.unwrap();
        assert_eq!(shared.outbound.lock().unwrap().len(), 1);
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(shared.outbound.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn terminal_cleanup_rejects_registration_and_releases_waiter() {
        let (shared, mut messages) = test_shared();
        let (pending, rx) = shared.register_outbound("waiting", Value::Null).unwrap();
        let id = request_id(&mut messages);
        shared.terminal_cleanup();
        assert!(shared.await_outbound(pending, rx).await.unwrap_err().to_string().contains("disconnected"));
        shared.resolve_outbound(Response::new_ok(id, Value::Null));
        assert!(shared.register_outbound("too late", Value::Null).is_err());
        assert!(messages.try_recv().is_err());
        assert!(shared.outbound.lock().unwrap().is_empty());
    }

    #[test]
    fn registration_racing_terminal_cleanup_leaves_no_pending_work() {
        for _ in 0..32 {
            let (shared, _messages) = test_shared();
            let barrier = Arc::new(std::sync::Barrier::new(2));
            std::thread::scope(|scope| {
                let worker = shared.clone();
                let barrier2 = barrier.clone();
                let registration = scope.spawn(move || {
                    barrier2.wait();
                    worker.register_outbound("racing", Value::Null)
                });
                barrier.wait();
                shared.terminal_cleanup();
                if let Ok((_pending, mut rx)) = registration.join().unwrap() {
                    assert!(matches!(rx.try_recv(), Err(tokio::sync::oneshot::error::TryRecvError::Closed)));
                }
            });
            assert!(shared.outbound.lock().unwrap().is_empty());
            assert!(shared.register_outbound("after shutdown", Value::Null).is_err());
        }
    }

    #[tokio::test]
    async fn progress_success_waits_for_create_and_ends_once() {
        let (shared, mut messages) = test_shared();
        let (token, _) = shared.begin_progress(&shared.sender, "stage", None, None);
        let create = shared.create_progress(&shared.sender, &token, "stage");
        tokio::pin!(create);
        poll_pending(create.as_mut()).await;
        let id = request_id(&mut messages);
        assert!(messages.try_recv().is_err(), "no begin before reply");
        shared.resolve_outbound(Response::new_ok(id, Value::Null));
        create.await;
        shared.end_progress(&shared.sender, &token);
        shared.end_progress(&shared.sender, &token);
        for phase in ["begin", "end"] {
            let Message::Notification(notice) = messages.try_recv().unwrap() else { panic!("expected progress") };
            assert_eq!(notice.method, "$/progress");
            assert_eq!(notice.params["token"], token);
            assert_eq!(notice.params["value"]["kind"], phase);
        }
        assert!(messages.try_recv().is_err());
        assert!(shared.outbound.lock().unwrap().is_empty());
        assert!(shared.progress.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn completed_or_terminal_progress_never_begins_after_create_reply() {
        for terminal in [false, true] {
            let (shared, mut messages) = test_shared();
            let (token, _) = shared.begin_progress(&shared.sender, "stage", None, None);
            let create = shared.create_progress(&shared.sender, &token, "stage");
            tokio::pin!(create);
            poll_pending(create.as_mut()).await;
            let id = request_id(&mut messages);
            // Resolve before cleanup but leave the awakened task unpolled: this
            // is the success-response/terminal race, not just a closed receiver.
            shared.resolve_outbound(Response::new_ok(id, Value::Null));
            if terminal { shared.terminal_cleanup(); }
            else { shared.end_progress(&shared.sender, &token); }
            create.await;
            shared.end_progress(&shared.sender, &token);
            assert!(messages.try_recv().is_err(), "no orphan begin/end");
            assert!(shared.outbound.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn failed_or_disconnected_create_keeps_tracking_without_visible_progress() {
        for disconnected in [false, true] {
            let (shared, mut messages) = test_shared();
            let (token, _) = shared.begin_progress(&shared.sender, "stage", None, None);
            let create = shared.create_progress(&shared.sender, &token, "stage");
            tokio::pin!(create);
            poll_pending(create.as_mut()).await;
            let id = request_id(&mut messages);
            if disconnected { shared.terminal_cleanup(); }
            else { shared.resolve_outbound(Response::new_err(id, -32000, "declined".into())); }
            create.await;
            assert!(shared.progress.lock().unwrap().contains_key(&token));
            assert!(!shared.progress.lock().unwrap()[&token].visible);
            shared.end_progress(&shared.sender, &token);
            assert!(messages.try_recv().is_err());
            assert!(shared.outbound.lock().unwrap().is_empty());
        }
    }

    #[test]
    fn disabled_progress_keeps_terminal_ownership_without_wire_messages() {
        let (shared, mut messages) = test_shared();
        let (token, cancel) = shared.begin_progress(&shared.sender, "stage", None, Some("repo"));
        assert!(!cancel.is_cancelled());
        assert!(shared.progress.lock().unwrap().contains_key(&token));
        assert!(!shared.progress.lock().unwrap()[&token].visible);
        shared.terminal_cleanup();
        assert!(cancel.is_cancelled());
        shared.end_progress(&shared.sender, &token);
        shared.end_progress(&shared.sender, &token);
        assert!(shared.progress.lock().unwrap().is_empty());
        assert!(shared.outbound.lock().unwrap().is_empty());
        assert!(messages.try_recv().is_err());
    }

    #[test]
    fn failed_outbound_send_removes_registration() {
        let (shared, messages) = test_shared();
        drop(messages);
        assert!(shared.register_outbound("disconnected", Value::Null).is_err());
        assert!(shared.outbound.lock().unwrap().is_empty());
    }

    #[test]
    fn commit_edit_releases_registry_and_repository_config_together() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(std::process::Command::new("git").args(["init", "-q"])
            .current_dir(tmp.path()).status().unwrap().success());
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "").unwrap();
        let config = Arc::new(
            ConfigService::load(LoadOptions {
                config_path: Some(path),
                ..Default::default()
            })
            .unwrap(),
        );
        let retained = Arc::downgrade(&config);
        let id = editor::registry().create(tmp.path().to_str().unwrap(), "test-connection").unwrap();
        let configs = Arc::new(Mutex::new(HashMap::from([(id.clone(), config)])));
        let edit = CommitEdit {
            id: id.clone(),
            configs: configs.clone(),
        };
        assert!(retained.upgrade().is_some());
        drop(edit);
        assert!(configs.lock().unwrap().is_empty());
        assert!(
            retained.upgrade().is_none(),
            "the edit retained its repository config"
        );
        assert!(!editor::registry().resolve(&id, editor::Reply::Ok));
    }
}
