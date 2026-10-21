use crate::errors;
use crate::git::{self, GitRepo};
use crate::protocol::*;
use crate::semantic_tokens;
use crate::session::Session;
use crate::status_view;
use crate::transport;
use crate::tree::{Document, Tree};
use crate::view::Viewport;
use crate::{Error, Result};
use lsp_server::{ErrorCode, Message, Notification, Request, Response};
use lsp_types::request::Request as LspRequest;
use lsp_types::{
    ExecuteCommandOptions, FoldingRangeProviderCapability, SemanticTokensFullOptions,
    SemanticTokensOptions, SemanticTokensResult, SemanticTokensServerCapabilities,
    ServerCapabilities,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncWrite, BufReader};
use tokio::sync::{mpsc, Mutex as AsyncMutex};
use tokio::task::JoinSet;

type Trees = Arc<Mutex<HashMap<String, Tree>>>;
type UriRepos = Arc<Mutex<HashMap<String, String>>>;
type Versions = Arc<Mutex<HashMap<String, u64>>>;
type RepoLocks = Arc<AsyncMutex<HashMap<String, Arc<AsyncMutex<()>>>>>;

/// Server state shared by concurrent request handlers.
#[derive(Clone)]
struct Shared {
    /// Last rendered tree per repository root, keyed by the resolved root.
    trees: Trees,
    /// Buffer URI → repository root, registered by `vcs/status`.
    uri_repos: UriRepos,
    /// Monotonic render version per repository.
    versions: Versions,
    repo_locks: RepoLocks,
    /// Open views per repository root.
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    /// View id → repository root, so notifications can find their session.
    view_repos: Arc<Mutex<HashMap<String, String>>>,
    /// Global view-id counter; ids are unique across repositories.
    next_view: Arc<AtomicU64>,
}

impl Shared {
    fn new() -> Self {
        Self {
            trees: Arc::new(Mutex::new(HashMap::new())),
            uri_repos: Arc::new(Mutex::new(HashMap::new())),
            versions: Arc::new(Mutex::new(HashMap::new())),
            repo_locks: Arc::new(AsyncMutex::new(HashMap::new())),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            view_repos: Arc::new(Mutex::new(HashMap::new())),
            next_view: Arc::new(AtomicU64::new(0)),
        }
    }

    fn bump_version(&self, repo_path: &str) -> u64 {
        let mut versions = self
            .versions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let version = versions.entry(repo_path.to_string()).or_insert(0);
        *version += 1;
        *version
    }

    /// Store the tree and return the document the client will render.
    fn store_tree(&self, repo_path: &str, mut tree: Tree) -> Document {
        tree.version = self.bump_version(repo_path);
        let document = Document::from(&tree);
        self.trees
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(repo_path.to_string(), tree);
        document
    }

    /// Register the buffer URI that displays `repo_path`.
    fn register_uri(&self, uri: &str, repo_path: &str) {
        self.uri_repos
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(uri.to_string(), repo_path.to_string());
    }

    /// The tree displayed by `uri`, if a status request registered it.
    fn tree_for_uri(&self, uri: &str) -> Option<Tree> {
        let repo = self
            .uri_repos
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(uri)
            .cloned()?;
        self.trees
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&repo)
            .cloned()
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
    /// Position-based requests use this so they cannot refer to a render the
    /// client has not applied yet.
    fn view_tree_for_uri(&self, uri: &str) -> Option<Tree> {
        let sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        sessions
            .values()
            .find_map(|session| session.acked_tree_for_uri(uri))
    }
}

/// Run the LSP server on stdio until the client requests shutdown.
pub fn run() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(Error::from)?;

    let code = runtime.block_on(serve())?;
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}

