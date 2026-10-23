//! Sessions and the view stack.
//!
//! One [`Session`] per repository root. It owns the open views, their rendered
//! trees and the last version each client acknowledged. Rendering is full on
//! the server; the wire form is a line-range update unless the client has not
//! acked the base version.

use crate::commands::{self, CommandStatus, Scope};
use crate::diff_text;
use crate::keyhandler::{self, MenuResolution, PendingChord, Resolution};
use crate::keymap::Keymap;
use crate::menu::{Menu, MenuStatus, MenuView, Reader};
use crate::process_log::ProcessLogView;
use crate::protocol::{
    Cursor, Effect, MessageLevel, PromptChoiceAnswer, PromptKind, Selection, ViewInvokeParams,
    ViewKeyParams, ViewKeymapParams, ViewPromptParams, ViewRender, ViewSetModeParams,
};
use crate::status_view::StatusView;
use crate::tree::{Node, NodeKind, Tree};
use crate::ui::PromptUi;
use crate::view::{
    Ctx, Placement, View, ViewDescriptor, ViewKind, ViewMode, Viewport, VisibilityCommand,
};
use crate::{Error, Result};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

/// The command that leaves or enters text mode in every scope.
const TOGGLE_TEXT_MODE: &str = "common.toggle-text-mode";

/// The URI a status view registers, e.g. `magit://status/home/me/repo`.
pub fn status_uri(repo_path: &str) -> String {
    format!("magit://status/{}", repo_path.trim_start_matches('/'))
}

/// One open view plus the versions the client has and has not acknowledged.
pub struct OpenView {
    descriptor: ViewDescriptor,
    view: Box<dyn View>,
    tree: Tree,
    version: u64,
    acked: Option<Tree>,
    acked_version: Option<u64>,
    /// Chord steps collected for this view since the last complete match.
    pending: PendingChord,
    /// A menu's pending key steps (`-`, `=` or a chord prefix).
    menu_pending: Vec<String>,
    /// The cursor the pending chord started at, to cancel it on cursor moves.
    key_cursor: Option<Cursor>,
    /// The current mode; text mode is plain editing with no server keys.
    mode: ViewMode,
    /// A refresh arrived while in text mode; it is applied on exit.
    dirty: bool,
}

impl OpenView {
    /// The descriptor sent to the client when the view was opened.
    pub fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    /// The most recently rendered tree.
    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// The mode this view is currently in.
    pub fn mode(&self) -> ViewMode {
        self.mode
    }

    /// The kind of this view.
    pub fn kind(&self) -> &ViewKind {
        &self.descriptor.kind
    }

    /// The structured menu model, when this is a menu view.
    pub fn menu(&self) -> Option<Menu> {
        self.view.menu()
    }

    /// The menu model for server-side key handling.
    pub fn menu_mut(&mut self) -> Option<&mut Menu> {
        self.view.menu_mut()
    }

    /// Whether this view is a menu.
    pub fn is_menu(&self) -> bool {
        matches!(self.descriptor.kind, ViewKind::Menu(_))
    }

    /// The menu steps typed so far, in canonical notation (`-`, `g`, …).
    pub fn pending(&self) -> String {
        self.menu_pending.join(" ")
    }

    /// Record a refresh for later: it is applied when text mode ends.
    fn queue_refresh(&mut self) {
        self.dirty = true;
    }

    /// Enter text mode: plain editing, no server keys except the toggle.
    fn enter_text_mode(&mut self) {
        self.mode = ViewMode::Text;
        self.descriptor.mode = ViewMode::Text;
        self.pending.clear();
    }

    /// Leave text mode: apply a queued refresh and replace the whole document.
    ///
    /// A full replace is required because the client's text diverged while the
    /// user edited; there is no common base to diff against.
    fn leave_text_mode(&mut self, cursor: Option<Cursor>) -> Result<ViewRender> {
        self.mode = ViewMode::Interactive;
        self.descriptor.mode = ViewMode::Interactive;
        if std::mem::take(&mut self.dirty) {
            self.view.on_refresh()?;
        }
        self.render_full(cursor)
    }

    /// The tree of the last acknowledged version, if any.
    pub fn acked_tree(&self) -> Option<&Tree> {
        self.acked.as_ref()
    }

    /// Apply a visibility command, resolving against the caller's tree.
    pub(crate) fn set_visibility(
        &mut self,
        tree: &Tree,
        line: Option<usize>,
        command: VisibilityCommand,
    ) -> bool {
        self.view.set_visibility(tree, line, command)
    }

    /// The current version, incremented on every render.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The last version the client acknowledged.
    pub fn acked_version(&self) -> Option<u64> {
        self.acked_version
    }

