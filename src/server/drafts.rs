use super::*;

/// Draft barriers on the receive-order lane; JSON-RPC ids correlate results.
#[cfg(unix)]
pub(super) fn draft_request(shared: &Shared, request: &Request) -> Result<Value> {
    let response = match request.method.as_str() {
        "workspace/executeCommand" => {
            let params: lsp_types::ExecuteCommandParams = from_params(request.params.clone())?;
            if params.command != "magit.command" || params.arguments.len() != 1 {
                return Err(Error::new(
                    "Expected magit.command with one semantic command",
                ));
            }
            if params.arguments[0]["command"] == "common.cancel" {
                let command: crate::protocol::EditAbortParams =
                    from_params(params.arguments[0].clone())?;
                if command.command != "common.cancel"
                    || editor::registry().run_for_uri(&shared.draft_owner, &command.uri)?
                        != command.run_id
                {
                    return Err(Error::new("Commit draft abort identity is stale or foreign"));
                }
                let documents = shared.documents.lock().unwrap_or_else(|p| p.into_inner());
                if !documents
                    .get(&command.uri)
                    .is_some_and(|doc| !doc.repair && doc.version == command.version)
                {
                    return Err(Error::new("Wait for the exact synchronized document version"));
                }
                drop(documents);
                if !editor::registry().resolve(&command.run_id, editor::Reply::Abort) {
                    return Err(Error::new("Commit draft is no longer pending"));
                }
                return serde_json::to_value(crate::protocol::EditAbortResult::Aborted {
                    run_id: command.run_id,
                    uri: command.uri,
                    version: command.version,
                })
                .map_err(|error| Error::new(error.to_string()));
            }
            let command: EditConfirmParams = from_params(params.arguments[0].clone())?;
            if command.command != "common.confirm" {
                return Err(Error::new("Only common.confirm is supported here"));
            }
            if editor::registry()
                .run_for_uri(&shared.draft_owner, &command.uri)
                .ok()
                .as_deref()
                != Some(command.run_id.as_str())
            {
                return Err(Error::new("Commit draft is no longer pending"));
            }
            let p = DraftFinishParams {
                run_id: command.run_id,
                uri: command.uri,
                version: command.version,
                reason: match command.source {
                    EditConfirmSource::Synchronized => DraftFinishReason::Confirm,
                    EditConfirmSource::SavedFile => DraftFinishReason::NormalClose,
                },
            };
            if p.reason == DraftFinishReason::Confirm {
                let documents = shared.documents.lock().unwrap_or_else(|p| p.into_inner());
                if !documents
                    .get(&p.uri)
                    .is_some_and(|doc| !doc.repair && doc.version == p.version)
                {
                    return Err(Error::new(
                        "Wait for the exact synchronized document version; full-text repair may be required",
                    ));
                }
            }
            if let Some(warning) = editor::registry().finish(&shared.draft_owner, &p)? {
                let _ = shared
                    .sender
                    .send(custom_ui(&shared.client).effects(ViewEffectsParams {
                        effects: vec![Effect::message(MessageLevel::Warn, warning)],
                    }));
            }
            EditConfirmation::Confirmed {
                run_id: p.run_id,
                uri: p.uri,
                version: p.version,
            }
        }
        _ => return Err(Error::new("Unknown commit draft request")),
    };
    serde_json::to_value(response).map_err(|error| Error::new(error.to_string()))
}

/// Whether a synchronized document belongs to this Plain connection: a pending
/// draft run or a server-owned presentation document. Rich clients keep their
/// own documents, so only Plain applies this ownership check.
#[cfg(unix)]
fn draft_document_is_owned(shared: &Shared, uri: &str) -> bool {
    if editor::registry()
        .run_for_uri(&shared.draft_owner, uri)
        .is_ok()
    {
        return true;
    }
    let plain = matches!(
        shared
            .client
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .presentation
            .as_ref(),
        Some(Presentation::Plain(_))
    );
    !plain || is_plain_provider_uri(shared, uri) || is_workspace_commit_message(shared, uri)
}

/// Whether the URI addresses a workspace repository's commit message file, which
/// a Plain client may open before a draft run claims it.
#[cfg(unix)]
fn is_workspace_commit_message(shared: &Shared, uri: &str) -> bool {
    let Some(path) = file_uri_path(uri) else {
        return false;
    };
    if !path.file_name().is_some_and(|name| name == "COMMIT_EDITMSG") {
        return false;
    }
    let Some(dir) = path.parent() else {
        return false;
    };
    let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    shared
        .client
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .workspace_repositories
        .iter()
        .any(|repository| repository.git_dir == dir)
}

