use super::requests::DeferredStatus;
use super::*;

/// Worker-owned same-repository mutation reservation.
pub(super) struct DeferredReservation {
    pub(super) repositories: Arc<Mutex<HashSet<String>>>,
    pub(super) repo: String,
}

impl Drop for DeferredReservation {
    fn drop(&mut self) {
        self.repositories
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&self.repo);
    }
}

pub(super) fn open_view(shared: &Shared, params: Value) -> Result<Dispatched> {
    let total = std::time::Instant::now();
    let p: ViewOpenParams = from_params(params)?;
    let repo = p
        .repo_path()
        .ok_or_else(|| Error::new("view/open needs args.repo_path for a status view"))?
        .to_string();
    let discovery = std::time::Instant::now();
    let (discovered_repo, branch) = crate::git::GitRepo::discover_with_branch(&repo)?
        .ok_or_else(|| Error::new("This folder is not inside a Git repository."))?;
    crate::runner::trace_current(
        "phase_repository_discovery",
        serde_json::json!({ "duration_ms": discovery.elapsed().as_secs_f64() * 1000.0 }),
    );
    let repo = crate::path::canonical(discovered_repo.root())?
        .to_string_lossy()
        .into_owned();
    let id = shared.next_view_id();
    let config = shared.config.for_repository(std::path::Path::new(&repo));
    let keymaps = config.keymaps();
    let handle = match shared.session(&repo) {
        Some(handle) => handle,
        None => {
            // Repository discovery/initialisation must not own the registry.
            let draft_owner = matches!(
                shared
                    .client
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .presentation,
                Some(Presentation::Plain(_))
            )
            .then(|| shared.draft_owner.as_ref().clone());
            let candidate = Arc::new(Mutex::new(Session::new_with_draft_owner(
                &repo,
                draft_owner,
            )));
            shared
                .sessions
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .entry(repo.clone())
                .or_insert(candidate)
                .clone()
        }
    };
    let session_start = std::time::Instant::now();
    let is_status = matches!(&p.kind, ViewKind::Status);
    // The opening discovery already resolved the repository identities, so the
    // deferred acquisition clones this handle instead of discovering again.
    let acquisition_repo = is_status.then(|| discovered_repo.clone());
    let (descriptor, render, keymap) = {
        let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
        let rich = match shared
            .client
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .presentation
            .as_ref()
        {
            Some(Presentation::Rich(caps)) => Some(caps.clone()),
            _ => None,
        };
        session.set_rich_capabilities(rich);
        shared
            .published
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entry(repo.clone())
            .or_insert_with(|| session.published());
        session.set_keymaps(keymaps);
        let placement = p.placement.unwrap_or_default();
        let (descriptor, render) = if is_status {
            session.open_status_loading(
                id.clone(),
                placement,
                p.viewport,
                discovered_repo,
                branch,
            )?
        } else {
            session.open_with_discovered_repo(
                id.clone(),
                p.kind,
                placement,
                p.viewport,
                p.args.as_ref(),
                discovered_repo,
            )?
        };
        let keymap = session.keymap_notice(&id);
        (descriptor, render, keymap)
    };
    crate::runner::trace_current(
        "phase_session_open_render",
        serde_json::json!({ "duration_ms": session_start.elapsed().as_secs_f64() * 1000.0 }),
    );
    let repo = repo.as_str();
    // Publish the status begin before the loading render: the client goes idle
    // as soon as it acknowledges that render, so a begin that waited for the
    // create reply could arrive after an early key was already sent.
    let status_progress = shared.begin_status_progress(&shared.sender, repo);
    if let Some(token) = status_progress.as_ref() {
        shared.track_status_progress(&id, token);
    }
    shared
        .view_repos
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(id.clone(), repo.to_string());
    // status opens arm in the deferred acquisition (`requests.rs`) so the early Head render never waits on the recursive inotify walk (`watcher.watch(…, Recursive)` at src/watcher.rs:110, measured 34–55 ms on the large repo).
    if !is_status {
        shared.ensure_watcher(repo);
    }

    let delivery = std::time::Instant::now();
    let mut notifications: Vec<_> = deliver_render(shared, render).into_iter().collect();
    crate::runner::trace_current(
        "phase_render_delivery",
        serde_json::json!({ "duration_ms": delivery.elapsed().as_secs_f64() * 1000.0 }),
    );
    if let Some(error) = config.last_error() {
        notifications.push(show_message(1, format!("magit-lsp config: {error}")));
    }
    if shared.dynamic_keymaps() {
        if let Some(keymap) = keymap {
            notifications.push(Message::Notification(Notification::new(
                VIEW_KEYMAP.to_string(),
                keymap,
            )));
        }
    }
    crate::runner::trace_current(
        "phase_view_open_total",
        serde_json::json!({ "duration_ms": total.elapsed().as_secs_f64() * 1000.0 }),
    );
    let dispatched = Dispatched::with(serde_json::to_value(descriptor)?, notifications);
    if let Some(repository) = acquisition_repo {
        Ok(dispatched.with_deferred(DeferredStatus {
            repo_path: repo.to_string(),
            repository,
            view_id: id,
            generation: 0,
            trace_id: crate::runner::current_trace_id(),
            progress_token: status_progress,
        }))
    } else {
        Ok(dispatched)
    }
}