    /// Re-read the backing state, then render and diff against the last tree.
    fn refresh_and_render(&mut self, cursor: Option<Cursor>) -> Result<ViewRender> {
        self.view.on_refresh()?;
        self.render(cursor, None)
    }
    /// Render without touching the repository, diffing against the last tree.
    ///
    /// `forced` replaces the identity-based successor cursor; section
    /// navigation uses it to move somewhere the old node cannot describe.
    fn render(&mut self, cursor: Option<Cursor>, forced: Option<Cursor>) -> Result<ViewRender> {
        let old = self.tree.clone();
        let old_version = self.version;
        let pending = self.menu_pending.clone();
        let mut tree = self.view.render(&Ctx {
            viewport: self.descriptor.viewport,
            pending: &pending,
        });
        let version = old_version + 1;
        tree.version = version;

        let successor =
            forced.or_else(|| cursor.map(|cursor| successor_cursor(&old, &tree, cursor)));
        // Edits are only safe when the client holds exactly the previous
        // version; otherwise fall back to a full replace.
        let base_version = (self.acked_version == Some(old_version)).then_some(old_version);
        let (text, edits) = match base_version {
            Some(_) => (None, Some(diff_text::line_edits(&old.lines, &tree.lines))),
            None => (Some(tree.lines.clone()), None),
        };

        self.tree = tree;
        self.version = version;
        Ok(ViewRender {
            view_id: self.descriptor.id.clone(),
            version,
            base_version,
            text,
            edits,
            cursor: successor,
            mode: self.mode,
            pending: self.pending(),
            menu: self.view.menu(),
        })
    }

    /// Render the current tree as a full text replace.
    fn render_full(&mut self, cursor: Option<Cursor>) -> Result<ViewRender> {
        let old = self.tree.clone();
        let pending = self.menu_pending.clone();
        let mut tree = self.view.render(&Ctx {
            viewport: self.descriptor.viewport,
            pending: &pending,
        });
        let version = self.version + 1;
        tree.version = version;
        let successor = cursor.map(|cursor| successor_cursor(&old, &tree, cursor));
        self.tree = tree;
        self.version = version;
        Ok(ViewRender {
            view_id: self.descriptor.id.clone(),
            version,
            base_version: None,
            text: Some(self.tree.lines.clone()),
            edits: None,
            cursor: successor,
            mode: self.mode,
            pending: self.pending(),
            menu: self.view.menu(),
        })
    }
}

/// The stack of views open in one session, most recent last.
#[derive(Default)]
pub struct ViewStack {
    views: Vec<OpenView>,
}

impl ViewStack {
    /// An empty stack.
    pub fn new() -> Self {
        Self { views: Vec::new() }
    }

    /// Push a view onto the top of the stack.
    pub fn push(&mut self, view: OpenView) {
        self.views.push(view);
    }

    /// Look up a view by id.
    pub fn get(&self, id: &str) -> Option<&OpenView> {
        self.views.iter().find(|view| view.descriptor.id == id)
    }

    /// Look up a view by id for mutation.
    pub fn get_mut(&mut self, id: &str) -> Option<&mut OpenView> {
        self.views.iter_mut().find(|view| view.descriptor.id == id)
    }

    /// Remove a view by id.
    pub fn remove(&mut self, id: &str) -> Option<OpenView> {
        let index = self
            .views
            .iter()
            .position(|view| view.descriptor.id == id)?;
        Some(self.views.remove(index))
    }

    /// Iterate over the open views in stack order.
    pub fn iter(&self) -> impl Iterator<Item = &OpenView> {
        self.views.iter()
    }

    /// The view immediately below `id` in the stack.
    pub fn previous_of(&self, id: &str) -> Option<&OpenView> {
        let index = self
            .views
            .iter()
            .position(|view| view.descriptor.id == id)?;
        self.views.get(index.checked_sub(1)?)
    }

    /// Whether no views are open.
    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }

    /// Drop every view, e.g. when `common.quit` runs.
    pub fn clear(&mut self) {
        self.views.clear();
    }
}

/// One repository's open views and render versions.
pub struct Session {
    repo_path: String,
    views: ViewStack,
    keymaps: Arc<BTreeMap<String, Keymap>>,
    /// The next prompt id handed to a prompt request.
    next_prompt: u64,
}

/// What a `view/invoke` produced: effects plus the render that follows it.
#[derive(Debug, Clone, PartialEq)]
pub struct InvokeResult {
    pub effects: Vec<Effect>,
    pub render: Option<ViewRender>,
    /// A fresh keymap to push when the view entered or left text mode.
    pub keymap: Option<ViewKeymapParams>,
    /// Initial renders and keymaps for views the effects pushed or opened.
    pub opened: Vec<(ViewRender, Option<ViewKeymapParams>)>,
    /// The repository whose `git commit` must run on a detached worker.
    pub commit: Option<String>,
}

