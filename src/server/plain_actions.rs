//! Plain action execution and offers.

use super::*;

/// Offer actions for the retained Plain document and selection.
pub(super) fn plain_code_actions(shared: &Shared, params: &lsp_types::CodeActionParams) -> Value {
    let uri = params.text_document.uri.as_str();
    #[cfg(unix)]
    if let Ok(run_id) = editor::registry().run_for_uri(&shared.draft_owner, uri) {
        let version = {
            let documents = shared.documents.lock().unwrap_or_else(|p| p.into_inner());
            documents
                .get(uri)
                .filter(|document| !document.repair)
                .map(|document| document.version)
        };
        if let Some(version) = version {
            let args = serde_json::json!({
                "command": "common.confirm",
                "runId": run_id,
                "uri": uri,
                "version": version,
                "source": "synchronized",
            });
            let abort = serde_json::json!({
                "command": "common.cancel",
                "runId": run_id,
                "uri": uri,
                "version": version,
            });
            return serde_json::json!([
                {"title":"Confirm commit","command":{"title":"Confirm commit","command":"magit.command","arguments":[args]}},
                {"title":"Abort commit","command":{"title":"Abort commit","command":"magit.command","arguments":[abort]}}
            ]);
        }
        return Value::Array(Vec::new());
    }
    let (text, tree, view_id, generation, scope, repo, menu) = {
        let _lifecycle = shared
            .plain_lifecycle
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let documents = shared.documents.lock().unwrap_or_else(|p| p.into_inner());
        let Some(document) = documents.get(uri).filter(|doc| !doc.repair) else {
            return Value::Array(Vec::new());
        };
        let text = document.text.clone();
        let documents = shared
            .plain_documents
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let Some(entry) = documents.get(uri) else {
            return Value::Array(Vec::new());
        };
        let Some((tree, view_id, generation, scope, menu)) =
            entry.controller.action_snapshot(&text)
        else {
            return Value::Array(Vec::new());
        };
        if view_id != entry.view_id {
            return Value::Array(Vec::new());
        }
        (
            text,
            tree.clone(),
            view_id.to_owned(),
            generation,
            scope.to_owned(),
            entry.repo.clone(),
            menu.cloned(),
        )
    };
    let config = shared.config.for_repository(std::path::Path::new(&repo));
    let capabilities = shared
        .client
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .plain
        .clone();
    let style = config.config().actions.title_style;
    let keymap = config.keymap(&scope);
    let start = params.range.start;
    let end = params.range.end;
    let encoding = shared
        .client
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .encoding;
    if !plain_position_valid(&text, start, encoding)
        || !plain_position_valid(&text, end, encoding)
        || (start.line, start.character) > (end.line, end.character)
    {
        return Value::Array(Vec::new());
    }
    let selection = if start != end {
        let end_line = if end.character == 0 && end.line > start.line {
            end.line - 1
        } else {
            end.line
        };
        if start.line > end_line {
            return Value::Array(Vec::new());
        }
        Some((start.line, end_line))
    } else {
        None
    };
    let Some(line_node) = tree
        .line_nodes
        .get(start.line as usize)
        .and_then(|line| tree.node(&line.node))
    else {
        return Value::Array(Vec::new());
    };
    if scope == "prompt"
        && line_node.kind == crate::tree::NodeKind::Line
        && prompt_row_id(&line_node.id)
    {
        let args = serde_json::json!({
            "command": "prompt.answer",
            "documentUri": uri,
            "target": { "kind": "node", "viewId": view_id, "nodeId": line_node.id },
            "viewGeneration": generation,
        });
        let title = line_node.label.clone();
        let action = serde_json::to_value(lsp_types::CodeAction {
            title: title.clone(),
            kind: None,
            diagnostics: None,
            is_preferred: None,
            disabled: None,
            edit: None,
            command: Some(lsp_types::Command {
                title,
                command: "magit.command".into(),
                arguments: Some(vec![args]),
            }),
            data: None,
        })
        .unwrap_or(Value::Null);
        return Value::Array(vec![action]);
    }
    let mut actions = Vec::new();
    if !matches!(scope.as_str(), "prompt" | "log.select" | "commit-message") {
        if let Some(policy) = tree.node("plain-keys-policy") {
            let enabled = policy.label.starts_with("Keys: on ");
            let title = if enabled { "Keys: off" } else { "Keys: on" };
            let args = serde_json::json!({
                "command": "plain.keys.toggle",
                "documentUri": uri,
                "target": { "kind": "view", "viewId": view_id },
                "viewGeneration": generation,
            });
            actions.push(serde_json::json!({
                "title": title,
                "command": {"title": title, "command": "magit.command", "arguments": [args]}
            }));
        }
    }
    if let Some(menu) = menu {
        for item in menu.groups.iter().flat_map(|group| &group.items) {
            let crate::menu::Item::Switch {
                key, label, status, ..
            } = item
            else {
                continue;
            };
            let implemented = matches!(status, crate::menu::MenuStatus::Implemented);
            if !implemented
                && !capabilities
                    .as_ref()
                    .is_some_and(|caps| caps.disabled_actions)
            {
                continue;
            }
            let args = serde_json::json!({
                "command": "menu.toggle-switch",
                "documentUri": uri,
                "target": { "kind": "menuItem", "viewId": view_id, "menuId": menu.id, "itemKey": key },
                "viewGeneration": generation,
            });
            let title = format!("Toggle {label}");
            actions.push(
                serde_json::to_value(lsp_types::CodeAction {
                    title: title.clone(),
                    kind: None,
                    diagnostics: None,
                    is_preferred: None,
                    disabled: (!implemented).then(|| lsp_types::CodeActionDisabled {
                        reason: "Unavailable: menu switch is not implemented".into(),
                    }),
                    edit: None,
                    command: implemented.then(|| lsp_types::Command {
                        title,
                        command: "magit.command".into(),
                        arguments: Some(vec![args.clone()]),
                    }),
                    data: (!implemented).then_some(args),
                })
                .unwrap_or(Value::Null),
            );
        }
        for item in menu.groups.iter().flat_map(|group| &group.items) {
            let crate::menu::Item::Option {
                key, label, status, ..
            } = item
            else {
                continue;
            };
            let implemented = matches!(status, crate::menu::MenuStatus::Implemented);
            if !implemented
                && !capabilities
                    .as_ref()
                    .is_some_and(|caps| caps.disabled_actions)
            {
                continue;
            }
            let args = serde_json::json!({
                "command": "menu.run-option",
                "documentUri": uri,
                "target": { "kind": "menuItem", "viewId": view_id, "menuId": menu.id, "itemKey": key },
                "viewGeneration": generation,
            });
            let title = format!("Set {label}");
            actions.push(
                serde_json::to_value(lsp_types::CodeAction {
                    title: title.clone(),
                    kind: None,
                    diagnostics: None,
                    is_preferred: None,
                    disabled: (!implemented).then(|| lsp_types::CodeActionDisabled {
                        reason: "Unavailable: menu option is not implemented".into(),
                    }),
                    edit: None,
                    command: implemented.then(|| lsp_types::Command {
                        title,
                        command: "magit.command".into(),
                        arguments: Some(vec![args.clone()]),
                    }),
                    data: (!implemented).then_some(args),
                })
                .unwrap_or(Value::Null),
            );
        }
        for item in menu.groups.iter().flat_map(|group| &group.items) {
            let crate::menu::Item::Action {
                key,
                command,
                label,
                status: crate::menu::MenuStatus::Implemented,
                ..
            } = item
            else {
                continue;
            };
            if !crate::dispatch::plain_menu_action(command) {
                continue;
            }
            let unavailable = crate::commands::registry()
                .get(command)
                .and_then(|registered| {
                    match capabilities
                        .as_ref()
                        .map(|caps| caps.plain_availability(registered))
                    {
                        Some(crate::presentation::PlainAvailability::Unavailable { reason }) => {
                            Some(reason)
                        }
                        _ => None,
                    }
                });
            if unavailable.is_some()
                && !capabilities
                    .as_ref()
                    .is_some_and(|caps| caps.disabled_actions)
            {
                continue;
            }
            let args = serde_json::json!({"command":"menu.run-action","documentUri":uri,"target":{"kind":"menuItem","viewId":view_id,"menuId":menu.id,"itemKey":key},"viewGeneration":generation});
            let title = if command == "log.current" {
                format!("Open {label}")
            } else {
                label.clone()
            };
            // A disabled row keeps the invocation arguments in `data` so the
            // client can still show the reason and retry after the cause clears.
            let disabled = unavailable.map(|reason| lsp_types::CodeActionDisabled {
                reason: format!("Unavailable: {reason}"),
            });
            actions.push(
                serde_json::to_value(lsp_types::CodeAction {
                    title: title.clone(),
                    kind: None,
                    diagnostics: None,
                    is_preferred: None,
                    disabled: disabled.clone(),
                    edit: None,
                    command: disabled.is_none().then(|| lsp_types::Command {
                        title,
                        command: "magit.command".into(),
                        arguments: Some(vec![args.clone()]),
                    }),
                    data: disabled.is_some().then_some(args),
                })
                .unwrap_or(Value::Null),
            );
        }
    }
    let selection_multi = shared.session(&repo).is_some_and(|handle| {
        let session = handle.lock().unwrap_or_else(|p| p.into_inner());
        session
            .views()
            .get(&view_id)
            .filter(|view| view.version() == generation)
            .and_then(|view| view.commit_selection())
            .is_some_and(|selection| selection.multi)
    });
    for command in crate::commands::registry().iter() {
        if !command.applies_to(&scope) {
            continue;
        }
        let registry_availability = crate::commands::registry().availability(command.id);
        if crate::dispatch::visibility_command(command.id).is_some()
            || crate::dispatch::plain_todo_action(command.id)
        {
            let todo = crate::dispatch::plain_todo_action(command.id);
            if (!todo && scope != "status")
                || (todo && (scope != "rebase-todo" || line_node.commit.is_none()))
            {
                continue;
            }
            let local = matches!(
                command.id,
                "common.toggle-section" | "common.show-section" | "common.hide-section"
            );
            // Offer global controls from section headers; their typed target is
            // still any retained status node when a client invokes them.
            if !todo && !local && line_node.kind != crate::tree::NodeKind::Section {
                continue;
            }
            if local && tree.collapsible_ancestor(&line_node.id).is_none() {
                continue;
            }
            let args = serde_json::json!({
                "command": command.id,
                "documentUri": uri,
                "target": { "kind": "node", "viewId": view_id, "nodeId": line_node.id },
                "viewGeneration": generation,
            });
            let title = plain_action_title(
                style,
                command.doc,
                keymap
                    .as_ref()
                    .and_then(|map| map.command(command.id))
                    .and_then(|binding| binding.keys.first())
                    .map(String::as_str),
                line_node.path.as_deref(),
            );
            actions.push(
                serde_json::to_value(lsp_types::CodeAction {
                    title: title.clone(),
                    kind: None,
                    diagnostics: None,
                    is_preferred: None,
                    disabled: None,
                    edit: None,
                    command: Some(lsp_types::Command {
                        title,
                        command: "magit.command".into(),
                        arguments: Some(vec![args]),
                    }),
                    data: None,
                })
                .unwrap_or(Value::Null),
            );
            continue;
        }
        if !crate::dispatch::content_command(command.id) {
            let semantic = (matches!(
                command.id,
                "open.commit" | "open.branch" | "open.log" | "open.rebase" | "rebase.todo-submit"
            ) && command.applies_to(&scope))
                || (command.id == "common.cancel" && scope != "status");
            if semantic {
                let args = serde_json::json!({
                    "command": command.id,
                    "documentUri": uri,
                    "target": { "kind": "view", "viewId": view_id },
                    "viewGeneration": generation,
                });
                let title = plain_action_title(
                    style,
                    command.doc,
                    keymap
                        .as_ref()
                        .and_then(|map| map.command(command.id))
                        .and_then(|binding| binding.keys.first())
                        .map(String::as_str),
                    line_node.path.as_deref(),
                );
                actions.push(
                    serde_json::to_value(lsp_types::CodeAction {
                        title: title.clone(),
                        kind: None,
                        diagnostics: None,
                        is_preferred: None,
                        disabled: None,
                        edit: None,
                        command: Some(lsp_types::Command {
                            title,
                            command: "magit.command".to_string(),
                            arguments: Some(vec![args]),
                        }),
                        data: None,
                    })
                    .unwrap_or(Value::Null),
                );
                continue;
            }
            if (scope == "log.select" && command.id == "common.confirm")
                || (command.id == "select.toggle-mark" && selection_multi)
            {
                // Commit-node offers carry a validated ActionTarget; identity
                // comes from the structured node, never rendered text.
                if let Some(target) =
                    crate::dispatch::offer_target(&tree, command.id, &line_node.id)
                {
                    let args = serde_json::json!({
                        "command": command.id,
                        "documentUri": uri,
                        "target": serde_json::to_value(
                        crate::dispatch::PlainActionTarget::Content(target.clone()),
                    )
                    .unwrap_or(Value::Null),
                        "viewGeneration": generation,
                    });
                    let title = plain_action_title(
                        style,
                        command.doc,
                        keymap
                            .as_ref()
                            .and_then(|map| map.command(command.id))
                            .and_then(|binding| binding.keys.first())
                            .map(String::as_str),
                        line_node.path.as_deref(),
                    );
                    actions.push(
                        serde_json::to_value(lsp_types::CodeAction {
                            title: title.clone(),
                            kind: None,
                            diagnostics: None,
                            is_preferred: None,
                            disabled: None,
                            edit: None,
                            command: Some(lsp_types::Command {
                                title,
                                command: "magit.command".into(),
                                arguments: Some(vec![args]),
                            }),
                            data: None,
                        })
                        .unwrap_or(Value::Null),
                    );
                    continue;
                }
            }
            // The Plain capability layer knows why a command cannot run there
            // (a commit run whose draft was never opened, or a missing
            // capability). Prefer that readable reason over the generic adapter
            // notice; a planned command keeps its roadmap reason.
            let plain_reason =
                capabilities
                    .as_ref()
                    .and_then(|caps| match caps.plain_availability(command) {
                        crate::presentation::PlainAvailability::Unavailable { reason } => {
                            Some(reason)
                        }
                        crate::presentation::PlainAvailability::Available => None,
                    });
            let reason = match registry_availability {
                Some(crate::commands::Availability::Planned(roadmap)) => {
                    Some(format!("Planned: roadmap {roadmap}"))
                }
                Some(crate::commands::Availability::Implemented) => Some(match plain_reason {
                    Some(reason) => format!("Unavailable: {reason}"),
                    None => "Unavailable: Plain client adapter is not implemented".to_owned(),
                }),
                None => continue,
            };
            if !capabilities
                .as_ref()
                .is_some_and(|caps| caps.disabled_actions)
            {
                continue;
            }
            let args = serde_json::json!({"command": command.id, "documentUri": uri, "viewGeneration": generation});
            let title = plain_action_title(
                style,
                command.doc,
                keymap
                    .as_ref()
                    .and_then(|map| map.command(command.id))
                    .and_then(|binding| binding.keys.first())
                    .map(String::as_str),
                line_node.path.as_deref(),
            );
            let action = lsp_types::CodeAction {
                title: title.clone(),
                kind: None,
                diagnostics: None,
                is_preferred: None,
                disabled: reason.map(|reason| lsp_types::CodeActionDisabled { reason }),
                edit: None,
                command: None,
                data: Some(args),
            };
            actions.push(serde_json::to_value(action).unwrap_or(Value::Null));
            continue;
        }
        let target = if let Some((start, end)) = selection {
            crate::dispatch::offer_selection_target(&tree, command.id, start, end)
        } else {
            crate::dispatch::offer_target(&tree, command.id, &line_node.id)
        };
        if let Some(target) = target {
            let unavailable = match capabilities
                .as_ref()
                .map(|caps| caps.plain_availability(command))
            {
                Some(crate::presentation::PlainAvailability::Unavailable { reason }) => {
                    Some(reason)
                }
                Some(crate::presentation::PlainAvailability::Available) => None,
                None => continue,
            };
            if unavailable.is_some()
                && !capabilities
                    .as_ref()
                    .is_some_and(|caps| caps.disabled_actions)
            {
                continue;
            }
            let args = serde_json::json!({
                "command": command.id,
                "documentUri": uri,
                "target": serde_json::to_value(
                        crate::dispatch::PlainActionTarget::Content(target.clone()),
                    )
                    .unwrap_or(Value::Null),
                "viewGeneration": generation,
            });
            let title = plain_action_title(
                style,
                command.doc,
                keymap
                    .as_ref()
                    .and_then(|map| map.command(command.id))
                    .and_then(|binding| binding.keys.first())
                    .map(String::as_str),
                line_node.path.as_deref(),
            );
            let action = lsp_types::CodeAction {
                title: title.clone(),
                kind: None,
                diagnostics: None,
                is_preferred: None,
                disabled: unavailable.map(|reason| lsp_types::CodeActionDisabled {
                    reason: reason.to_owned(),
                }),
                edit: None,
                command: unavailable.is_none().then(|| lsp_types::Command {
                    title,
                    command: "magit.command".to_string(),
                    arguments: Some(vec![args.clone()]),
                }),
                data: unavailable.map(|_| args),
            };
            actions.push(serde_json::to_value(action).unwrap_or(Value::Null));
        }
    }
    Value::Array(actions)
}

