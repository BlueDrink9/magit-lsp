//! Menu key resolution, option readers and retained Plain menu actions.

use super::{
    unbound, unsupported_menu_item, Execution, InvokeResult, Session, SessionContinuation,
};
use crate::commands::{self, SelectionOperation, SelectionRequest};
use crate::git::{GitLogOptions, GitRepo};
use crate::keyhandler::{self, MenuResolution};
use crate::log_view::CommitSelection;
use crate::menu::{MenuStatus, Reader};
use crate::protocol::{
    Cursor, Effect, MessageLevel, PickItem, PromptKind, ViewKeyParams, ViewPickParams,
    ViewPromptParams,
};
use crate::ui::PromptUi;
use crate::{Error, Result};

impl Session {
    /// Resolve a key inside a menu against its item keys and pending steps.
    pub(super) fn menu_key(
        &mut self,
        params: &ViewKeyParams,
        cursor_moved: bool,
        ui: &dyn PromptUi,
    ) -> Result<InvokeResult> {
        let resolution = {
            let view = self
                .views
                .get_mut(&params.view_id)
                .ok_or_else(|| Error::new(format!("unknown view: {}", params.view_id)))?;
            if cursor_moved {
                view.menu_pending.clear();
            }
            let keys: Vec<String> = view
                .menu()
                .map(|menu| {
                    menu.groups
                        .iter()
                        .flat_map(|group| &group.items)
                        .map(|item| item.key().to_string())
                        .collect()
                })
                .unwrap_or_default();
            keyhandler::resolve_menu(&mut view.menu_pending, &params.key, &keys)
        };

        match resolution {
            MenuResolution::Pending => self.menu_rerender(&params.view_id, params.cursor),
            MenuResolution::Ignored => Ok(InvokeResult::empty()),
            MenuResolution::Pop => self.pop_view(&params.view_id),
            MenuResolution::SaveArgs => {
                let mut result = self.run_command(
                    &params.view_id,
                    "menu.save-args",
                    Some(params.cursor),
                    None,
                    None,
                    Some(params.key.clone()),
                    ui,
                )?;
                if result.keymap.is_none() {
                    result.keymap = self.keymap_notice(&params.view_id);
                }
                Ok(result)
            }
            MenuResolution::ResetArgs => {
                let mut result = self.run_command(
                    &params.view_id,
                    "menu.reset-args",
                    Some(params.cursor),
                    None,
                    None,
                    Some(params.key.clone()),
                    ui,
                )?;
                if result.keymap.is_none() {
                    result.keymap = self.keymap_notice(&params.view_id);
                }
                Ok(result)
            }
            MenuResolution::Unbound { chord } => Ok(unbound(&chord)),
            MenuResolution::Complete(key) => self.menu_complete(&params.view_id, &key, params, ui),
        }
    }

    /// Re-render a menu after its pending steps changed and push the live keys.
    pub(super) fn menu_rerender(&mut self, view_id: &str, cursor: Cursor) -> Result<InvokeResult> {
        let render = self.render_view(view_id, Some(cursor))?;
        let keymap = self.keymap_notice(view_id);
        Ok(InvokeResult {
            render: Some(render),
            keymap,
            ..InvokeResult::empty()
        })
    }

    /// Toggle an implemented switch in the exact logical menu and rerender it.
    pub fn toggle_menu_switch(
        &mut self,
        view_id: &str,
        generation: u64,
        menu_id: &str,
        key: &str,
    ) -> Result<InvokeResult> {
        let matches_menu = self.views.get(view_id).is_some_and(|view| {
            view.version() == generation && view.menu().is_some_and(|menu| menu.id == menu_id)
        });
        if !matches_menu {
            return Ok(unbound(key));
        }
        let result = self.menu_switch(view_id, key, Cursor { line: 0, column: 0 })?;
        self.publish();
        Ok(result)
    }

