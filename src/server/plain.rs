//! Plain document render delivery and lifecycle.

use super::*;

/// Start an asynchronous bootstrap after the didOpen notification is accepted.
pub(super) fn schedule_plain_open(shared: &Shared, uri: String) -> Option<(String, InvokeResult)> {
    let format = {
        let documents = shared.documents.lock().unwrap_or_else(|p| p.into_inner());
        let Some(document) = documents.get(&uri) else {
            return None;
        };
        DocumentFormat::from_text(&document.text)
    };
    let Some(path) = file_uri_path(&uri) else {
        return None;
    };
    if !crate::config::is_presentation_file(&path, &shared.config.config().presentation_files) {
        return None;
    }
    let close = close_plain(shared, &uri);
    let incarnation = shared
        .next_plain_incarnation
        .fetch_add(1, Ordering::Relaxed)
        + 1;
    shared
        .plain_incarnations
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(uri.clone(), incarnation);
    let shared = shared.clone();
    tokio::spawn(async move {
        bootstrap_plain(&shared, uri, incarnation, format).await;
    });
    close
}

async fn bootstrap_plain(shared: &Shared, uri: String, incarnation: u64, format: DocumentFormat) {
    if shared.closing.load(Ordering::SeqCst)
        || !plain_incarnation_is_current(shared, &uri, incarnation)
    {
        return;
    }
    let (caps, workspaces, encoding) = {
        let client = shared.client.lock().unwrap_or_else(|p| p.into_inner());
        (
            client.plain.clone(),
            client
                .workspace_repositories
                .iter()
                .map(|r| r.root.clone())
                .collect::<Vec<_>>(),
            client.encoding,
        )
    };
    let config = shared.config.config();
    let Some(path) = file_uri_path(&uri) else {
        return;
    };
    if !crate::config::is_presentation_file(&path, &config.presentation_files) {
        return;
    }
    let Some(caps) = caps else {
        return;
    };
    if !caps.apply_edit {
        let _ = shared.sender.send(show_message(2, format!("magit-lsp: cannot render {} because the client does not support workspace/applyEdit", path.display())));
        return;
    }
    let resolution = crate::presentation::resolve::resolve(None, &workspaces, path.parent());
    let repository = match resolution {
        crate::presentation::resolve::Resolution::Resolved(repository) => repository,
        crate::presentation::resolve::Resolution::Choose(candidates) => {
            let choices = candidates
                .iter()
                .map(|item| item.root.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let _ = shared.sender.send(show_message(2, format!("magit-lsp: cannot choose a repository for {}; workspace roots are ambiguous: {choices}", path.display())));
            return;
        }
        crate::presentation::resolve::Resolution::None => return,
    };
    let repo = repository.root.to_string_lossy().into_owned();
    let handle = repository_session(shared, &repo);
    let config = shared.config.for_repository(Path::new(&repo));
    let view_id = shared.next_view_id();
    let opened = {
        let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
        session.set_keymaps(config.keymaps());
        session.open_with_section_line_limit(
            view_id.clone(),
            ViewKind::Status,
            crate::view::Placement::default(),
            Viewport::default(),
            config.config().plain.max_section_lines as usize,
        )
    };
    // `open` uses (id, kind, placement, viewport); preserve the root session's
    // authoritative Tree while keeping this logical view private to Plain.
    let (descriptor, _) = match opened {
        Ok(opened) => opened,
        Err(error) => {
            let _ = shared.sender.send(show_message(
                1,
                format!("magit-lsp: cannot render status: {error}"),
            ));
            return;
        }
    };
    let published = handle.lock().unwrap_or_else(|p| p.into_inner()).published();
    let _plain = shared
        .plain_lifecycle
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if shared.closing.load(Ordering::SeqCst)
        || !plain_incarnation_is_current(shared, &uri, incarnation)
    {
        let _ = handle
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .close(&descriptor.id);
        return;
    }
    let current = shared
        .documents
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&uri)
        .cloned();
    let Some(current) = current else {
        let _ = handle
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .close(&descriptor.id);
        return;
    };
    let mut controller = PlainDocument::new(uri.clone(), encoding, caps.document_changes, format);
    if let Some((mut tree, generation, scope, menu)) = published.current_snapshot(&descriptor.id) {
        if !matches!(scope.as_str(), "prompt" | "log.select" | "commit-message") {
            append_plain_keys_header(shared, &repo, None, &mut tree);
        }
        controller.render_actions(tree, descriptor.id.clone(), generation, scope, menu);
    } else {
        let _ = handle
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .close(&descriptor.id);
        return;
    }
    let first = controller.open(current.version, current.text);
    if !plain_incarnation_is_current(shared, &uri, incarnation) {
        let _ = handle
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .close(&descriptor.id);
        return;
    }
    {
        shared
            .plain_documents
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(
                uri.clone(),
                PlainEntry {
                    incarnation,
                    repo: repo.clone(),
                    view_id: descriptor.id.clone(),
                    controller,
                    keys_override: None,
                    key_state: Default::default(),
                },
            );
        shared
            .view_repos
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(descriptor.id, repo.clone());
        shared
            .published
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entry(repo.clone())
            .or_insert(published);
    }
    shared.ensure_watcher(&repo);
    if let Some(edit) = first {
        send_plain_edit(shared, uri, incarnation, edit);
    }
}