fn plain_action_title(
    style: crate::config::TitleStyle,
    doc: &str,
    chord: Option<&str>,
    path: Option<&str>,
) -> String {
    let mnemonic = matches!(
        style,
        crate::config::TitleStyle::Mnemonic | crate::config::TitleStyle::MnemonicPath
    )
    .then_some(chord)
    .flatten();
    let path = matches!(
        style,
        crate::config::TitleStyle::Path | crate::config::TitleStyle::MnemonicPath
    )
    .then_some(path)
    .flatten();
    match (mnemonic, path) {
        (Some(chord), Some(path)) => format!("{chord} {path}: {doc}"),
        (Some(chord), None) => format!("{chord} {doc}"),
        (None, Some(path)) => format!("{path}: {doc}"),
        (None, None) => doc.to_owned(),
    }
}

#[cfg(test)]
mod plain_action_title_tests {
    use super::plain_action_title;
    use crate::config::TitleStyle;

    #[test]
    fn title_styles_omit_missing_components() {
        assert_eq!(
            plain_action_title(TitleStyle::Plain, "Stage", None, None),
            "Stage"
        );
        assert_eq!(
            plain_action_title(TitleStyle::Mnemonic, "Stage", None, None),
            "Stage"
        );
        assert_eq!(
            plain_action_title(TitleStyle::Path, "Stage", None, Some("src/a")),
            "src/a: Stage"
        );
        assert_eq!(
            plain_action_title(TitleStyle::MnemonicPath, "Stage", Some("s"), None),
            "s Stage"
        );
        assert_eq!(
            plain_action_title(TitleStyle::MnemonicPath, "Stage", Some("s"), Some("src/a")),
            "s src/a: Stage"
        );
    }
}

