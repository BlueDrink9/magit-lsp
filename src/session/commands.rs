//! Version-bound command execution and application of command outcomes.

use super::{
    prompt_busy, unsupported_feature, InvokeResult, RenderDelivery, Session, SessionContinuation,
    TOGGLE_TEXT_MODE,
};
use crate::commands::{self, SelectionOperation};
use crate::git::GitRepo;
use crate::protocol::{Cursor, Effect, MessageLevel, Selection, ViewRender};
use crate::tree::NodeKind;
use crate::ui::PromptUi;
use crate::view::{Placement, ViewKind, ViewMode};
use crate::{Error, Result};
use serde_json::Value;
use std::sync::Arc;

impl Session {
    /// Bind positional input to its exact retained tree, never a newer ack.
    pub(super) fn run_versioned_command(
        &mut self,
        view_id: &str,
        command_id: &str,
        version: Option<u64>,
        cursor: Option<Cursor>,
        selection: Option<Selection>,
        validated_offer: bool,
        args: Option<&Value>,
        key: Option<String>,
        ui: &dyn PromptUi,
    ) -> Result<InvokeResult> {
        if self.pending_prompt.is_some() && command_id != "common.refresh" {
            return Ok(prompt_busy());
        }
        let view = self
            .views
            .get_mut(view_id)
            .ok_or_else(|| Error::new(format!("unknown view: {view_id}")))?;
        // These handlers consume document coordinates. Menu keys, refresh,
        // edit confirmation and other non-positional actions need no tree.
        let requires_ack = command_id == "select.toggle-mark"
            || (command_id == "common.confirm" && view.commit_selection().is_some());
        let positional = requires_ack
            || matches!(
                command_id,
                "status.stage"
                    | "status.unstage"
                    | "status.discard"
                    | "common.copy-value"
                    | "common.visit"
                    | "common.toggle-section"
                    | "common.show-section"
                    | "common.hide-section"
                    | "common.next-section"
                    | "common.prev-section"
                    | "common.jump"
                    | "common.next-sibling"
                    | "common.prev-sibling"
                    | "common.parent-section"
                    | "common.cycle-visibility"
            );
        // Commit selection intentionally waits for an acknowledgement. Other
        // positional actions may address the exact current, initially unacked
        // render, but neither policy substitutes a different retained version.
        if positional
            && (requires_ack || cursor.is_some() || selection.is_some())
            && view.mode() != ViewMode::Text
        {
            let source = match version {
                Some(version) if Some(version) == view.acked_version => view.acked.as_ref(),
                Some(v) if validated_offer && view.acked_version.is_none() && v == view.version => {
                    Some(&view.tree)
                }
                Some(version) if !requires_ack && version == view.version => Some(&view.tree),
                _ => None,
            };
            let Some(source) = source else {
                let render = Some(view.refresh_and_render(None)?);
                return Ok(InvokeResult {
                    effects: vec![Effect::message(
                        MessageLevel::Warn,
                        crate::errors::STALE_VIEW,
                    )],
                    render,
                    ..InvokeResult::empty()
                });
            };
            view.request_tree = Some(source.clone());
        }
        if crate::dispatch::content_command(command_id) && view.mode() != ViewMode::Text {
            let source = view.command_source_tree().unwrap_or_else(|| view.tree());
            if let Some(node) = cursor
                .and_then(|cursor| source.line_nodes.get(cursor.line))
                .and_then(|line| source.node(&line.node))
            {
                let hunk = if node.kind == crate::tree::NodeKind::Line {
                    source.nodes.iter().find(|candidate| {
                        candidate.kind == crate::tree::NodeKind::Hunk
                            && candidate.path == node.path
                            && candidate.section == node.section
                            && candidate.hunk_id == node.hunk_id
                    })
                } else {
                    Some(node)
                };
                if let Some(hunk) = hunk.filter(|node| {
                    matches!(
                        node.kind,
                        crate::tree::NodeKind::Hunk | crate::tree::NodeKind::Line
                    )
                }) {
                    if !crate::dispatch::hunk_section_compatible(
                        command_id,
                        hunk.section.as_deref(),
                    ) {
                        let effects = if command_id == "status.stage"
                            && hunk.section.as_deref() == Some("staged")
                        {
                            vec![]
                        } else {
                            vec![Effect::message(
                                MessageLevel::Info,
                                match command_id {
                                    "status.stage" => crate::dispatch::NOTHING_TO_STAGE,
                                    "status.unstage" => crate::dispatch::NOTHING_TO_UNSTAGE,
                                    _ => crate::dispatch::NOTHING_TO_DISCARD,
                                },
                            )]
                        };
                        return Ok(InvokeResult {
                            effects,
                            render: None,
                            ..InvokeResult::empty()
                        });
                    }
                }
            }
            let targets = crate::dispatch::displayed_targets(source, command_id, cursor, selection);
            let repo = crate::git::GitRepo::open(&self.repo_path)?;
            let mut rejection = None;
            for target in &targets {
                match crate::dispatch::target_is_current(&repo, source, command_id, target) {
                    Ok(true) => continue,
                    Ok(false) => {
                        rejection = Some(vec![Effect::message(
                            MessageLevel::Warn,
                            crate::errors::HUNK_CHANGED,
                        )])
                    }
                    Err(error) => {
                        view.request_tree = None;
                        rejection = Some(crate::errors::effects_for_result(Err(error))?);
                    }
                }
                break;
            }
            if let Some(effects) = rejection {
                view.request_tree = None;
                let render = self
                    .views
                    .get_mut(view_id)
                    .expect("view was checked above")
                    .refresh_and_render(None)?;
                return Ok(InvokeResult {
                    effects,
                    render: Some(render),
                    ..InvokeResult::empty()
                });
            }
        }
        let result = self.run_command(
            view_id,
            command_id,
            cursor,
            selection,
            args,
            key,
            Some(super::MenuOrigin {
                view_id: view_id.to_string(),
                version: version
                    .unwrap_or_else(|| self.views.get(view_id).map_or(0, |v| v.version())),
                cursor,
                selection,
            }),
            ui,
        );
        if let Some(view) = self.views.get_mut(view_id) {
            view.request_tree = None;
        }
        result
    }