/// Apply synchronized text and forward accepted snapshots to any pending draft.
#[cfg(unix)]
pub(super) fn draft_notification(shared: &Shared, notification: &Notification) -> Result<()> {
    use lsp_types::{
        DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    };
    if shared.closing.load(Ordering::SeqCst) {
        return Err(Error::new("Server is shutting down"));
    }
    match notification.method.as_str() {
        "textDocument/didOpen" => {
            let p: DidOpenTextDocumentParams = from_params(notification.params.clone())?;
            let doc = p.text_document;
            let uri = doc.uri.as_str();
            if !draft_document_is_owned(shared, uri) {
                return Err(Error::new(
                    "Commit draft URI is not pending on this connection",
                ));
            }
            if let Ok(run) = editor::registry().run_for_uri(&shared.draft_owner, uri) {
                editor::registry().apply(&shared.draft_owner, &run, uri, |draft, context| {
                    draft.did_open(context, doc.version, doc.text.as_bytes())
                })?;
            }
            shared
                .documents
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .open(uri, doc.text, doc.version);
            Ok(())
        }
        "textDocument/didChange" => {
            let p: DidChangeTextDocumentParams = from_params(notification.params.clone())?;
            let uri = p.text_document.uri.as_str();
            if !draft_document_is_owned(shared, uri) {
                return Err(Error::new(
                    "Commit draft change is detached, stale or duplicated",
                ));
            }
            let encoding = shared
                .client
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .encoding;
            let changes = p
                .content_changes
                .iter()
                .map(|change| StoredChange {
                    start: change.range.map(|r| (r.start.line, r.start.character)),
                    end: change.range.map(|r| (r.end.line, r.end.character)),
                    text: change.text.clone(),
                })
                .collect::<Vec<_>>();
            let mut docs = shared.documents.lock().unwrap_or_else(|p| p.into_inner());
            let Some(mut candidate) = docs.get(uri).cloned() else {
                return Err(Error::new("Document is not open"));
            };
            if candidate.change(p.text_document.version, &changes, encoding).is_err() {
                let stale_full_text =
                    p.content_changes.len() == 1 && p.content_changes[0].range.is_none();
                drop(docs);
                if stale_full_text {
                    return Err(Error::new(
                        "Commit draft change is detached, stale or duplicated",
                    ));
                }
                return Err(Error::new(
                    "Invalid document change or stale or duplicated version; document requires repair",
                ));
            }
            let text = candidate.text.clone();
            if let Ok(run) = editor::registry().run_for_uri(&shared.draft_owner, uri) {
                editor::registry().apply(&shared.draft_owner, &run, uri, |draft, context| {
                    if draft.opened() {
                        draft.did_change(context, p.text_document.version, text.as_bytes())
                    } else {
                        // A preopened buffer has no second didOpen. Attach only
                        // after its synchronized echo matches the new template;
                        // an applyEdit success alone never proves delivery.
                        draft.did_open(context, p.text_document.version, text.as_bytes())
                    }
                })?;
            }
            docs.replace(uri, candidate).map_err(|_| Error::new("Document is not open"))?;
            Ok(())
        }
        "textDocument/didClose" => {
            let p: DidCloseTextDocumentParams = from_params(notification.params.clone())?;
            let uri = p.text_document.uri.as_str();
            let owned = draft_document_is_owned(shared, uri);
            shared
                .documents
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .close(uri);
            if editor::registry()
                .run_for_uri(&shared.draft_owner, uri)
                .is_err()
            {
                return if owned {
                    // A close after an abort just detaches the document store.
                    Ok(())
                } else {
                    Err(Error::new(
                        "Commit draft URI is not pending on this connection",
                    ))
                };
            }
            let run = editor::registry().run_for_uri(&shared.draft_owner, uri)?;
            let close_confirms = shared
                .client
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .close_confirms;
            if close_confirms {
                let result = editor::registry().finish(
                    &shared.draft_owner,
                    &DraftFinishParams {
                        run_id: run.clone(),
                        uri: uri.to_string(),
                        version: 0,
                        reason: DraftFinishReason::NormalClose,
                    },
                );
                if result.is_err() {
                    editor::registry().apply(
                        &shared.draft_owner,
                        &run,
                        uri,
                        |draft, context| draft.did_close(context),
                    )?;
                }
                result?;
                Ok(())
            } else {
                editor::registry().apply(&shared.draft_owner, &run, uri, |draft, context| {
                    draft.did_close(context)
                })
            }
        }
        "textDocument/didSave" => {
            let p: lsp_types::DidSaveTextDocumentParams = from_params(notification.params.clone())?;
            if let Some(text) = p.text {
                shared
                    .documents
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .save_text(p.text_document.uri.as_str(), &text)
                    .map_err(|_| Error::new("Save text does not match an open document; synchronize changes before saving"))?;
            }
            Ok(())
        }
        _ => Err(Error::new("Unknown commit draft notification")),
    }
}

/// Reject managed drafts on platforms without the validated helper boundary.
#[cfg(not(unix))]
pub(super) fn draft_request(_shared: &Shared, _request: &Request) -> Result<Value> {
    Err(Error::new("managed commit drafts require Unix"))
}

/// Reject managed draft sync on platforms without Unix support.
#[cfg(not(unix))]
pub(super) fn draft_notification(_shared: &Shared, _notification: &Notification) -> Result<()> {
    Err(Error::new("managed commit drafts require Unix"))
}