/// What one acquisition produced once it reached its publication turn.
enum InstallStatus {
    /// The live generation's render, if the view still exists interactive, plus
    /// the deferred opening's background fetch request when there is one.
    Installed {
        render: Option<ViewRender>,
        fetch: Option<crate::status_view::TimedFetch>,
    },
    /// The completed generation was superseded by a coalesced refresh.
    Coalesced(u64),
}

/// Acquire one status generation off-runtime, then publish it in repository order.
///
/// `repository` is the handle discovered by the opening request; every
/// generation reuses a clone of it instead of running discovery again.
pub(super) async fn acquire_status(
    shared: Shared,
    repository: crate::git::GitRepo,
    repo_path: String,
    view_id: String,
    generation: u64,
    trace_id: u64,
    token: Option<String>,
) {
    // The open request published the begin before its loading render, so the
    // client stays non-idle until this acquisition ends the token after its
    // completion render. Clients without progress support pass no token.
    acquire_status_with_token(
        shared, repository, repo_path, view_id, generation, trace_id, token,
    )
    .await;
}

/// Acquire one generation under a caller-owned progress token.
async fn acquire_status_with_token(
    shared: Shared,
    repository: crate::git::GitRepo,
    repo_path: String,
    view_id: String,
    generation: u64,
    trace_id: u64,
    token: Option<String>,
) {
    let acquire_repo = repository.clone();
    let acquired = tokio::task::spawn_blocking(move || {
        crate::runner::with_trace_id(trace_id, || {
            crate::status_view::StatusView::acquire_for_generation(&acquire_repo, generation)
        })
    })
    .await;
    finish_status_acquisition(
        shared, repository, repo_path, view_id, generation, trace_id, acquired, token,
    )
    .await;
}

/// Start the single coalesced follow-up acquisition on its own task.
fn spawn_status_acquisition(
    shared: Shared,
    repository: crate::git::GitRepo,
    repo_path: String,
    view_id: String,
    generation: u64,
    trace_id: u64,
    token: Option<String>,
) {
    tokio::spawn(acquire_status_with_token(
        shared, repository, repo_path, view_id, generation, trace_id, token,
    ));
}

/// Publish an already-acquired status generation through its repository's turn.
///
/// Acquisition happens before this call, so the turn only covers live-generation
/// validation, installation and enqueueing the render. A request ordered after
/// this ticket therefore cannot publish between installation and delivery, and
/// a close ordered before it leaves nothing to install. The blocking install
/// mirrors command dispatch: no Tokio worker waits on the session mutex.
pub(super) async fn finish_status_acquisition(
    shared: Shared,
    repository: crate::git::GitRepo,
    repo_path: String,
    view_id: String,
    generation: u64,
    trace_id: u64,
    acquired: std::result::Result<
        std::result::Result<crate::status_view::FreshStatus, crate::Error>,
        tokio::task::JoinError,
    >,
    token: Option<String>,
) {
    let turn = shared.reserve_mutation(repo_path.clone());
    turn.wait().await;
    let lock = repository_lock(&shared.repo_locks, &repo_path).await;
    let _guard = lock.lock().await;
    let install_shared = shared.clone();
    let install_repo = repo_path.clone();
    let install_view = view_id.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        install_status_acquisition(
            install_shared,
            install_repo,
            install_view,
            generation,
            acquired,
        )
    })
    .await;
    // The coalesced successor inherits the progress token, so the client stays
    // busy across consecutive generations with no idle gap between them.
    let token = match outcome {
        Ok(InstallStatus::Coalesced(next_generation)) => {
            spawn_status_acquisition(
                shared.clone(),
                repository,
                repo_path,
                view_id.clone(),
                next_generation,
                trace_id,
                token,
            );
            None
        }
        Ok(InstallStatus::Installed { render, fetch }) => {
            if let Some(render) = render {
                if let Some(message) = super::plain::deliver_render(&shared, render) {
                    let _ = shared.sender.send(message);
                }
            }
            // The closed render is the section publish; the promise runs from here.
            let published = std::time::Instant::now();
            if let Some(fetch) = fetch {
                spawn_timed_hunks(
                    shared.clone(),
                    repository,
                    repo_path.clone(),
                    view_id.clone(),
                    trace_id,
                    published,
                    fetch,
                );
            }
            token
        }
        Err(error) => {
            tracing::warn!(%error, "status completion install failed");
            token
        }
    };
    if trace_id > 0 {
        crate::runner::trace(
            trace_id,
            "status_completion_processed",
            serde_json::json!({ "view": view_id }),
        );
    }
    // End the token only after the completion render is enqueued (or the
    // install failed), so the client's idle observation stays false until the
    // new state is on its way. This also covers an Installed(None) render where
    // the view was closed or missing, leaving no stuck busy token.
    if let Some(token) = token.as_ref() {
        shared.end_status_progress(&shared.sender, &view_id, token);
    }
    drop(turn);
}