/// Read framed messages until `exit`, then return the process exit code.
async fn serve() -> Result<i32> {
    let mut reader = BufReader::new(tokio::io::stdin());
    let (sender, receiver) = mpsc::unbounded_channel();
    let writer = tokio::spawn(writer_loop(tokio::io::stdout(), receiver));
    let shared = Shared::new();
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
                handlers.spawn(handle_request(request, shared.clone(), sender.clone()));
            }
            Ok(Some(Message::Notification(notification))) => match notification.method.as_str() {
                "exit" => break if shutdown_requested { 0 } else { 1 },
                // Acks and resizes update session state; they never answer.
                "view/rendered" | "view/resize" => {
                    handle_view_notification(&shared, &notification)
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
    handlers.abort_all();
    while handlers.join_next().await.is_some() {}
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

async fn handle_request(request: Request, shared: Shared, sender: mpsc::UnboundedSender<Message>) {
    let id = request.id.clone();
    let method = request.method.clone();

    let (response, notifications) = if !is_request_method(&method) {
        (
            Response::new_err(
                id,
                ErrorCode::MethodNotFound as i32,
                format!("unknown method: {method}"),
            ),
            Vec::new(),
        )
    } else if let Some(repo_path) = mutation_repo_path(&shared, &method, &request.params) {
        let lock = repository_lock(&shared.repo_locks, &repo_path).await;
        let _guard = lock.lock().await;
        maybe_test_delay().await;
        dispatch_response(request, &shared).await
    } else {
        dispatch_response(request, &shared).await
    };

    // The response creates or identifies the view, so it goes first; render
    // notifications then refer to something the client already knows.
    let _ = sender.send(Message::Response(response));
    for notification in notifications {
        let _ = sender.send(notification);
    }
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
async fn dispatch_response(
    request: Request,
    shared: &Shared,
) -> (Response, Vec<Message>) {
    let id = request.id.clone();
    let method = request.method.clone();
    let shared = shared.clone();

    match tokio::task::spawn_blocking(move || dispatch(&request, &shared)).await {
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
fn handle_view_notification(shared: &Shared, notification: &Notification) {
    match notification.method.as_str() {
        VIEW_RENDERED => {
            let Ok(params) =
                serde_json::from_value::<ViewRenderedParams>(notification.params.clone())
            else {
                tracing::warn!("ignoring malformed view/rendered");
                return;
            };
            let Some(repo) = shared.repo_for_view(&params.view_id) else {
                return;
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
        }
        VIEW_RESIZE => {
            let Ok(params) =
                serde_json::from_value::<ViewResizeParams>(notification.params.clone())
            else {
                tracing::warn!("ignoring malformed view/resize");
                return;
            };
            let Some(repo) = shared.repo_for_view(&params.view_id) else {
                return;
            };
            let mut sessions = shared
                .sessions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(session) = sessions.get_mut(&repo) {
                let viewport = Viewport {
                    cols: params.cols,
                    rows: params.rows,
                };
                if let Err(error) = session.resize(&params.view_id, viewport) {
                    tracing::warn!(%error, "failed to resize view");
                }
            }
        }
        _ => {}
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
        "initialize" => Ok(Dispatched::value(
            serde_json::json!({ "capabilities": capabilities()? }),
        )),
        "shutdown" => Ok(Dispatched::value(Value::Null)),
        StatusRequest::METHOD => {
            let p: StatusParams = from_params(params)?;
            let status = GitRepo::open(&p.repo_path)?.status()?;
            let document = render_status(shared, &status, p.uri.as_deref());
            Ok(Dispatched::value(serde_json::to_value(StatusResponse {
                status,
                document,
            })?))
        }
        DiffRequest::METHOD => {
            let p: DiffParams = from_params(params)?;
            let diff = if p.staged {
                GitRepo::open(&p.repo_path)?.diff_staged(&p.path)?
            } else {
                GitRepo::open(&p.repo_path)?.diff_unstaged(&p.path)?
            };
            Ok(Dispatched::value(serde_json::to_value(DiffResponse {
                path: p.path,
                staged: p.staged,
                diff,
            })?))
        }
        StageHunkRequest::METHOD => {
            let p: HunkMutationParams = from_params(params)?;
            Ok(Dispatched::value(serde_json::to_value(run_mutation(
                shared,
                &p.repo_path,
                |repo| repo.stage_hunk(&p.path, &p.hunk_id).map(|_| true),
            )?)?))
        }
        UnstageHunkRequest::METHOD => {
            let p: HunkMutationParams = from_params(params)?;
            Ok(Dispatched::value(serde_json::to_value(run_mutation(
                shared,
                &p.repo_path,
                |repo| repo.unstage_hunk(&p.path, &p.hunk_id).map(|_| true),
            )?)?))
        }
        StageLinesRequest::METHOD => {
            let p: LinesMutationParams = from_params(params)?;
            Ok(Dispatched::value(serde_json::to_value(run_mutation(
                shared,
                &p.repo_path,
                |repo| repo.stage_lines(&p.path, &p.hunk_id, p.start, p.end),
            )?)?))
        }
        UnstageLinesRequest::METHOD => {
            let p: LinesMutationParams = from_params(params)?;
            Ok(Dispatched::value(serde_json::to_value(run_mutation(
                shared,
                &p.repo_path,
                |repo| repo.unstage_lines(&p.path, &p.hunk_id, p.start, p.end),
            )?)?))
        }
        StageFileRequest::METHOD => {
            let p: FileMutationParams = from_params(params)?;
            Ok(Dispatched::value(serde_json::to_value(run_mutation(
                shared,
                &p.repo_path,
                |repo| repo.stage_file(&p.path).map(|_| true),
            )?)?))
        }
        UnstageFileRequest::METHOD => {
            let p: FileMutationParams = from_params(params)?;
            Ok(Dispatched::value(serde_json::to_value(run_mutation(
                shared,
                &p.repo_path,
                |repo| repo.unstage_file(&p.path).map(|_| true),
            )?)?))
        }
        StageAllRequest::METHOD => {
            let p: RepoMutationParams = from_params(params)?;
            Ok(Dispatched::value(serde_json::to_value(run_mutation(
                shared,
                &p.repo_path,
                |repo| repo.stage_all().map(|_| true),
            )?)?))
        }
        StageUntrackedRequest::METHOD => {
            let p: RepoMutationParams = from_params(params)?;
            Ok(Dispatched::value(serde_json::to_value(run_mutation(
                shared,
                &p.repo_path,
                |repo| repo.stage_untracked().map(|_| true),
            )?)?))
        }
        UnstageAllRequest::METHOD => {
            let p: RepoMutationParams = from_params(params)?;
            Ok(Dispatched::value(serde_json::to_value(run_mutation(
                shared,
                &p.repo_path,
                |repo| repo.unstage_all().map(|_| true),
            )?)?))
        }
        ViewOpenRequest::METHOD => open_view(shared, params),
        ViewCloseRequest::METHOD => close_view(shared, params),
        ViewInvokeRequest::METHOD => invoke_view(shared, params),
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
        TargetsRequest::METHOD => {
            let p: TargetsParams = from_params(params)?;
            let targets = tree_for_positional_request(shared, &p.uri)
                .map(|tree| tree.targets())
                .unwrap_or_default();
            Ok(Dispatched::value(serde_json::to_value(TargetsResponse {
                targets,
            })?))
        }
        "textDocument/foldingRange" => {
            let p: lsp_types::FoldingRangeParams = from_params(params)?;
            let ranges = tree_for_positional_request(shared, p.text_document.uri.as_str())
                .map(|tree| tree.folding_ranges())
                .unwrap_or_default();
            Ok(Dispatched::value(serde_json::to_value(ranges)?))
        }
        "textDocument/semanticTokens/full" => {
            let p: lsp_types::SemanticTokensParams = from_params(params)?;
            let tokens = tree_for_positional_request(shared, p.text_document.uri.as_str())
                .map(|tree| semantic_tokens::encode(&tree))
                .unwrap_or_default();
            Ok(Dispatched::value(serde_json::to_value(
                SemanticTokensResult::Tokens(tokens),
            )?))
        }
        TransientMenuRequest::METHOD => Ok(Dispatched::value(serde_json::to_value(
            stub_transient_menu(),
        )?)),
        "workspace/executeCommand" => Ok(Dispatched::value(Value::Null)),
        other => Err(Error::new(format!("unknown method: {other}"))),
    }
}

/// The last acked tree of a view, falling back to the legacy `vcs/status` tree.
fn tree_for_positional_request(shared: &Shared, uri: &str) -> Option<Tree> {
    shared
        .view_tree_for_uri(uri)
        .or_else(|| shared.tree_for_uri(uri))
}

fn open_view(shared: &Shared, params: Value) -> Result<Dispatched> {
    let p: ViewOpenParams = from_params(params)?;
    let repo = p
        .repo_path()
        .ok_or_else(|| Error::new("view/open needs args.repo_path for a status view"))?
        .to_string();
    let id = shared.next_view_id();
    let mut sessions = shared
        .sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = sessions
        .entry(repo.clone())
        .or_insert_with(|| Session::new(&repo));
    let (descriptor, render) = session.open(
        id.clone(),
        p.kind,
        p.placement.unwrap_or_default(),
        p.viewport,
    )?;
    drop(sessions);
    shared
        .view_repos
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(id, repo);

    let notification = Notification::new(VIEW_RENDER.to_string(), render);
    Ok(Dispatched::with(
        serde_json::to_value(descriptor)?,
        vec![Message::Notification(notification)],
    ))
}

fn close_view(shared: &Shared, params: Value) -> Result<Dispatched> {
    let p: ViewCloseParams = from_params(params)?;
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
    session.close(&p.view_id)?;
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
    let outcome = session.invoke(&p)?;
    drop(sessions);

    let notifications = outcome
        .render
        .map(|render| vec![Message::Notification(Notification::new(
            VIEW_RENDER.to_string(),
            render,
        ))])
        .unwrap_or_default();
    Ok(Dispatched::with(
        serde_json::to_value(ViewInvokeResponse {
            effects: outcome.effects,
        })?,
        notifications,
    ))
}

/// Build, version and remember the tree for `status`; register the buffer URI.
fn render_status(shared: &Shared, status: &git::Status, uri: Option<&str>) -> Document {
    let tree = status_view::build(status);
    let document = shared.store_tree(&status.repo_path, tree);
    if let Some(uri) = uri {
        shared.register_uri(uri, &status.repo_path);
    }
    document
}

/// Run a mutation for `repo_path`, storing the fresh tree for later requests.
fn run_mutation<F>(shared: &Shared, repo_path: &str, op: F) -> Result<MutationResponse>
where
    F: FnOnce(&GitRepo) -> Result<bool>,
{
    let (mut response, tree) = mutation_parts(repo_path, op)?;
    response.document = shared.store_tree(&response.status.repo_path, tree);
    Ok(response)
}

/// Run a mutation for `repo_path`, reporting Git failures as message effects.
///
/// A failed Git command still produces a successful response with the current
/// status; only internal faults become JSON-RPC errors.
pub fn mutation<F>(repo_path: &str, op: F) -> Result<MutationResponse>
where
    F: FnOnce(&GitRepo) -> Result<bool>,
{
    mutation_parts(repo_path, op).map(|(response, _tree)| response)
}

/// The mutation response plus the tree it was rendered from.
fn mutation_parts<F>(repo_path: &str, op: F) -> Result<(MutationResponse, Tree)>
where
    F: FnOnce(&GitRepo) -> Result<bool>,
{
    let repo = match GitRepo::open(repo_path) {
        Ok(repo) => repo,
        Err(err) => {
            let effect = Effect::from_error(&err).ok_or(err)?;
            let status = git::Status::empty(repo_path);
            let tree = status_view::build(&status);
            let document = Document::from(&tree);
            return Ok((
                MutationResponse {
                    status,
                    effects: vec![effect],
                    document,
                },
                tree,
            ));
        }
    };

    let effects = errors::effects_for_result(op(&repo))?;
    let status = match repo.status() {
        Ok(status) => status,
        Err(err) => match Effect::from_error(&err) {
            Some(effect) => {
                let mut effects = effects;
                effects.push(effect);
                let status = git::Status::empty(repo_path);
                let tree = status_view::build(&status);
                let document = Document::from(&tree);
                return Ok((
                    MutationResponse {
                        status,
                        effects,
                        document,
                    },
                    tree,
                ));
            }
            None => return Err(err),
        },
    };
    let tree = status_view::build(&status);
    let document = Document::from(&tree);
    Ok((
        MutationResponse {
            status,
            effects,
            document,
        },
        tree,
    ))
}

fn capabilities() -> Result<Value> {
    serde_json::to_value(&ServerCapabilities {
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: semantic_tokens::legend(),
                full: Some(SemanticTokensFullOptions::Bool(true)),
                range: None,
                work_done_progress_options: Default::default(),
            },
        )),
        execute_command_provider: Some(ExecuteCommandOptions {
            commands: vec!["magit-lsp.executeVCSCommand".to_string()],
            ..Default::default()
        }),
        experimental: Some(serde_json::json!({
            "magit-lsp": true,
            "transientMenuProvider": true
        })),
        ..Default::default()
    })
    .map_err(|e| Error::new(e.to_string()))
}

fn is_request_method(method: &str) -> bool {
    matches!(
        method,
        "initialize"
            | "shutdown"
            | "textDocument/foldingRange"
            | "textDocument/semanticTokens/full"
            | "workspace/executeCommand"
            | StatusRequest::METHOD
            | DiffRequest::METHOD
            | StageHunkRequest::METHOD
            | UnstageHunkRequest::METHOD
            | StageLinesRequest::METHOD
            | UnstageLinesRequest::METHOD
            | StageFileRequest::METHOD
            | UnstageFileRequest::METHOD
            | StageAllRequest::METHOD
            | StageUntrackedRequest::METHOD
            | UnstageAllRequest::METHOD
            | TargetsRequest::METHOD
            | TransientMenuRequest::METHOD
            | ViewOpenRequest::METHOD
            | ViewCloseRequest::METHOD
            | ViewInvokeRequest::METHOD
            | ViewResyncRequest::METHOD
    )
}

fn mutation_repo_path(shared: &Shared, method: &str, params: &Value) -> Option<String> {
    if method == ViewInvokeRequest::METHOD {
        // Only stage/unstage touch the index; refresh stays a read.
        let command = params.get("command").and_then(Value::as_str)?;
        if !matches!(command, "status.stage" | "status.unstage") {
            return None;
        }
        let view_id = params.get("viewId").and_then(Value::as_str)?;
        return shared.repo_for_view(view_id);
    }

    let is_mutation = matches!(
        method,
        StageHunkRequest::METHOD
            | UnstageHunkRequest::METHOD
            | StageLinesRequest::METHOD
            | UnstageLinesRequest::METHOD
            | StageFileRequest::METHOD
            | UnstageFileRequest::METHOD
            | StageAllRequest::METHOD
            | StageUntrackedRequest::METHOD
            | UnstageAllRequest::METHOD
    );
    if !is_mutation {
        return None;
    }
    params
        .get("repo_path")
        .and_then(Value::as_str)
        .map(str::to_string)
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

fn stub_transient_menu() -> TransientMenuResponse {
    TransientMenuResponse {
        title: "Commit".to_string(),
        flags: vec![TransientFlag {
            flag: "--amend".to_string(),
            description: "Amend the last commit".to_string(),
            active: false,
        }],
        actions: vec![TransientAction {
            key: "c".to_string(),
            command: "commit".to_string(),
            description: "Create commit".to_string(),
        }],
    }
}

fn from_params<T: serde::de::DeserializeOwned>(params: Value) -> Result<T> {
    serde_json::from_value(params).map_err(|e| Error::new(e.to_string()))
}