impl InvokeResult {
    fn empty() -> Self {
        Self {
            effects: Vec::new(),
            render: None,
            keymap: None,
            opened: Vec::new(),
            commit: None,
        }
    }
}

impl Session {
    /// Create a session for a repository root.
    pub fn new(repo_path: impl Into<String>) -> Self {
        Self {
            repo_path: repo_path.into(),
            views: ViewStack::new(),
            keymaps: Arc::new(BTreeMap::new()),
            next_prompt: 0,
        }
    }

    /// Replace the resolved keymaps, e.g. after a config hot reload.
    pub fn set_keymaps(&mut self, keymaps: Arc<BTreeMap<String, Keymap>>) {
        self.keymaps = keymaps;
    }

    /// The complete keymap to push for `view_id`, if the scope resolves.
    ///
    /// In text mode only the toggle remains bound: every other key is plain
    /// editing, and capture is off.
    pub fn keymap_notice(&self, view_id: &str) -> Option<ViewKeymapParams> {
        let view = self.views.get(view_id)?;
        let scope = view.descriptor().scope.as_str();
        let keymap = self.keymap_for(scope)?;
        let text = view.mode() == ViewMode::Text;
        let bindings = if text {
            keymap.only(TOGGLE_TEXT_MODE)
        } else {
            keymap.bindings().to_vec()
        };
        Some(ViewKeymapParams {
            view_id: view_id.to_string(),
            scope: scope.to_string(),
            version: view.version,
            capture: view.descriptor().capture && !text,
            bindings,
            mode: view.mode(),
            pending: if text { String::new() } else { view.pending() },
            live_keys: if text || !view.is_menu() {
                Vec::new()
            } else {
                menu_live_keys(view)
            },
        })
    }

    /// The keymap for a view scope. Every `menu.*` scope shares `menu`.
    fn keymap_for(&self, scope: &str) -> Option<&Keymap> {
        self.keymaps.get(scope).or_else(|| {
            if scope.starts_with("menu.") {
                self.keymaps.get("menu")
            } else {
                None
            }
        })
    }

    /// The repository this session renders.
    pub fn repo_path(&self) -> &str {
        &self.repo_path
    }

    /// The open views.
    pub fn views(&self) -> &ViewStack {
        &self.views
    }

    /// Open a view and return its descriptor plus the initial full render.
    pub fn open(
        &mut self,
        id: impl Into<String>,
        kind: ViewKind,
        placement: Placement,
        viewport: Viewport,
    ) -> Result<(ViewDescriptor, ViewRender)> {
        self.open_with_args(id, kind, placement, viewport, None)
    }

    /// Open a view with kind-specific arguments (e.g. preset menu arguments).
    pub fn open_with_args(
        &mut self,
        id: impl Into<String>,
        kind: ViewKind,
        placement: Placement,
        viewport: Viewport,
        args: Option<&Value>,
    ) -> Result<(ViewDescriptor, ViewRender)> {
        let id = id.into();
        if self.views.get(&id).is_some() {
            return Err(Error::new(format!("view already open: {id}")));
        }

        let mut view = create_view(&self.repo_path, &kind, args, &self.keymaps)?;
        let mut tree = view.render(&Ctx {
            viewport,
            pending: &[],
        });
        let version = 1;
        tree.version = version;

        let descriptor = ViewDescriptor {
            id: id.clone(),
            kind: view.kind(),
            uri: view_uri(&self.repo_path, &kind, &id),
            title: view.title(),
            placement,
            mode: view.mode(),
            scope: view.scope(),
            capture: view.capture(),
            viewport,
        };
        let render = ViewRender {
            view_id: id.clone(),
            version,
            base_version: None,
            text: Some(tree.lines.clone()),
            edits: None,
            cursor: None,
            mode: descriptor.mode,
            pending: String::new(),
            menu: view.menu(),
        };
        self.views.push(OpenView {
            descriptor: descriptor.clone(),
            view,
            tree,
            version,
            acked: None,
            acked_version: None,
            pending: PendingChord::default(),
            menu_pending: Vec::new(),
            key_cursor: None,
            mode: descriptor.mode,
            dirty: false,
        });
        Ok((descriptor, render))
    }

    /// Close a view; closing one the client already closed is a no-op.
    pub fn close(&mut self, view_id: &str) -> Result<()> {
        self.views.remove(view_id);
        Ok(())
    }