pub(super) fn plain_keys_effective(shared: &Shared, repo: &str, document: Option<bool>) -> crate::config::EffectivePlainKeys {
    let client = shared.client.lock().unwrap_or_else(|p| p.into_inner());
    crate::config::resolve_plain_keys(
        &shared.config.for_repository(std::path::Path::new(repo)).config().plain,
        client.client_name.as_deref(), document, client.plain_keys_allowed,
    )
}

pub(super) fn append_plain_keys_header(shared: &Shared, repo: &str, document: Option<bool>, tree: &mut crate::tree::Tree) {
    let effective = plain_keys_effective(shared, repo, document);
    let source = match effective.source {
        crate::config::PlainKeysSource::BuiltIn => "built-in",
        crate::config::PlainKeysSource::UserDefault => "default",
        crate::config::PlainKeysSource::ExactClient => "client",
        crate::config::PlainKeysSource::Document => "document",
        crate::config::PlainKeysSource::Quirk => "quirk",
    };
    tree.append_plain_key_policy(effective.enabled, source);
}

fn plain_incarnation_is_current(shared: &Shared, uri: &str, incarnation: u64) -> bool {
    shared
        .plain_incarnations
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(uri)
        == Some(&incarnation)
}

fn repository_session(shared: &Shared, repo: &str) -> SessionHandle {
    if let Some(handle) = shared.session(repo) {
        return handle;
    }
    let draft_owner = matches!(
        shared.client.lock().unwrap_or_else(|p| p.into_inner()).presentation,
        Some(Presentation::Plain(_))
    )
    .then(|| shared.draft_owner.as_ref().clone());
    let candidate = Arc::new(Mutex::new(Session::new_with_draft_owner(repo, draft_owner)));
    let mut sessions = shared.sessions.lock().unwrap_or_else(|p| p.into_inner());
    sessions.entry(repo.to_owned()).or_insert(candidate).clone()
}

/// Re-render the views that show pending drafts for `repo` and return the renders.
///
/// Must not be called on a thread that already holds the draft registry lock:
/// it re-enters that lock through `EditRegistry::pending_for`.
pub(super) fn refresh_pending_views(shared: &Shared, repo: &str) -> Result<Vec<ViewRender>> {
    let Some(handle) = shared.session(repo) else {
        return Ok(Vec::new());
    };
    let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
    let ids: Vec<_> = session
        .views()
        .iter()
        .map(|view| view.descriptor().id.clone())
        .collect();
    let mut renders = Vec::new();
    for id in ids {
        if let Some(render) = session.refresh(&id)? {
            renders.push(render);
        }
    }
    Ok(renders)
}

pub(super) fn report_pending_refresh_error(shared: &Shared, error: Error) {
    tracing::warn!(%error, "failed to refresh pending commit draft rows");
    let effects = errors::effects_for_result(Err(error)).unwrap_or_else(|error| {
        vec![Effect::message(MessageLevel::Error, error.to_string())]
    });
    for message in effect_notifications(&shared.client, effects) {
        let _ = shared.sender.send(message);
    }
}