/// Validate a Plain document position in the negotiated encoding.
pub(super) fn plain_position_valid(
    text: &str,
    position: lsp_types::Position,
    encoding: Encoding,
) -> bool {
    let Some(line) = text.split('\n').nth(position.line as usize) else {
        return false;
    };
    let line = line.strip_suffix('\r').unwrap_or(line);
    let mut units = 0u32;
    if position.character == 0 {
        return true;
    }
    for ch in line.chars() {
        units = match encoding {
            Encoding::Utf8 => units.saturating_add(ch.len_utf8() as u32),
            Encoding::Utf16 => units.saturating_add(ch.len_utf16() as u32),
            Encoding::Utf32 => units.saturating_add(1),
        };
        if units == position.character {
            return true;
        }
        if units > position.character {
            return false;
        }
    }
    units == position.character
}

/// Whether `id` is an answerable prompt row rather than a header, field or
/// detail row.
///
/// Row ids are `prompt:<prompt-id>:<row>`; answerable rows are the confirm,
/// text and choice actions plus any `item:` row for choice and multi-choice
/// prompts.
fn prompt_row_id(id: &str) -> bool {
    id.strip_prefix("prompt:")
        .and_then(|rest| rest.split_once(':'))
        .is_some_and(|(_, row)| {
            matches!(row, "yes" | "no" | "submit" | "cancel" | "done") || row.starts_with("item:")
        })
}