    /// Record the client's ack of `version`.
    pub fn ack(&mut self, view_id: &str, version: u64) -> Result<()> {
        let view = self
            .views
            .get_mut(view_id)
            .ok_or_else(|| Error::new(format!("unknown view: {view_id}")))?;
        if view.version == version {
            view.acked = Some(view.tree.clone());
            view.acked_version = Some(version);
        }
        Ok(())
    }

    /// Record a new viewport size from the client.
    ///
    /// Menus re-lay out their columns immediately; other views keep their
    /// current render until the next refresh.
    pub fn resize(&mut self, view_id: &str, viewport: Viewport) -> Result<Option<ViewRender>> {
        let view = self
            .views
            .get_mut(view_id)
            .ok_or_else(|| Error::new(format!("unknown view: {view_id}")))?;
        view.descriptor.viewport = viewport;
        if view.is_menu() {
            Ok(Some(view.render(None, None)?))
        } else {
            Ok(None)
        }
    }

    /// The current full text, for a client that lost an update.
    pub fn resync(&self, view_id: &str) -> Result<ViewRender> {
        let view = self
            .views
            .get(view_id)
            .ok_or_else(|| Error::new(format!("unknown view: {view_id}")))?;
        Ok(ViewRender {
            view_id: view_id.to_string(),
            version: view.version,
            base_version: None,
            text: Some(view.tree.lines.clone()),
            edits: None,
            cursor: None,
            mode: view.mode,
            pending: view.pending(),
            menu: view.menu(),
        })
    }

    /// Run a registered command against a view by id.
    ///
    /// Kept for callers with no prompt UI (tests and non-interactive paths); it
    /// delegates to [`Session::invoke_ui`] with a [`crate::ui::NullUi`].
    pub fn invoke(&mut self, params: &ViewInvokeParams) -> Result<InvokeResult> {
        self.invoke_ui(params, &crate::ui::NullUi)
    }

    /// Run a registered command, asking the user through `ui` when needed.
    pub fn invoke_ui(
        &mut self,
        params: &ViewInvokeParams,
        ui: &dyn PromptUi,
    ) -> Result<InvokeResult> {
        self.run_command(
            &params.view_id,
            &params.command,
            params.cursor,
            params.selection,
            params.args.as_ref(),
            None,
            ui,
        )
    }

    /// Handle one key step: resolve it through the view's keymap and either run
    /// the matched command or answer `PassThrough`.
    ///
    /// Cursor and selection come from the request, so the command path is
    /// identical to `view/invoke`; the node is resolved by identity against the
    /// acked tree, never by a raw line index. Menus use their own two-step
    /// matcher instead of the profile keymap.
    pub fn key(&mut self, params: &ViewKeyParams) -> Result<InvokeResult> {
        self.key_ui(params, &crate::ui::NullUi)
    }

    /// Handle one key step, asking the user through `ui` when the command does.
    pub fn key_ui(&mut self, params: &ViewKeyParams, ui: &dyn PromptUi) -> Result<InvokeResult> {
        let mode = params.editor_mode;
        let (scope, capture, cursor_moved, is_menu) = {
            let view = self
                .views
                .get_mut(&params.view_id)
                .ok_or_else(|| Error::new(format!("unknown view: {}", params.view_id)))?;
            let moved = view.key_cursor.is_some() && view.key_cursor != Some(params.cursor);
            view.key_cursor = Some(params.cursor);
            (
                view.descriptor().scope.as_str().to_string(),
                view.descriptor().capture,
                moved,
                view.is_menu(),
            )
        };

        if is_menu {
            return self.menu_key(params, cursor_moved, ui);
        }

        let keymap = self.keymap_for(&scope).cloned();

        let resolution = {
            let view = self
                .views
                .get_mut(&params.view_id)
                .expect("view checked above");
            if cursor_moved {
                view.pending.clear();
            }
            keyhandler::resolve(keymap.as_ref(), &mut view.pending, &params.key, mode, capture)?
        };

        match resolution {
            Resolution::Pending => Ok(InvokeResult::empty()),
            Resolution::Run { command, chord } => self.run_command(
                &params.view_id,
                &command,
                Some(params.cursor),
                params.selection,
                None,
                Some(chord),
                ui,
            ),
            Resolution::PassThrough { chord } => Ok(InvokeResult {
                effects: vec![Effect::PassThrough { key: chord }],
                ..InvokeResult::empty()
            }),
            Resolution::Swallow { chord } => Ok(InvokeResult {
                effects: vec![Effect::message(
                    MessageLevel::Info,
                    format!("Unbound key: {chord}"),
                )],
                ..InvokeResult::empty()
            }),
        }
    }