pub(super) fn plain_change(
    shared: &Shared,
    uri: &str,
    accepted: Option<super::drafts::AcceptedChange>,
) -> Option<PlainKeyDispatch> {
    let snapshot = shared
        .documents
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(uri)
        .cloned();
    let Some(snapshot) = snapshot else {
        return None;
    };

    // The policy header is a structured node. If the client edits that row,
    // restore the retained authoritative tree without interpreting document text.
    let header_repair = {
        let mut documents = shared.plain_documents.lock().unwrap_or_else(|p| p.into_inner());
        documents.get_mut(uri).and_then(|entry| {
            let (tree, view_id, generation, scope, menu) = entry.controller.latest_action_meta()?;
            let node = tree.node("plain-keys-policy")?;
            if snapshot.text.lines().nth(node.start_line) == Some(node.label.as_str()) {
                return None;
            }
            let (tree, view_id, generation, scope, menu) =
                (tree.clone(), view_id.to_owned(), generation, scope.to_owned(), menu.cloned());
            entry.controller.observe(snapshot.version, snapshot.text.clone());
            entry.controller.render_actions(tree, view_id, generation, scope, menu)
                .map(|edit| (entry.incarnation, edit))
        })
    };
    if let Some((incarnation, edit)) = header_repair {
        send_plain_edit(shared, uri.to_owned(), incarnation, edit);
        return None;
    }

    // Route an edit of an open prompt or log-selection document through the
    // session instead of the ordinary presentation controller. Never hold this
    // lock across the session call: the session may publish views that reload
    // this controller.
    let editable = {
        let documents = shared
            .plain_documents
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        documents.get(uri).and_then(|entry| {
            let (_, view_id, generation, scope, _) = entry.controller.latest_action_meta()?;
            (scope == "prompt" || scope == "log.select").then(|| {
                (
                    scope.to_owned(),
                    entry.repo.clone(),
                    view_id.to_owned(),
                    generation,
                    entry.incarnation,
                )
            })
        })
    };

    if let Some((scope, repo, view_id, generation, incarnation)) = editable {
        if let Some(handle) = shared.session(&repo) {
            let reconciled = {
                let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
                if scope == "prompt" {
                    session.reconcile_prompt_document(&snapshot.text)
                } else {
                    session.reconcile_log_select_document(&view_id, &snapshot.text)
                }
            };
            match reconciled {
                Ok(Some(PromptReconcile {
                    repaired: true,
                    tree,
                    render: Some(render),
                })) => {
                    let edit = shared
                        .plain_documents
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .get_mut(uri)
                        .filter(|entry| entry.incarnation == incarnation)
                        .and_then(|entry| {
                            entry.controller.observe(snapshot.version, snapshot.text.clone());
                            entry.controller.render_actions(
                                tree,
                                view_id,
                                render.version,
                                scope.clone(),
                                None,
                            )
                        });
                    if let Some(edit) = edit {
                        send_plain_edit(shared, uri.to_owned(), incarnation, edit);
                    }
                    return None;
                }
                Ok(Some(PromptReconcile {
                    repaired: false,
                    tree,
                    ..
                })) => {
                    let mut documents = shared
                        .plain_documents
                        .lock()
                        .unwrap_or_else(|p| p.into_inner());
                    if let Some(entry) = documents
                        .get_mut(uri)
                        .filter(|entry| entry.incarnation == incarnation)
                    {
                        entry.controller.adopt(snapshot.version, snapshot.text.clone());
                        entry
                            .controller
                            .retain_actions(tree, &view_id, generation, &scope, None);
                    }
                    return None;
                }
                Ok(Some(PromptReconcile {
                    repaired: true,
                    render: None,
                    ..
                })) => {
                    tracing::error!("a repaired editable document produced no render");
                }
                Ok(None) => {
                    tracing::debug!("the prompt ended before the edit; using document sync");
                }
                Err(error) => {
                    tracing::error!(%error, "failed to reconcile an edited prompt document");
                }
            }
        }
    }

    if let Some(change) = accepted.as_ref() {
        match plain_key_step(shared, uri, change, &snapshot) {
            PlainKeyStep::Continue => {}
            PlainKeyStep::Wait => return None,
            PlainKeyStep::Dispatch(dispatch) => return Some(dispatch),
        }
    }

    let (request, warning) = shared
        .plain_documents
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get_mut(uri)
        .map(|entry| {
            let request = entry.controller.change(snapshot.version, snapshot.text);
            (request, entry.controller.take_format_warning())
        })
        .unwrap_or((None, false));
    if warning {
        let _ = shared.sender.send(show_message(2, "magit-lsp: the editor keeps changing the presentation file's line endings or final newline; close and reopen the file to resynchronize".into()));
    }
    if let Some(edit) = request {
        let incarnation = shared
            .plain_documents
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(uri)
            .map(|entry| entry.incarnation);
        if let Some(incarnation) = incarnation {
            send_plain_edit(shared, uri.to_owned(), incarnation, edit);
        }
    }
    None
}

/// Message shown when a key edit no longer matches the displayed session state.
const PLAIN_KEY_STALE: &str = "changed since displayed; refreshed";

/// Decision for one accepted Plain edit before the ordinary repair tail runs.
enum PlainKeyStep {
    /// Not a dispatchable key edit: fall through to the ordinary repair path.
    Continue,
    /// A deletion half waits for its related insertion; keep the document text.
    Wait,
    /// Run this key, or refresh the document, once the lifecycle guard drops.
    Dispatch(PlainKeyDispatch),
}

/// One Plain key action to execute once the Plain lifecycle guard is released.
pub(super) enum PlainKeyDispatch {
    /// Run one key step through the existing shared session key handler.
    Key {
        uri: String,
        repo: String,
        params: crate::protocol::ViewKeyParams,
    },
    /// Discard a stale edit and render the session's current state.
    Refresh { uri: String, repo: String },
}

