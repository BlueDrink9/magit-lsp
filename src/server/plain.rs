//! Plain document render delivery and lifecycle.

use super::*;

/// Start an asynchronous bootstrap after the didOpen notification is accepted.
pub(super) fn schedule_plain_open(shared: &Shared, uri: String) -> Option<(String, String)> {
    let format = {
        let documents = shared.documents.lock().unwrap_or_else(|p| p.into_inner());
        let document = documents.get(&uri)?;
        DocumentFormat::from_text(&document.text)
    };
    let path = crate::file_uri::path_from_uri(&uri)?;
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
    let Some(path) = crate::file_uri::path_from_uri(&uri) else {
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
    let lock = repository_lock(&shared.repo_locks, &repo).await;
    let guard = lock.lock_owned().await;
    let shared = shared.clone();
    let _ = tokio::task::spawn_blocking(move || {
        let _guard = guard;
        bootstrap_plain_view(&shared, uri, incarnation, format, caps, encoding, repo);
    })
    .await;
}

/// Open and publish a Plain view while holding the repository execution lock.
fn bootstrap_plain_view(
    shared: &Shared,
    uri: String,
    incarnation: u64,
    format: DocumentFormat,
    caps: PlainCapabilities,
    encoding: Encoding,
    repo: String,
) {
    if shared.closing.load(Ordering::SeqCst)
        || !plain_incarnation_is_current(shared, &uri, incarnation)
    {
        return;
    }
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
                    key_executing: false,
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
    // Arm the watcher on a deferred blocking task so the recursive inotify walk
    // leaves the first-render critical path and the repository execution lock.
    // `ensure_watcher` emits its own phase, so the trace id has to be installed
    // on this blocking thread, not around the spawn call.
    let trace = crate::runner::current_trace_id();
    let shared_clone = shared.clone();
    let repo_clone = repo.clone();
    drop(tokio::task::spawn_blocking(move || {
        crate::runner::with_trace_id(trace, move || shared_clone.ensure_watcher(&repo_clone))
    }));
    if let Some(edit) = first {
        send_plain_edit(shared, uri, incarnation, edit);
    }
}

pub(super) fn plain_keys_effective(
    shared: &Shared,
    repo: &str,
    document: Option<bool>,
) -> crate::config::EffectivePlainKeys {
    let client = shared.client.lock().unwrap_or_else(|p| p.into_inner());
    crate::config::resolve_plain_keys(
        &shared
            .config
            .for_repository(std::path::Path::new(repo))
            .config()
            .plain,
        client.client_name.as_deref(),
        document,
        client.plain_keys_allowed,
    )
}

pub(super) fn append_plain_keys_header(
    shared: &Shared,
    repo: &str,
    document: Option<bool>,
    tree: &mut crate::tree::Tree,
) {
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
        shared
            .client
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .presentation,
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
    let effects = errors::effects_for_result(Err(error))
        .unwrap_or_else(|error| vec![Effect::message(MessageLevel::Error, error.to_string())]);
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
    let snapshot = snapshot?;

    // The policy header is a structured node. If the client edits that row,
    // restore the retained authoritative tree without interpreting document text.
    let header_repair = {
        let mut documents = shared
            .plain_documents
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        documents.get_mut(uri).and_then(|entry| {
            let (tree, view_id, generation, scope, menu) = entry.controller.latest_action_meta()?;
            let node = tree.node("plain-keys-policy")?;
            if snapshot.text.lines().nth(node.start_line) == Some(node.label.as_str()) {
                return None;
            }
            let (tree, view_id, generation, scope, menu) = (
                tree.clone(),
                view_id.to_owned(),
                generation,
                scope.to_owned(),
                menu.cloned(),
            );
            entry
                .controller
                .observe(snapshot.version, snapshot.text.clone());
            entry
                .controller
                .render_actions(tree, view_id, generation, scope, menu)
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
                            entry
                                .controller
                                .observe(snapshot.version, snapshot.text.clone());
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
                        entry
                            .controller
                            .adopt(snapshot.version, snapshot.text.clone());
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
#[derive(Clone)]
pub(super) enum PlainKeyDispatch {
    /// Run one key step through the existing shared session key handler.
    Key {
        uri: String,
        repo: String,
        incarnation: u64,
        params: crate::protocol::ViewKeyParams,
    },
    /// Discard a stale edit and render the session's current state.
    Refresh {
        uri: String,
        repo: String,
        incarnation: u64,
    },
}

impl PlainKeyDispatch {
    /// Repository whose execution lock and mutation turn own this dispatch.
    pub(super) fn repo(&self) -> &str {
        match self {
            Self::Key { repo, .. } | Self::Refresh { repo, .. } => repo,
        }
    }

    fn identity(&self) -> (&str, u64) {
        match self {
            Self::Key {
                uri, incarnation, ..
            }
            | Self::Refresh {
                uri, incarnation, ..
            } => (uri, *incarnation),
        }
    }
}

/// Classify one accepted Plain edit as a key against its matched retained snapshot.
fn plain_key_step(
    shared: &Shared,
    uri: &str,
    change: &super::drafts::AcceptedChange,
    snapshot: &crate::docstore::Document,
) -> PlainKeyStep {
    use crate::presentation::keys::{self, State};
    let encoding = shared
        .client
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .encoding;
    let mut coalesced = None;
    let (repo, incarnation, view_id, generation, prior, mut state) = {
        let mut documents = shared
            .plain_documents
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let Some(entry) = documents.get_mut(uri) else {
            return PlainKeyStep::Continue;
        };
        // Incremental clients may debounce the applyEdit echo and the next
        // user edit into one notification. Split only at the exact outstanding
        // echo, not at an arbitrary retained render or a guessed text diff.
        if let Some((_, version, hash)) = entry.key_state.repair_echo {
            if change.pre.version < version && version < change.post_version {
                for count in 1..change.changes.len() {
                    let mut pre = change.pre.clone();
                    if pre
                        .change(version, &change.changes[..count], encoding)
                        .is_ok()
                        && crate::presentation::plain::hash_text(&pre.text) == hash
                    {
                        entry.controller.observe(version, pre.text.clone());
                        entry.key_state.repair_echo = None;
                        if !entry.key_executing {
                            entry.key_state.classifier = State::Ready;
                        }
                        coalesced = Some(super::drafts::AcceptedChange {
                            pre,
                            changes: change.changes[count..].to_vec(),
                            post_version: change.post_version,
                            post_text: change.post_text.clone(),
                        });
                        break;
                    }
                }
            }
        }
        let change = coalesced.as_ref().unwrap_or(change);
        // Recognize only the exact version/hash produced by the outstanding
        // repair, before treating its mutated pre-text as fresh key input.
        if entry
            .key_state
            .repair_echo
            .is_some_and(|(_, version, hash)| {
                version == change.post_version
                    && hash == crate::presentation::plain::hash_text(&change.post_text)
            })
        {
            entry.key_state.repair_echo = None;
            if !entry.key_executing {
                entry.key_state.classifier = State::Ready;
            }
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
            entry.incarnation,
            view_id.to_owned(),
            generation,
            state.classifier.clone(),
            state,
        )
    };
    let change = coalesced.as_ref().unwrap_or(change);
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
        State::PendingDeletion { edit, .. } => {
            edit.pre_range
                .map(|((start, _), (end, col))| crate::protocol::Selection {
                    start_line: start as usize,
                    end_line: keys::inclusive_lines(start, end, col).1 as usize,
                })
        }
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
                    entry
                        .controller
                        .observe(snapshot.version, snapshot.text.clone());
                }
                keys::Decision::Dispatch(_) | keys::Decision::Repair | keys::Decision::DropBusy => {
                }
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
            entry.key_executing = true;
            entry.key_state.classifier = State::Executing;
        }
        return PlainKeyStep::Dispatch(PlainKeyDispatch::Refresh {
            uri: uri.to_owned(),
            repo,
            incarnation,
        });
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
    if let Some(entry) = shared
        .plain_documents
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get_mut(uri)
    {
        entry.key_executing = true;
        entry.key_state.classifier = State::Executing;
        entry.key_state.repair_due = false;
    }
    PlainKeyStep::Dispatch(PlainKeyDispatch::Key {
        uri: uri.to_owned(),
        repo,
        incarnation,
        params,
    })
}

/// Run a tracked Plain handler in repository order, off the transport reader.
pub(super) async fn handle_plain_key(
    shared: Shared,
    dispatch: PlainKeyDispatch,
    turn: MutationTurn,
) {
    turn.wait().await;
    let lock = repository_lock(&shared.repo_locks, dispatch.repo()).await;
    let _guard = lock.lock().await;
    let (token, cancel) =
        shared.begin_progress(&shared.sender, "view/key", None, Some(dispatch.repo()));
    let worker_shared = shared.clone();
    let worker_dispatch = dispatch.clone();
    let result = tokio::task::spawn_blocking(move || {
        runner::with_cancel(Some(cancel), || {
            execute_plain_key(&worker_shared, worker_dispatch)
        })
    })
    .await;
    let error = match result {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(error.to_string()),
        Err(error) => Some(format!("Plain key worker failed: {error}")),
    };
    if let Some(error) = error {
        let _ = shared.sender.send(show_message(
            1,
            format!("Cannot execute Plain key: {error}"),
        ));
    }
    let (uri, incarnation) = dispatch.identity();
    repair_plain_authoritative(&shared, uri, incarnation);
    {
        let _lifecycle = shared
            .plain_lifecycle
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if let Some(entry) = shared
            .plain_documents
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get_mut(uri)
            .filter(|entry| entry.incarnation == incarnation)
        {
            entry.key_executing = false;
            entry.key_state.repair_due = false;
            if entry.key_state.repair_echo.is_none() {
                entry.key_state.classifier = crate::presentation::keys::State::Ready;
            }
        }
    }
    shared.end_progress(&shared.sender, &token);
    drop(turn);
}

/// Execute one admitted Plain key with no lifecycle guard held during Git/render delivery.
fn execute_plain_key(shared: &Shared, dispatch: PlainKeyDispatch) -> Result<()> {
    let (uri, incarnation) = dispatch.identity();
    if shared.closing.load(Ordering::SeqCst)
        || !plain_incarnation_is_current(shared, uri, incarnation)
    {
        return Ok(());
    }
    match dispatch {
        PlainKeyDispatch::Refresh {
            uri,
            repo,
            incarnation,
        } => {
            let view_id = shared
                .plain_documents
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get(&uri)
                .filter(|entry| entry.incarnation == incarnation && entry.repo == repo)
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
                    route_plain_render(shared, &view_id, Some(snapshot), None);
                }
                _ => repair_plain_authoritative(shared, &uri, incarnation),
            }
            Ok(())
        }
        PlainKeyDispatch::Key {
            uri,
            repo,
            incarnation,
            params,
        } => {
            let handle = shared
                .session(&repo)
                .ok_or_else(|| Error::new("Plain key session closed"))?;
            // Session ownership serializes admission with execution; lifecycle
            // ownership is needed only to validate the live URI, never for Git.
            let step = {
                let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
                let lifecycle = shared
                    .plain_lifecycle
                    .lock()
                    .unwrap_or_else(|p| p.into_inner());
                let eligible = shared
                    .plain_documents
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .get(&uri)
                    .is_some_and(|entry| {
                        entry.incarnation == incarnation
                            && entry.repo == repo
                            && entry.view_id == params.view_id
                            && plain_keys_effective(shared, &repo, entry.keys_override).enabled
                            && entry.controller.latest_action_meta().is_some_and(
                                |(_, view_id, _, scope, _)| {
                                    view_id == params.view_id
                                        && !matches!(
                                            scope,
                                            "prompt" | "log.select" | "commit-message"
                                        )
                                },
                            )
                    });
                if !eligible
                    || !plain_incarnation_is_current(shared, &uri, incarnation)
                    || shared.closing.load(Ordering::SeqCst)
                {
                    return Ok(());
                }
                let current = shared
                    .published
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .values()
                    .find_map(|views| {
                        views
                            .current_snapshot(&params.view_id)
                            .map(|(_, version, _, _)| version)
                    });
                if current != Some(params.version) {
                    let _ = shared.sender.send(show_message(2, PLAIN_KEY_STALE.into()));
                    return Ok(());
                }
                session.set_keymaps(shared.config.for_repository(Path::new(&repo)).keymaps());
                use crate::presentation::keys::KeyPreflight;
                match session.preflight_key(&params)? {
                    KeyPreflight::Pending | KeyPreflight::Allowed { .. } => {}
                    KeyPreflight::Unbound => {
                        let _ = shared.sender.send(show_message(
                            2,
                            "key is not bound in the active profile".into(),
                        ));
                        return Ok(());
                    }
                    KeyPreflight::Refused { message, .. } => {
                        let _ = shared.sender.send(show_message(2, message));
                        return Ok(());
                    }
                }
                // Admission must match `key_view`: cancellation cleanup and a
                // detached worker's mutation authority both refuse before any
                // command starts, leaving the typed text to the authoritative repair.
                if let Some(message) =
                    super::view_execution::key_admission_refusal(shared, &repo, || {
                        session.resolved_key_command(&params)
                    })?
                {
                    let _ = shared.sender.send(show_message(2, message.into()));
                    return Ok(());
                }
                drop(lifecycle);
                session.start_key(&params)?
            };
            let outcome = super::view_execution::drive_execution(shared, &repo, step)?;
            if !plain_incarnation_is_current(shared, &uri, incarnation) {
                return Ok(());
            }
            let dispatched = super::view_execution::dispatched_outcome(shared, &repo, outcome)?;
            for notification in dispatched.notifications {
                let _ = shared.sender.send(notification);
            }
            Ok(())
        }
    }
}

/// Repair one Plain document to its newest retained authoritative render.
fn repair_plain_authoritative(shared: &Shared, uri: &str, incarnation: u64) {
    let _lifecycle = shared
        .plain_lifecycle
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if !plain_incarnation_is_current(shared, uri, incarnation) {
        return;
    }
    let (text, version) = {
        let documents = shared.documents.lock().unwrap_or_else(|p| p.into_inner());
        let Some(document) = documents.get(uri).filter(|document| !document.repair) else {
            return;
        };
        (document.text.clone(), document.version)
    };
    let mut documents = shared
        .plain_documents
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let Some(entry) = documents
        .get_mut(uri)
        .filter(|entry| entry.incarnation == incarnation)
    else {
        return;
    };
    // An outstanding echo suppresses a repair only while the buffer still holds
    // that edit's authoritative text. A refused key leaves different text behind,
    // so the repair must run even while the previous push is unacknowledged.
    if entry
        .key_state
        .repair_echo
        .is_some_and(|(_, _, hash)| hash == crate::presentation::plain::hash_text(&text))
    {
        return;
    }
    let Some((tree, view_id, generation, scope, menu)) = entry.controller.latest_action_meta()
    else {
        return;
    };
    let (tree, view_id, generation, scope, menu) = (
        tree.clone(),
        view_id.to_owned(),
        generation,
        scope.to_owned(),
        menu.cloned(),
    );
    entry.controller.observe(version, text);
    let edit = entry
        .controller
        .render_actions(tree, view_id, generation, scope, menu);
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
pub(super) fn reconcile_rich_prompt(
    shared: &Shared,
    uri: &str,
    sender: &mpsc::UnboundedSender<Message>,
) {
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

pub(super) fn close_plain(shared: &Shared, uri: &str) -> Option<(String, String)> {
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
    let mut entry = entry?;
    entry.controller.close();
    shared
        .view_repos
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(&entry.view_id);
    Some((entry.repo, entry.view_id))
}

/// Deliver the renders and effects a closed Plain view resumed.
///
/// Callers must drop `plain_lifecycle` first: routing these notifications
/// reacquires it.
pub(super) fn deliver_plain_close_outcome(shared: &Shared, close: Option<(String, String)>) {
    let Some((repo, view_id)) = close else {
        return;
    };
    let shared = shared.clone();
    tokio::spawn(async move {
        let lock = repository_lock(&shared.repo_locks, &repo).await;
        let guard = lock.lock_owned().await;
        let _ = tokio::task::spawn_blocking(move || {
            let _guard = guard;
            let outcome = shared.session(&repo).and_then(|session| {
                session
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .close(&view_id)
                    .ok()
            });
            if let Some(outcome) = outcome {
                // A replacement document owns its own focus/render lifecycle.
                // Closing the old logical view must not redirect that document.
                let replacement = shared
                    .plain_documents
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .values()
                    .any(|entry| entry.repo == repo);
                if !replacement {
                    match dispatched_outcome(&shared, &repo, outcome) {
                        Ok(dispatched) => {
                            for notification in dispatched.notifications {
                                let _ = shared.sender.send(notification);
                            }
                        }
                        Err(error) => {
                            tracing::warn!(%error, "failed to deliver a Plain view close outcome")
                        }
                    }
                }
            }
            shared.stop_watcher_if_idle(&repo);
        })
        .await;
    });
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
                // Pending-draft rows arrive with explicit renders and run
                // outcomes, which append them from the registry. Do not push an
                // extra detached render while delivering the template: that
                // unsolicited frame races the commit handshake and consumes
                // outbound ids between `window/showDocument` and confirmation.
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
                whole_document_edit(&uri, document.version, &document.text, &text, encoding)
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
    if route_plain_render(shared, &render.view_id, snapshot, render.cursor) {
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
    cursor: Option<Cursor>,
) -> bool {
    if shared
        .client
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .plain
        .is_none()
    {
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
                if let Some(document) = shared
                    .documents
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .get(uri)
                {
                    entry
                        .controller
                        .observe(document.version, document.text.clone());
                }
            }
            if !matches!(scope.as_str(), "prompt" | "log.select" | "commit-message") {
                append_plain_keys_header(shared, &entry.repo, entry.keys_override, &mut tree);
            }
            let entering = entry
                .controller
                .latest_action_meta()
                .is_some_and(|(_, previous, _, _, _)| previous != view_id);
            let selection = cursor.filter(|_| entering).and_then(|cursor| {
                // The selection includes the server-owned heading and prompt,
                // not just the first commit, which may leave both off-screen.
                let heading = tree.node("log:header")?;
                tree.node("log:prompt")?;
                Some(lsp_types::Range::new(
                    lsp_types::Position::new(heading.start_line as u32, 0),
                    lsp_types::Position::new(cursor.line as u32, cursor.column as u32),
                ))
            });
            let request =
                entry
                    .controller
                    .render_actions(tree, view_id.to_owned(), generation, scope, menu);
            if let Some(selection) = selection {
                entry.controller.navigate_after_edit(selection);
            }
            request.map(|request| (entry.incarnation, uri.clone(), request))
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
        let mut document = shared
            .documents
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&uri)?
            .clone();
        let version = document.version.checked_add(1)?;
        let changes = edit
            .edit
            .get("documentChanges")
            .and_then(Value::as_array)
            .and_then(|changes| changes.first());
        let edits = if let Some(change) = changes {
            if change.pointer("/textDocument/version")?.as_i64()? != i64::from(document.version) {
                return None;
            }
            change.get("edits")?
        } else {
            edit.edit.get("changes")?.get(&uri)?
        };
        let mut edits: Vec<lsp_types::TextEdit> = serde_json::from_value(edits.clone()).ok()?;
        edits.sort_by_key(|edit| {
            std::cmp::Reverse((edit.range.start.line, edit.range.start.character))
        });
        let changes: Vec<_> = edits
            .into_iter()
            .map(|edit| crate::docstore::Change {
                start: Some((edit.range.start.line, edit.range.start.character)),
                end: Some((edit.range.end.line, edit.range.end.character)),
                text: edit.new_text,
            })
            .collect();
        let encoding = shared
            .client
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .encoding;
        document.change(version, &changes, encoding).ok()?;
        Some((
            edit.id,
            version,
            crate::presentation::plain::hash_text(&document.text),
        ))
    })();
    if let Some(entry) = shared
        .plain_documents
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get_mut(&uri)
        .filter(|entry| entry.incarnation == incarnation)
    {
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
                    if !entry.key_executing { entry.key_state.classifier = crate::presentation::keys::State::Ready; }
                }
                let navigation = entry.controller.take_navigation(edit.id, applied);
                let next = entry.controller.response(edit.id, applied);
                let warning = entry.controller.take_format_warning();
                (next, warning, navigation)
            })
        };
        if let Some((next, warning, navigation)) = next {
            if warning { let _ = shared.sender.send(show_message(2, "magit-lsp: the editor keeps changing the presentation file's line endings or final newline; close and reopen the file to resynchronize".into())); }
            if let Some(next) = next { send_plain_edit(&shared, uri.clone(), incarnation, next); }
            if let Some(selection) = navigation {
                let supported = shared.client.lock().unwrap_or_else(|p| p.into_inner())
                    .plain.as_ref().is_some_and(|caps| caps.show_document);
                if supported && !shared.closing.load(Ordering::SeqCst) {
                    if let Ok((pending, receiver)) = shared.register_outbound("window/showDocument",
                        serde_json::json!({"uri":uri,"external":false,"takeFocus":true,"selection":selection})) {
                        let shared = shared.clone();
                        let runtime = shared.runtime.clone().expect("Plain transport owns a Tokio runtime");
                        runtime.spawn(async move { let _ = shared.await_outbound(pending, receiver).await; });
                    }
                }
            }
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
                        .and_then(|entry| {
                            let next = entry.controller.expire(edit_id);
                            if entry.key_state.repair_echo.is_some_and(|(id, _, _)| id == edit_id) {
                                entry.key_state.repair_echo = None;
                                if !entry.key_executing { entry.key_state.classifier = crate::presentation::keys::State::Ready; }
                            }
                            next
                        });
                    if let Some(next) = next { send_plain_edit(&shared, uri, incarnation, next); }
                });
            }
        }
    });
}
