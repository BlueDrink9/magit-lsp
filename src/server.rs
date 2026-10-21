use crate::document;
use crate::errors;
use crate::git::{self, GitRepo};
use crate::protocol::*;
use crate::{Error, Result};
use lsp_server::{Connection, Message, Notification, Response};
use lsp_types::request::Request;
use lsp_types::{
    ExecuteCommandOptions, FoldingRangeProviderCapability, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
};
use std::collections::HashMap;

type Documents = HashMap<String, String>;

/// Run the LSP server on stdio until the client requests shutdown.
pub fn run() -> Result<()> {
    let (connection, io_threads) = Connection::stdio();

    let capabilities = serde_json::to_value(&ServerCapabilities {
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
    .map_err(|e| Error::new(e.to_string()))?;

    connection
        .initialize(capabilities)
        .map_err(|e| Error::new(e.to_string()))?;

    let mut documents: Documents = HashMap::new();

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection
                    .handle_shutdown(&req)
                    .map_err(|e| Error::new(e.to_string()))?
                {
                    break;
                }
                let response = match dispatch(&req, &documents) {
                    Ok(value) => Message::Response(Response::new_ok(req.id, value)),
                    Err(err) => Message::Response(error_response(req.id, err)),
                };
                connection
                    .sender
                    .send(response)
                    .map_err(|e| Error::new(e.to_string()))?;
            }
            Message::Notification(not) => handle_notification(&not, &mut documents),
            Message::Response(_) => {}
        }
    }

    match io_threads.join() {
        Ok(_) => Ok(()),
        Err(e) => Err(Error::new(e.to_string())),
    }
}

fn handle_notification(not: &Notification, documents: &mut Documents) {
    match not.method.as_str() {
        // Client pushes the rendered status text so the server can compute
        // folding ranges and navigation targets from the real document.
        "vcs/didRender" => {
            if let (Some(uri), Some(text)) = (
                not.params.get("uri").and_then(|u| u.as_str()),
                not.params.get("text").and_then(|t| t.as_str()),
            ) {
                documents.insert(uri.to_string(), text.to_string());
            }
        }
        "textDocument/didOpen" => {
            let uri = not
                .params
                .get("textDocument")
                .and_then(|d| d.get("uri"))
                .and_then(|u| u.as_str());
            let text = not
                .params
                .get("textDocument")
                .and_then(|d| d.get("text"))
                .and_then(|t| t.as_str());
            if let (Some(uri), Some(text)) = (uri, text) {
                documents.insert(uri.to_string(), text.to_string());
            }
        }
        "textDocument/didChange" => {
            let uri = not
                .params
                .get("textDocument")
                .and_then(|d| d.get("uri"))
                .and_then(|u| u.as_str());
            let text = not
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
            if let Some(uri) = not
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

fn dispatch(req: &lsp_server::Request, documents: &Documents) -> Result<serde_json::Value> {
    let params = req.params.clone();
    match req.method.as_str() {
        StatusRequest::METHOD => {
            let p: StatusParams = from_params(params)?;
            Ok(serde_json::to_value(GitRepo::open(&p.repo_path)?.status()?)?)
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
                repo.stage_hunk(&p.path, p.hunk_index).map(|_| true)
            })?)?)
        }
        UnstageHunkRequest::METHOD => {
            let p: HunkMutationParams = from_params(params)?;
            Ok(serde_json::to_value(mutation(&p.repo_path, |repo| {
                repo.unstage_hunk(&p.path, p.hunk_index).map(|_| true)
            })?)?)
        }
        StageLinesRequest::METHOD => {
            let p: LinesMutationParams = from_params(params)?;
            Ok(serde_json::to_value(mutation(&p.repo_path, |repo| {
                repo.stage_lines(&p.path, p.hunk_index, p.start, p.end)
            })?)?)
        }
        UnstageLinesRequest::METHOD => {
            let p: LinesMutationParams = from_params(params)?;
            Ok(serde_json::to_value(mutation(&p.repo_path, |repo| {
                repo.unstage_lines(&p.path, p.hunk_index, p.start, p.end)
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
        UnstageAllRequest::METHOD => {
            let p: RepoMutationParams = from_params(params)?;
            Ok(serde_json::to_value(mutation(&p.repo_path, |repo| {
                repo.unstage_all().map(|_| true)
            })?)?)
        }
        TargetsRequest::METHOD => {
            let p: TargetsParams = from_params(params)?;
            let text = documents.get(&p.uri).cloned().unwrap_or_default();
            Ok(serde_json::to_value(TargetsResponse {
                targets: document::targets(&text),
            })?)
        }
        "textDocument/foldingRange" => {
            let p: lsp_types::FoldingRangeParams = from_params(params)?;
            let text = documents
                .get(p.text_document.uri.as_str())
                .cloned()
                .unwrap_or_default();
            Ok(serde_json::to_value(document::folding_ranges(&text))?)
        }
        TransientMenuRequest::METHOD => Ok(serde_json::to_value(stub_transient_menu())?),
        "workspace/executeCommand" => Ok(serde_json::Value::Null),
        other => Err(Error::new(format!("unknown method: {other}"))),
    }
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

fn from_params<T: serde::de::DeserializeOwned>(params: serde_json::Value) -> Result<T> {
    serde_json::from_value(params).map_err(|e| Error::new(e.to_string()))
}