    /// Resolve a key inside a menu against its item keys and pending steps.
    fn menu_key(
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
            MenuResolution::Complete(key) => {
                self.menu_complete(&params.view_id, &key, params, ui)
            }
        }
    }

    /// Re-render a menu after its pending steps changed and push the live keys.
    fn menu_rerender(&mut self, view_id: &str, cursor: Cursor) -> Result<InvokeResult> {
        let render = self.render_view(view_id, Some(cursor))?;
        let keymap = self.keymap_notice(view_id);
        Ok(InvokeResult {
            render: Some(render),
            keymap,
            ..InvokeResult::empty()
        })
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
            Some(crate::menu::Item::Switch { .. }) => {
                let toggled = self
                    .views
                    .get_mut(view_id)
                    .and_then(|view| view.menu_mut())
                    .map(|menu| menu.toggle_switch(key))
                    .unwrap_or(false);
                if !toggled {
                    return Ok(unbound(key));
                }
                self.menu_rerender(view_id, params.cursor)
            }
            Some(crate::menu::Item::Option {
                value,
                reader,
                label,
                flag,
                ..
            }) => {
                if value.is_some() {
                    if let Some(menu) = self
                        .views
                        .get_mut(view_id)
                        .and_then(|view| view.menu_mut())
                    {
                        menu.set_option(key, None);
                    }
                    return self.menu_rerender(view_id, params.cursor);
                }

                let request = self.option_prompt(&label, &flag, reader);
                let answer = match reader {
                    Reader::Choice => ui.choice(&request)?.map(|answer| match answer {
                        PromptChoiceAnswer::Id { id } => id,
                        PromptChoiceAnswer::Custom { custom } => custom,
                    }),
                    _ => ui.text(&request)?,
                };
                let Some(value) = answer else {
                    // Cancelling leaves the option as it was, but the pending
                    // steps were already consumed.
                    return Ok(InvokeResult {
                        keymap: self.keymap_notice(view_id),
                        ..InvokeResult::empty()
                    });
                };
                if let Some(menu) = self
                    .views
                    .get_mut(view_id)
                    .and_then(|view| view.menu_mut())
                {
                    menu.set_option(key, Some(value));
                }
                self.menu_rerender(view_id, params.cursor)
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

    /// Run a menu action, reporting Planned/Unavailable items and routing
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
            MenuStatus::Unavailable { reason } => Ok(InvokeResult {
                effects: vec![Effect::message(MessageLevel::Info, reason)],
                ..InvokeResult::empty()
            }),
            MenuStatus::Implemented => {
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
                if registered.scope.matches(&scope) {
                    return self.run_command(
                        view_id,
                        registered.id,
                        Some(params.cursor),
                        params.selection,
                        None,
                        Some(key.to_string()),
                        ui,
                    );
                }

                // The action belongs to an earlier view: close this menu (and
                // any menus stacked above it) and run the command there, so
                // e.g. an action that refreshes the status buffer never runs
                // against a menu.
                let target = match self.matching_ancestor(view_id, registered.scope) {
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
                let mut run = self.run_command(
                    &target,
                    registered.id,
                    None,
                    None,
                    None,
                    Some(key.to_string()),
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

    /// The nearest view below `view_id` that a command in `scope` applies to.
    fn matching_ancestor(&self, view_id: &str, scope: Scope) -> Option<String> {
        let mut current = self.views.previous_of(view_id);
        while let Some(view) = current {
            let id = view.descriptor().id.clone();
            if scope.matches(view.descriptor().scope.as_str()) {
                return Some(id);
            }
            current = self.views.previous_of(&id);
        }
        None
    }

    /// Remove a view and return focus to the one below it.
    fn pop_view(&mut self, view_id: &str) -> Result<InvokeResult> {
        self.views.remove(view_id);
        let mut effects = vec![Effect::PopView {
            view_id: view_id.to_string(),
        }];
        let keymap = match self.views.iter().last() {
            Some(previous) => {
                let id = previous.descriptor().id.clone();
                effects.push(Effect::FocusView { view_id: id.clone() });
                self.keymap_notice(&id)
            }
            None => None,
        };
        Ok(InvokeResult {
            effects,
            render: None,
            keymap,
            opened: Vec::new(),
            commit: None,
        })
    }

    /// Render a view without touching the repository.
    fn render_view(&mut self, view_id: &str, cursor: Option<Cursor>) -> Result<ViewRender> {
        self.views
            .get_mut(view_id)
            .ok_or_else(|| Error::new(format!("unknown view: {view_id}")))?
            .render(cursor, None)
    }

    /// Push a menu above `parent_id` and return its descriptor and first render.
    fn push_menu(&mut self, parent_id: &str, menu: Menu) -> Result<(ViewDescriptor, ViewRender)> {
        let kind = ViewKind::Menu(format!("menu.{}", menu.id));
        self.push_view(
            parent_id,
            kind,
            Placement::Bottom,
            Box::new(MenuView::new(menu)),
        )
    }

    /// Create a non-menu view and push it above `parent_id`.
    fn push_kind(
        &mut self,
        parent_id: &str,
        kind: ViewKind,
        placement: Placement,
    ) -> Result<(ViewDescriptor, ViewRender)> {
        let view = create_view(&self.repo_path, &kind, None, &self.keymaps)?;
        self.push_view(parent_id, kind, placement, view)
    }

    /// Push `view` above `parent_id` under a fresh id and render it once.
    fn push_view(
        &mut self,
        parent_id: &str,
        kind: ViewKind,
        placement: Placement,
        mut view: Box<dyn View>,
    ) -> Result<(ViewDescriptor, ViewRender)> {
        let viewport = self
            .views
            .get(parent_id)
            .map(|view| view.descriptor().viewport)
            .unwrap_or_default();
        let base = format!("{parent_id}:{}", kind.as_str());
        let mut id = base.clone();
        let mut suffix = 2;
        while self.views.get(&id).is_some() {
            id = format!("{base}-{suffix}");
            suffix += 1;
        }

        let mut tree = view.render(&Ctx {
            viewport,
            pending: &[],
        });
        let version = 1;
        tree.version = version;
        let descriptor = ViewDescriptor {
            id: id.clone(),
            kind: view.kind(),
            uri: view_uri(&self.repo_path, &kind, &id),
            title: view.title(),
            placement,
            mode: view.mode(),
            scope: view.scope(),
            capture: view.capture(),
            viewport,
        };
        let render = ViewRender {
            view_id: id.clone(),
            version,
            base_version: None,
            text: Some(tree.lines.clone()),
            edits: None,
            cursor: None,
            mode: descriptor.mode,
            pending: String::new(),
            menu: view.menu(),
        };
        self.views.push(OpenView {
            descriptor: descriptor.clone(),
            view,
            tree,
            version,
            acked: None,
            acked_version: None,
            pending: PendingChord::default(),
            menu_pending: Vec::new(),
            key_cursor: None,
            mode: descriptor.mode,
            dirty: false,
        });
        Ok((descriptor, render))
    }

    /// The single execution path shared by `view/invoke` and `view/key`.
    ///
    /// The command table lives in [`crate::commands`]; this method only looks
    /// it up, reports `Planned`/`Unavailable`/scope mismatches, and renders.
    /// In text mode every command but the toggle is refused, so a key press or
    /// invoke can never reach Git while the user edits plain text.
    fn run_command(
        &mut self,
        view_id: &str,
        command_id: &str,
        cursor: Option<Cursor>,
        selection: Option<Selection>,
        args: Option<&serde_json::Value>,
        key: Option<String>,
        ui: &dyn PromptUi,
    ) -> Result<InvokeResult> {
        let Some(command) = commands::registry().get(command_id) else {
            return Err(Error::new(format!("unknown command: {command_id}")));
        };
        let repo_path = self.repo_path.clone();

        let text_mode = self
            .views
            .get(view_id)
            .map(|view| view.mode() == ViewMode::Text)
            .ok_or_else(|| Error::new(format!("unknown view: {view_id}")))?;
        if text_mode && command_id != TOGGLE_TEXT_MODE {
            // A requested refresh is remembered, not run: it applies on exit.
            if command_id == "common.refresh" {
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

        let (effects, outcome, render) = {
            let view = self
                .views
                .get_mut(view_id)
                .ok_or_else(|| Error::new(format!("unknown view: {view_id}")))?;

            match &command.status {
                CommandStatus::Planned { roadmap_id } => {
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
                CommandStatus::Unavailable { reason } => {
                    return Ok(InvokeResult {
                        effects: vec![Effect::message(MessageLevel::Info, *reason)],
                        ..InvokeResult::empty()
                    });
                }
                CommandStatus::Implemented => {}
            }

            if !command.scope.matches(view.descriptor().scope.as_str()) {
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
            )
            .with_cancel(crate::runner::current_cancel());
            let outcome = (command.run)(&mut ctx)?;
            let effects = ctx.into_effects();

            if outcome.pop
                || outcome.open_menu.is_some()
                || outcome.open_view.is_some()
                || outcome.commit
            {
                (effects, outcome, None)
            } else {
                let mode_change = outcome.mode.filter(|mode| *mode != view.mode());
                let render = match mode_change {
                    Some(ViewMode::Text) => {
                        view.enter_text_mode();
                        None
                    }
                    Some(ViewMode::Interactive) => Some(view.leave_text_mode(cursor)?),
                    None => Some(if outcome.refresh {
                        view.refresh_and_render(cursor)?
                    } else {
                        view.render(cursor, outcome.cursor)?
                    }),
                };
                (effects, outcome, render)
            }
        };

        // `common.quit` closes every view of this session; the client gets the
        // same effects and drops its windows.
        if effects.iter().any(|effect| matches!(effect, Effect::Quit)) {
            self.views.clear();
            return Ok(InvokeResult {
                effects,
                ..InvokeResult::empty()
            });
        }

        let mode_changed = outcome.mode.is_some();
        if outcome.pop {
            let mut result = self.pop_view(view_id)?;
            let mut all = effects;
            all.extend(result.effects);
            result.effects = all;
            return Ok(result);
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
            let (descriptor, render) = self.push_menu(&parent_id, menu)?;
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
            let (descriptor, render) = self.push_kind(view_id, kind, placement)?;
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
            commit: outcome.commit.then(|| self.repo_path.clone()),
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

    /// Re-read the backing state and render, unless the view is in text mode.
    ///
    /// Text mode queues the refresh instead: returning `None` keeps the client's
    /// edited text in place. The file watcher (R-3.5) and `common.refresh` both
    /// route through here.
    pub fn refresh(&mut self, view_id: &str) -> Result<Option<ViewRender>> {
        let view = self
            .views
            .get_mut(view_id)
            .ok_or_else(|| Error::new(format!("unknown view: {view_id}")))?;
        if view.mode() == ViewMode::Text {
            view.queue_refresh();
            return Ok(None);
        }
        Ok(Some(view.refresh_and_render(None)?))
    }

    /// Switch a view to `mode` through the same path as the toggle.
    ///
    /// Setting the mode it already has is a no-op.
    pub fn set_mode(&mut self, params: &ViewSetModeParams) -> Result<InvokeResult> {
        self.set_mode_ui(params, &crate::ui::NullUi)
    }

    /// Switch a view to `mode`, asking the user through `ui` when needed.
    pub fn set_mode_ui(
        &mut self,
        params: &ViewSetModeParams,
        ui: &dyn PromptUi,
    ) -> Result<InvokeResult> {
        let current = self
            .views
            .get(&params.view_id)
            .ok_or_else(|| Error::new(format!("unknown view: {}", params.view_id)))?
            .mode();
        if current == params.mode {
            return Ok(InvokeResult::empty());
        }
        // There are exactly two modes, so toggling reaches the requested one.
        self.run_command(
            &params.view_id,
            TOGGLE_TEXT_MODE,
            None,
            None,
            None,
            None,
            ui,
        )
    }

    /// Whether the view showing `uri` is in text mode.
    pub fn is_text_uri(&self, uri: &str) -> bool {
        self.views
            .iter()
            .any(|view| view.descriptor.uri == uri && view.mode() == ViewMode::Text)
    }

    /// The last acknowledged tree for the view showing `uri`.
    ///
    /// Returns `None` while that view is in text mode: §3.4 gives positional
    /// requests no meaning when the client's text may differ from the server's.
    pub fn acked_tree_for_uri(&self, uri: &str) -> Option<Tree> {
        let view = self
            .views
            .iter()
            .find(|view| view.descriptor.uri == uri)?;
        if view.mode() == ViewMode::Text {
            return None;
        }
        Some(view.acked.clone().unwrap_or_else(|| view.tree.clone()))
    }
}

fn create_view(
    repo_path: &str,
    kind: &ViewKind,
    args: Option<&Value>,
    keymaps: &BTreeMap<String, Keymap>,
) -> Result<Box<dyn View>> {
    match kind {
        ViewKind::Status => Ok(Box::new(StatusView::open(repo_path)?)),
        ViewKind::ProcessLog => Ok(Box::new(ProcessLogView::new())),
        ViewKind::Menu(name) => {
            let name = name.trim_start_matches("menu.");
            let mut menu = crate::menus::by_name(name, keymaps)
                .ok_or_else(|| Error::new(format!("The `{name}` menu is not implemented yet.")))?;
            if let Some(saved) = args.and_then(|args| args.get("saved_args")) {
                if let Ok(saved) = serde_json::from_value::<crate::menu::Args>(saved.clone()) {
                    menu.saved_args = saved.clone();
                    menu.apply_args(&saved);
                }
            }
            Ok(Box::new(MenuView::new(menu)))
        }
        ViewKind::Other(name) => Err(Error::new(format!(
            "The `{name}` view is not implemented yet."
        ))),
    }
}

/// A URI unique to one view, so positional requests resolve the right tree.
fn view_uri(repo_path: &str, kind: &ViewKind, id: &str) -> String {
    match kind {
        ViewKind::Status => status_uri(repo_path),
        other => format!("magit://{}/{}", other.as_str(), id),
    }
}

/// The menu keys that do something in the current pending state.
fn menu_live_keys(view: &OpenView) -> Vec<String> {
    let Some(menu) = view.menu() else {
        return Vec::new();
    };
    let pending = &view.menu_pending;
    let mut keys = Vec::new();
    for group in &menu.groups {
        for item in &group.items {
            if crate::menu::key_matches_pending(item.key(), pending) {
                keys.push(item.key().to_string());
            }
        }
    }
    if !pending.is_empty() {
        keys.push("Backspace".to_string());
    }
    for control in ["Escape", "ctrl+c", "q", "ctrl+s", "ctrl+r"] {
        keys.push(control.to_string());
    }
    keys
}

/// A one-line hint for an unbound key inside a capture view.
fn unbound(chord: &str) -> InvokeResult {
    InvokeResult {
        effects: vec![Effect::message(
            MessageLevel::Info,
            format!("Unbound key: {chord}"),
        )],
        ..InvokeResult::empty()
    }
}

/// Find the node in `new` that continues `old` by content, not position.
pub(crate) fn re_resolve<'a>(new: &'a Tree, old: &Node) -> Option<&'a Node> {
    if let (Some(path), Some(hunk_id)) = (&old.path, &old.hunk_id) {
        let hunk = new.nodes.iter().find(|node| {
            node.kind == NodeKind::Hunk
                && node.path.as_deref() == Some(path)
                && node.hunk_id.as_deref() == Some(hunk_id)
        });
        if old.kind == NodeKind::Line {
            if let Some(line) = new.nodes.iter().find(|node| {
                node.kind == NodeKind::Line
                    && node.path.as_deref() == Some(path)
                    && node.hunk_id.as_deref() == Some(hunk_id)
                    && node.marker == old.marker
                    && node.line_index == old.line_index
            }) {
                return Some(line);
            }
            return hunk;
        }
        return hunk;
    }
    if let Some(path) = &old.path {
        return new
            .nodes
            .iter()
            .find(|node| node.kind == NodeKind::File && node.path.as_deref() == Some(path));
    }
    None
}

/// Where the cursor should land after `old` was re-rendered into `new`.
///
/// The old Node ID is tried first; then identity (content), the nearest
/// surviving ancestor, the next sibling and finally the section header.
fn successor_cursor(old: &Tree, new: &Tree, cursor: Cursor) -> Cursor {
    let old_node = old
        .line_nodes
        .get(cursor.line)
        .and_then(|line| old.node(&line.node));
    let old_text = old.lines.get(cursor.line).map(String::as_str);

    let target = old_node.and_then(|node| {
        new.node(&node.id)
            .or_else(|| re_resolve(new, node))
            .or_else(|| nearest_ancestor(old, new, node))
            .or_else(|| next_sibling(old, new, node))
            .or_else(|| section_header(new, node))
    });

    let line = target
        .map(|node| node.start_line)
        .unwrap_or(cursor.line)
        .min(new.lines.len().saturating_sub(1));
    let text = new.lines.get(line).map(String::as_str).unwrap_or("");
    // Keep the column only when the exact line text survived.
    let column = if old_text.is_some() && old_text == Some(text) {
        cursor.column.min(text.len())
    } else {
        0
    };
    Cursor { line, column }
}

fn nearest_ancestor<'a>(old: &Tree, new: &'a Tree, node: &Node) -> Option<&'a Node> {
    let mut current = node;
    while let Some(parent) = old
        .nodes
        .iter()
        .find(|candidate| candidate.children.iter().any(|id| id == &current.id))
    {
        if let Some(found) = new.node(&parent.id) {
            return Some(found);
        }
        current = parent;
    }
    None
}

fn next_sibling<'a>(old: &Tree, new: &'a Tree, node: &Node) -> Option<&'a Node> {
    let parent = old
        .nodes
        .iter()
        .find(|candidate| candidate.children.iter().any(|id| id == &node.id))?;
    let position = parent.children.iter().position(|id| id == &node.id)?;
    parent.children[position + 1..]
        .iter()
        .find_map(|id| new.node(id))
}

fn section_header<'a>(new: &'a Tree, node: &Node) -> Option<&'a Node> {
    let section = node.section.as_ref()?;
    new.node(&format!("section:{section}"))
}