    fn menu_switch(&mut self, view_id: &str, key: &str, cursor: Cursor) -> Result<InvokeResult> {
        let item = self
            .views
            .get(view_id)
            .and_then(|view| view.menu())
            .and_then(|menu| menu.item(key).cloned());
        let Some(crate::menu::Item::Switch { status, label, .. }) = item else {
            return Ok(unbound(key));
        };
        if let Some(result) = unsupported_menu_item(status, &label) {
            return Ok(result);
        }
        let toggled = self
            .views
            .get_mut(view_id)
            .and_then(|view| view.menu_mut())
            .is_some_and(|menu| menu.toggle_switch(key));
        if !toggled {
            return Ok(unbound(key));
        }
        self.menu_rerender(view_id, cursor)
    }

    /// Run the item whose complete key was typed, then clear the pending state.
    fn menu_complete(
        &mut self,
        view_id: &str,
        key: &str,
        params: &ViewKeyParams,
        ui: &dyn PromptUi,
    ) -> Result<InvokeResult> {
        let item = self
            .views
            .get(view_id)
            .and_then(|view| view.menu())
            .and_then(|menu| menu.item(key).cloned());
        match item {
            Some(crate::menu::Item::Switch { .. }) => self.menu_switch(view_id, key, params.cursor),
            Some(crate::menu::Item::Option {
                value,
                reader,
                label,
                flag,
                status,
                ..
            }) => {
                if let Some(result) = unsupported_menu_item(status, &label) {
                    return Ok(result);
                }
                if value.is_some() {
                    if let Some(menu) = self.views.get_mut(view_id).and_then(|view| view.menu_mut())
                    {
                        menu.set_option(key, None);
                    }
                    return self.menu_rerender(view_id, params.cursor);
                }

                if reader == Reader::Commit {
                    let menu = self
                        .views
                        .get(view_id)
                        .and_then(|view| view.menu())
                        .expect("menu option")
                        .clone();
                    let request = SelectionRequest {
                        options: GitLogOptions::default(),
                        settings: CommitSelection {
                            purpose: format!("{label} ({flag})"),
                            prompt: format!("Select a commit for {flag}"),
                            multi: false,
                            allow_free_text: true,
                        },
                        operation: SelectionOperation::PickCommit,
                    };
                    return self.push_selection(
                        view_id,
                        request,
                        Some(SessionContinuation::Option {
                            key: key.to_string(),
                            menu,
                            cursor: params.cursor,
                            reader,
                        }),
                        Vec::new(),
                    );
                }

                if matches!(reader, Reader::Ref | Reader::Remote) {
                    let pick = self.option_pick(key, &label, &flag, reader)?;
                    let menu = self
                        .views
                        .get(view_id)
                        .and_then(|view| view.menu())
                        .expect("menu option")
                        .clone();
                    self.prepare_pick(
                        view_id,
                        Some(params.cursor),
                        pick,
                        SessionContinuation::Option {
                            key: key.to_string(),
                            menu,
                            cursor: params.cursor,
                            reader,
                        },
                    );
                    return Ok(InvokeResult::empty());
                }

                let request = self.option_prompt(&label, &flag, reader);
                if let Some(result) = self.unsupported_prompt(&request) {
                    return Ok(result);
                }
                let menu = self
                    .views
                    .get(view_id)
                    .and_then(|view| view.menu())
                    .expect("menu option")
                    .clone();
                self.prepare_prompt(
                    view_id,
                    Some(params.cursor),
                    request,
                    SessionContinuation::Option {
                        key: key.to_string(),
                        menu,
                        cursor: params.cursor,
                        reader,
                    },
                );
                Ok(InvokeResult::empty())
            }
            Some(crate::menu::Item::Action { .. }) => {
                let mut result = self.menu_action(view_id, key, params, ui)?;
                if result.keymap.is_none() {
                    result.keymap = self.keymap_notice(view_id);
                }
                Ok(result)
            }
            None => Ok(unbound(key)),
        }
    }