/// Parse the Plain action shape from an `executeCommand` request.
pub(super) fn plain_execute_params(
    request: &Request,
) -> Option<crate::dispatch::PlainActionParams> {
    let params: lsp_types::ExecuteCommandParams =
        serde_json::from_value(request.params.clone()).ok()?;
    if params.command != "magit.command" || params.arguments.len() != 1 {
        return None;
    }
    let argument = &params.arguments[0];
    if argument.get("documentUri").is_none() || argument.get("viewGeneration").is_none() {
        return None;
    }
    serde_json::from_value(argument.clone()).ok()
}

/// Stale Plain action feedback without running the command or touching Git.
fn plain_stale_message() -> Result<Dispatched> {
    Ok(Dispatched::with(
        serde_json::to_value(ViewInvokeResponse {
            effects: vec![Effect::message(
                MessageLevel::Warn,
                crate::errors::STALE_VIEW,
            )],
        })?,
        vec![show_message(2, crate::errors::STALE_VIEW.to_string())],
    ))
}

/// Refuse an unsupported Plain action without entering dispatch or registering prompts.
pub(super) fn plain_unsupported_message(reason: &'static str) -> Result<Dispatched> {
    Ok(Dispatched::value(serde_json::to_value(
        ViewInvokeResponse {
            effects: vec![Effect::message(MessageLevel::Warn, reason)],
        },
    )?))
}

