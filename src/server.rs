use crate::config::{ConfigService, LoadOptions};
use crate::docstore::{Change as StoredChange, DocStore, Encoding};
use crate::editor;
use crate::errors;
use crate::presentation::plain::{whole_document_edit, ApplyEdit, DocumentFormat, PlainDocument};
use crate::presentation::{
    negotiate, CustomRender, PlainCapabilities, Presentation, PromptSupport, Ui,
};
use crate::protocol::*;
use crate::runner::{self, CancelHandle};
use crate::semantic_tokens;
use crate::session::{Execution, InvokeResult, PromptReconcile, PublishedViews, Session};
use crate::transport;
use crate::tree::Tree;
use crate::ui::PromptUi;
use crate::view::{ViewKind, ViewMode, Viewport};
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
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc as std_mpsc, Arc, Mutex};
use tokio::io::{AsyncWrite, BufReader};
use tokio::sync::{mpsc, Mutex as AsyncMutex, Notify};
use tokio::task::JoinSet;

mod drafts;
mod plain;
mod plain_actions;
mod requests;
mod ui;
mod view_execution;

#[cfg(test)]
use view_execution::CommitEdit;
use view_execution::{
    cancelling_response, close_view, deferred_operation_response, dispatched_outcome,
    drive_execution, edit_keymap, invoke_view, key_view, open_view, repository_mutation_command,
    set_mode_view, DeferredReservation,
};

use plain::{
    close_plain, deliver_plain_close_outcome, handle_plain_key, plain_change,
    reconcile_rich_prompt, refresh_pending_views, report_pending_refresh_error,
};
use plain::{deliver_plain_effects, deliver_render, route_plain_render, schedule_plain_open};
use requests::{handle_request, Dispatched};
use ui::{custom_ui, dynamic_delivery, refresh_view_keymaps, PickBridge, PromptBridge, ServerUi};

type SessionHandle = Arc<Mutex<Session>>;
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

/// Keeps a repository's publication window open until the holder is done.
///
/// Constructed without clearing the marker; the drop clears it, so the
/// watcher may refresh only after the holder's last publication was sent. Any
/// watcher render still deferred for the repository is discarded: the owning
/// worker flushed the ones the settled tree still carries.
struct PublicationGuard {
    publications: Arc<Mutex<HashSet<String>>>,
    deferred: Arc<Mutex<HashMap<String, Vec<ViewRender>>>>,
    repo: String,
}

impl Drop for PublicationGuard {
    fn drop(&mut self) {
        self.publications
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&self.repo);
        self.deferred
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&self.repo);
    }
}

/// Startup options parsed from the command line.
#[derive(Debug, Clone, Default)]
pub struct ServerOptions {
    /// Explicit `--config <path>`; wins over `MAGIT_LSP_CONFIG`.
    pub config_path: Option<std::path::PathBuf>,
}

/// Client capabilities that affect key delivery (§3.9).
#[derive(Debug, Clone)]
struct ClientCaps {
    key_modifiers: KeyModifiers,
    /// Selected presentation recorded from initialize; routing is a later slice.
    presentation: Option<Presentation>,
    close_confirms: bool,
    work_done_progress: bool,
    encoding: Encoding,
    /// `clientInfo.name` reported at initialize; selects per-client settings.
    client_name: Option<String>,
    /// Data-driven quirk-table ceiling for Plain key policy.
    plain_keys_allowed: bool,
    /// Git repositories discovered from initialize workspace roots for Plain resolution.
    workspace_repositories: Vec<crate::presentation::resolve::Repository>,
    plain: Option<crate::presentation::PlainCapabilities>,
}

impl Default for ClientCaps {
    fn default() -> Self {
        Self {
            key_modifiers: KeyModifiers::Full,
            presentation: None,
            close_confirms: true,
            work_done_progress: false,
            encoding: Encoding::Utf16,
            client_name: None,
            plain_keys_allowed: true,
            workspace_repositories: Vec::new(),
            plain: None,
        }
    }
}

/// Server state shared by concurrent request handlers.
#[derive(Clone)]
struct Shared {
    /// Runtime captured on the transport thread for native watcher callbacks.
    runtime: Option<tokio::runtime::Handle>,
    /// Serializes Plain document acceptance, incarnation transitions and sends.
    plain_lifecycle: Arc<Mutex<()>>,
    /// Trusted identity allocated for this transport, never read from client JSON.
    draft_owner: Arc<String>,
    repo_locks: RepoLocks,
    /// Open views per repository root.
    sessions: Arc<Mutex<HashMap<String, SessionHandle>>>,
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
    documents: Arc<Mutex<DocStore>>,
    /// Plain document incarnations and their server-owned status views.
    plain_documents: Arc<Mutex<HashMap<String, PlainEntry>>>,
    /// Current open incarnation for each presentation URI, including bootstrap.
    plain_incarnations: Arc<Mutex<HashMap<String, u64>>>,
    next_plain_incarnation: Arc<AtomicU64>,
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
    /// The native pick transport.
    picks: Arc<PickBridge>,
    /// Running mutations keyed by their progress token, so a client cancel can
    /// kill the child on the blocking worker thread.
    progress: Arc<Mutex<HashMap<String, ProgressRun>>>,
    /// Status-open progress tokens keyed by view, so a text-mode switch can
    /// retire the busy indicator before the client goes idle.
    status_progress: Arc<Mutex<HashMap<String, String>>>,
    closing: Arc<AtomicBool>,
    progress_changed: Arc<Notify>,
    /// Global progress-token counter.
    next_progress: Arc<AtomicU64>,
    /// Per-repository mutation tickets, allocated in request order.
    mutation_gates: Arc<Mutex<HashMap<String, Arc<MutationGate>>>>,
    /// Repositories owned by detached workers after their request returned.
    deferred_repositories: Arc<Mutex<HashSet<String>>>,
    /// Repositories with an open publication window, set by the confirm lane
    /// and cleared by the detached worker after its last publication.
    deferred_publications: Arc<Mutex<HashSet<String>>>,
    /// Watcher renders withheld while a publication window was open, keyed by
    /// repository. The owning worker flushes the ones the settled session still
    /// carries before it publishes the outcome.
    deferred_renders: Arc<Mutex<HashMap<String, Vec<ViewRender>>>>,
    /// Running repository watchers, keyed by repository root.
    watchers: Arc<Mutex<HashMap<String, RepoWatcher>>>,
}

struct PlainEntry {
    incarnation: u64,
    repo: String,
    view_id: String,
    controller: PlainDocument,
    /// Per-document Plain key policy override; absent means inherit settings.
    keys_override: Option<bool>,
    /// Per-document key classifier and repair latch.
    key_state: crate::presentation::keys::DocumentState,
    /// A worker still owns execution, independently of a repair awaiting echo.
    key_executing: bool,
}

#[derive(Clone)]
struct ProgressRun {
    cancel: CancelHandle,
    edit_id: Option<String>,
    edit_scope: Option<Arc<editor::EditScope>>,
    cancellation_requested: bool,
    repo: Option<String>,
    visible: bool,
    // A `window/workDoneProgress/create` reply is still awaited, so a racing
    // end defers to the confirmed create instead of dropping the token.
    create_pending: bool,
    // The owner released the token before its begin was published; the create
    // reply flushes the pair.
    ended: bool,
}

/// Removes an outbound request if its waiter future is abandoned.
struct PendingOutbound {
    outbound: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<Response>>>>,
    key: String,
}