/// Classify one accepted Plain edit as a key against its matched retained snapshot.
fn plain_key_step(
    shared: &Shared,
    uri: &str,
    change: &super::drafts::AcceptedChange,
    snapshot: &crate::docstore::Document,
) -> PlainKeyStep {
    use crate::presentation::keys::{self, KeyPreflight, State};
    let encoding = shared
        .client
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .encoding;
    let (repo, view_id, generation, prior, mut state) = {
        let mut documents = shared
            .plain_documents
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let Some(entry) = documents.get_mut(uri) else {
            return PlainKeyStep::Continue;
        };
        // Recognize only the exact version/hash produced by the outstanding
        // repair, before treating its mutated pre-text as fresh key input.
        if entry.key_state.repair_echo.is_some_and(|(_, version, hash)| version == change.post_version && hash == crate::presentation::plain::hash_text(&change.post_text)) {
            entry.key_state.repair_echo = None;
            entry.key_state.classifier = State::Ready;
            return PlainKeyStep::Continue;
        }
        if !plain_keys_effective(shared, &entry.repo, entry.keys_override).enabled {
            return PlainKeyStep::Continue;
        }
        let Some((_, view_id, generation, scope, _)) =
            entry.controller.action_snapshot(&change.pre.text)
        else {
            return PlainKeyStep::Continue;
        };
        if matches!(scope, "prompt" | "log.select" | "commit-message") {
            return PlainKeyStep::Continue;
        }
        let state = entry.key_state.clone();
        (
            entry.repo.clone(),
            view_id.to_owned(),
            generation,
            state.classifier.clone(),
            state,
        )
    };
    let timeout = shared
        .config
        .for_repository(Path::new(&repo))
        .config()
        .plain
        .keys_pair_timeout_ms;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0);
    let decision = keys::decide(
        &mut state.classifier,
        &change.pre.text,
        &change.changes,
        change.pre.version,
        change.post_version,
        &change.post_text,
        encoding,
        now,
        timeout,
    );
    let selection = match &prior {
        State::PendingDeletion { edit, .. } => edit.pre_range.map(|((start, _), (end, col))| {
            crate::protocol::Selection {
                start_line: start as usize,
                end_line: keys::inclusive_lines(start, end, col).1 as usize,
            }
        }),
        _ => None,
    };
    {
        let mut documents = shared
            .plain_documents
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if let Some(entry) = documents.get_mut(uri) {
            entry.key_state = state.clone();
            match &decision {
                keys::Decision::Wait => {
                    entry.controller.observe(snapshot.version, snapshot.text.clone());
                }
                keys::Decision::Dispatch(_) | keys::Decision::Repair | keys::Decision::DropBusy => {}
            }
        }
    }
    let key = match decision {
        keys::Decision::Wait => return PlainKeyStep::Wait,
        keys::Decision::Repair => return PlainKeyStep::Continue,
        keys::Decision::DropBusy => {
            if let Some(entry) = shared
                .plain_documents
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get_mut(uri)
            {
                entry.key_state.repair_due = true;
            }
            return PlainKeyStep::Continue;
        }
        keys::Decision::Dispatch(key) => key,
    };
    // The key must resolve against the snapshot the client was actually shown.
    let current = shared
        .published
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .values()
        .find_map(|views| {
            views
                .current_snapshot(&view_id)
                .map(|(_, version, _, _)| version)
        });
    let focused = shared
        .plain_documents
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(uri)
        .is_some_and(|entry| entry.view_id == view_id);
    if current != Some(generation) || !focused {
        let _ = shared.sender.send(show_message(2, PLAIN_KEY_STALE.into()));
        if let Some(entry) = shared
            .plain_documents
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get_mut(uri)
        {
            entry.key_state = Default::default();
        }
        return PlainKeyStep::Dispatch(PlainKeyDispatch::Refresh { uri: uri.to_owned(), repo });
    }
    let edit = keys::derive_edit(
        &change.pre.text,
        change.pre.version,
        change.post_version,
        &change.changes,
        encoding,
    );
    let Some(((line, column), _)) = edit.pre_range else {
        return PlainKeyStep::Continue;
    };
    let params = crate::protocol::ViewKeyParams {
        view_id,
        version: generation,
        key: key.clone(),
        editor_mode: crate::keymap::EditorMode::Normal,
        cursor: Cursor {
            line: line as usize,
            column: column as usize,
        },
        selection,
    };
    let Some(handle) = shared.session(&repo) else {
        return PlainKeyStep::Continue;
    };
    let keymaps = shared
        .config
        .for_repository(Path::new(&repo))
        .keymaps();
    let preflight = {
        let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
        session.set_keymaps(keymaps);
        session.preflight_key(&params)
    };
    match preflight {
        Ok(KeyPreflight::Pending) => PlainKeyStep::Dispatch(PlainKeyDispatch::Key {
            uri: uri.to_owned(),
            repo,
            params,
        }),
        Ok(KeyPreflight::Allowed { .. }) => PlainKeyStep::Dispatch(PlainKeyDispatch::Key {
            uri: uri.to_owned(),
            repo,
            params,
        }),
        Ok(KeyPreflight::Unbound) => {
            let _ = shared
                .sender
                .send(show_message(2, "key is not bound in the active profile".into()));
            PlainKeyStep::Continue
        }
        Ok(KeyPreflight::Refused { message, .. }) => {
            let _ = shared.sender.send(show_message(2, message));
            PlainKeyStep::Continue
        }
        Err(error) => {
            tracing::warn!(%error, "failed to preflight a Plain key");
            PlainKeyStep::Continue
        }
    }
}