/// Install or coalesce one acquired generation; the caller owns the turn.
fn install_status_acquisition(
    shared: Shared,
    repo_path: String,
    view_id: String,
    generation: u64,
    acquired: std::result::Result<
        std::result::Result<crate::status_view::FreshStatus, crate::Error>,
        tokio::task::JoinError,
    >,
) -> InstallStatus {
    let Some(handle) = shared.session(&repo_path) else {
        return InstallStatus::Installed {
            render: None,
            fetch: None,
        };
    };
    let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(next_generation) = session.take_pending_status_refresh(&view_id, generation) {
        return InstallStatus::Coalesced(next_generation);
    }
    let installed = match acquired {
        Ok(Ok(fresh)) => session.install_fresh_status(&view_id, fresh),
        Ok(Err(error)) => {
            // A terminal acquisition failure still completes this lifecycle: the
            // view stores the error as its state and stays interactive, so no
            // later command is refused on its behalf.
            tracing::warn!(
                %error,
                repo = %repo_path,
                view = %view_id,
                generation,
                "status acquisition failed"
            );
            session
                .fail_fresh_status(
                    &view_id,
                    generation,
                    format!("Unable to load repository status: {error}"),
                )
                .map(|render| (render, None))
        }
        Err(error) => {
            tracing::warn!(
                %error,
                repo = %repo_path,
                view = %view_id,
                generation,
                "status acquisition task failed"
            );
            session
                .fail_fresh_status(
                    &view_id,
                    generation,
                    format!("Unable to load repository status: {error}"),
                )
                .map(|render| (render, None))
        }
    };
    match installed {
        Ok((render, fetch)) => InstallStatus::Installed { render, fetch },
        // Installation failures still complete this acquisition; a later
        // request sees the installed loading/error state, not a stale retry.
        Err(error) => {
            tracing::warn!(%error, "status completion install failed");
            InstallStatus::Installed {
                render: None,
                fetch: None,
            }
        }
    }
}

/// Fetch a deferred opening's hunks in the background, then publish the
/// expanded render through its repository's turn only if the fetch beat the
/// promise deadline. Past the deadline the sections stay closed for on-demand
/// loading; a superseded generation is discarded by the install.
fn spawn_timed_hunks(
    shared: Shared,
    repository: crate::git::GitRepo,
    repo_path: String,
    view_id: String,
    trace_id: u64,
    published: std::time::Instant,
    fetch: crate::status_view::TimedFetch,
) {
    tokio::spawn(async move {
        let fetch_repo = repository.clone();
        let fetched = tokio::task::spawn_blocking(move || {
            crate::runner::with_trace_id(trace_id, || {
                crate::status_view::StatusView::fetch_timed_hunks(&fetch_repo, fetch)
            })
        });
        // Bound the wait by the promise itself: a fetch still running at the
        // deadline is abandoned so the closed render stands.
        let remaining = crate::status_view::OPEN_PROMISE.saturating_sub(published.elapsed());
        let hunks = match tokio::time::timeout(remaining, fetched).await {
            Ok(Ok(hunks)) => hunks,
            _ => return,
        };
        if !crate::status_view::open_within_promise(published, std::time::Instant::now()) {
            return;
        }
        let turn = shared.reserve_mutation(repo_path.clone());
        turn.wait().await;
        let lock = repository_lock(&shared.repo_locks, &repo_path).await;
        let _guard = lock.lock().await;
        let install_shared = shared.clone();
        let install_repo = repo_path.clone();
        let install_view = view_id.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            install_timed_hunks(install_shared, install_repo, install_view, hunks)
        })
        .await;
        match outcome {
            Ok(Some(render)) => {
                if let Some(message) = super::plain::deliver_render(&shared, render) {
                    let _ = shared.sender.send(message);
                }
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "background hunk install failed"),
        }
        drop(turn);
    });
}

/// Install a background hunk fetch into its still-current deferred view.
fn install_timed_hunks(
    shared: Shared,
    repo_path: String,
    view_id: String,
    hunks: crate::status_view::TimedHunks,
) -> Option<ViewRender> {
    let handle = shared.session(&repo_path)?;
    let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
    match session.install_timed_hunks(&view_id, hunks) {
        Ok(render) => render,
        Err(error) => {
            tracing::warn!(%error, "background hunk install failed");
            None
        }
    }
}

pub(super) fn close_view(shared: &Shared, params: Value) -> Result<Dispatched> {
    let p: ViewCloseParams = from_params(params)?;
    let Some(repo) = shared.repo_for_view(&p.view_id) else {
        // The client may have closed the view before its own close landed.
        return Ok(Dispatched::value(Value::Null));
    };
    let Some(handle) = shared.session(&repo) else {
        shared
            .view_repos
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&p.view_id);
        shared.stop_watcher_if_idle(&repo);
        return Ok(Dispatched::value(Value::Null));
    };
    let outcome = handle
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .close(&p.view_id)?;
    // Closing a prompt view or its origin resumes the sibling refreshes it
    // held. Deliver them, but keep the protocol result null.
    let dispatched = dispatched_outcome(shared, &repo, outcome)?;
    shared
        .view_repos
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&p.view_id);
    shared.stop_watcher_if_idle(&repo);
    Ok(Dispatched::with(Value::Null, dispatched.notifications))
}