/// Execute a Plain action only while its retained offer still matches live state.
pub(super) fn execute_plain_action(
    shared: &Shared,
    params: crate::dispatch::PlainActionParams,
) -> Result<Dispatched> {
    if params.command == "plain.keys.toggle" {
        return toggle_plain_keys(shared, &params);
    }
    let caps = shared
        .client
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .plain
        .clone();
    if let Some(caps) = caps.as_ref() {
        if crate::dispatch::content_command(&params.command) {
            if let Some(command) = crate::commands::registry().get(&params.command) {
                if let crate::presentation::PlainAvailability::Unavailable { reason } =
                    caps.plain_availability(command)
                {
                    return plain_unsupported_message(reason);
                }
            }
        }
    } else {
        return plain_unsupported_message("Plain action capabilities are unavailable");
    }
    let uri = params.document_uri.as_str();
    let text = {
        let documents = shared.documents.lock().unwrap_or_else(|p| p.into_inner());
        match documents.get(uri) {
            Some(document) if !document.repair => document.text.clone(),
            _ => return plain_stale_message(),
        }
    };
    let (repo, view_id, line, scope, effective_command) = {
        let documents = shared
            .plain_documents
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let Some(entry) = documents.get(uri) else {
            return plain_stale_message();
        };
        let Some((tree, view_id, generation, scope, menu)) =
            entry.controller.action_snapshot(&text)
        else {
            return plain_stale_message();
        };
        if view_id != entry.view_id || generation != params.view_generation {
            tracing::warn!(
                view_id,
                active_view = entry.view_id,
                generation,
                offered_generation = params.view_generation,
                command = params.command,
                "Plain action retained snapshot identity/generation mismatch"
            );
            return plain_stale_message();
        }
        let mut effective_command = params.command.clone();
        let line = match &params.target {
            crate::dispatch::PlainActionTarget::Content(target) => {
                let Some(node) = tree.node(&target.node_id) else {
                    return plain_stale_message();
                };
                let git = crate::git::GitRepo::open(&entry.repo)?;
                if !crate::dispatch::target_is_current(&git, tree, &params.command, target)? {
                    return plain_stale_message();
                }
                node.start_line
            }
            crate::dispatch::PlainActionTarget::View(target) => {
                let supported = matches!(
                    params.command.as_str(),
                    "open.commit"
                        | "open.branch"
                        | "open.log"
                        | "open.rebase"
                        | "common.cancel"
                        | "rebase.todo-submit"
                );
                let applies = crate::commands::registry()
                    .get(&params.command)
                    .is_some_and(|command| command.applies_to(&scope));
                if !supported
                    || target.view_id != view_id
                    || !(applies || (params.command == "common.cancel" && scope != "status"))
                {
                    return plain_stale_message();
                }
                0
            }
            crate::dispatch::PlainActionTarget::Node(target) => {
                if params.command == "prompt.answer" {
                    let valid = scope == "prompt"
                        && target.view_id == view_id
                        && tree.node(&target.node_id).is_some_and(|node| {
                            node.kind == crate::tree::NodeKind::Line && prompt_row_id(&node.id)
                        });
                    if !valid {
                        return plain_stale_message();
                    }
                } else if crate::dispatch::plain_todo_action(&params.command) {
                    if scope != "rebase-todo"
                        || target.view_id != view_id
                        || !tree
                            .node(&target.node_id)
                            .is_some_and(|node| node.commit.is_some())
                    {
                        return plain_stale_message();
                    }
                } else if !crate::dispatch::visibility_command(&params.command).is_some()
                    || target.view_id != view_id
                    || scope != "status"
                {
                    return plain_stale_message();
                }
                let Some(node) = tree.node(&target.node_id) else {
                    return plain_stale_message();
                };
                if params.command != "prompt.answer"
                    && matches!(
                        params.command.as_str(),
                        "common.toggle-section" | "common.show-section" | "common.hide-section"
                    )
                    && tree.collapsible_ancestor(&node.id).is_none()
                {
                    return plain_stale_message();
                }
                node.start_line
            }
            crate::dispatch::PlainActionTarget::MenuItem(target) => {
                let valid = target.view_id == view_id
                    && menu.as_ref().is_some_and(|menu| {
                        if menu.id != target.menu_id {
                            return false;
                        }
                        match (params.command.as_str(), menu.item(&target.item_key)) {
                            (
                                "menu.toggle-switch",
                                Some(crate::menu::Item::Switch {
                                    status: crate::menu::MenuStatus::Implemented,
                                    ..
                                }),
                            ) => true,
                            (
                                "menu.run-action",
                                Some(crate::menu::Item::Action {
                                    command,
                                    status: crate::menu::MenuStatus::Implemented,
                                    ..
                                }),
                            ) => {
                                effective_command = command.clone();
                                crate::dispatch::plain_menu_action(command)
                            }
                            (
                                "menu.run-option",
                                Some(crate::menu::Item::Option {
                                    status: crate::menu::MenuStatus::Implemented,
                                    ..
                                }),
                            ) => true,
                            _ => false,
                        }
                    });
                if !valid {
                    return plain_stale_message();
                }
                0
            }
        };
        if matches!(
            params.target,
            crate::dispatch::PlainActionTarget::Content(_)
        ) && !crate::dispatch::content_command(&params.command)
            && !matches!(
                params.command.as_str(),
                "common.confirm" | "select.toggle-mark"
            )
        {
            return plain_stale_message();
        }
        let repo = entry.repo.clone();
        let view_id = entry.view_id.clone();
        (repo, view_id, line, scope.to_string(), effective_command)
    };
    if shared.cancellation_pending(&repo) {
        return cancelling_response();
    }
    if shared.deferred_pending(&repo) && repository_mutation_command(&effective_command) {
        return deferred_operation_response();
    }
    let handle = shared
        .session(&repo)
        .ok_or_else(|| Error::new("Plain action session closed"))?;
    if params.command == "common.cancel"
        && scope == "log"
        && matches!(params.target, crate::dispatch::PlainActionTarget::View(_))
    {
        let result = handle
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .back_from_plain_log(&view_id, params.view_generation)?;
        return dispatched_outcome(shared, &repo, result);
    }
    if let crate::dispatch::PlainActionTarget::MenuItem(target) = &params.target {
        if params.command == "menu.toggle-switch" {
            let result = handle
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .toggle_menu_switch(
                    &view_id,
                    params.view_generation,
                    &target.menu_id,
                    &target.item_key,
                )?;
            return dispatched_outcome(shared, &repo, result);
        }
        if params.command == "menu.run-action" {
            let ui = ServerUi::new(shared);
            let (mut result, pending) = {
                let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
                let result = session.run_plain_menu_action(
                    &view_id,
                    params.view_generation,
                    &target.menu_id,
                    &target.item_key,
                    &ui,
                )?;
                (result, session.pending_prompt_request())
            };
            if let Some(request) = pending {
                let next =
                    drive_execution(shared, &repo, crate::session::Execution::Prompt(request))?;
                result.effects.extend(next.effects);
                result.opened.extend(next.opened);
                result.render = next.render.or(result.render);
                result.keymap = next.keymap.or(result.keymap);
                result.commit = next.commit.or(result.commit);
            }
            return dispatched_outcome(shared, &repo, result);
        }
        if params.command == "menu.run-option" {
            let step = handle
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .start_plain_menu_option(
                    &view_id,
                    params.view_generation,
                    &target.menu_id,
                    &target.item_key,
                )?;
            let outcome = drive_execution(shared, &repo, step)?;
            return dispatched_outcome(shared, &repo, outcome);
        }
    }
    if params.command == "prompt.answer" {
        let node_id = match &params.target {
            crate::dispatch::PlainActionTarget::Node(target) => target.node_id.clone(),
            _ => return plain_stale_message(),
        };
        let crate::session::PromptCompletion { mut result, next } = {
            let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
            session.answer_document_prompt(&node_id, &text)?
        };
        let next = drive_execution(shared, &repo, next)?;
        result.effects.extend(next.effects);
        result.opened.extend(next.opened);
        result.render = next.render.or(result.render);
        result.keymap = next.keymap.or(result.keymap);
        result.commit = next.commit.or(result.commit);
        return dispatched_outcome(shared, &repo, result);
    }
    let request = crate::dispatch::DispatchRequest {
        command: params.command,
        target: crate::dispatch::DispatchTarget {
            view_id: view_id.clone(),
            version: Some(params.view_generation),
            cursor: matches!(
                params.target,
                crate::dispatch::PlainActionTarget::Content(_)
                    | crate::dispatch::PlainActionTarget::Node(_)
            )
            .then_some(Cursor { line, column: 0 }),
            selection: match &params.target {
                crate::dispatch::PlainActionTarget::Content(target) => {
                    target
                        .selection
                        .map(|selection| crate::protocol::Selection {
                            start_line: selection.start_line as usize,
                            end_line: selection.end_line as usize,
                        })
                }
                _ => None,
            },
            key: None,
        },
        action_target: match params.target {
            crate::dispatch::PlainActionTarget::Content(target) => Some(target),
            _ => None,
        },
        args: None,
    };
    let step = handle
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .start_dispatch(request)?;
    let outcome = drive_execution(shared, &repo, step)?;
    dispatched_outcome(shared, &repo, outcome)
}