/// Execute one accepted Plain key with no lifecycle guard held.
pub(super) fn execute_plain_key(shared: &Shared, dispatch: PlainKeyDispatch) -> Result<()> {
    match dispatch {
        PlainKeyDispatch::Refresh { uri, repo } => {
            let view_id = shared
                .plain_documents
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .values()
                .find(|entry| entry.repo == repo)
                .map(|entry| entry.view_id.clone());
            let snapshot = view_id.as_ref().and_then(|view_id| {
                shared
                    .published
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .values()
                    .find_map(|views| views.current_snapshot(view_id))
            });
            match (view_id, snapshot) {
                (Some(view_id), Some(snapshot)) => {
                    route_plain_render(shared, &view_id, Some(snapshot));
                }
                _ => repair_plain_authoritative(shared, &uri),
            }
            return Ok(());
        }
        PlainKeyDispatch::Key {
            uri,
            repo,
            params,
        } => {
            // Admission must match `key_view`: cancellation cleanup and a
            // detached worker's mutation authority both refuse before any
            // command starts, leaving the typed text to the authoritative repair.
            if let Some(message) = super::view_execution::key_admission_refusal(shared, &repo, || {
                let Some(handle) = shared.session(&repo) else {
                    return Ok(None);
                };
                let resolved = handle
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .resolved_key_command(&params);
                resolved
            })? {
                let _ = shared.sender.send(show_message(2, message.into()));
                repair_plain_authoritative(shared, &uri);
                return Ok(());
            }
            if let Some(entry) = shared.plain_documents.lock().unwrap_or_else(|p| p.into_inner()).get_mut(&uri) {
                entry.key_state.classifier = crate::presentation::keys::State::Executing;
                entry.key_state.repair_due = false;
            }
            let handle = shared
                .session(&repo)
                .ok_or_else(|| Error::new("Plain key session closed"))?;
            let keymaps = shared
                .config
                .for_repository(Path::new(&repo))
                .keymaps();
            let step = {
                let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
                session.set_keymaps(keymaps);
                session.start_key(&params)?
            };
            let outcome = super::view_execution::drive_execution(shared, &repo, step)?;
            let dispatched = super::view_execution::dispatched_outcome(shared, &repo, outcome)?;
            for notification in dispatched.notifications {
                let _ = shared.sender.send(notification);
            }
            repair_plain_authoritative(shared, &uri);
            Ok(())
        }
    }
}

/// Repair one Plain document to its newest retained authoritative render.
fn repair_plain_authoritative(shared: &Shared, uri: &str) {
    let (text, version) = {
        let documents = shared
            .documents
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let Some(document) = documents.get(uri).filter(|document| !document.repair) else {
            return;
        };
        (document.text.clone(), document.version)
    };
    let _lifecycle = shared
        .plain_lifecycle
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let mut documents = shared
        .plain_documents
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let Some(entry) = documents.get_mut(uri) else {
        return;
    };
    if entry.key_state.repair_echo.is_some() {
        return;
    }
    let Some((tree, view_id, generation, scope, menu)) = entry.controller.latest_action_meta() else {
        return;
    };
    let (tree, view_id, generation, scope, menu) =
        (tree.clone(), view_id.to_owned(), generation, scope.to_owned(), menu.cloned());
    entry.controller.observe(version, text);
    let edit = entry.controller.render_actions(tree, view_id, generation, scope, menu);
    if let Some(edit) = edit {
        let incarnation = entry.incarnation;
        drop(documents);
        send_plain_edit(shared, uri.to_owned(), incarnation, edit);
    }
}

/// Reconcile an edited Rich prompt document against the session-owned prompt.
///
/// A field edit is adopted without a revert; a non-field edit is repaired by
/// re-rendering the prompt view. Adopted edits, ended prompts and errors fall
/// through to ordinary document synchronization.
pub(super) fn reconcile_rich_prompt(shared: &Shared, uri: &str, sender: &mpsc::UnboundedSender<Message>) {
    let rich = matches!(
        shared
            .client
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .presentation
            .as_ref(),
        Some(Presentation::Rich(_))
    );
    if !rich {
        return;
    }
    let Some(view_id) = uri.strip_prefix("magit://prompt/") else {
        return;
    };
    let Some(repo) = shared.repo_for_view(view_id) else {
        return;
    };
    let Some(handle) = shared.session(&repo) else {
        return;
    };
    let snapshot = shared
        .documents
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(uri)
        .cloned();
    let Some(snapshot) = snapshot else {
        return;
    };
    let reconciled = {
        // Never hold the document store across the session call: publishing a
        // repaired render reloads the store on this same reader lane.
        let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
        session.reconcile_prompt_document(&snapshot.text)
    };
    match reconciled {
        Ok(Some(PromptReconcile {
            repaired: true,
            render: Some(render),
            ..
        })) => {
            if let Some(message) = deliver_render(shared, render) {
                let _ = sender.send(message);
            }
        }
        Ok(Some(_)) | Ok(None) => {}
        Err(error) => {
            tracing::debug!(%error, "failed to reconcile a Rich prompt document edit");
        }
    }
}

pub(super) fn close_plain(shared: &Shared, uri: &str) -> Option<(String, InvokeResult)> {
    shared
        .plain_incarnations
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(uri);
    let entry = shared
        .plain_documents
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(uri);
    let Some(mut entry) = entry else {
        return None;
    };
    entry.controller.close();
    let outcome = shared.session(&entry.repo).and_then(|session| {
        session
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .close(&entry.view_id)
            .ok()
            .map(|outcome| (entry.repo.clone(), outcome))
    });
    shared
        .view_repos
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(&entry.view_id);
    let repo = entry.repo;
    let shared = shared.clone();
    let runtime = shared
        .runtime
        .clone()
        .expect("Plain transport owns a Tokio runtime");
    runtime.spawn_blocking(move || shared.stop_watcher_if_idle(&repo));
    outcome
}