    /// The single execution path shared by `view/invoke` and `view/key`.
    ///
    /// The command table lives in [`crate::commands`]; this method only looks
    /// it up, reports `Planned`/scope mismatches, and renders.
    /// In text mode every command but the toggle is refused, so a key press or
    /// invoke can never reach Git while the user edits plain text.
    pub(super) fn run_command(
        &mut self,
        view_id: &str,
        command_id: &str,
        cursor: Option<Cursor>,
        selection: Option<Selection>,
        args: Option<&serde_json::Value>,
        key: Option<String>,
        origin: Option<super::MenuOrigin>,
        ui: &dyn PromptUi,
    ) -> Result<InvokeResult> {
        if let Some(required) = self
            .rich_capabilities
            .as_ref()
            .and_then(|caps| caps.command_requirement(command_id))
        {
            return Ok(unsupported_feature(required.name()));
        }
        let Some(command) = commands::registry().get(command_id) else {
            return Err(Error::new(format!("unknown command: {command_id}")));
        };

        let repo_path = self.repo_path.clone();

        let text_mode = self
            .views
            .get(view_id)
            .map(|view| view.mode() == ViewMode::Text)
            .ok_or_else(|| Error::new(format!("unknown view: {view_id}")))?;
        if text_mode && command.id != TOGGLE_TEXT_MODE {
            // A requested refresh is remembered, not run: it applies on exit.
            if command.id == "common.refresh" {
                if let Some(view) = self.views.get_mut(view_id) {
                    view.queue_refresh();
                }
            }
            let hint = self.text_mode_hint(view_id);
            return Ok(InvokeResult {
                effects: vec![Effect::message(MessageLevel::Info, hint)],
                ..InvokeResult::empty()
            });
        }

        let (effects, outcome) = {
            let view = self
                .views
                .get_mut(view_id)
                .ok_or_else(|| Error::new(format!("unknown view: {view_id}")))?;

            match commands::registry().availability(command.id) {
                Some(commands::Availability::Planned(roadmap_id)) => {
                    return Ok(InvokeResult {
                        effects: vec![Effect::message(
                            MessageLevel::Info,
                            format!(
                                "Not implemented yet: {} (roadmap {})",
                                command.doc, roadmap_id
                            ),
                        )],
                        ..InvokeResult::empty()
                    });
                }
                Some(commands::Availability::Implemented) => {}
                None => return Err(Error::new(format!("unknown command: {command_id}"))),
            }

            if !command.applies_to(view.descriptor().scope.as_str()) {
                return Ok(InvokeResult {
                    effects: vec![Effect::message(
                        MessageLevel::Info,
                        "Not available in this view",
                    )],
                    ..InvokeResult::empty()
                });
            }

            let keymaps = Arc::clone(&self.keymaps);
            let mut ctx = commands::CommandCtx::new(
                view, &repo_path, &keymaps, cursor, selection, args, key, ui,
            );
            let outcome = (command.run)(&mut ctx)?;
            let effects = ctx.into_effects();
            (effects, outcome)
        };
        self.finish_command(view_id, command.id, cursor, effects, outcome, origin)
    }