pub(super) fn invoke_view(shared: &Shared, params: Value) -> Result<Dispatched> {
    let p: ViewInvokeParams = from_params(params)?;
    let repo = shared
        .repo_for_view(&p.view_id)
        .ok_or_else(|| Error::new(format!("unknown view: {}", p.view_id)))?;
    if shared.cancellation_pending(&repo) {
        return cancelling_response();
    }
    if shared.deferred_pending(&repo) && repository_mutation_command(&p.command) {
        return deferred_operation_response();
    }
    let handle = shared
        .session(&repo)
        .ok_or_else(|| Error::new(format!("unknown view: {}", p.view_id)))?;
    let step = handle
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .start_invoke(&p)?;
    let outcome = drive_execution(shared, &repo, step)?;
    Ok(dispatched_outcome(shared, &repo, outcome)?)
}

/// Handle `view/key`: resolve one key step and answer like `view/invoke`.
pub(super) fn key_view(shared: &Shared, params: Value) -> Result<Dispatched> {
    let p: ViewKeyParams = from_params(params)?;
    let repo = shared
        .repo_for_view(&p.view_id)
        .ok_or_else(|| Error::new(format!("unknown view: {}", p.view_id)))?;
    if shared.cancellation_pending(&repo) {
        return cancelling_response();
    }
    let keymaps = shared
        .config
        .for_repository(std::path::Path::new(&repo))
        .keymaps();
    let handle = shared
        .session(&repo)
        .ok_or_else(|| Error::new(format!("unknown view: {}", p.view_id)))?;
    if shared.deferred_pending(&repo)
        && handle
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .resolved_key_command(&p)?
            .is_some_and(|command| repository_mutation_command(&command))
    {
        return deferred_operation_response();
    }
    let (completion, step): (
        Option<crate::session::PromptCompletion>,
        Option<crate::session::Execution>,
    ) = {
        let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
        session.set_keymaps(keymaps);
        let enter = matches!(p.key.as_str(), "\r" | "<CR>" | "Enter");
        if enter {
            if let Some((row_id, text)) =
                session.prompt_key_target(&p.view_id, p.version, p.cursor.line)?
            {
                let completion = session.answer_document_prompt(&row_id, &text)?;
                (Some(completion), None)
            } else {
                (None, Some(session.start_key(&p)?))
            }
        } else {
            (None, Some(session.start_key(&p)?))
        }
    };
    if let Some(crate::session::PromptCompletion { mut result, next }) = completion {
        let next = drive_execution(shared, &repo, next)?;
        result.effects.extend(next.effects);
        result.opened.extend(next.opened);
        result.render = next.render.or(result.render);
        result.keymap = next.keymap.or(result.keymap);
        result.commit = next.commit.or(result.commit);
        return dispatched_outcome(shared, &repo, result);
    }
    let step = step.expect("a key step was prepared");
    let outcome = drive_execution(shared, &repo, step)?;
    Ok(dispatched_outcome(shared, &repo, outcome)?)
}

/// Readable refusal while the prior operation still owns repository cleanup.
const CANCELLING_MESSAGE: &str = "Cancelling… wait for the current Git operation to finish";

/// Readable refusal while a detached worker owns repository mutation authority.
const DEFERRED_MESSAGE: &str = "Wait for the current Git operation to finish";

/// Readable refusal for a key that must not start now, if admission is closed.
///
/// Mirrors `view_key`: cancellation cleanup refuses every key, and a detached
/// worker's mutation authority refuses keys whose resolved command can mutate
/// the repository or index. `resolve_command` is only called when a detached
/// operation is pending, matching `view/key` admission.
pub(super) fn key_admission_refusal(
    shared: &Shared,
    repo: &str,
    resolve_command: impl FnOnce() -> Result<Option<String>>,
) -> Result<Option<&'static str>> {
    if shared.cancellation_pending(repo) {
        return Ok(Some(CANCELLING_MESSAGE));
    }
    if shared.deferred_pending(repo)
        && resolve_command()?.is_some_and(|command| repository_mutation_command(&command))
    {
        return Ok(Some(DEFERRED_MESSAGE));
    }
    Ok(None)
}

/// Readable refusal while the prior operation still owns repository cleanup.
pub(super) fn cancelling_response() -> Result<Dispatched> {
    Ok(Dispatched::value(
        serde_json::json!({ "effects": [Effect::message(
        MessageLevel::Info, CANCELLING_MESSAGE.to_string(),
    )] }),
    ))
}

/// Readable refusal while a detached worker owns repository mutation authority.
pub(super) fn deferred_operation_response() -> Result<Dispatched> {
    Ok(Dispatched::value(
        serde_json::json!({ "effects": [Effect::message(
        MessageLevel::Info, DEFERRED_MESSAGE.to_string(),
    )] }),
    ))
}