/// Deliver the renders and effects a closed Plain view resumed.
///
/// Callers must drop `plain_lifecycle` first: routing these notifications
/// reacquires it.
pub(super) fn deliver_plain_close_outcome(shared: &Shared, close: Option<(String, InvokeResult)>) {
    let Some((repo, outcome)) = close else {
        return;
    };
    match dispatched_outcome(shared, &repo, outcome) {
        Ok(dispatched) => {
            for notification in dispatched.notifications {
                let _ = shared.sender.send(notification);
            }
        }
        Err(error) => tracing::warn!(%error, "failed to deliver a Plain view close outcome"),
    }
}

/// Default time a Plain client has to open a commit draft before it is canceled.
const DRAFT_OPEN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Default time a Plain client has to answer a commit draft template edit.
const DRAFT_DELIVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Deadline for the client to answer a Plain commit draft template edit.
///
/// `MAGIT_LSP_DRAFT_DELIVERY_TIMEOUT_MS` overrides the default so tests can
/// treat an unanswered delivery as failed without a production-length wait.
fn draft_delivery_timeout() -> std::time::Duration {
    std::env::var("MAGIT_LSP_DRAFT_DELIVERY_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(std::time::Duration::from_millis)
        .unwrap_or(DRAFT_DELIVERY_TIMEOUT)
}

/// Deadline for the client to open a Plain commit draft.
///
/// `MAGIT_LSP_DRAFT_OPEN_TIMEOUT_MS` overrides the default so tests can expire a
/// run deterministically without a production-length wait.
fn draft_open_timeout() -> std::time::Duration {
    std::env::var("MAGIT_LSP_DRAFT_OPEN_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(std::time::Duration::from_millis)
        .unwrap_or(DRAFT_OPEN_TIMEOUT)
}

/// Cancel a commit draft run the client never opened, allowing a new attempt.
///
/// The wait runs as a runtime task so it never blocks the edit emitter, which
/// calls `deliver_plain_effects` while the registry lock is held. It holds only a
/// [`Weak`] handle: a timer that kept [`Shared`] alive past `serve` teardown would
/// also keep the writer `sender` open and stop the process from exiting. The
/// upgrade at expiry doubles as the shutdown check.
fn arm_draft_open_timeout(shared: &Arc<Shared>, run_id: String) {
    let runtime = shared
        .runtime
        .clone()
        .expect("Plain transport owns a Tokio runtime");
    let delay = draft_open_timeout();
    let shared = Arc::downgrade(shared);
    runtime.spawn(async move {
        tokio::time::sleep(delay).await;
        let Some(shared) = shared.upgrade() else {
            return;
        };
        if shared.closing.load(Ordering::SeqCst) || !editor::registry().expire_unopened(&run_id) {
            return;
        }
        deliver_plain_effects(
            &shared,
            vec![Effect::message(
                MessageLevel::Warn,
                "The commit draft was not opened in time; the commit was canceled. Retry to reopen it.",
            )],
        );
    });
}

/// Deliver effects to a Plain client through standard LSP notifications only.
///
/// Plain clients never receive the custom `view/effects` channel: messages
/// become `window/showMessage` and an edit document is re-presented through
/// `window/showDocument` (or manual instructions) as declared.
pub(super) fn deliver_plain_effects(shared: &Arc<Shared>, effects: Vec<Effect>) {
    let (capabilities, encoding) = {
        let client = shared.client.lock().unwrap_or_else(|p| p.into_inner());
        let capabilities = match client.presentation.as_ref() {
            Some(Presentation::Plain(capabilities)) => capabilities.clone(),
            _ => PlainCapabilities::default(),
        };
        (capabilities, client.encoding)
    };
    for effect in effects {
        match effect {
            Effect::EditDocument {
                run_id,
                uri,
                text,
                title,
                kind,
                ..
            } => {
                send_plain_edit_document(
                    Shared::clone(shared),
                    run_id.clone(),
                    uri,
                    text,
                    title,
                    capabilities.clone(),
                    encoding,
                );
                if kind == editor::COMMIT_MESSAGE_KIND {
                    arm_draft_open_timeout(shared, run_id.clone());
                }
                let repo = shared
                    .progress
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .values()
                    .find(|run| run.edit_id.as_deref() == Some(&run_id))
                    .and_then(|run| run.repo.clone());
                if let Some(repo) = repo {
                    let shared = shared.clone();
                    // The emitter that delivers these effects runs inside
                    // `handle_edit` while `editor::registry().lock()` is held, so
                    // the refresh must run off the emitting thread: it re-enters
                    // that lock through `EditRegistry::pending_for` and would
                    // self-deadlock inline. This detached thread only reads
                    // registry state and delivers renders through the sender.
                    std::thread::spawn(move || match refresh_pending_views(&shared, &repo) {
                        Ok(renders) => {
                            for render in renders {
                                if let Some(message) = deliver_render(&shared, render) {
                                    let _ = shared.sender.send(message);
                                }
                            }
                        }
                        Err(error) => report_pending_refresh_error(&shared, error),
                    });
                }
            }
            message @ Effect::Message { .. } => {
                for notification in effect_notifications(&shared.client, vec![message]) {
                    let _ = shared.sender.send(notification);
                }
            }
            _ => {}
        }
    }
}

fn send_plain_edit_document(
    shared: Shared,
    run_id: String,
    uri: String,
    text: String,
    title: String,
    capabilities: PlainCapabilities,
    encoding: Encoding,
) {
    if shared.closing.load(Ordering::SeqCst) {
        editor::registry().resolve(&run_id, editor::Reply::Abort);
        return;
    }
    // An already-open buffer is replaced with the new template by a versioned
    // whole-document edit. The version comes from the document store so the
    // client can reject the edit if its buffer has moved independently.
    let apply = if capabilities.apply_edit && capabilities.document_changes {
        shared
            .documents
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&uri)
            .map(|document| {
                whole_document_edit(
                    &uri,
                    document.version,
                    &document.text,
                    &text,
                    encoding,
                )
            })
    } else {
        None
    }
    .and_then(|edit| {
        shared
            .register_outbound("workspace/applyEdit", serde_json::json!({ "edit": edit }))
            .ok()
    });
    let runtime = shared
        .runtime
        .clone()
        .expect("editor bridge runs inside the server runtime");
    // Every delivered draft document replaces the previous delivery as the one
    // whose outcome may still act: an older `applyEdit` reply that arrives after
    // a `didOpen`, a confirmation or a newer delivery is merely stale.
    let delivery = editor::registry().begin_delivery(&run_id);
    if let Some((apply_pending, apply_receiver)) = apply {
        let shared = shared.clone();
        let uri = uri.clone();
        runtime.spawn(async move {
            let applied = shared
                .await_outbound_for(apply_pending, apply_receiver, draft_delivery_timeout())
                .await
                .ok()
                .and_then(|response| response.response_result.ok())
                .and_then(|value| value.get("applied").and_then(Value::as_bool))
                .unwrap_or(false);
            if applied || shared.closing.load(Ordering::SeqCst) {
                return;
            }
            // A rejected, failed or unanswered template leaves the client's
            // buffer stale, so its later didOpen would be refused as a version
            // mismatch. Cancel the run and say why instead of assuming delivery,
            // but only while this exact delivery is still the current unopened
            // one: the lifecycle lock owns that decision, never a bare run id.
            if editor::registry().abort_unopened_delivery(&run_id, delivery) {
                let _ = shared.sender.send(show_message(
                    2,
                    format!(
                        "magit-lsp: the commit draft template could not be applied to the open {uri} buffer; close and reopen the draft, then retry the commit."
                    ),
                ));
            }
        });
    }
    if !capabilities.show_document {
        send_manual_edit_instructions(&shared, &uri, &title);
        return;
    }
    let Ok((pending, receiver)) = shared.register_outbound(
        "window/showDocument",
        serde_json::json!({"uri":uri,"external":false,"takeFocus":true}),
    ) else {
        if !shared.closing.load(Ordering::SeqCst) {
            send_manual_edit_instructions(&shared, &uri, &title);
        }
        return;
    };
    runtime.spawn(async move {
        let opened = shared
            .await_outbound(pending, receiver)
            .await
            .ok()
            .and_then(|response| response.response_result.ok())
            .and_then(|result| result.get("success").and_then(Value::as_bool))
            == Some(true);
        if !opened && !shared.closing.load(Ordering::SeqCst) {
            send_manual_edit_instructions(&shared, &uri, &title);
        }
    });
}

fn send_manual_edit_instructions(shared: &Shared, uri: &str, title: &str) {
    if shared.closing.load(Ordering::SeqCst) {
        return;
    }
    let message = format!(
        "magit-lsp: Open {uri} to edit {title}, then use the Confirm commit or Abort commit code action."
    );
    let _ = shared.sender.send(show_message(3, message));
}

/// Feed an authoritative logical-view render into its Plain document, if any.
pub(super) fn deliver_render(shared: &Shared, render: ViewRender) -> Option<Message> {
    let snapshot = shared
        .published
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .values()
        .find_map(|views| views.current_snapshot(&render.view_id));
    if route_plain_render(shared, &render.view_id, snapshot) {
        None
    } else {
        Some(custom_ui(&shared.client).render(render))
    }
}

/// Feed an authoritative logical-view render into its Plain document, if any.
pub(super) fn route_plain_render(
    shared: &Shared,
    view_id: &str,
    snapshot: Option<(crate::tree::Tree, u64, String, Option<crate::menu::Menu>)>,
) -> bool {
    if shared.client.lock().unwrap_or_else(|p| p.into_inner()).plain.is_none() {
        return false;
    }
    let _plain = shared
        .plain_lifecycle
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if shared.closing.load(Ordering::SeqCst) {
        return true;
    }
    let request = {
        let mut documents = shared
            .plain_documents
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let Some((uri, entry)) = documents
            .iter_mut()
            .find(|(_, entry)| entry.view_id == view_id)
        else {
            // The inactive views still belong to this Plain session. A watcher
            // refresh must not turn their absence from the active document map
            // into permission to send the rich protocol.
            return true;
        };
        snapshot.and_then(|(mut tree, generation, scope, menu)| {
            if entry.key_state.classifier == crate::presentation::keys::State::Executing {
                if let Some(document) = shared.documents.lock().unwrap_or_else(|p| p.into_inner()).get(uri) {
                    entry.controller.observe(document.version, document.text.clone());
                }
            }
            if !matches!(scope.as_str(), "prompt" | "log.select" | "commit-message") {
                append_plain_keys_header(shared, &entry.repo, entry.keys_override, &mut tree);
            }
            entry
                .controller
                .render_actions(tree, view_id.to_owned(), generation, scope, menu)
                .map(|request| (entry.incarnation, uri.clone(), request))
        })
    };
    if let Some((incarnation, uri, request)) = request {
        send_plain_edit(shared, uri, incarnation, request);
    }
    true
}

/// Send one controller edit and reconcile its correlated standard-LSP reply.
pub(super) fn send_plain_edit(shared: &Shared, uri: String, incarnation: u64, edit: ApplyEdit) {
    if shared.closing.load(Ordering::SeqCst)
        || !plain_incarnation_is_current(shared, &uri, incarnation)
    {
        return;
    }
    let echo = (|| {
        let mut document = shared.documents.lock().unwrap_or_else(|p| p.into_inner()).get(&uri)?.clone();
        let version = document.version.checked_add(1)?;
        let changes = edit.edit.get("documentChanges").and_then(Value::as_array).and_then(|changes| changes.first());
        let edits = if let Some(change) = changes {
            if change.pointer("/textDocument/version")?.as_i64()? != i64::from(document.version) { return None; }
            change.get("edits")?
        } else {
            edit.edit.get("changes")?.get(&uri)?
        };
        let mut edits: Vec<lsp_types::TextEdit> = serde_json::from_value(edits.clone()).ok()?;
        edits.sort_by_key(|edit| std::cmp::Reverse((edit.range.start.line, edit.range.start.character)));
        let changes: Vec<_> = edits.into_iter().map(|edit| crate::docstore::Change { start: Some((edit.range.start.line, edit.range.start.character)), end: Some((edit.range.end.line, edit.range.end.character)), text: edit.new_text }).collect();
        let encoding = shared.client.lock().unwrap_or_else(|p| p.into_inner()).encoding;
        document.change(version, &changes, encoding).ok()?;
        Some((edit.id, version, crate::presentation::plain::hash_text(&document.text)))
    })();
    if let Some(entry) = shared.plain_documents.lock().unwrap_or_else(|p| p.into_inner()).get_mut(&uri).filter(|entry| entry.incarnation == incarnation) {
        entry.key_state.repair_echo = echo;
    }
    let Ok((pending, receiver)) = shared.register_outbound(
        "workspace/applyEdit",
        serde_json::json!({ "edit": edit.edit }),
    ) else {
        return;
    };
    let shared = shared.clone();
    let runtime = shared
        .runtime
        .clone()
        .expect("Plain transport owns a Tokio runtime");
    runtime.spawn(async move {
        let applied = shared.await_outbound(pending, receiver).await.ok()
            .and_then(|response| response.response_result.ok())
            .and_then(|value| value.get("applied").and_then(Value::as_bool))
            .unwrap_or(false);
        let _plain = shared.plain_lifecycle.lock().unwrap_or_else(|p| p.into_inner());
        let next = {
            let mut documents = shared.plain_documents.lock().unwrap_or_else(|p| p.into_inner());
            documents.get_mut(&uri).filter(|entry| entry.incarnation == incarnation).map(|entry| {
                if !applied && entry.key_state.repair_echo.is_some_and(|(id, _, _)| id == edit.id) {
                    entry.key_state.repair_echo = None;
                }
                let next = entry.controller.response(edit.id, applied);
                let warning = entry.controller.take_format_warning();
                (next, warning)
            })
        };
        if let Some((next, warning)) = next {
            if warning { let _ = shared.sender.send(show_message(2, "magit-lsp: the editor keeps changing the presentation file's line endings or final newline; close and reopen the file to resynchronize".into())); }
            if let Some(next) = next { send_plain_edit(&shared, uri.clone(), incarnation, next); }
            if applied {
                let shared = shared.clone();
                let uri = uri.clone();
                let edit_id = edit.id;
                let runtime = shared.runtime.clone().expect("Plain transport owns a Tokio runtime");
                runtime.spawn(async move {
                    // Give the client one second to echo an accepted edit; late callbacks are harmless because expiry correlates both flight ID and URI incarnation.
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    let _plain = shared.plain_lifecycle.lock().unwrap_or_else(|p| p.into_inner());
                    if shared.closing.load(Ordering::SeqCst) || !plain_incarnation_is_current(&shared, &uri, incarnation) { return; }
                    let next = shared.plain_documents.lock().unwrap_or_else(|p| p.into_inner())
                        .get_mut(&uri).filter(|entry| entry.incarnation == incarnation)
                        .and_then(|entry| entry.controller.expire(edit_id));
                    if let Some(next) = next { send_plain_edit(&shared, uri, incarnation, next); }
                });
            }
        }
    });
}