impl Drop for PendingOutbound {
    fn drop(&mut self) {
        self.outbound
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&self.key);
    }
}

impl Shared {
    /// Clone a repository session handle without retaining the registry lock.
    fn session(&self, repo: &str) -> Option<SessionHandle> {
        self.sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(repo)
            .cloned()
    }

    fn new(
        config: Arc<ConfigService>,
        startup_error: Option<String>,
        sender: mpsc::UnboundedSender<Message>,
    ) -> Self {
        static NEXT_CONNECTION: AtomicU64 = AtomicU64::new(0);
        let client = Arc::new(Mutex::new(ClientCaps::default()));
        Self {
            runtime: tokio::runtime::Handle::try_current().ok(),
            plain_lifecycle: Arc::new(Mutex::new(())),
            draft_owner: Arc::new(format!(
                "connection-{}",
                NEXT_CONNECTION.fetch_add(1, Ordering::Relaxed)
            )),
            repo_locks: Arc::new(AsyncMutex::new(HashMap::new())),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            published: Arc::new(Mutex::new(HashMap::new())),
            view_repos: Arc::new(Mutex::new(HashMap::new())),
            next_view: Arc::new(AtomicU64::new(0)),
            config,
            edit_configs: Arc::new(Mutex::new(HashMap::new())),
            client: client.clone(),
            documents: Arc::new(Mutex::new(DocStore::default())),
            plain_documents: Arc::new(Mutex::new(HashMap::new())),
            plain_incarnations: Arc::new(Mutex::new(HashMap::new())),
            next_plain_incarnation: Arc::new(AtomicU64::new(0)),
            startup_error,
            sender: sender.clone(),
            outbound: Arc::new(Mutex::new(HashMap::new())),
            next_outbound: Arc::new(AtomicU64::new(0)),
            prompts: Arc::new(PromptBridge::new(&sender, client)),
            picks: Arc::new(PickBridge::new(&sender)),
            progress: Arc::new(Mutex::new(HashMap::new())),
            status_progress: Arc::new(Mutex::new(HashMap::new())),
            closing: Arc::new(AtomicBool::new(false)),
            progress_changed: Arc::new(Notify::new()),
            next_progress: Arc::new(AtomicU64::new(0)),
            mutation_gates: Arc::new(Mutex::new(HashMap::new())),
            deferred_repositories: Arc::new(Mutex::new(HashSet::new())),
            deferred_publications: Arc::new(Mutex::new(HashSet::new())),
            deferred_renders: Arc::new(Mutex::new(HashMap::new())),
            watchers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register before sending so a fast response cannot arrive before its waiter exists.
    fn register_outbound(
        &self,
        method: &str,
        params: Value,
    ) -> Result<(PendingOutbound, tokio::sync::oneshot::Receiver<Response>)> {
        let next = self.next_outbound.fetch_add(1, Ordering::Relaxed) + 1;
        let id = RequestId::from(format!("magit-lsp/outbound/{next}"));
        let key = serde_json::to_string(&id).map_err(|e| Error::new(e.to_string()))?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut outbound = self.outbound.lock().unwrap_or_else(|p| p.into_inner());
        if self.closing.load(Ordering::SeqCst) {
            return Err(Error::new("client request transport is closed"));
        }
        outbound.insert(key.clone(), tx);
        if self
            .sender
            .send(Message::Request(Request::new(
                id,
                method.to_string(),
                params,
            )))
            .is_err()
        {
            outbound.remove(&key);
            return Err(Error::new("client request transport is closed"));
        }
        drop(outbound);
        Ok((
            PendingOutbound {
                outbound: self.outbound.clone(),
                key,
            },
            rx,
        ))
    }

    /// Await a registered request, removing it on response, timeout or cancellation.
    async fn await_outbound(
        &self,
        pending: PendingOutbound,
        rx: tokio::sync::oneshot::Receiver<Response>,
    ) -> Result<Response> {
        self.await_outbound_for(pending, rx, std::time::Duration::from_secs(30))
            .await
    }

    async fn await_outbound_for(
        &self,
        _pending: PendingOutbound,
        rx: tokio::sync::oneshot::Receiver<Response>,
        duration: std::time::Duration,
    ) -> Result<Response> {
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
        let Ok(key) = serde_json::to_string(&response.id) else {
            return;
        };
        if let Some(sender) = self
            .outbound
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&key)
        {
            let _ = sender.send(response);
        }
    }

    /// Cancel every request still awaiting a client response.
    fn cancel_outbound(&self) {
        self.outbound
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
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

    /// Park this repository's publication window for its detached worker.
    ///
    /// The confirm lane inserts the marker before Git resumes; the owning
    /// worker clears it after its last publication, so no watcher refresh can
    /// interleave between the confirm's own effects and the commit outcome.
    /// Inserting never blocks: the confirm runs on the reader loop, which
    /// must stay live while a publication window is open.
    fn hold_deferred_publication(&self, repo: String) {
        self.deferred_publications
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(repo);
    }

    /// Return a publication window that never reached its worker.
    fn reclaim_deferred_publication(&self, repo: &str) {
        self.deferred_publications
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(repo);
        self.deferred_renders
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(repo);
    }

    /// Hold this repository's publication window open until the guard drops.
    ///
    /// Constructing never clears the marker: the holder keeps the window open
    /// through its publications, and the drop clears it, so the watcher may
    /// refresh only after the last publication was sent. An absent marker
    /// makes the drop a no-op for workers without a confirm behind them.
    fn publication_guard(&self, repo: &str) -> PublicationGuard {
        PublicationGuard {
            publications: Arc::clone(&self.deferred_publications),
            deferred: Arc::clone(&self.deferred_renders),
            repo: repo.to_string(),
        }
    }

    /// Withhold this repository's watcher renders until its worker publishes.
    ///
    /// Called while the caller owns the session, so the owning worker cannot
    /// flush between the refresh that produced these renders and their storage.
    fn defer_renders(&self, repo: &str, renders: Vec<ViewRender>) {
        if renders.is_empty() {
            return;
        }
        self.deferred_renders
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entry(repo.to_string())
            .or_default()
            .extend(renders);
    }

    /// Take every watcher render withheld for this repository.
    fn take_deferred_renders(&self, repo: &str) -> Vec<ViewRender> {
        self.deferred_renders
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(repo)
            .unwrap_or_default()
    }

    /// Bounded synchronous wait for this repository's publication window to
    /// close, for the watcher thread.
    ///
    /// The watcher runs off the reader loop, so it may poll. A closing server
    /// or an overdue window proceeds with the refresh anyway — bounded lanes
    /// never deadlock, and the owning worker's guard drop backstops leaks.
    fn wait_publication_window(&self, repo: &str) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while self
            .deferred_publications
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contains(repo)
        {
            if self.closing.load(Ordering::SeqCst) || std::time::Instant::now() > deadline {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// Whether this repository's publication window is currently open.
    ///
    /// A caller that already owns the session may check this without waiting:
    /// the owning worker needs that same ownership to publish and drop the
    /// window, so an open window seen here cannot close underneath the check.
    fn publication_window_open(&self, repo: &str) -> bool {
        self.deferred_publications
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contains(repo)
    }

    /// Reserve exclusive mutation authority until a detached worker exits.
    fn reserve_deferred(&self, repo: String) -> Option<DeferredReservation> {
        let mut repositories = self
            .deferred_repositories
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        repositories
            .insert(repo.clone())
            .then(|| DeferredReservation {
                repositories: self.deferred_repositories.clone(),
                repo,
            })
    }

    /// Whether a detached worker still owns mutation authority for `repo`.
    fn deferred_pending(&self, repo: &str) -> bool {
        self.deferred_repositories
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contains(repo)
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
        let started = std::time::Instant::now();
        let emit_phase = |outcome: &str| {
            crate::runner::trace_current(
                "phase_watcher_setup",
                serde_json::json!({
                    "duration_ms": started.elapsed().as_secs_f64() * 1000.0,
                    "outcome": outcome,
                }),
            );
        };
        let already_armed = {
            let watchers = self
                .watchers
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if self.closing.load(Ordering::SeqCst) {
                return;
            }
            watchers.contains_key(repo)
        };
        if already_armed {
            emit_phase("already_armed");
            return;
        }
        // A deferred arm may run after its view closed; `stop_watcher_if_idle`
        // already ran, so arming would strand a watcher.
        let view_open = self
            .view_repos
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .any(|owner| owner.as_str() == repo);
        if !view_open {
            emit_phase("skipped");
            return;
        }
        let shared = self.clone();
        let repo_key = repo.to_string();
        let repo_for_handler = repo_key.clone();
        let presentation_files = self.config.config().presentation_files.clone();
        // The watch setup walks the whole tree off the open's critical path;
        // a short-lived client can exit before it returns. Record a distinct
        // start marker now so the open is always traced; completion emits the
        // terminal, full-duration `phase_watcher_setup` event below.
        crate::runner::trace_current(
            "phase_watcher_setup_start",
            serde_json::json!({
                "duration_ms": started.elapsed().as_secs_f64() * 1000.0,
                "outcome": "start",
            }),
        );
        let result = RepoWatcher::start_with_presentation_files(
            Path::new(repo),
            WatcherConfig::from_env(),
            presentation_files,
            move |refresh: Refresh| match refresh {
                Refresh::Changed(_paths) => {
                    // Only this repository's session serialises refresh and mutation.
                    if shared.closing.load(Ordering::SeqCst) {
                        return;
                    }
                    // A commit's publication window may be open: hold this
                    // burst off until the detached worker published the
                    // outcome, so no refresh interleaves between the
                    // confirm's own effects and the Committed message. This
                    // watcher thread may wait bounded; the reader loop never
                    // waits, so nothing here can starve a confirm.
                    shared.wait_publication_window(&repo_for_handler);
                    let renders = {
                        match shared.session(&repo_for_handler) {
                            Some(handle) => {
                                let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
                                let renders = session.refresh_all();
                                // A commit may have opened its publication
                                // window while this refresh's Git reads ran —
                                // its confirm passed the check above before the
                                // marker existed. The worker owns the settled
                                // render, so this burst must not publish the
                                // tree it read. Withhold it for the worker,
                                // which publishes the withheld render only when
                                // the settled session still carries it; a read
                                // the worker superseded is dropped instead. The
                                // check and the withholding happen under the
                                // session lock, where the worker cannot publish
                                // and close the window before they are seen.
                                if shared.publication_window_open(&repo_for_handler) {
                                    shared.defer_renders(&repo_for_handler, renders);
                                    Vec::new()
                                } else {
                                    renders
                                }
                            }
                            None => Vec::new(),
                        }
                    };
                    for render in renders {
                        let snapshot = shared
                            .published
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .get(&repo_for_handler)
                            .and_then(|views| views.current_snapshot(&render.view_id));
                        if !route_plain_render(&shared, &render.view_id, snapshot, render.cursor) {
                            let _ = shared.sender.send(custom_ui(&shared.client).render(render));
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
                // Re-check after the recursive walk: another thread may have
                // armed this repo, or its last view may have closed meanwhile.
                // Lock `watchers` before `view_repos`, matching
                // `stop_watcher_if_idle`, so a concurrent close cannot slip
                // between this check and the insert and strand the watcher.
                let mut watchers = self
                    .watchers
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let view_open = self
                    .view_repos
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .values()
                    .any(|owner| owner.as_str() == repo);
                if self.closing.load(Ordering::SeqCst) || !view_open {
                    drop(watchers);
                    drop(watcher);
                    emit_phase("skipped");
                    return;
                }
                if watchers.contains_key(repo) {
                    drop(watchers);
                    drop(watcher);
                    emit_phase("already_armed");
                    return;
                }
                watchers.insert(repo_key, watcher);
                drop(watchers);
                emit_phase("armed");
            }
            Err(error) => {
                tracing::debug!(%error, "repository watcher failed to start");
                let _ = self.sender.send(show_message(
                    2,
                    format!("magit-lsp: file watching unavailable ({error}); views refresh only on common.refresh"),
                ));
                emit_phase("skipped");
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
        let watcher = if no_views_left {
            watchers.remove(repo)
        } else {
            None
        };
        drop(watchers);
        if let Some(mut watcher) = watcher {
            watcher.join();
        }
    }

    /// Break watcher callback ownership before dropping the writer transport.
    fn stop_watchers(&self) {
        let watchers =
            std::mem::take(&mut *self.watchers.lock().unwrap_or_else(|p| p.into_inner()));
        for (_, mut watcher) in watchers {
            watcher.join();
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
        self.begin_progress_with_scope(sender, command, edit_id, repo, None)
    }

    /// Announce progress whose cancellation also closes scoped editor admission.
    fn begin_progress_with_scope(
        &self,
        sender: &mpsc::UnboundedSender<Message>,
        command: &str,
        edit_id: Option<String>,
        repo: Option<&str>,
        edit_scope: Option<Arc<editor::EditScope>>,
    ) -> (String, CancelHandle) {
        let next = self.next_progress.fetch_add(1, Ordering::Relaxed) + 1;
        let token = format!("magit-lsp/mutation/{next}");
        let handle = CancelHandle::new();
        let mut progress = self
            .progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let closing = self.closing.load(Ordering::SeqCst);
        progress.insert(
            token.clone(),
            ProgressRun {
                cancel: handle.clone(),
                edit_id: edit_id.clone(),
                edit_scope: edit_scope.clone(),
                cancellation_requested: false,
                repo: repo.map(str::to_string),
                visible: false,
                create_pending: false,
                ended: false,
            },
        );
        drop(progress);
        if closing {
            handle.cancel();
            if let Some(edit_id) = edit_id {
                editor::registry().resolve(&edit_id, editor::Reply::Abort);
            }
            if let Some(scope) = edit_scope {
                scope.revoke();
            }
        }
        let enabled = self
            .client
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .work_done_progress;
        if enabled {
            // Announce the create request synchronously so it precedes, on
            // the same ordered channel, any begin the caller publishes next;
            // a spawned announcer could race the caller and put the begin on
            // the wire first, where a client may drop it without its create.
            if let Some((pending, rx)) = self.announce_progress_create(&token) {
                let shared = self.clone();
                let sender = sender.clone();
                let token = token.clone();
                let command = command.to_string();
                tokio::spawn(async move {
                    shared
                        .await_progress_create(&sender, &token, &command, pending, rx)
                        .await;
                });
            }
        }
        (token, handle)
    }

    /// Begin a status-open progress span before its loading render is sent.
    ///
    /// The client observes idle as soon as it applies and acknowledges the
    /// loading render, so a begin published only after the create reply can
    /// arrive too late and let an early key hit the loading guard. The create
    /// request is announced first, in the caller's publication order, and
    /// this begin follows it on the same ordered channel, still ahead of the
    /// render. The reply does not publish a second begin for an
    /// already-visible token.
    fn begin_status_progress(
        &self,
        sender: &mpsc::UnboundedSender<Message>,
        repo: &str,
    ) -> Option<String> {
        let enabled = self
            .client
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .work_done_progress;
        if !enabled {
            return None;
        }
        let (token, _cancel) = self.begin_progress(sender, "status open", None, Some(repo));
        {
            let mut progress = self
                .progress
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(run) = progress.get_mut(&token) {
                run.visible = true;
            }
        }
        let _ = sender.send(Message::Notification(Notification::new(
            "$/progress".to_string(),
            serde_json::json!({ "token": token, "value": {
                "kind": "begin", "title": "Git", "message": "status open", "cancellable": true,
            } }),
        )));
        Some(token)
    }

    /// Remember which status view owns a published progress token.
    pub(super) fn track_status_progress(&self, view_id: &str, token: &str) {
        self.status_progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(view_id.to_string(), token.to_string());
    }

    /// Retire a status-open token when its view enters text mode.
    ///
    /// Text mode owns the buffer and accepts no state-dependent keys, so the
    /// busy indicator ends with the text-mode render. The completion task then
    /// finds no token and cannot end it a second time.
    pub(super) fn release_status_progress(&self, view_id: &str) -> Option<Message> {
        let token = self
            .status_progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(view_id)?;
        self.progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&token);
        self.progress_changed.notify_waiters();
        Some(Message::Notification(Notification::new(
            "$/progress".to_string(),
            serde_json::json!({ "token": token, "value": { "kind": "end" } }),
        )))
    }

    /// End a status-open token, forgetting its view once the work completes.
    pub(super) fn end_status_progress(
        &self,
        sender: &mpsc::UnboundedSender<Message>,
        view_id: &str,
        token: &str,
    ) {
        self.status_progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(view_id);
        self.end_progress(sender, token);
    }

    /// Publish progress only after successful creation, while its owner is live.
    async fn create_progress(
        &self,
        sender: &mpsc::UnboundedSender<Message>,
        token: &str,
        command: &str,
    ) {
        let Some((pending, rx)) = self.announce_progress_create(token) else {
            return;
        };
        self.await_progress_create(sender, token, command, pending, rx)
            .await;
    }

    /// Put a progress token's create request on the wire in the caller's own
    /// publication order, ahead of any begin the caller publishes next.
    ///
    /// Marking the create in flight before announcing it keeps a racing end
    /// deferring to the reply instead of removing the run unseen. Returns
    /// `None`, after abandoning the pending create, when the run is gone or
    /// the client request transport is closed.
    fn announce_progress_create(
        &self,
        token: &str,
    ) -> Option<(PendingOutbound, tokio::sync::oneshot::Receiver<Response>)> {
        {
            let mut runs = self.progress.lock().unwrap_or_else(|p| p.into_inner());
            match runs.get_mut(token) {
                Some(run) => run.create_pending = true,
                None => return None,
            }
        }
        match self.register_outbound(
            "window/workDoneProgress/create",
            serde_json::json!({ "token": token }),
        ) {
            Ok(pending) => Some(pending),
            Err(_) => {
                self.abandon_pending_create(token);
                None
            }
        }
    }

    /// Await a create reply, then publish the begin it unlocks, or the
    /// complete begin/end pair for an owner that already ended.
    async fn await_progress_create(
        &self,
        sender: &mpsc::UnboundedSender<Message>,
        token: &str,
        command: &str,
        pending: PendingOutbound,
        rx: tokio::sync::oneshot::Receiver<Response>,
    ) {
        if self.await_outbound(pending, rx).await.is_err() {
            self.abandon_pending_create(token);
            return;
        }
        let mut runs = self.progress.lock().unwrap_or_else(|p| p.into_inner());
        if self.closing.load(Ordering::SeqCst) {
            drop(runs);
            self.abandon_pending_create(token);
            return;
        }
        let Some(run) = runs.get_mut(token) else {
            return;
        };
        run.create_pending = false;
        if run.visible {
            // The status-open token published its begin before the loading
            // render, so a late confirmation must not repeat it. The owner
            // still ends the token once the completion render is enqueued.
            return;
        }
        if run.ended {
            // The owner finished before the client confirmed the token, so
            // nothing was published at the time. Publish the complete pair
            // now: the client observes begin then end and never a token that
            // was confirmed but silently dropped.
            runs.remove(token);
            let _ = sender.send(Message::Notification(Notification::new(
                "$/progress".to_string(),
                serde_json::json!({ "token": token, "value": {
                    "kind": "begin", "title": "Git", "message": command, "cancellable": true,
                } }),
            )));
            let _ = sender.send(Message::Notification(Notification::new(
                "$/progress".to_string(),
                serde_json::json!({ "token": token, "value": { "kind": "end" } }),
            )));
            drop(runs);
            self.progress_changed.notify_waiters();
            return;
        }
        run.visible = true;
        let _ = sender.send(Message::Notification(Notification::new("$/progress".to_string(), serde_json::json!({ "token": token, "value": { "kind": "begin", "title": "Git", "message": command, "cancellable": true } }))));
    }

    /// Retire an in-flight create that can no longer publish.
    ///
    /// The pending marker is cleared, and a run whose owner already ended is
    /// removed: no reply will ever arrive to flush its pair, so the token
    /// stays unobserved and cannot wedge progress waiting.
    fn abandon_pending_create(&self, token: &str) {
        let mut runs = self.progress.lock().unwrap_or_else(|p| p.into_inner());
        let Some(run) = runs.get_mut(token) else {
            return;
        };
        run.create_pending = false;
        if run.ended {
            runs.remove(token);
            drop(runs);
            self.progress_changed.notify_waiters();
        }
    }

    /// Finish the progress sequence started by [`Self::begin_progress`].
    fn end_progress(&self, sender: &mpsc::UnboundedSender<Message>, token: &str) {
        // Serialize report/end sends with removal, so a racing cancel cannot
        // resurrect an ended token. Repeated terminal release is harmless.
        let mut progress = self.progress.lock().unwrap_or_else(|p| p.into_inner());
        let Some(run) = progress.get_mut(token) else {
            return;
        };
        let (visible, create_pending) = (run.visible, run.create_pending);
        if !visible && create_pending {
            // The client has not confirmed the token yet, so the end cannot
            // be published without its begin. Defer it: the confirmed create
            // publishes begin then end instead of silently dropping both.
            run.ended = true;
            drop(progress);
            self.progress_changed.notify_waiters();
            return;
        }
        progress.remove(token);
        if visible {
            let _ = sender.send(Message::Notification(Notification::new(
                "$/progress".to_string(),
                serde_json::json!({ "token": token, "value": { "kind": "end" } }),
            )));
        }
        self.progress_changed.notify_waiters();
    }

    /// Stop new work and cancel every operation that can wait on the client.
    fn terminal_cleanup(&self) {
        let _plain = self
            .plain_lifecycle
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let runs = {
            let progress = self.progress.lock().unwrap_or_else(|p| p.into_inner());
            // Serialize the terminal boundary with publication of begin.
            self.closing.store(true, Ordering::SeqCst);
            progress.values().cloned().collect::<Vec<_>>()
        };
        self.cancel_outbound();
        self.plain_incarnations
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        let mut documents = self
            .plain_documents
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        for entry in documents.values_mut() {
            entry.controller.close();
        }
        documents.clear();
        drop(documents);
        for run in runs {
            run.cancel.cancel();
            if let Some(edit_id) = run.edit_id {
                editor::registry().resolve(&edit_id, editor::Reply::Abort);
            }
            if let Some(scope) = run.edit_scope {
                scope.revoke();
            }
        }
        editor::registry().abort_all();
        self.prompts.close();
        self.picks.close();
    }

    /// Wait until each progress-owning worker has run its terminal release.
    async fn wait_progress(&self) {
        loop {
            let notified = self.progress_changed.notified();
            if self
                .progress
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .is_empty()
            {
                return;
            }
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
            if run.cancellation_requested {
                return;
            }
            run.cancellation_requested = true;
            // Acknowledge intent, not termination or repository outcome. Keep
            // progress and the repository reservation until the owner finishes.
            if run.visible {
                let _ = self.sender.send(Message::Notification(Notification::new(
                    "$/progress".to_string(),
                    serde_json::json!({ "token": token, "value": {
                    "kind": "report", "message": "Cancelling…", "cancellable": false,
                } }),
                )));
            }
            for message in effect_notifications(
                &self.client,
                vec![Effect::message(MessageLevel::Info, "Cancelling…")],
            ) {
                let _ = self.sender.send(message);
            }
            run.cancel.cancel();
            if let Some(edit_id) = &run.edit_id {
                editor::registry().resolve(edit_id, editor::Reply::Abort);
            }
            if let Some(scope) = &run.edit_scope {
                scope.revoke();
            }
        }
    }

    /// Refuse new commands until a cancelled owner has finished cleanup.
    fn cancellation_pending(&self, repo: &str) -> bool {
        self.progress
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
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

    /// Deliver a `view/pickRespond` answer to the waiting command.
    fn respond_pick(&self, params: ViewPickRespondParams) -> Result<()> {
        self.picks.respond(params)
    }

    /// Whether this rich client requested pushed keymaps.
    fn dynamic_keymaps(&self) -> bool {
        dynamic_delivery(&self.client)
    }

    /// The repository root that owns `uri`, found through its open view.
    fn repo_for_uri(&self, uri: &str) -> Option<String> {
        let snapshots: Vec<_> = self
            .published
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .map(|(repo, session)| (repo.clone(), session.clone()))
            .collect();
        snapshots
            .into_iter()
            .find_map(|(repo, views)| views.contains_uri(uri).then_some(repo))
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
        let handles: Vec<_> = self
            .published
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .cloned()
            .collect();
        handles
            .into_iter()
            .find_map(|session| session.acked_tree_for_uri(uri))
    }

    /// Whether the view showing `uri` is in text mode.
    fn is_text_uri(&self, uri: &str) -> bool {
        let handles: Vec<_> = self
            .published
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .cloned()
            .collect();
        handles.into_iter().any(|session| session.is_text_uri(uri))
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
    let wirelog = match crate::wirelog::WireLog::from_env() {
        Ok(log) => log.map(Arc::new),
        Err(error) => {
            eprintln!("wirelog unavailable: {error}");
            None
        }
    };
    let writer = tokio::spawn(writer_loop(tokio::io::stdout(), receiver, wirelog.clone()));

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
    let listener_owner = Arc::new(shared.clone());
    if let Some(path) = editor::start_listener(
        {
            let shared = Arc::downgrade(&listener_owner);
            Arc::new(move |effects: Vec<Effect>| {
                let Some(shared) = shared.upgrade() else {
                    for effect in &effects {
                        if let Effect::EditDocument { run_id, .. } = effect {
                            editor::registry().resolve(run_id, editor::Reply::Abort);
                        }
                    }
                    return;
                };
                if shared.closing.load(Ordering::SeqCst) {
                    for effect in &effects {
                        if let Effect::EditDocument { run_id, .. } = effect {
                            editor::registry().resolve(run_id, editor::Reply::Abort);
                        }
                    }
                    return;
                }
                let presentation = shared
                    .client
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .presentation
                    .clone();
                if matches!(presentation, Some(Presentation::Plain(_))) {
                    deliver_plain_effects(&shared, effects);
                    return;
                }
                let unsupported_edit = shared.client.lock().unwrap_or_else(|p| p.into_inner())
                    .presentation.as_ref().is_some_and(|presentation| matches!(
                        presentation, Presentation::Rich(caps) if !caps.supports_effect("editDocument")
                    ));
                if unsupported_edit {
                    for effect in &effects {
                        if let Effect::EditDocument { run_id, .. } = effect {
                            editor::registry().resolve(run_id, editor::Reply::Abort);
                        }
                    }
                }
                let _ = shared
                    .sender
                    .send(custom_ui(&shared.client).effects(ViewEffectsParams {
                        effects: effects.clone(),
                    }));
                // A dynamic client gets the edit's keymap as a normal
                // `view/keymap`, keyed by the edit id. Static clients use
                // their own bindings and ignore it.
                if !unsupported_edit && dynamic_delivery(&shared.client) {
                    for effect in &effects {
                        if let Effect::EditDocument { run_id, kind, .. } = effect {
                            let config = shared
                                .edit_configs
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .get(run_id)
                                .cloned();
                            if let Some(keymap) =
                                config.and_then(|config| edit_keymap(&config, run_id, kind))
                            {
                                let _ = shared.sender.send(Message::Notification(
                                    Notification::new(VIEW_KEYMAP.to_string(), keymap),
                                ));
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
        let read = transport::read_message(&mut reader).await;
        if let (Some(log), Ok(Some(message))) = (&wirelog, &read) {
            if let Err(error) = log.record("in", message) {
                eprintln!("wirelog record failed: {error}");
            }
        }
        match read {
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
                    let _ = sender.send(Message::Response(Response::new_err(
                        request.id,
                        ErrorCode::InvalidRequest as i32,
                        "server is shutting down".to_string(),
                    )));
                    continue;
                }
                // Draft barriers and document notifications execute on the
                // reader's ordered lane, never in unordered request tasks.
                if request.method == "workspace/executeCommand" {
                    if let Some(reason) = plain_unsupported_execute_reason(&request) {
                        let (response, notifications) =
                            match plain_actions::plain_unsupported_message(reason) {
                                Ok(dispatched) => (
                                    Response::new_ok(request.id, dispatched.result),
                                    dispatched.notifications,
                                ),
                                Err(error) => (error_response(request.id, error), Vec::new()),
                            };
                        for message in notifications {
                            let _ = sender.send(message);
                        }
                        let _ = sender.send(Message::Response(response));
                        continue;
                    }
                    if let Some(params) = plain_actions::plain_execute_params(&request) {
                        let (response, notifications) =
                            match plain_actions::execute_plain_action(&shared, params) {
                                Ok(dispatched) => (
                                    Response::new_ok(request.id, dispatched.result),
                                    dispatched.notifications,
                                ),
                                Err(error) => (error_response(request.id, error), Vec::new()),
                            };
                        for message in notifications {
                            let _ = sender.send(message);
                        }
                        let _ = sender.send(Message::Response(response));
                        continue;
                    }
                    let response = match drafts::draft_request(&shared, &request) {
                        Ok(result) => Response::new_ok(request.id, result),
                        Err(error) => {
                            let mut response = Response::new_err(
                                request.id,
                                ErrorCode::InvalidParams as i32,
                                error.to_string(),
                            );
                            if let Err(error) = &mut response.response_result {
                                let params = if request.method == "workspace/executeCommand" {
                                    request
                                        .params
                                        .pointer("/arguments/0")
                                        .unwrap_or(&request.params)
                                } else {
                                    &request.params
                                };
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
                "textDocument/didOpen"
                | "textDocument/didChange"
                | "textDocument/didClose"
                | "textDocument/didSave" => {
                    let uri = notification
                        .params
                        .pointer("/textDocument/uri")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let draft_run = editor::registry()
                        .run_for_uri(&shared.draft_owner, uri)
                        .ok();
                    let _plain = shared
                        .plain_lifecycle
                        .lock()
                        .unwrap_or_else(|p| p.into_inner());
                    let plain = matches!(
                        shared
                            .client
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .presentation
                            .as_ref(),
                        Some(Presentation::Plain(_))
                    );
                    let notification_result = drafts::draft_notification(&shared, &notification);
                    if let Err(error) = notification_result.as_ref() {
                        if notification.method == "textDocument/didChange" {
                            shared
                                .documents
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .require_repair(
                                    uri,
                                    notification
                                        .params
                                        .pointer("/textDocument/version")
                                        .and_then(Value::as_i64)
                                        .and_then(|v| i32::try_from(v).ok()),
                                );
                        }
                        if let Some(run) = draft_run {
                            editor::registry().notification_failure(
                                &shared.draft_owner,
                                &run,
                                uri,
                                error.to_string(),
                                notification.method == "textDocument/didClose",
                                |effects| {
                                    if plain {
                                        // The reader holds `plain_lifecycle` for the
                                        // whole notification arm, and only
                                        // `terminal_cleanup` sets `closing` (under
                                        // that same lock). A shutdown therefore
                                        // cannot race this delivery: either the
                                        // draft is still pending here, or terminal
                                        // cleanup already aborted it and
                                        // `notification_failure` emitted nothing.
                                        deliver_plain_effects(&listener_owner, effects);
                                    } else {
                                        let _ = sender.send(
                                            custom_ui(&shared.client)
                                                .effects(ViewEffectsParams { effects }),
                                        );
                                    }
                                },
                            );
                        } else {
                            tracing::debug!(%error, "rejected document synchronization; repair required");
                        }
                        if notification.method == "textDocument/didClose" {
                            let close = close_plain(&shared, uri);
                            // Routing the resumed renders reacquires the
                            // lifecycle guard, so release it first.
                            drop(_plain);
                            deliver_plain_close_outcome(&shared, close);
                        }
                    } else {
                        match notification.method.as_str() {
                            "textDocument/didOpen" => {
                                let close = schedule_plain_open(&shared, uri.to_owned());
                                drop(_plain);
                                deliver_plain_close_outcome(&shared, close);
                            }
                            "textDocument/didChange" => {
                                let dispatched =
                                    plain_change(&shared, uri, notification_result.ok().flatten());
                                // Delivery of a repaired render re-enters the
                                // Plain lifecycle guard, so it must not be
                                // held across the key execution or this call.
                                drop(_plain);
                                if let Some(dispatch) = dispatched {
                                    let turn = shared.reserve_mutation(dispatch.repo().to_owned());
                                    handlers.spawn(handle_plain_key(
                                        shared.clone(),
                                        dispatch,
                                        turn,
                                    ));
                                }
                                reconcile_rich_prompt(&shared, uri, &sender);
                            }
                            "textDocument/didClose" => {
                                let close = close_plain(&shared, uri);
                                drop(_plain);
                                deliver_plain_close_outcome(&shared, close);
                            }
                            _ => {}
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
    shared.stop_watchers();
    // `Shared` also holds a sender clone, so it must go before the writer
    // channel can close.
    drop(listener_owner);
    drop(shared);
    drop(sender);

    match writer.await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => return Err(Error::from(error)),
        Err(error) => return Err(Error::new(error.to_string())),
    }
    if let Some(log) = wirelog {
        if let Err(error) = log.finish() {
            eprintln!("wirelog finish failed: {error}");
        }
    }
    if let Some(error) = read_error {
        return Err(error);
    }
    Ok(exit_code)
}

/// Find registered Plain commands that must be refused before target parsing or execution.
fn plain_unsupported_execute_reason(request: &Request) -> Option<&'static str> {
    let params = request.params.as_object()?;
    if params.get("command")?.as_str()? != "magit.command" {
        return None;
    }
    let argument = request.params.pointer("/arguments/0")?;
    // Rich commands such as common.confirm also use magit.command. Only
    // classify requests carrying the Plain document/generation envelope.
    if argument.get("documentUri").is_none() && argument.get("viewGeneration").is_none() {
        return None;
    }
    let command = argument.get("command")?.as_str()?;
    match crate::commands::registry().availability(command)? {
        crate::commands::Availability::Planned(_) => {
            Some("This command is planned and cannot run in Plain mode")
        }
        crate::commands::Availability::Implemented
            if !crate::dispatch::content_command(command)
                && crate::dispatch::visibility_command(command).is_none()
                && !crate::dispatch::plain_todo_action(command)
                && !matches!(
                    command,
                    "open.commit"
                        | "open.rebase"
                        | "rebase.todo-submit"
                        | "open.branch"
                        | "open.log"
                        | "menu.run-action"
                        | "common.cancel"
                        | "common.confirm"
                        | "select.toggle-mark"
                ) =>
        {
            Some("This command requires a Plain client adapter that is not available")
        }
        crate::commands::Availability::Implemented => None,
    }
}

/// Own stdout: serialise every response through one channel so bytes never
/// interleave, and flush each frame before reading the next.
async fn writer_loop<W>(
    mut writer: W,
    mut receiver: mpsc::UnboundedReceiver<Message>,
    wirelog: Option<Arc<crate::wirelog::WireLog>>,
) -> std::io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    while let Some(message) = receiver.recv().await {
        if let Some(log) = &wirelog {
            if let Err(error) = log.record("out", &message) {
                eprintln!("wirelog record failed: {error}");
            }
        }
        transport::write_message(&mut writer, &message).await?;
    }
    Ok(())
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
            if let Some(handle) = shared.session(&repo) {
                if let Err(error) = handle
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .ack(&params.view_id, params.version)
                {
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
            let Some(handle) = shared.session(&repo) else {
                return Vec::new();
            };
            let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
            let viewport = Viewport {
                cols: params.cols,
                rows: params.rows,
            };
            let resized = session.resize(&params.view_id, viewport);
            drop(session);
            match resized {
                // A menu re-flows its columns for the new width.
                Ok(Some(render)) => deliver_render(shared, render).into_iter().collect(),
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

fn initialize_workspace_roots(params: &Value) -> Vec<crate::presentation::resolve::Repository> {
    use crate::git::GitRepo;
    let mut paths = Vec::new();
    if let Some(folders) = params
        .pointer("/workspaceFolders")
        .and_then(Value::as_array)
    {
        paths.extend(folders.iter().filter_map(|folder| {
            folder
                .get("uri")
                .and_then(Value::as_str)
                .and_then(crate::file_uri::path_from_uri)
        }));
    }
    if paths.is_empty() {
        if let Some(path) = params
            .get("rootUri")
            .and_then(Value::as_str)
            .and_then(crate::file_uri::path_from_uri)
        {
            paths.push(path);
        }
    }
    let mut repositories = Vec::new();
    for path in paths {
        if let Some(repository) = GitRepo::discover(&path).ok().flatten().map(|repo| {
            crate::presentation::resolve::Repository {
                root: repo.root().to_owned(),
                git_dir: repo.git_dir().to_owned(),
                common_dir: repo.common_dir().to_owned(),
            }
        }) {
            if !repositories
                .iter()
                .any(|item: &crate::presentation::resolve::Repository| item.root == repository.root)
            {
                repositories.push(repository);
            }
        }
    }
    repositories
}

#[cfg(all(test, unix))]
mod repository_uri_tests {
    use crate::file_uri::path_from_uri as file_uri_path;

    #[test]
    fn file_uri_decoding_is_complete_and_rejects_nonlocal_or_malformed_uris() {
        assert_eq!(
            file_uri_path("file:///tmp/a%20%C3%A9%23%25").unwrap(),
            std::path::PathBuf::from("/tmp/a é#%")
        );
        assert_eq!(
            file_uri_path("file://localhost/tmp/a%20b").unwrap(),
            std::path::PathBuf::from("/tmp/a b")
        );
        for invalid in [
            "https://example.test/a",
            "file://remote/tmp/a",
            "file:///tmp/%Q0",
            "file:///tmp/%FF",
            "file:///tmp/%2",
        ] {
            assert!(file_uri_path(invalid).is_none(), "accepted {invalid}");
        }
    }
}

/// The last acked tree of the view showing `uri`.
fn tree_for_positional_request(shared: &Shared, uri: &str) -> Option<Tree> {
    shared.view_tree_for_uri(uri)
}

fn plain_provider_snapshot(shared: &Shared, uri: &str) -> Option<(Tree, String, String)> {
    let _lifecycle = shared
        .plain_lifecycle
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let documents = shared.documents.lock().unwrap_or_else(|p| p.into_inner());
    let document = documents.get(uri).filter(|doc| !doc.repair)?;
    let text = document.text.clone();
    let entries = shared
        .plain_documents
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let entry = entries.get(uri)?;
    let (tree, view_id, _, _, _) = entry.controller.action_snapshot(&text)?;
    if view_id != entry.view_id {
        return None;
    }
    Some((tree.clone(), entry.repo.clone(), text))
}

fn is_plain_provider_uri(shared: &Shared, uri: &str) -> bool {
    let negotiated = shared
        .client
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .plain
        .is_some();
    negotiated
        && crate::file_uri::path_from_uri(uri).is_some_and(|path| {
            crate::config::is_presentation_file(&path, &shared.config.config().presentation_files)
        })
}

fn effect_notifications(client: &Arc<Mutex<ClientCaps>>, effects: Vec<Effect>) -> Vec<Message> {
    let plain = matches!(
        client
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .presentation
            .as_ref(),
        Some(Presentation::Plain(_))
    );
    if !plain {
        return vec![custom_ui(client).effects(ViewEffectsParams { effects })];
    }
    effects
        .into_iter()
        .filter_map(|effect| match effect {
            Effect::Message {
                level,
                text,
                hint,
                detail,
            } => {
                let mut message = text;
                for part in hint.into_iter().chain(detail) {
                    message.push('\n');
                    message.push_str(&part);
                }
                let kind = match level {
                    MessageLevel::Error => 1,
                    MessageLevel::Warn => 2,
                    MessageLevel::Info => 3,
                };
                Some(show_message(kind, message))
            }
            _ => None,
        })
        .collect()
}

/// A `window/showMessage` notification: 1 error, 2 warning, 3 info, 4 log.
fn show_message(kind: u8, message: String) -> Message {
    Message::Notification(Notification::new(
        "window/showMessage".to_string(),
        serde_json::json!({ "type": kind, "message": message }),
    ))
}

fn capabilities(encoding: Encoding) -> Result<Value> {
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
    // Work-done progress is a client capability, not a server advertisement.
    if let Some(object) = capabilities.as_object_mut() {
        object.insert("codeActionProvider".to_string(), serde_json::json!(true));
        object.insert(
            "executeCommandProvider".to_string(),
            serde_json::json!({ "commands": ["magit.command"] }),
        );
        object.insert(
            "textDocumentSync".to_string(),
            serde_json::json!({ "openClose": true, "change": 2, "save": { "includeText": true } }),
        );
        object.insert(
            "positionEncoding".to_string(),
            serde_json::json!(match encoding {
                Encoding::Utf8 => "utf-8",
                Encoding::Utf16 => "utf-16",
                Encoding::Utf32 => "utf-32",
            }),
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
            | "textDocument/codeAction"
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
            | ViewPickRespondRequest::METHOD
            | ViewResyncRequest::METHOD
            | "window/workDoneProgress/create"
    )
}

/// The repository a mutation must lock; only the staging commands write.
fn mutation_repo_path(shared: &Shared, method: &str, params: &Value) -> Option<String> {
    // View lifecycle requests compete with status completion for publication,
    // so close, mode changes and every invoke (including refresh) join the
    // repository's ordering. Read-only requests stay outside it.
    if !matches!(
        method,
        ViewKeyRequest::METHOD
            | ViewInvokeRequest::METHOD
            | ViewCloseRequest::METHOD
            | ViewSetModeRequest::METHOD
    ) {
        return None;
    }
    let view_id = params.get("viewId").and_then(Value::as_str)?;
    let repo = shared.repo_for_view(view_id)?;
    // Prompting work retains its ordering turn, but new commands must reach
    // the session's readable refusal rather than queue behind human input.
    // Closing or changing mode must not wait for an answer either.
    let published = shared
        .published
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&repo)
        .cloned();
    if published.is_some_and(|views| views.has_pending_prompt()) {
        return None;
    }
    Some(repo)
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
    use crate::view::Placement;

    fn test_shared() -> (Shared, mpsc::UnboundedReceiver<Message>) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "").unwrap();
        let config = ConfigService::load(LoadOptions {
            config_path: Some(path),
            ..Default::default()
        })
        .unwrap();
        let (sender, receiver) = mpsc::unbounded_channel();
        (Shared::new(Arc::new(config), None, sender), receiver)
    }

    async fn poll_pending<F: std::future::Future>(mut future: std::pin::Pin<&mut F>) {
        std::future::poll_fn(|cx| {
            assert!(future.as_mut().poll(cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
    }

    fn request_id(receiver: &mut mpsc::UnboundedReceiver<Message>) -> RequestId {
        let Message::Request(request) = receiver.try_recv().expect("outbound request") else {
            panic!("expected request")
        };
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
        shared.resolve_outbound(Response::new_ok(
            RequestId::from("unknown".to_string()),
            Value::Null,
        ));
        assert_eq!(shared.outbound.lock().unwrap().len(), 2);
        shared.resolve_outbound(Response::new_ok(
            id2.clone(),
            serde_json::json!("second result"),
        ));
        shared.resolve_outbound(Response::new_err(id2, -32000, "duplicate".into()));
        assert_eq!(shared.outbound.lock().unwrap().len(), 1);
        assert_eq!(
            shared
                .await_outbound(second, rx2)
                .await
                .unwrap()
                .response_result
                .unwrap(),
            serde_json::json!("second result")
        );
        shared.resolve_outbound(Response::new_err(id1.clone(), -32000, "declined".into()));
        assert!(shared
            .await_outbound(first, rx1)
            .await
            .unwrap_err()
            .to_string()
            .contains("declined"));
        shared.resolve_outbound(Response::new_ok(id1, Value::Null));
        assert!(shared.outbound.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn outbound_timeout_and_unpolled_await_remove_registered_entries() {
        let (shared, mut messages) = test_shared();
        let (pending, rx) = shared.register_outbound("timeout", Value::Null).unwrap();
        let id = request_id(&mut messages);
        assert!(shared
            .await_outbound_for(pending, rx, std::time::Duration::ZERO)
            .await
            .unwrap_err()
            .to_string()
            .contains("timed out"));
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
        assert!(shared
            .await_outbound(pending, rx)
            .await
            .unwrap_err()
            .to_string()
            .contains("disconnected"));
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
                    assert!(matches!(
                        rx.try_recv(),
                        Err(tokio::sync::oneshot::error::TryRecvError::Closed)
                    ));
                }
            });
            assert!(shared.outbound.lock().unwrap().is_empty());
            assert!(shared
                .register_outbound("after shutdown", Value::Null)
                .is_err());
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
            let Message::Notification(notice) = messages.try_recv().unwrap() else {
                panic!("expected progress")
            };
            assert_eq!(notice.method, "$/progress");
            assert_eq!(notice.params["token"], token);
            assert_eq!(notice.params["value"]["kind"], phase);
        }
        assert!(messages.try_recv().is_err());
        assert!(shared.outbound.lock().unwrap().is_empty());
        assert!(shared.progress.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn late_create_reply_flushes_ended_progress_unless_terminal() {
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
            if terminal {
                shared.terminal_cleanup();
                create.await;
                shared.end_progress(&shared.sender, &token);
                assert!(messages.try_recv().is_err(), "no orphan begin/end");
            } else {
                shared.end_progress(&shared.sender, &token);
                create.await;
                shared.end_progress(&shared.sender, &token);
                // The confirmed create publishes the deferred pair in order,
                // so the client never sees an end without its begin.
                for phase in ["begin", "end"] {
                    let Message::Notification(notice) = messages.try_recv().unwrap() else {
                        panic!("expected the flushed progress pair")
                    };
                    assert_eq!(notice.method, "$/progress");
                    assert_eq!(notice.params["token"], token);
                    assert_eq!(notice.params["value"]["kind"], phase);
                }
                assert!(messages.try_recv().is_err(), "no phase after the pair");
            }
            assert!(shared.outbound.lock().unwrap().is_empty());
            assert!(shared.progress.lock().unwrap().is_empty());
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
            if disconnected {
                shared.terminal_cleanup();
            } else {
                shared.resolve_outbound(Response::new_err(id, -32000, "declined".into()));
            }
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
        assert!(shared
            .register_outbound("disconnected", Value::Null)
            .is_err());
        assert!(shared.outbound.lock().unwrap().is_empty());
    }

    /// Registry lifecycle for a managed draft needs the Unix draft boundary.
    #[cfg(unix)]
    #[test]
    fn commit_edit_releases_registry_and_repository_config_together() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(tmp.path())
            .status()
            .unwrap()
            .success());
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
        let id = editor::registry()
            .create(
                tmp.path().to_str().unwrap(),
                "test-connection",
                editor::DraftKind::CommitMessage,
            )
            .unwrap();
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

    fn git_in(dir: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    /// A repository with one commit and one unstaged change.
    fn changed_repository() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        git_in(tmp.path(), &["init", "-q"]);
        git_in(tmp.path(), &["config", "user.email", "test@example.com"]);
        git_in(tmp.path(), &["config", "user.name", "Test"]);
        git_in(tmp.path(), &["config", "commit.gpgsign", "false"]);
        std::fs::write(tmp.path().join("f.txt"), "initial\n").unwrap();
        git_in(tmp.path(), &["add", "."]);
        git_in(tmp.path(), &["commit", "-q", "-m", "init"]);
        std::fs::write(tmp.path().join("f.txt"), "changed\n").unwrap();
        tmp
    }

    #[test]
    fn closing_a_prompt_view_or_origin_delivers_resumed_sibling_refreshes() {
        let tmp = changed_repository();
        let repo = tmp
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        for close_origin in [false, true] {
            let (shared, _messages) = test_shared();
            let handle = Arc::new(Mutex::new(Session::new(&repo)));
            let (status, log, prompt, status_version, log_version) = {
                let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
                let (status, _) = session
                    .open(
                        "status",
                        ViewKind::Status,
                        Placement::Main,
                        Viewport::default(),
                    )
                    .unwrap();
                let (log, _) = session
                    .open("log", ViewKind::Log, Placement::Main, Viewport::default())
                    .unwrap();
                let step = session
                    .start_invoke(&ViewInvokeParams {
                        view_id: status.id.clone(),
                        command: "commit.create".to_string(),
                        version: None,
                        args: None,
                        cursor: None,
                        selection: None,
                    })
                    .unwrap();
                let Execution::Prompt(request) = step else {
                    panic!("commit.create asks before staging");
                };
                session.open_document_prompt(&request).unwrap();
                let prompt = session
                    .prompt_view_id()
                    .expect("the document prompt is open")
                    .to_string();
                let status_version = session.views().get(&status.id).unwrap().version();
                let log_version = session.views().get(&log.id).unwrap().version();

                assert!(
                    session.refresh_all().is_empty(),
                    "an open prompt holds every refresh"
                );
                assert!(session.views().get(&status.id).unwrap().dirty());
                assert!(session.views().get(&log.id).unwrap().dirty());
                (status.id, log.id, prompt, status_version, log_version)
            };
            shared
                .sessions
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(repo.clone(), handle.clone());
            {
                let mut view_repos = shared.view_repos.lock().unwrap_or_else(|p| p.into_inner());
                for view_id in [&status, &log, &prompt] {
                    view_repos.insert(view_id.clone(), repo.clone());
                }
            }

            let target = if close_origin { &status } else { &prompt };
            let dispatched = close_view(
                &shared,
                serde_json::to_value(ViewCloseParams {
                    view_id: target.clone(),
                })
                .unwrap(),
            )
            .unwrap();
            assert_eq!(dispatched.result, Value::Null, "close answers null");

            let renders: Vec<(String, u64)> = dispatched
                .notifications
                .iter()
                .filter_map(|message| {
                    let Message::Notification(notification) = message else {
                        return None;
                    };
                    if notification.method != VIEW_RENDER {
                        return None;
                    }
                    Some((
                        notification.params.get("viewId")?.as_str()?.to_string(),
                        notification.params.get("version")?.as_u64()?,
                    ))
                })
                .collect();
            assert!(
                renders
                    .iter()
                    .any(|(view_id, version)| view_id == &log && *version > log_version),
                "the log view resumes after close_origin={close_origin}: {renders:?}"
            );
            if close_origin {
                assert!(
                    !renders.iter().any(|(view_id, _)| view_id == &status),
                    "the closed origin does not render: {renders:?}"
                );
            } else {
                assert!(
                    renders.iter().any(|(view_id, version)| {
                        view_id == &status && *version > status_version
                    }),
                    "the origin resumes after the prompt closes: {renders:?}"
                );
            }

            let session = handle.lock().unwrap_or_else(|p| p.into_inner());
            assert!(!session.has_pending_prompt());
            let log_view = session.views().get(&log).unwrap();
            assert!(log_view.version() > log_version);
            assert!(!log_view.dirty(), "the resumed log has no refresh left");
            if !close_origin {
                let status_view = session.views().get(&status).unwrap();
                assert!(status_view.version() > status_version);
                assert!(
                    !status_view.dirty(),
                    "the resumed origin has no refresh left"
                );
            }
        }
    }
}