    /// Build the pick request for a ref or remote menu option.
    fn option_pick(
        &self,
        key: &str,
        label: &str,
        flag: &str,
        reader: Reader,
    ) -> Result<ViewPickParams> {
        let repo = GitRepo::open(&self.repo_path)?;
        let names = match reader {
            Reader::Remote => repo.remote_names()?,
            _ => repo.ref_names()?,
        };
        let items = names
            .into_iter()
            .map(|name| PickItem {
                id: name.clone(),
                label: name,
                description: None,
                detail: None,
                group: None,
            })
            .collect();
        Ok(ViewPickParams {
            pick_id: String::new(),
            purpose: format!("option:{key}"),
            prompt: format!("{label} ({flag})"),
            multi: false,
            allow_free_text: true,
            preselection: Vec::new(),
            items,
            fallback_view: None,
        })
    }

    /// Build the prompt a menu option asks for, choosing the kind by `reader`.
    ///
    /// Free-form revision readers (`Ref`, `Commit`) are text for now; picking a
    /// revision from a select-mode view arrives with R-5.x.
    fn option_prompt(&mut self, label: &str, flag: &str, reader: Reader) -> ViewPromptParams {
        self.next_prompt += 1;
        let prompt_id = format!("prompt-{}", self.next_prompt);
        let title = format!("{label} ({flag})");
        match reader {
            Reader::Choice => ViewPromptParams {
                prompt_id,
                kind: PromptKind::Choice,
                title,
                detail: None,
                yes_label: None,
                no_label: None,
                destructive: None,
                default: None,
                placeholder: None,
                secret: None,
                items: Some(Vec::new()),
                allow_custom: Some(true),
                preselected: None,
                min: None,
                max: None,
            },
            _ => ViewPromptParams {
                prompt_id,
                kind: PromptKind::Text,
                title,
                detail: None,
                yes_label: None,
                no_label: None,
                destructive: None,
                default: None,
                placeholder: Some(flag.trim_end_matches('=').to_string()),
                secret: None,
                items: None,
                allow_custom: None,
                preselected: None,
                min: None,
                max: None,
            },
        }
    }

    /// Check a free-text option value still names a usable revision.
    pub fn validate_option_value(
        &self,
        reader: Reader,
        value: &str,
    ) -> std::result::Result<(), String> {
        match reader {
            Reader::Commit => {
                let repo = GitRepo::open(&self.repo_path)
                    .map_err(|_| format!("Unknown commit: {value}"))?;
                repo.resolve_commit(value)
                    .map_err(|_| format!("Unknown commit: {value}"))?;
                Ok(())
            }
            Reader::Ref => {
                let repo = GitRepo::open(&self.repo_path)
                    .map_err(|_| format!("Unknown ref or remote: {value}"))?;
                if repo.verify_ref_or_remote(value) {
                    Ok(())
                } else {
                    Err(format!("Unknown ref or remote: {value}"))
                }
            }
            Reader::Remote => {
                let repo = GitRepo::open(&self.repo_path)
                    .map_err(|_| format!("Unknown remote: {value}"))?;
                if repo.verify_remote(value) {
                    Ok(())
                } else {
                    Err(format!("Unknown remote: {value}"))
                }
            }
            _ => Ok(()),
        }
    }

