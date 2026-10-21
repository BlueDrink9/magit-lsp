use crate::document;
use crate::errors;
use crate::git::{self, GitRepo};
use crate::protocol::*;
use crate::transport;
use crate::{Error, Result};
use lsp_server::{ErrorCode, Message, Notification, Request, Response};
use lsp_types::request::Request as LspRequest;
use lsp_types::{
    ExecuteCommandOptions, FoldingRangeProviderCapability, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncWrite, BufReader};
use tokio::sync::{mpsc, Mutex as AsyncMutex};
use tokio::task::JoinSet;

type Documents = HashMap<String, String>;
type RepoLocks = Arc<AsyncMutex<HashMap<String, Arc<AsyncMutex<()>>>>>;

/// Server state shared by concurrent request handlers.
#[derive(Clone)]
struct Shared {
    documents: Arc<Mutex<Documents>>,
    repo_locks: RepoLocks,
}

impl Shared {
    fn new() -> Self {
        Self {
            documents: Arc::new(Mutex::new(HashMap::new())),
            repo_locks: Arc::new(AsyncMutex::new(HashMap::new())),
        }
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
                "initialized" | "$/cancelRequest" => {}
                _ => handle_notification(&notification, &shared.documents),
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

    let response = if !is_request_method(&method) {
        Response::new_err(
            id,
            ErrorCode::MethodNotFound as i32,
            format!("unknown method: {method}"),
        )
    } else if let Some(repo_path) = mutation_repo_path(&method, &request.params) {
        let lock = repository_lock(&shared.repo_locks, &repo_path).await;
        let _guard = lock.lock().await;
        maybe_test_delay().await;
        dispatch_response(request, &shared).await
    } else {
        dispatch_response(request, &shared).await
    };

    let _ = sender.send(Message::Response(response));
}

/// Run the synchronous Git work off the runtime, keeping the reader free.
async fn dispatch_response(request: Request, shared: &Shared) -> Response {
    let id = request.id.clone();
    let method = request.method.clone();
    let documents = shared.documents.clone();

    match tokio::task::spawn_blocking(move || dispatch(&request, &documents)).await {
        Ok(Ok(value)) => Response::new_ok(id, value),
        Ok(Err(error)) => {
            tracing::warn!(method = %method, error = %error, "request failed");
            error_response(id, error)
        }
        Err(error) => Response::new_err(
            id,
            ErrorCode::InternalError as i32,
            format!("request task failed: {error}"),
        ),
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

fn handle_notification(notification: &Notification, documents: &Mutex<Documents>) {
    let mut documents = documents
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match notification.method.as_str() {
        // Client pushes the rendered status text so the server can compute
        // folding ranges and navigation targets from the real document.
        "vcs/didRender" => {
            if let (Some(uri), Some(text)) = (
                notification.params.get("uri").and_then(|u| u.as_str()),
                notification.params.get("text").and_then(|t| t.as_str()),
            ) {
                documents.insert(uri.to_string(), text.to_string());
            }
        }
        "textDocument/didOpen" => {
            let uri = notification
                .params
                .get("textDocument")
                .and_then(|d| d.get("uri"))
                .and_then(|u| u.as_str());
            let text = notification
                .params
                .get("textDocument")
                .and_then(|d| d.get("text"))
                .and_then(|t| t.as_str());
            if let (Some(uri), Some(text)) = (uri, text) {
                documents.insert(uri.to_string(), text.to_string());
            }
        }
        "textDocument/didChange" => {
            let uri = notification
                .params
                .get("textDocument")
                .and_then(|d| d.get("uri"))
                .and_then(|u| u.as_str());
            let text = notification
                .params
                .get("contentChanges")
                .and_then(|c| c.as_array())
                .and_then(|a| a.last())
                .and_then(|c| c.get("text"))
                .and_then(|t| t.as_str());
            if let (Some(uri), Some(text)) = (uri, text) {
                documents.insert(uri.to_string(), text.to_string());
            }
        }
        "textDocument/didClose" => {
            if let Some(uri) = notification
                .params
                .get("textDocument")
                .and_then(|d| d.get("uri"))
                .and_then(|u| u.as_str())
            {
                documents.remove(uri);
            }
        }
        _ => {}
    }
}

fn dispatch(request: &Request, documents: &Mutex<Documents>) -> Result<Value> {
    let params = request.params.clone();
    match request.method.as_str() {
        "initialize" => Ok(serde_json::json!({ "capabilities": capabilities()? })),
        "shutdown" => Ok(Value::Null),
        StatusRequest::METHOD => {
            let p: StatusParams = from_params(params)?;
            Ok(serde_json::to_value(
                GitRepo::open(&p.repo_path)?.status()?,
            )?)
        }
        DiffRequest::METHOD => {
            let p: DiffParams = from_params(params)?;
            let diff = if p.staged {
                GitRepo::open(&p.repo_path)?.diff_staged(&p.path)?
            } else {
                GitRepo::open(&p.repo_path)?.diff_unstaged(&p.path)?
            };
            Ok(serde_json::to_value(DiffResponse {
                path: p.path,
                staged: p.staged,
                diff,
            })?)
        }
        StageHunkRequest::METHOD => {
            let p: HunkMutationParams = from_params(params)?;
            Ok(serde_json::to_value(mutation(&p.repo_path, |repo| {
                repo.stage_hunk(&p.path, &p.hunk_id).map(|_| true)
            })?)?)
        }
        UnstageHunkRequest::METHOD => {
            let p: HunkMutationParams = from_params(params)?;
            Ok(serde_json::to_value(mutation(&p.repo_path, |repo| {
                repo.unstage_hunk(&p.path, &p.hunk_id).map(|_| true)
            })?)?)
        }
        StageLinesRequest::METHOD => {
            let p: LinesMutationParams = from_params(params)?;
            Ok(serde_json::to_value(mutation(&p.repo_path, |repo| {
                repo.stage_lines(&p.path, &p.hunk_id, p.start, p.end)
            })?)?)
        }
        UnstageLinesRequest::METHOD => {
            let p: LinesMutationParams = from_params(params)?;
            Ok(serde_json::to_value(mutation(&p.repo_path, |repo| {
                repo.unstage_lines(&p.path, &p.hunk_id, p.start, p.end)
            })?)?)
        }
        StageFileRequest::METHOD => {
            let p: FileMutationParams = from_params(params)?;
            Ok(serde_json::to_value(mutation(&p.repo_path, |repo| {
                repo.stage_file(&p.path).map(|_| true)
            })?)?)
        }
        UnstageFileRequest::METHOD => {
            let p: FileMutationParams = from_params(params)?;
            Ok(serde_json::to_value(mutation(&p.repo_path, |repo| {
                repo.unstage_file(&p.path).map(|_| true)
            })?)?)
        }
        StageAllRequest::METHOD => {
            let p: RepoMutationParams = from_params(params)?;
            Ok(serde_json::to_value(mutation(&p.repo_path, |repo| {
                repo.stage_all().map(|_| true)
            })?)?)
        }
        StageUntrackedRequest::METHOD => {
            let p: RepoMutationParams = from_params(params)?;
            Ok(serde_json::to_value(mutation(&p.repo_path, |repo| {
                repo.stage_untracked().map(|_| true)
            })?)?)
        }
        UnstageAllRequest::METHOD => {
            let p: RepoMutationParams = from_params(params)?;
            Ok(serde_json::to_value(mutation(&p.repo_path, |repo| {
                repo.unstage_all().map(|_| true)
            })?)?)
        }
        TargetsRequest::METHOD => {
            let p: TargetsParams = from_params(params)?;
            let text = document_text(documents, &p.uri);
            Ok(serde_json::to_value(TargetsResponse {
                targets: document::targets(&text),
            })?)
        }
        "textDocument/foldingRange" => {
            let p: lsp_types::FoldingRangeParams = from_params(params)?;
            let text = document_text(documents, p.text_document.uri.as_str());
            Ok(serde_json::to_value(document::folding_ranges(&text))?)
        }
        TransientMenuRequest::METHOD => Ok(serde_json::to_value(stub_transient_menu())?),
        "workspace/executeCommand" => Ok(Value::Null),
        other => Err(Error::new(format!("unknown method: {other}"))),
    }
}

fn document_text(documents: &Mutex<Documents>, uri: &str) -> String {
    documents
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(uri)
        .cloned()
        .unwrap_or_default()
}

/// Run a mutation for `repo_path`, reporting Git failures as message effects.
///
/// A failed Git command still produces a successful response with the current
/// status; only internal faults become JSON-RPC errors.
pub fn mutation<F>(repo_path: &str, op: F) -> Result<MutationResponse>
where
    F: FnOnce(&GitRepo) -> Result<bool>,
{
    let repo = match GitRepo::open(repo_path) {
        Ok(repo) => repo,
        Err(err) => {
            let effect = Effect::from_error(&err).ok_or(err)?;
            return Ok(MutationResponse {
                status: git::Status::empty(repo_path),
                effects: vec![effect],
            });
        }
    };

    let effects = errors::effects_for_result(op(&repo))?;
    let status = match repo.status() {
        Ok(status) => status,
        Err(err) => match Effect::from_error(&err) {
            Some(effect) => {
                let mut effects = effects;
                effects.push(effect);
                return Ok(MutationResponse {
                    status: git::Status::empty(repo_path),
                    effects,
                });
            }
            None => return Err(err),
        },
    };
    Ok(MutationResponse { status, effects })
}

fn capabilities() -> Result<Value> {
    serde_json::to_value(&ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::FULL),
                ..Default::default()
            },
        )),
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
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
    )
}

fn mutation_repo_path(method: &str, params: &Value) -> Option<String> {
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
