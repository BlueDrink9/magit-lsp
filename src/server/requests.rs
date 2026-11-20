use super::*;

pub(super) async fn handle_request(
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
        let (token, cancel) =
            shared.begin_progress(&sender, &method, None, Some(&active.repo_path));
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
pub(super) struct Dispatched {
    pub(super) result: Value,
    pub(super) notifications: Vec<Message>,
}

impl Dispatched {
    pub(super) fn value(result: Value) -> Self {
        Self {
            result,
            notifications: Vec::new(),
        }
    }

    pub(super) fn with(result: Value, notifications: Vec<Message>) -> Self {
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

fn dispatch(request: &Request, shared: &Shared) -> Result<Dispatched> {
    let params = request.params.clone();
    match request.method.as_str() {
        "initialize" => {
            let mut notifications = Vec::new();
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
            let config = shared.config.config();
            let caps = params.get("capabilities").unwrap_or(&Value::Null);
            let identity = params.get("clientInfo").unwrap_or(&Value::Null);
            let resolution = crate::quirks::resolve_validated(
                crate::quirks::Client {
                    name: identity.get("name").and_then(Value::as_str),
                    version: identity.get("version").and_then(Value::as_str),
                    capabilities: caps,
                },
                config
                    .quirks
                    .iter()
                    .map(|(id, enabled)| (id.as_str(), *enabled)),
            );
            let mut presentation =
                negotiate(&params).map_err(|error| Error::new(error.to_string()))?;
            if let Presentation::Plain(plain) = &mut presentation {
                for id in &resolution.enabled {
                    if let Some(quirk) = crate::quirks::REGISTRY.iter().find(|q| q.id == *id) {
                        if let Some(override_) = quirk.capability {
                            match override_.capability {
                                crate::quirks::Capability::DocumentChanges => {
                                    plain.document_changes = override_.enabled
                                }
                                crate::quirks::Capability::ShowDocument => {
                                    plain.show_document = override_.enabled
                                }
                            }
                        }
                    }
                }
            }
            let key_modifiers = match &presentation {
                Presentation::Rich(rich) => rich.modifiers,
                Presentation::Plain(_) => KeyModifiers::Full,
            };
            {
                let mut client = shared
                    .client
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                client.key_modifiers = key_modifiers;
                client.presentation = Some(presentation.clone());
                client.plain = match client.presentation.as_ref() {
                    Some(Presentation::Plain(caps)) => Some(caps.clone()),
                    _ => None,
                };
                client.workspace_repositories = initialize_workspace_roots(&params);
                client.client_name = params
                    .pointer("/clientInfo/name")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                client.plain_keys_allowed = resolution.enabled.iter().all(|id| {
                    crate::quirks::REGISTRY
                        .iter()
                        .find(|quirk| quirk.id == *id)
                        .and_then(|quirk| quirk.plain_keys_allowed)
                        != Some(false)
                });
                client.encoding = params
                    .pointer("/capabilities/general/positionEncodings")
                    .and_then(Value::as_array)
                    .and_then(|encodings| {
                        encodings
                            .iter()
                            .filter_map(Value::as_str)
                            .find_map(|value| match value {
                                "utf-8" => Some(Encoding::Utf8),
                                "utf-16" => Some(Encoding::Utf16),
                                "utf-32" => Some(Encoding::Utf32),
                                _ => None,
                            })
                    })
                    .unwrap_or(Encoding::Utf16);
                client.work_done_progress = params
                    .pointer("/capabilities/window/workDoneProgress")
                    .and_then(Value::as_bool)
                    == Some(true);
                client.close_confirms = !params
                    .pointer("/clientInfo/name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| {
                        name.to_ascii_lowercase().contains("visual studio code")
                            || name.to_ascii_lowercase().contains("codium")
                    });
            }
            tracing::debug!(?key_modifiers, "client key capabilities");
            if let Some(error) = &shared.startup_error {
                notifications.push(show_message(1, format!("magit-lsp config: {error}")));
            }
            tracing::debug!(files = ?shared.config.files(), "configuration ready");
            let mut server_caps = capabilities(
                shared
                    .client
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .encoding,
            )?;
            let effective_caps = match &presentation {
                Presentation::Plain(plain) => serde_json::json!({
                    "applyEdit": plain.apply_edit,
                    "documentChanges": plain.document_changes,
                    "codeActions": plain.code_actions,
                    "disabledActions": plain.disabled_actions,
                    "showDocument": plain.show_document,
                    "workDoneProgress": plain.work_done_progress,
                    "completion": plain.completion,
                    "foldingRange": plain.folding_range,
                    "positionEncodings": plain.position_encodings,
                }),
                Presentation::Rich(rich) => serde_json::json!({
                    "keys": rich.keys,
                    "modifiers": rich.modifiers,
                    "layout": rich.layout,
                    "effects": rich.effects,
                    "prompts": rich.prompts,
                }),
            };
            let enabled: Vec<_> = resolution
                .enabled
                .iter()
                .filter_map(|id| crate::quirks::REGISTRY.iter().find(|q| q.id == *id))
                .map(|q| serde_json::json!({"id":q.id,"model":q.model,"workaround":q.workaround}))
                .collect();
            server_caps["experimental"]["magit"]["protocolVersion"] = serde_json::json!(1);
            server_caps["experimental"]["magit"]["effective"] =
                serde_json::json!({"capabilities":effective_caps,"quirks":enabled});
            Ok(Dispatched::with(
                serde_json::json!({
                    "capabilities": server_caps,
                    "serverInfo": {"name": "magit-lsp", "version": env!("CARGO_PKG_VERSION")}
                }),
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
        ViewPickRespondRequest::METHOD => {
            let p: ViewPickRespondParams = from_params(params)?;
            shared.respond_pick(p)?;
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
            let result = if is_plain_provider_uri(shared, uri) {
                let encoding = shared
                    .client
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .encoding;
                plain_provider_snapshot(shared, uri)
                    .map(|(tree, _, _)| tree.document_symbols_with_encoding(encoding))
            } else {
                tree_for_positional_request(shared, uri).map(|tree| tree.document_symbols())
            };
            let Some(result) = result else {
                return Ok(Dispatched::value(Value::Null));
            };
            let result = serde_json::to_value(result)?;
            Ok(Dispatched::value(result))
        }
        "textDocument/codeAction" => {
            let p: lsp_types::CodeActionParams = from_params(params)?;
            Ok(Dispatched::value(plain_actions::plain_code_actions(shared, &p)))
        }
        "textDocument/definition" => {
            let p: lsp_types::GotoDefinitionParams = from_params(params)?;
            let position = p.text_document_position_params;
            let uri = position.text_document.uri.as_str();
            if shared.is_text_uri(uri) {
                return Ok(Dispatched::value(Value::Null));
            }
            let location = if is_plain_provider_uri(shared, uri) {
                let encoding = shared
                    .client
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .encoding;
                plain_provider_snapshot(shared, uri).and_then(|(tree, repo, text)| {
                    if !plain_actions::plain_position_valid(&text, position.position, encoding) {
                        return None;
                    }
                    tree.definition_with_encoding(&repo, position.position, encoding)
                })
            } else {
                tree_for_positional_request(shared, uri).and_then(|tree| {
                    let repo = shared.repo_for_uri(uri)?;
                    tree.definition(&repo, position.position)
                })
            };
            Ok(Dispatched::value(serde_json::to_value(location)?))
        }
        "textDocument/documentLink" => {
            let p: lsp_types::DocumentLinkParams = from_params(params)?;
            let uri = p.text_document.uri.as_str();
            if shared.is_text_uri(uri) {
                return Ok(Dispatched::value(Value::Null));
            }
            let links = if is_plain_provider_uri(shared, uri) {
                let encoding = shared
                    .client
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .encoding;
                plain_provider_snapshot(shared, uri)
                    .map(|(tree, repo, _)| tree.document_links_with_encoding(&repo, encoding))
            } else {
                tree_for_positional_request(shared, uri)
                    .and_then(|tree| Some(tree.document_links(&shared.repo_for_uri(uri)?)))
            };
            let Some(links) = links else {
                return Ok(Dispatched::value(Value::Null));
            };
            Ok(Dispatched::value(serde_json::to_value(links)?))
        }
        "textDocument/foldingRange" => {
            let p: lsp_types::FoldingRangeParams = from_params(params)?;
            let uri = p.text_document.uri.as_str();
            if shared.is_text_uri(uri) {
                return Ok(Dispatched::value(Value::Null));
            }
            let ranges = if is_plain_provider_uri(shared, uri) {
                plain_provider_snapshot(shared, uri).map(|(tree, _, _)| tree.folding_ranges())
            } else {
                tree_for_positional_request(shared, uri).map(|tree| tree.folding_ranges())
            };
            Ok(Dispatched::value(serde_json::to_value(ranges)?))
        }
        "textDocument/semanticTokens/full" => {
            let p: lsp_types::SemanticTokensParams = from_params(params)?;
            let uri = p.text_document.uri.as_str();
            if shared.is_text_uri(uri) {
                return Ok(Dispatched::value(Value::Null));
            }
            let plain = is_plain_provider_uri(shared, uri);
            let tokens = if plain {
                let encoding = shared
                    .client
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .encoding;
                plain_provider_snapshot(shared, uri)
                    .map(|(tree, _, _)| semantic_tokens::encode_with_encoding(&tree, encoding))
            } else {
                Some(
                    tree_for_positional_request(shared, uri)
                        .map(|tree| semantic_tokens::encode(&tree))
                        .unwrap_or_default(),
                )
            };
            let Some(tokens) = tokens else {
                return Ok(Dispatched::value(Value::Null));
            };
            let result = serde_json::to_value(SemanticTokensResult::Tokens(tokens))?;
            Ok(Dispatched::value(result))
        }
        other => Err(Error::new(format!("unknown method: {other}"))),
    }
}