    /// Run a menu action, reporting Planned items and routing
    /// Implemented commands through the registry.
    fn menu_action(
        &mut self,
        view_id: &str,
        key: &str,
        params: &ViewKeyParams,
        ui: &dyn PromptUi,
    ) -> Result<InvokeResult> {
        let action = self
            .views
            .get(view_id)
            .and_then(|view| view.menu())
            .and_then(|menu| menu.item(key).cloned());
        let Some(crate::menu::Item::Action {
            command,
            label,
            status,
            ..
        }) = action
        else {
            return Ok(unbound(key));
        };

        match status {
            MenuStatus::Planned { roadmap_id } => Ok(InvokeResult {
                effects: vec![Effect::message(
                    MessageLevel::Info,
                    format!("Not implemented yet: {label} (roadmap {roadmap_id})"),
                )],
                ..InvokeResult::empty()
            }),
            MenuStatus::Implemented => {
                if !matches!(
                    command.as_str(),
                    "common.cancel" | "menu.back" | "menu.reset-args"
                ) {
                    let options = self
                        .views
                        .get(view_id)
                        .and_then(|view| view.menu())
                        .map(|menu| {
                            menu.groups
                                .iter()
                                .flat_map(|group| group.items.iter())
                                .filter_map(|item| match item {
                                    crate::menu::Item::Option {
                                        value: Some(value),
                                        reader,
                                        label,
                                        ..
                                    } => Some((*reader, label.clone(), value.clone())),
                                    _ => None,
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    for (reader, label, value) in options {
                        if let Err(message) = self.validate_option_value(reader, &value) {
                            return Ok(InvokeResult {
                                effects: vec![Effect::message(
                                    MessageLevel::Warn,
                                    format!("The selected {label} is no longer valid: {message}"),
                                )],
                                ..InvokeResult::empty()
                            });
                        }
                    }
                }
                let Some(registered) = commands::registry().get(&command) else {
                    return Ok(InvokeResult {
                        effects: vec![Effect::message(
                            MessageLevel::Info,
                            format!("Not implemented yet: {label} (roadmap R-2.8)"),
                        )],
                        ..InvokeResult::empty()
                    });
                };
                let scope = self
                    .views
                    .get(view_id)
                    .map(|view| view.descriptor().scope.as_str().to_string())
                    .unwrap_or_default();
                let menu_args = self
                    .views
                    .get(view_id)
                    .and_then(|view| view.menu())
                    .map(|menu| menu.args());
                let serialized_args = menu_args.as_ref().map(serde_json::to_value).transpose()?;
                if registered.applies_to(&scope) {
                    return self.dispatch(
                        crate::dispatch::DispatchRequest {
                            action_target: None,
                            command: registered.id.to_string(),
                            target: crate::dispatch::DispatchTarget {
                                view_id: view_id.to_string(),
                                version: Some(params.version),
                                cursor: Some(params.cursor),
                                selection: params.selection,
                                key: Some(key.to_string()),
                            },
                            args: serialized_args,
                        },
                        ui,
                    );
                }

                // The action belongs to an earlier view: close this menu (and
                // any menus stacked above it) and run the command there, so
                // e.g. an action that refreshes the status buffer never runs
                // against a menu.
                let target = match self.matching_ancestor(view_id, registered) {
                    Some(target) => target,
                    None => {
                        return Ok(InvokeResult {
                            effects: vec![Effect::message(
                                MessageLevel::Info,
                                "Not available in this view",
                            )],
                            ..InvokeResult::empty()
                        });
                    }
                };

                let mut effects = Vec::new();
                let mut keymap = None;
                loop {
                    let top = match self.views.iter().last() {
                        Some(view) => view.descriptor().id.clone(),
                        None => break,
                    };
                    if top == target {
                        break;
                    }
                    let popped = self.pop_view(&top)?;
                    effects.extend(popped.effects);
                    if popped.keymap.is_some() {
                        keymap = popped.keymap;
                    }
                }
                let mut run = self.dispatch(
                    crate::dispatch::DispatchRequest {
                        action_target: None,
                        command: registered.id.to_string(),
                        target: crate::dispatch::DispatchTarget {
                            view_id: target,
                            version: None,
                            cursor: None,
                            selection: None,
                            key: Some(key.to_string()),
                        },
                        args: serialized_args,
                    },
                    ui,
                )?;
                effects.extend(run.effects);
                run.effects = effects;
                if run.keymap.is_none() {
                    run.keymap = keymap;
                }
                Ok(run)
            }
        }
    }

    /// Execute an exact retained Plain menu action through normal menu semantics.
    pub fn run_plain_menu_action(
        &mut self,
        view_id: &str,
        generation: u64,
        menu_id: &str,
        key: &str,
        ui: &dyn PromptUi,
    ) -> Result<InvokeResult> {
        let valid = self.views.get(view_id).is_some_and(|view| view.version() == generation
            && view.menu().is_some_and(|menu| menu.id == menu_id
                && matches!(menu.item(key), Some(crate::menu::Item::Action { command, status: MenuStatus::Implemented, .. })
                    if crate::dispatch::plain_menu_action(command))));
        if !valid {
            return Ok(unbound(key));
        }
        let result = self.menu_action(
            view_id,
            key,
            &ViewKeyParams {
                view_id: view_id.to_string(),
                version: generation,
                key: key.to_string(),
                editor_mode: crate::keymap::EditorMode::Normal,
                cursor: Cursor { line: 0, column: 0 },
                selection: None,
            },
            ui,
        )?;
        self.publish();
        Ok(result)
    }

    /// Stage the prompt for an exact retained Plain menu option row.
    ///
    /// Any mismatch (unknown view, stale generation, wrong menu, non-option or
    /// unimplemented row) leaves state untouched and returns `Done` for `key`.
    pub fn start_plain_menu_option(
        &mut self,
        view_id: &str,
        generation: u64,
        menu_id: &str,
        key: &str,
    ) -> Result<Execution> {
        let item = self
            .views
            .get(view_id)
            .filter(|view| view.version() == generation)
            .and_then(|view| view.menu())
            .filter(|menu| menu.id == menu_id)
            .and_then(|menu| menu.item(key).cloned());
        let Some(crate::menu::Item::Option {
            value,
            reader,
            label,
            flag,
            status: MenuStatus::Implemented,
            ..
        }) = item
        else {
            return Ok(Execution::Done(unbound(key)));
        };
        let menu = self
            .views
            .get(view_id)
            .and_then(|view| view.menu())
            .expect("menu checked above")
            .clone();
        let cursor = Cursor { line: 0, column: 0 };
        if value.is_some() {
            if let Some(menu) = self.views.get_mut(view_id).and_then(|view| view.menu_mut()) {
                menu.set_option(key, None);
            }
            let result = self.menu_rerender(view_id, cursor)?;
            self.publish();
            return Ok(Execution::Done(result));
        }
        if reader == Reader::Commit {
            let request = SelectionRequest {
                options: GitLogOptions::default(),
                settings: CommitSelection {
                    purpose: format!("{label} ({flag})"),
                    prompt: format!("Select a commit for {flag}"),
                    multi: false,
                    allow_free_text: true,
                },
                operation: SelectionOperation::PickCommit,
            };
            let result = self.push_selection(
                view_id,
                request,
                Some(SessionContinuation::Option {
                    key: key.to_string(),
                    menu,
                    cursor,
                    reader,
                }),
                Vec::new(),
            )?;
            self.publish();
            return Ok(Execution::Done(result));
        }
        if matches!(reader, Reader::Ref | Reader::Remote) {
            let pick = self.option_pick(key, &label, &flag, reader)?;
            self.prepare_pick(
                view_id,
                Some(cursor),
                pick,
                SessionContinuation::Option {
                    key: key.to_string(),
                    menu,
                    cursor,
                    reader,
                },
            );
            let pick = self
                .pending_pick_request()
                .expect("pick just staged");
            self.publish();
            return Ok(Execution::Pick(pick));
        }
        let request = self.option_prompt(&label, &flag, reader);
        if let Some(result) = self.unsupported_prompt(&request) {
            return Ok(Execution::Done(result));
        }
        self.prepare_prompt(
            view_id,
            Some(cursor),
            request,
            SessionContinuation::Option {
                key: key.to_string(),
                menu,
                cursor,
                reader,
            },
        );
        let request = self
            .pending_prompt
            .as_ref()
            .expect("prompt just staged")
            .request
            .clone();
        self.publish();
        Ok(Execution::Prompt(request))
    }
}