    /// Apply a command outcome to the session's views and pending interactions.
    pub(super) fn finish_command(
        &mut self,
        view_id: &str,
        command_id: &str,
        cursor: Option<Cursor>,
        mut effects: Vec<Effect>,
        mut outcome: commands::CommandOutcome,
        origin: Option<super::MenuOrigin>,
    ) -> Result<InvokeResult> {
        commands::enforce_effects(command_id, &mut effects);
        if let Some((request, action)) = outcome.prompt.take() {
            if let Some(result) = self.unsupported_prompt(&request) {
                return Ok(result);
            }
            self.prepare_prompt(
                view_id,
                cursor,
                request,
                SessionContinuation::Command {
                    command_id: command_id.to_string(),
                    action,
                    effects,
                },
            );
            return Ok(InvokeResult::empty());
        }
        let render = {
            let view = self
                .views
                .get_mut(view_id)
                .ok_or_else(|| Error::new(format!("unknown view: {view_id}")))?;
            if outcome.pop
                || outcome.open_menu.is_some()
                || outcome.open_view.is_some()
                || outcome.open_selection.is_some()
                || outcome.selected_commits.is_some()
                || outcome.commit.is_some()
            {
                None
            } else {
                let mode_change = outcome.mode.filter(|mode| *mode != view.mode());
                let mut render = match mode_change {
                    Some(ViewMode::Text) => {
                        view.enter_text_mode();
                        Some(ViewRender {
                            view_id: view.descriptor.id.clone(),
                            version: view.version,
                            base_version: None,
                            text: None,
                            edits: None,
                            cursor: None,
                            mode: view.mode,
                            pending: view.pending(),
                            menu: view.view.menu(),
                        })
                    }
                    Some(ViewMode::Interactive) => Some(view.leave_text_mode(cursor)?),
                    None => Some(if outcome.refresh {
                        match view.refresh_and_render(cursor) {
                            Ok(render) => render,
                            Err(err)
                                if command_id == "common.refresh"
                                    && view.descriptor().kind == ViewKind::Log =>
                            {
                                let effect = Effect::from_error(&err).unwrap_or_else(|| {
                                    Effect::message(MessageLevel::Error, err.to_string())
                                });
                                return Ok(InvokeResult {
                                    effects: vec![effect],
                                    ..InvokeResult::empty()
                                });
                            }
                            Err(err) => return Err(err),
                        }
                    } else {
                        view.render_with_delivery(
                            cursor,
                            outcome.cursor,
                            RenderDelivery::Acknowledged,
                        )?
                    }),
                };
                if let (Some(render), Some(node)) = (
                    &mut render,
                    outcome
                        .cursor_nodes
                        .iter()
                        .find_map(|id| view.tree.node(id)),
                ) {
                    render.cursor = Some(Cursor {
                        line: node.start_line,
                        column: 0,
                    });
                }
                render
            }
        };

        // Nested `common.quit` returns to the existing parent. Only quitting
        // the root view clears the whole session.
        if effects.iter().any(|effect| matches!(effect, Effect::Quit)) {
            if let Some(pending) = self.pending_selections.remove(view_id) {
                self.views.remove(view_id);
                return Ok(InvokeResult {
                    effects: vec![
                        Effect::PopView {
                            view_id: view_id.to_string(),
                        },
                        Effect::FocusView {
                            view_id: pending.caller_id.clone(),
                        },
                    ],
                    keymap: self.keymap_notice(&pending.caller_id),
                    ..InvokeResult::empty()
                });
            }
            if self.views.previous_of(view_id).is_some() {
                return self.pop_view(view_id);
            }
            self.views.clear();
            self.pending_selections.clear();
            return Ok(InvokeResult {
                effects,
                ..InvokeResult::empty()
            });
        }

        let mode_changed = outcome.mode.is_some();
        if outcome.pop {
            if let Some(pending) = self.pending_selections.remove(view_id) {
                self.views.remove(view_id);
                let mut effects = vec![Effect::PopView {
                    view_id: view_id.to_string(),
                }];
                effects.push(Effect::FocusView {
                    view_id: pending.caller_id.clone(),
                });
                return Ok(InvokeResult {
                    effects,
                    keymap: self.keymap_notice(&pending.caller_id),
                    ..InvokeResult::empty()
                });
            }
            let mut result = self.pop_view(view_id)?;
            let mut all = effects;
            all.extend(result.effects);
            result.effects = all;
            return Ok(result);
        }

        if let Some(request) = outcome.open_selection {
            return self.push_selection(view_id, request, None, effects);
        }

        if let Some(commits) = outcome.selected_commits {
            let allow_free_text = self
                .views
                .get(view_id)
                .and_then(|view| view.commit_selection())
                .is_some_and(|selection| selection.allow_free_text);
            let loaded = |oid: &str| {
                self.views.get(view_id).is_some_and(|view| {
                    view.tree().nodes.iter().any(|node| {
                        node.kind == NodeKind::Commit && node.commit.as_deref() == Some(oid)
                    })
                })
            };
            let valid = if !commits.is_empty() && commits.iter().all(|oid| loaded(oid)) {
                Some(commits)
            } else if allow_free_text {
                let repo = crate::git::GitRepo::open(&self.repo_path)?;
                let resolved: Result<Vec<String>> =
                    commits.iter().map(|oid| repo.resolve_commit(oid)).collect();
                resolved.ok().filter(|resolved| {
                    !resolved.is_empty() && resolved.iter().all(|oid| !oid.is_empty())
                })
            } else {
                None
            };
            let Some(valid) = valid else {
                // A selection neither the tree nor the Git boundary can
                // resolve: refuse readably while keeping the caller and picker
                // intact.
                return Ok(InvokeResult {
                    effects: vec![Effect::message(
                        MessageLevel::Info,
                        "No valid commits were selected".to_string(),
                    )],
                    ..InvokeResult::empty()
                });
            };
            if let Some(pending) = self.pending_selections.get(view_id) {
                let caller = pending.caller_id.clone();
                let operation = pending.operation;
                let continuation = pending.continuation.clone();
                if operation == SelectionOperation::CopyCommits {
                    self.pending_selections.remove(view_id);
                    self.views.remove(view_id);
                    return Ok(InvokeResult {
                        effects: vec![
                            Effect::Clipboard {
                                text: valid.join("\n"),
                            },
                            Effect::PopView {
                                view_id: view_id.to_string(),
                            },
                            Effect::FocusView {
                                view_id: caller.clone(),
                            },
                        ],
                        keymap: self.keymap_notice(&caller),
                        ..InvokeResult::empty()
                    });
                }
                if operation == SelectionOperation::PickCommit {
                    let Some(oid) = valid.first().filter(|_| valid.len() == 1).cloned() else {
                        return Ok(InvokeResult {
                            effects: vec![Effect::message(
                                MessageLevel::Warn,
                                "Select exactly one commit for this option",
                            )],
                            ..InvokeResult::empty()
                        });
                    };
                    let Some(SessionContinuation::Option {
                        key,
                        menu,
                        cursor,
                        reader: _,
                    }) = continuation
                    else {
                        return Ok(InvokeResult {
                            effects: vec![Effect::message(
                                MessageLevel::Warn,
                                "No menu option is waiting for this commit",
                            )],
                            ..InvokeResult::empty()
                        });
                    };
                    let menu_matches = self
                        .views
                        .get(&caller)
                        .and_then(|view| view.menu())
                        .is_some_and(|current| current == menu);
                    if !menu_matches {
                        // The captured arguments no longer describe the caller,
                        // so keep the picker open instead of dropping the pick.
                        return Ok(InvokeResult {
                            effects: vec![Effect::message(
                                MessageLevel::Warn,
                                "The commit menu changed; the selection was not applied",
                            )],
                            ..InvokeResult::empty()
                        });
                    }
                    self.pending_selections.remove(view_id);
                    self.views.remove(view_id);
                    self.views
                        .get_mut(&caller)
                        .and_then(|view| view.menu_mut())
                        .expect("checked menu")
                        .set_option(&key, Some(oid));
                    let rerender = self.menu_rerender(&caller, cursor)?;
                    return Ok(InvokeResult {
                        effects: vec![
                            Effect::PopView {
                                view_id: view_id.to_string(),
                            },
                            Effect::FocusView {
                                view_id: caller.clone(),
                            },
                        ],
                        render: rerender.render,
                        keymap: rerender.keymap.or_else(|| self.keymap_notice(&caller)),
                        ..InvokeResult::empty()
                    });
                }
                if operation == SelectionOperation::FixupCommit {
                    let Some(oid) = valid.first().filter(|_| valid.len() == 1).cloned() else {
                        return Ok(InvokeResult {
                            effects: vec![Effect::message(
                                MessageLevel::Warn,
                                "Select exactly one commit to fix up",
                            )],
                            ..InvokeResult::empty()
                        });
                    };
                    let repo = crate::git::GitRepo::open(&self.repo_path)?;
                    let subject = match repo.commit_subject(&oid) {
                        Ok(subject) => subject,
                        Err(error) => {
                            return Ok(InvokeResult {
                                effects: vec![Effect::from_error(&error).unwrap_or_else(|| {
                                    Effect::message(MessageLevel::Error, error.to_string())
                                })],
                                ..InvokeResult::empty()
                            })
                        }
                    };
                    let result = repo.commit(&["--fixup", &oid], &[]);
                    self.pending_selections.remove(view_id);
                    self.views.remove(view_id);
                    let mut effects = vec![
                        Effect::PopView {
                            view_id: view_id.to_string(),
                        },
                        Effect::FocusView {
                            view_id: caller.clone(),
                        },
                    ];
                    match result {
                        Ok(_) => effects.push(Effect::message(
                            MessageLevel::Info,
                            format!("Created fixup! {subject}"),
                        )),
                        Err(error) => {
                            effects.push(Effect::from_error(&error).unwrap_or_else(|| {
                                Effect::message(MessageLevel::Error, error.to_string())
                            }))
                        }
                    }
                    return Ok(InvokeResult {
                        effects,
                        keymap: self.keymap_notice(&caller),
                        ..InvokeResult::empty()
                    });
                }
                if let SelectionOperation::InstantAutosquash(kind) = operation {
                    let Some(oid) = valid.first().filter(|_| valid.len() == 1).cloned() else {
                        return Ok(InvokeResult {
                            effects: vec![Effect::message(
                                MessageLevel::Warn,
                                "Select exactly one commit for instant autosquash",
                            )],
                            ..InvokeResult::empty()
                        });
                    };
                    let repo = GitRepo::open(&self.repo_path)?;
                    let authority = match crate::instant_rebase::InstantRebaseAuthority::prepare(
                        &repo, &oid, kind,
                    ) {
                        Ok(authority) => authority,
                        Err(error) => {
                            return Ok(InvokeResult {
                                effects: vec![Effect::message(
                                    MessageLevel::Error,
                                    error.to_string(),
                                )],
                                ..InvokeResult::empty()
                            })
                        }
                    };
                    self.pending_selections.remove(view_id);
                    self.views.remove(view_id);
                    return Ok(InvokeResult {
                        effects: vec![
                            Effect::PopView {
                                view_id: view_id.to_string(),
                            },
                            Effect::FocusView {
                                view_id: caller.clone(),
                            },
                        ],
                        keymap: self.keymap_notice(&caller),
                        commit: Some(crate::git::RepositoryOperation::InstantRebase(authority)),
                        ..InvokeResult::empty()
                    });
                }
                if operation == SelectionOperation::InitialRebase {
                    let Some(base) = valid.first().filter(|_| valid.len() == 1).cloned() else {
                        return Ok(InvokeResult {
                            effects: vec![Effect::message(
                                MessageLevel::Warn,
                                "Select exactly one rebase base",
                            )],
                            ..InvokeResult::empty()
                        });
                    };
                    let repo = GitRepo::open(&self.repo_path)?;
                    let plan = match crate::rebase_plan::PreparedRebase::prepare(&repo, &base) {
                        Ok(plan) => plan,
                        Err(error) => {
                            return Ok(InvokeResult {
                                effects: vec![Effect::message(
                                    MessageLevel::Error,
                                    error.to_string(),
                                )],
                                ..InvokeResult::empty()
                            })
                        }
                    };
                    self.pending_selections.remove(view_id);
                    self.views.remove(view_id);
                    let (descriptor, render) = self.push_view(
                        &caller,
                        ViewKind::RebaseTodo,
                        Placement::Main,
                        Box::new(crate::rebase_todo_view::RebaseTodoView::prepared(
                            repo, plan,
                        )),
                    )?;
                    let prepared_id = descriptor.id.clone();
                    return Ok(InvokeResult {
                        effects: vec![
                            Effect::PopView {
                                view_id: view_id.to_string(),
                            },
                            Effect::PushView { descriptor },
                            Effect::FocusView {
                                view_id: prepared_id.clone(),
                            },
                        ],
                        render: Some(render),
                        keymap: self.keymap_notice(&prepared_id),
                        ..InvokeResult::empty()
                    });
                }
            }
            return Ok(InvokeResult::empty());
        }

        if let Some(menu) = outcome.open_menu {
            let mut all = effects;
            let mut parent_id = view_id.to_string();
            // A menu opened from a menu replaces it: a top-level menu is a
            // sibling of the current one, not a child. Deliberate sub-menus
            // (none yet) will need an explicit nesting flag.
            if self
                .views
                .get(view_id)
                .map(|view| view.is_menu())
                .unwrap_or(false)
            {
                let popped = self.pop_view(view_id)?;
                all.extend(popped.effects);
                if let Some(previous) = self.views.iter().last() {
                    parent_id = previous.descriptor().id.clone();
                }
            }
            let inherited = self
                .views
                .get(&parent_id)
                .and_then(|v| v.menu_origin.clone());
            let (descriptor, render) = self.push_menu(&parent_id, menu, inherited.or(origin))?;
            all.push(Effect::PushView { descriptor });
            let keymap = self.keymap_notice(&render.view_id);
            return Ok(InvokeResult {
                effects: all,
                render: None,
                keymap: None,
                opened: vec![(render, keymap)],
                commit: None,
            });
        }

        if let Some((kind, placement)) = outcome.open_view {
            let handles_factory_error = matches!(kind, ViewKind::Log | ViewKind::RebaseTodo);
            let (descriptor, render) =
                match self.push_kind(view_id, kind, placement, outcome.open_args.as_ref()) {
                    Ok(opened) => opened,
                    Err(err) if handles_factory_error => {
                        let effect = Effect::from_error(&err).unwrap_or_else(|| {
                            Effect::message(MessageLevel::Error, err.to_string())
                        });
                        return Ok(InvokeResult {
                            effects: [effects, vec![effect]].concat(),
                            ..InvokeResult::empty()
                        });
                    }
                    Err(err) => return Err(err),
                };
            let mut all = effects;
            all.push(Effect::PushView { descriptor });
            let keymap = self.keymap_notice(&render.view_id);
            return Ok(InvokeResult {
                effects: all,
                render: None,
                keymap: None,
                opened: vec![(render, keymap)],
                commit: None,
            });
        }

        let keymap = if mode_changed {
            self.keymap_notice(view_id)
        } else {
            None
        };
        Ok(InvokeResult {
            effects,
            render,
            keymap,
            opened: Vec::new(),
            commit: outcome.commit,
        })
    }

    /// The command that leaves text mode, naming its key when one is bound.
    fn text_mode_hint(&self, view_id: &str) -> String {
        let key = self
            .views
            .get(view_id)
            .and_then(|view| self.keymap_for(view.descriptor().scope.as_str()))
            .and_then(|keymap| keymap.command(TOGGLE_TEXT_MODE))
            .and_then(|binding| binding.keys.first().cloned());
        match key {
            Some(key) => format!("Text mode: press {key} to return"),
            None => "Text mode: press the text-mode key to return".to_string(),
        }
    }
}