/// Whether a command can change repository or index state.
pub(super) fn repository_mutation_command(command: &str) -> bool {
    matches!(
        command,
        "status.stage"
            | "status.unstage"
            | "status.stage-all"
            | "status.unstage-all"
            | "status.discard"
            | "commit.create"
            | "commit.amend"
            | "commit.extend"
            | "commit.reword"
            | "commit.fixup"
            | "commit.instant-fixup"
            | "commit.instant-squash"
            | "rebase.continue"
            | "rebase.skip"
            | "rebase.abort"
            | "rebase.todo-submit"
    )
}

/// Wait with no session ownership; reacquire only to validate and apply an answer.
pub(super) fn drive_execution(
    shared: &Shared,
    repo: &str,
    mut step: Execution,
) -> Result<InvokeResult> {
    loop {
        match step {
            Execution::Done(result) => return Ok(result),
            Execution::Pick(mut pick) => {
                let ui = custom_ui(&shared.client);
                let declared = ui.pick_limit();
                let config = shared.config.config();
                let preference = config.pick.max_items;
                let over_limit = matches!(
                    (preference, declared),
                    (Some(pref), Some(limit)) if pref > limit
                );
                let client_name = shared
                    .client
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .client_name
                    .clone();
                // A configured per-client limit may permit a native pick above
                // the declared maximum, capped by the global preference.
                let overridden = match (over_limit, preference) {
                    (true, Some(pref)) => client_name
                        .as_ref()
                        .and_then(|name| config.pick.clients.get(name))
                        .map(|limit| (*limit).min(pref)),
                    _ => None,
                };
                let limit = if over_limit { overridden } else { preference };
                if let Some(limit) = limit {
                    let handle = shared
                        .session(repo)
                        .ok_or_else(|| Error::new("Prompt session closed"))?;
                    let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
                    session.truncate_pending_pick(limit);
                    if let Some(updated) = session.pending_pick_request() {
                        pick = updated;
                    }
                }
                let native = match overridden {
                    Some(limit) => ui.allows_pick_with_limit(&pick, Some(limit)),
                    None => !over_limit && ui.allows_pick(&pick),
                };
                if native {
                    let answer = shared.picks.ask(&ui, &pick);
                    let handle = shared
                        .session(repo)
                        .ok_or_else(|| Error::new("Prompt session closed"))?;
                    let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
                    let prompt_id = session
                        .pending_prompt_request()
                        .ok_or_else(|| Error::new("no prompt is pending"))?
                        .prompt_id;
                    let value = match answer {
                        Some(answer) if !answer.cancelled => {
                            let free = answer.free_text.filter(|text| !text.trim().is_empty());
                            if let Some(custom) = free {
                                Some(PromptValue::Choice(PromptChoiceAnswer::Custom { custom }))
                            } else if answer.selected.is_empty() {
                                None
                            } else if pick.multi {
                                Some(PromptValue::Multi {
                                    ids: answer.selected,
                                })
                            } else {
                                Some(PromptValue::Choice(PromptChoiceAnswer::Id {
                                    id: answer.selected[0].clone(),
                                }))
                            }
                        }
                        _ => None,
                    };
                    step = session.resume_prompt(&prompt_id, value)?;
                } else {
                    let handle = shared
                        .session(repo)
                        .ok_or_else(|| Error::new("Prompt session closed"))?;
                    let request = {
                        let session = handle.lock().unwrap_or_else(|p| p.into_inner());
                        session
                            .pending_prompt_request()
                            .ok_or_else(|| Error::new("no pick is pending"))?
                    };
                    let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
                    let mut result = session.open_document_prompt(&request)?;
                    if let (true, Some(pref), Some(declared)) = (over_limit, preference, declared) {
                        let hint = match client_name.as_deref() {
                            Some(name) => format!("Set pick.clients[\"{name}\"] to override."),
                            None => "Set pick.clients.<client> to override.".to_string(),
                        };
                        result.effects.insert(
                            0,
                            Effect::message(
                                MessageLevel::Warn,
                                format!(
                                    "This client declares at most {declared} picker items; \
                                     the configured preference is {pref}. {hint} Using the \
                                     document picker."
                                ),
                            ),
                        );
                    }
                    return Ok(result);
                }
            }
            Execution::Prompt(request) => {
                let native = custom_ui(&shared.client).allows_prompt(&request);
                match crate::presentation::prompt::delivery(&request, native) {
                    crate::presentation::prompt::PromptDelivery::Document => {
                        let handle = shared
                            .session(repo)
                            .ok_or_else(|| Error::new("Prompt session closed"))?;
                        let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
                        return session.open_document_prompt(&request);
                    }
                    crate::presentation::prompt::PromptDelivery::RefuseSecret => {
                        let handle = shared
                            .session(repo)
                            .ok_or_else(|| Error::new("Prompt session closed"))?;
                        let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
                        let mut result = match session.resume_prompt(&request.prompt_id, None)? {
                            Execution::Done(result) => result,
                            Execution::Prompt(_) => {
                                return Err(Error::new("refused prompt did not finish"))
                            }
                            Execution::Pick(_) => return Err(Error::new("pick did not finish")),
                        };
                        result.effects.insert(
                            0,
                            Effect::message(
                                MessageLevel::Warn,
                                crate::presentation::prompt::SECRET_UNAVAILABLE.to_string(),
                            ),
                        );
                        return Ok(result);
                    }
                    crate::presentation::prompt::PromptDelivery::Native => {}
                }
                let answer = crate::ui::answer(&ServerUi::new(shared), &request);
                let handle = shared
                    .session(repo)
                    .ok_or_else(|| Error::new("Prompt session closed"))?;
                let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
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
pub(super) fn set_mode_view(shared: &Shared, params: Value) -> Result<Dispatched> {
    let p: ViewSetModeParams = from_params(params)?;
    let repo = shared
        .repo_for_view(&p.view_id)
        .ok_or_else(|| Error::new(format!("unknown view: {}", p.view_id)))?;
    let handle = shared
        .session(&repo)
        .ok_or_else(|| Error::new(format!("unknown view: {}", p.view_id)))?;
    let ui = ServerUi::new(shared);
    let outcome = handle
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .set_mode_ui(&p, &ui)?;
    Ok(dispatched_outcome(shared, &repo, outcome)?)
}

/// Turn a command outcome into its response plus render/keymap notifications.
pub(super) fn dispatched_outcome(
    shared: &Shared,
    repo: &str,
    mut outcome: InvokeResult,
) -> Result<Dispatched> {
    outcome.effects = custom_ui(&shared.client)
        .prepare_effects(ViewEffectsParams {
            effects: outcome.effects,
        })
        .effects;
    let mut notifications = Vec::new();
    let pending_render = if let Some(operation) = outcome
        .commit
        .filter(|_| !shared.closing.load(Ordering::SeqCst))
    {
        // Git must not run while this request still holds any session or repo
        // lock: it waits for the user to edit and confirm.
        spawn_repository_operation(shared.clone(), repo.to_string(), operation)
    } else {
        None
    };
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
                Effect::PushView { descriptor } => {
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
    // Plain presents the stack's active logical view in one stable URI. Update
    // that routing identity before any render from this outcome is delivered.
    let mut plain_focus = Vec::new();
    {
        let _lifecycle = shared
            .plain_lifecycle
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let mut documents = shared
            .plain_documents
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        for effect in &outcome.effects {
            match effect {
                Effect::PushView { descriptor } => {
                    if let Some(entry) = documents.values_mut().find(|entry| entry.repo == repo) {
                        entry.view_id = descriptor.id.clone();
                    }
                }
                Effect::FocusView { view_id } => {
                    if let Some(entry) = documents.values_mut().find(|entry| entry.repo == repo) {
                        entry.view_id = view_id.clone();
                        plain_focus.push(view_id.clone());
                    }
                }
                _ => {}
            }
        }
    }
    for view_id in plain_focus {
        // The admission render below already publishes this view's new
        // snapshot; routing it again here would duplicate the status edit.
        if pending_render
            .as_ref()
            .is_some_and(|render| render.view_id == view_id)
        {
            continue;
        }
        let snapshot = shared
            .published
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .find_map(|views| views.current_snapshot(&view_id));
        route_plain_render(shared, &view_id, snapshot, None);
    }
    // Text mode takes ownership of the buffer and accepts no state-dependent
    // keys, so a loading status token can end with its text-mode render. The
    // completion task then sees no token and never ends it twice.
    let text_view = [pending_render.as_ref(), outcome.render.as_ref()]
        .into_iter()
        .flatten()
        .find(|render| render.mode == ViewMode::Text)
        .map(|render| render.view_id.clone());
    if let Some(render) = pending_render {
        notifications.extend(deliver_render(shared, render));
    }
    if let Some(render) = outcome.render {
        notifications.extend(deliver_render(shared, render));
    }
    // A mode change replaces the binding set, so the client must see it even
    // when the scope's ordinary keymap would not change. Keep the established
    // render-before-keymap order for rich clients.
    if shared.dynamic_keymaps() {
        if let Some(keymap) = outcome.keymap {
            notifications.push(Message::Notification(Notification::new(
                VIEW_KEYMAP.to_string(),
                keymap,
            )));
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
        notifications.extend(deliver_render(shared, render));
        if shared.dynamic_keymaps() {
            if let Some(keymap) = keymap {
                notifications.push(Message::Notification(Notification::new(
                    VIEW_KEYMAP.to_string(),
                    keymap,
                )));
            }
        }
    }

    if shared
        .client
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .plain
        .is_some()
    {
        notifications.extend(effect_notifications(
            &shared.client,
            outcome.effects.clone(),
        ));
    }

    if let Some(view) = text_view {
        if let Some(end) = shared.release_status_progress(&view) {
            notifications.push(end);
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
pub(super) fn edit_keymap(
    config: &ConfigService,
    edit_id: &str,
    kind: &str,
) -> Option<ViewKeymapParams> {
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
pub(super) struct CommitEdit {
    pub(super) id: String,
    pub(super) configs: Arc<Mutex<HashMap<String, Arc<ConfigService>>>>,
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

/// Worker-owned editor resources for one typed repository operation.
struct RepositoryEdit {
    _commit: Option<CommitEdit>,
    scope: Option<Arc<editor::EditScope>>,
}

impl Drop for RepositoryEdit {
    fn drop(&mut self) {
        if let Some(scope) = &self.scope {
            scope.revoke();
        }
    }
}

/// Publish the watcher renders withheld inside a commit's publication window.
///
/// A withheld render is delivered only while the settled session still carries
/// the version it produced: a read the worker's own refresh superseded has a
/// stale version and is dropped, while a read that already matched the settled
/// tree is the render the client still needs. Delivery happens before the
/// outcome message, so the client applies the committed status before
/// `Committed:` reaches it.
fn flush_deferred_renders(shared: &Shared, repo: &str) -> Vec<Message> {
    let deferred = shared.take_deferred_renders(repo);
    if deferred.is_empty() {
        return Vec::new();
    }
    let current: Vec<ViewRender> = {
        let Some(handle) = shared.session(repo) else {
            return Vec::new();
        };
        let session = handle.lock().unwrap_or_else(|p| p.into_inner());
        deferred
            .into_iter()
            .filter(|render| {
                session
                    .views()
                    .get(&render.view_id)
                    .is_some_and(|view| view.version() == render.version)
            })
            .collect()
    };
    current
        .into_iter()
        .filter_map(|render| deliver_render(shared, render))
        .collect()
}

/// Run `git commit` on a detached worker and refresh the views afterwards.
///
/// The worker owns the editor wait, so neither the sessions mutex nor the repo
/// lock is held while Git blocks on the helper. Only the final refresh takes
/// the session lock again, briefly. For a commit draft, the returned render is
/// the admission of the pending row that the caller must deliver.
fn spawn_repository_operation(
    shared: Shared,
    repo: String,
    operation: crate::git::RepositoryOperation,
) -> Option<ViewRender> {
    let mut pending_render = None;
    let Some(reservation) = shared.reserve_deferred(repo.clone()) else {
        let _ = shared.sender.send(show_message(
            1,
            "Wait for the current Git operation to finish".to_string(),
        ));
        return None;
    };
    let unsupported = shared
        .client
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .presentation
        .as_ref()
        .is_some_and(|presentation| {
            matches!(
                presentation, Presentation::Rich(caps) if !caps.supports_effect("editDocument")
            )
        });
    if unsupported {
        let _ = shared
            .sender
            .send(custom_ui(&shared.client).effects(ViewEffectsParams {
                effects: vec![Effect::message(
                    MessageLevel::Warn,
                    "Client does not support editDocument",
                )],
            }));
        return None;
    }
    let sender = shared.sender.clone();
    // The begin is sent before the invoke response, so the client is not idle
    // until the editor (or the failure) has been answered.
    let (edit, edit_id, edit_scope) = match &operation {
        crate::git::RepositoryOperation::Commit(_) => {
            let edit_id = match editor::registry().create(
                &repo,
                &shared.draft_owner,
                editor::DraftKind::CommitMessage,
            ) {
                Ok(id) => id,
                Err(error) => {
                    let _ = sender.send(show_message(
                        1,
                        format!("Cannot open commit draft: {error}"),
                    ));
                    return None;
                }
            };
            // The command published the focused view before this draft existed,
            // so render it again now: the published snapshot then already shows
            // the pending row and an unchanged watcher refresh is a no-op.
            if let Some(handle) = shared.session(&repo) {
                let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
                match session.publish_pending_draft() {
                    Ok(render) => pending_render = render,
                    Err(error) => report_pending_refresh_error(&shared, error),
                }
            }
            shared
                .edit_configs
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(
                    edit_id.clone(),
                    shared.config.for_repository(std::path::Path::new(&repo)),
                );
            let commit = CommitEdit {
                id: edit_id.clone(),
                configs: shared.edit_configs.clone(),
            };
            (
                RepositoryEdit {
                    _commit: Some(commit),
                    scope: None,
                },
                Some(edit_id),
                None,
            )
        }
        crate::git::RepositoryOperation::InstantRebase(_) => {
            let config = shared.config.for_repository(std::path::Path::new(&repo));
            let admit_configs = shared.edit_configs.clone();
            let release_configs = shared.edit_configs.clone();
            let scope = match editor::registry().scope_with_lifecycle(
                &repo,
                &shared.draft_owner,
                Arc::new(move |run_id| {
                    admit_configs
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .insert(run_id.to_string(), config.clone());
                }),
                Arc::new(move |run_id| {
                    release_configs
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .remove(run_id);
                }),
            ) {
                Ok(scope) => Arc::new(scope),
                Err(error) => {
                    let _ = sender.send(show_message(
                        1,
                        format!("Cannot open instant autosquash draft scope: {error}"),
                    ));
                    return None;
                }
            };
            (
                RepositoryEdit {
                    _commit: None,
                    scope: Some(scope.clone()),
                },
                None,
                Some(scope),
            )
        }
    };
    let (token, cancel) = shared.begin_progress_with_scope(
        &sender,
        "commit",
        edit_id.clone(),
        Some(&repo),
        edit_scope.clone(),
    );
    let progress = ProgressEnd {
        shared: shared.clone(),
        token,
    };
    let worker = std::thread::Builder::new()
        .name("magit-lsp-commit".to_string())
        .spawn(move || {
            let reservation = reservation;
            let _progress = progress;
            let result = runner::with_cancel(Some(cancel), || match editor::socket_path() {
                Some(socket) => {
                    let executable = std::env::current_exe()
                        .map_err(|error| Error::new(format!("cannot locate server executable: {error}")))?;
                    editor::execute_operation_scoped(
                        &repo,
                        edit_id.as_deref().unwrap_or(""),
                        socket,
                        &operation,
                        &executable,
                        edit.scope.as_deref(),
                    )
                },
                None => Err(Error::new("the editor bridge is not running")),
            });
            // Release both resources before any outcome or progress-end reaches
            // the client, including when Git was killed before opening an editor.
            drop(edit);

            // Hold the publication window the confirm lane opened: the
            // commit's renders and outcome messages publish before any
            // watcher refresh held off by the window, covering the early
            // return below. The underscore-prefixed binding still lives to
            // the closure's end, closing the window only after every
            // publication is sent.
            let _publication = shared.publication_guard(&repo);

            let mut messages = Vec::new();
            match result {
                Ok(crate::git::RepositoryOperationOutcome::Commit(outcome)) => {
                    let renders: Vec<ViewRender> = if shared.closing.load(Ordering::SeqCst) {
                        Vec::new()
                    } else {
                        let Some(handle) = shared.session(&repo) else {
                            return;
                        };
                        let mut session = handle.lock().unwrap_or_else(|p| p.into_inner());
                        let ids: Vec<String> = session
                            .views()
                            .iter()
                            .map(|view| view.descriptor().id.clone())
                            .collect();
                        ids.into_iter()
                            .filter_map(|id| session.refresh(&id).ok().flatten())
                            .collect()
                    };
                    messages.extend(
                        renders
                            .into_iter()
                            .filter_map(|render| deliver_render(&shared, render)),
                    );
                    messages.extend(flush_deferred_renders(&shared, &repo));
                    // Replace the "Committing…" feedback with the outcome.
                    let subject = outcome
                        .message
                        .as_deref()
                        .unwrap_or("")
                        .lines()
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let text = if subject.is_empty() {
                        "Committed".to_string()
                    } else {
                        format!("Committed: {subject}")
                    };
                    let mut effects = vec![Effect::message(MessageLevel::Info, text)];
                    if let Some(warning) = outcome.warning {
                        effects.push(Effect::message(MessageLevel::Warn, warning));
                    }
                    messages.extend(effect_notifications(&shared.client, effects));
                }
                Ok(crate::git::RepositoryOperationOutcome::InstantRebase(outcome)) => {
                    if !shared.closing.load(Ordering::SeqCst) {
                        match refresh_pending_views(&shared, &repo) {
                            Ok(renders) => for render in renders {
                                if let Some(message) = deliver_render(&shared, render) {
                                    messages.push(message);
                                }
                            },
                            Err(refresh_error) => report_pending_refresh_error(&shared, refresh_error),
                        }
                    }
                    messages.extend(flush_deferred_renders(&shared, &repo));
                    let (level, text) = match outcome {
                        crate::instant_rebase::InstantRebaseOutcome::Refused(error) =>
                            (MessageLevel::Warn, format!("Instant autosquash refused: {error}")),
                        crate::instant_rebase::InstantRebaseOutcome::MarkerFailed(error) =>
                            (MessageLevel::Error, format!("Instant autosquash marker failed: {error}")),
                        crate::instant_rebase::InstantRebaseOutcome::RebaseStopped { marker, phase, error } =>
                            (MessageLevel::Error, format!("Instant autosquash stopped during {phase}; marker {marker} retained: {error}")),
                        crate::instant_rebase::InstantRebaseOutcome::Completed { marker, warning } => {
                            if let Some(warning) = warning {
                                messages.extend(effect_notifications(&shared.client, vec![Effect::message(MessageLevel::Warn, warning)]));
                            }
                            (MessageLevel::Info, format!("Instant autosquash completed; marker {marker} autosquashed"))
                        }
                    };
                    messages.extend(effect_notifications(&shared.client, vec![Effect::message(level, text)]));
                }
                Err(error) => {
                    if !shared.closing.load(Ordering::SeqCst) {
                        match refresh_pending_views(&shared, &repo) {
                            Ok(renders) => for render in renders {
                                if let Some(message) = deliver_render(&shared, render) {
                                    messages.push(message);
                                }
                            },
                            Err(refresh_error) => report_pending_refresh_error(&shared, refresh_error),
                        }
                    }
                    messages.extend(flush_deferred_renders(&shared, &repo));
                    let effects = errors::effects_for_result(Err(error)).unwrap_or_else(|error| {
                        vec![Effect::message(MessageLevel::Error, error.to_string())]
                    });
                    messages.extend(effect_notifications(&shared.client, effects));
                }
            }
            // Keep the repository reserved until all prepared outcomes are
            // published; lexical drop order ends progress before releasing it.
            for message in messages {
                let _ = sender.send(message);
            }
        });
    if let Err(error) = worker {
        tracing::warn!(%error, "failed to start the commit worker");
    }
    pending_render
}