/// Toggle key arming for one retained Plain document and repair its header.
fn toggle_plain_keys(
    shared: &Shared,
    params: &crate::dispatch::PlainActionParams,
) -> Result<Dispatched> {
    let uri = params.document_uri.as_str();
    let text = {
        let documents = shared.documents.lock().unwrap_or_else(|p| p.into_inner());
        match documents.get(uri) {
            Some(document) if !document.repair => document.text.clone(),
            _ => return plain_stale_message(),
        }
    };
    let edit = {
        let mut documents = shared
            .plain_documents
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let Some(entry) = documents.get_mut(uri) else {
            return plain_stale_message();
        };
        let Some((tree, view_id, generation, scope, menu)) =
            entry.controller.action_snapshot(&text)
        else {
            return plain_stale_message();
        };
        if generation != params.view_generation
            || view_id != entry.view_id
            || scope == "prompt"
            || scope == "log.select"
            || scope == "commit-message"
            || matches!(&params.target, crate::dispatch::PlainActionTarget::View(target) if target.view_id != view_id)
        {
            return plain_stale_message();
        }
        let mut tree = tree.clone();
        let next =
            !super::plain::plain_keys_effective(shared, &entry.repo, entry.keys_override).enabled;
        entry.keys_override = Some(next);
        super::plain::append_plain_keys_header(shared, &entry.repo, entry.keys_override, &mut tree);
        entry
            .controller
            .render_actions(
                tree,
                view_id.to_owned(),
                generation,
                scope.to_owned(),
                menu.cloned(),
            )
            .map(|edit| (entry.incarnation, edit))
    };
    if let Some((incarnation, edit)) = edit {
        super::plain::send_plain_edit(shared, uri.to_owned(), incarnation, edit);
    }
    Ok(Dispatched::value(serde_json::to_value(
        ViewInvokeResponse { effects: vec![] },
    )?))
}
