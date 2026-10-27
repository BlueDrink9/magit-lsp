//! Shared in-process [`TestSession`] harness.
//!
//! The harness owns one real [`Session`], the focused view, the cursor and the
//! scripted prompt UI. It always drives the production `view/key` and
//! `view/invoke` paths, remembers the last render version it applied and sends
//! that version with every key request, so tests never hard-code a version.
//!
//! Cursor targets are resolved through the server tree (`line_nodes`, `path`,
//! `marker`, `text`, `copy`); rendered text is never parsed to find structure.

use super::RepoFixture;
use magit_lsp::config::{Config, Profile};
use magit_lsp::keymap::{self, EditorMode, Keymap};
use magit_lsp::protocol::{
    Cursor, Effect, Selection, ViewInvokeParams, ViewKeyParams, ViewRender, ViewSetModeParams,
};
use magit_lsp::session::{InvokeResult, Session};
use magit_lsp::tree::{Node, NodeKind, Tree};
use magit_lsp::ui::ScriptedUi;
use magit_lsp::view::{Placement, ViewDescriptor, ViewKind, ViewMode, Viewport};
use magit_lsp::Error;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

/// The viewport most status-view tests open with.
pub const STATUS_VIEWPORT: Viewport = Viewport { cols: 80, rows: 24 };

/// The wider viewport menu, prompt and process-log tests use.
pub const WIDE_VIEWPORT: Viewport = Viewport {
    cols: 100,
    rows: 30,
};

/// Resolve the keymaps for `profile`, failing loudly on an invalid profile.
pub fn keymaps(profile: Profile) -> Arc<BTreeMap<String, Keymap>> {
    let config = Config {
        profile,
        ..Default::default()
    };
    Arc::new(keymap::validate(&config).expect("resolve keymaps"))
}

/// The text of every [`Effect::Message`], in order.
pub fn message_texts(effects: &[Effect]) -> Vec<&str> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Message { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// One in-process session driven like a client: key routing, invokes, acks and
/// prompt answers all go through the real [`Session`] entry points.
pub struct TestSession {
    /// The real session under test.
    pub session: Session,
    /// The config whose keymaps this session uses.
    config: Config,
    /// The focused view: where keys and invokes are sent and what inspections
    /// read by default.
    view: String,
    /// The cursor used by [`Self::press`] and [`Self::invoke_here`].
    cursor: Cursor,
    /// The last render version applied for each view.
    versions: BTreeMap<String, u64>,
    /// Answers for commands that ask the user a question.
    pub ui: ScriptedUi,
    /// The version sent with the most recent `view/key` request.
    key_version: u64,
}

impl TestSession {
    /// A session for `repo` with the profile's keymaps and no open view.
    pub fn new(repo: impl Into<String>, profile: Profile) -> Self {
        Self::with_config(
            repo,
            Config {
                profile,
                ..Default::default()
            },
        )
    }

    /// A session for `repo` with an explicit configuration.
    pub fn with_config(repo: impl Into<String>, config: Config) -> Self {
        let mut session = Session::new(repo);
        session.set_keymaps(Arc::new(
            keymap::validate(&config).expect("resolve keymaps"),
        ));
        Self {
            session,
            config,
            view: "v1".to_string(),
            cursor: Cursor { line: 0, column: 0 },
            versions: BTreeMap::new(),
            ui: ScriptedUi::new(),
            key_version: 0,
        }
    }

    /// Open a status view at the default viewport.
    ///
    /// The initial render is returned but not acknowledged: a test that wants
    /// an acked client calls [`Self::ack`], and a test of unacked behaviour
    /// deliberately does not.
    pub fn open_status(fixture: &RepoFixture, profile: Profile) -> (Self, ViewRender) {
        Self::open_status_at(fixture, profile, STATUS_VIEWPORT)
    }

    /// Open a status view at `viewport`.
    pub fn open_status_at(
        fixture: &RepoFixture,
        profile: Profile,
        viewport: Viewport,
    ) -> (Self, ViewRender) {
        let mut test = Self::new(fixture.path().to_string_lossy(), profile);
        let render = test.open_status_view(viewport);
        (test, render)
    }

    /// Open the focused session's status view with [`Self::open_view`].
    pub fn open_status_view(&mut self, viewport: Viewport) -> ViewRender {
        self.open_view("v1", ViewKind::Status, Placement::Main, viewport)
            .1
    }

    /// Open a view, track its initial version and focus it.
    pub fn open_view(
        &mut self,
        id: impl Into<String>,
        kind: ViewKind,
        placement: Placement,
        viewport: Viewport,
    ) -> (ViewDescriptor, ViewRender) {
        let (descriptor, render) = self
            .session
            .open(id, kind, placement, viewport)
            .expect("open view");
        self.apply(&render);
        self.view = descriptor.id.clone();
        (descriptor, render)
    }

    /// Open a view with kind-specific arguments (e.g. a menu's `saved_args`
    /// for the real `view/open` path), track its initial version and focus it.
    pub fn open_view_with_args(
        &mut self,
        id: impl Into<String>,
        kind: ViewKind,
        placement: Placement,
        viewport: Viewport,
        args: &Value,
    ) -> (ViewDescriptor, ViewRender) {
        let (descriptor, render) = self
            .session
            .open_with_args(id, kind, placement, viewport, Some(args))
            .expect("open view with args");
        self.apply(&render);
        self.view = descriptor.id.clone();
        (descriptor, render)
    }

    /// Run a command that pushes a view, focus it and return it with its first
    /// render. The caller still decides when to acknowledge that render.
    pub fn open_via_command(&mut self, command: &str) -> (ViewDescriptor, ViewRender) {
        let result = self.invoke(command);
        let descriptor = result
            .effects
            .iter()
            .find_map(|effect| match effect {
                Effect::PushView { descriptor } => Some(descriptor.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{command} pushed no view"));
        let (render, _keymap) = result
            .opened
            .into_iter()
            .next()
            .expect("the pushed view's initial render");
        (descriptor, render)
    }

    /// Focus `view_id` for later keys, invokes and inspections.
    pub fn focus(&mut self, view_id: impl Into<String>) {
        self.view = view_id.into();
    }

    /// The focused view id.
    pub fn view_id(&self) -> &str {
        &self.view
    }

    /// The focused view's last applied render version.
    pub fn version(&self) -> u64 {
        self.version_of(&self.view)
    }

    /// The last applied render version of `view_id`.
    pub fn version_of(&self, view_id: &str) -> u64 {
        *self
            .versions
            .get(view_id)
            .unwrap_or_else(|| panic!("no render version tracked for view {view_id}"))
    }

    /// The version sent with the most recent key request.
    pub fn last_key_version(&self) -> u64 {
        self.key_version
    }

    /// The focused view's acknowledged version, if the client acked one.
    pub fn acked_version(&self) -> Option<u64> {
        self.acked_version_of(&self.view)
    }

    /// The acknowledged version of `view_id`, if any.
    pub fn acked_version_of(&self, view_id: &str) -> Option<u64> {
        self.session
            .views()
            .get(view_id)
            .and_then(|view| view.acked_version())
    }

    /// Acknowledge the focused view's last applied render.
    pub fn ack(&mut self) {
        self.ack_view(&self.view.clone());
    }

    /// Acknowledge the last applied render of `view_id`.
    pub fn ack_view(&mut self, view_id: &str) {
        let version = self.version_of(view_id);
        self.session
            .ack(view_id, version)
            .unwrap_or_else(|error| panic!("ack {view_id} at version {version}: {error}"));
        assert_eq!(
            self.acked_version_of(view_id),
            Some(version),
            "ack of {view_id} at version {version} did not take"
        );
    }

    /// The cursor used by [`Self::press`] and [`Self::invoke_here`].
    pub fn cursor(&self) -> Cursor {
        self.cursor
    }

    /// Set the cursor used by later presses.
    pub fn set_cursor(&mut self, cursor: Cursor) {
        self.cursor = cursor;
    }

    /// Move the cursor to `line`, column 0, and return it.
    pub fn goto(&mut self, line: usize) -> usize {
        self.cursor = Cursor { line, column: 0 };
        line
    }

    /// Move the cursor to the first diff line for `path` with `marker`.
    pub fn goto_marker(&mut self, path: &str, marker: char) -> usize {
        let line = self
            .tree()
            .line_nodes
            .iter()
            .position(|line| {
                line.kind == NodeKind::Line
                    && line.path.as_deref() == Some(path)
                    && line.marker == Some(marker)
            })
            .unwrap_or_else(|| panic!("no {marker:?} line for {path}"));
        self.goto(line)
    }

    /// Move the cursor to the first diff line for `path` whose text is `text`.
    pub fn goto_text(&mut self, path: &str, text: &str) -> usize {
        let line = self
            .tree()
            .line_nodes
            .iter()
            .position(|line| {
                line.kind == NodeKind::Line
                    && line.path.as_deref() == Some(path)
                    && line.text.as_deref() == Some(text)
            })
            .unwrap_or_else(|| panic!("no line with text {text:?} for {path}"));
        self.goto(line)
    }

    /// Move the cursor to the first line carrying `marker`, whatever its path.
    pub fn goto_first_marker(&mut self, marker: char) -> usize {
        let line = self
            .tree()
            .line_nodes
            .iter()
            .position(|line| line.kind == NodeKind::Line && line.marker == Some(marker))
            .unwrap_or_else(|| panic!("no {marker:?} line"));
        self.goto(line)
    }

    /// Move the cursor to the first rendered line of `kind`.
    pub fn goto_first(&mut self, kind: NodeKind) -> usize {
        let line = self
            .tree()
            .line_nodes
            .iter()
            .position(|line| line.kind == kind)
            .unwrap_or_else(|| panic!("no {kind:?} line"));
        self.goto(line)
    }

    /// Move the cursor to the section header named `section`.
    pub fn goto_section(&mut self, section: &str) -> usize {
        let line = self
            .tree()
            .line_nodes
            .iter()
            .position(|line| {
                line.kind == NodeKind::Section && line.section.as_deref() == Some(section)
            })
            .unwrap_or_else(|| panic!("no section header for {section}"));
        self.goto(line)
    }

    /// Move the cursor to the file header for `section`/`path`.
    pub fn goto_file(&mut self, section: &str, path: &str) -> usize {
        let line = self
            .tree()
            .line_nodes
            .iter()
            .position(|line| {
                line.kind == NodeKind::File
                    && line.section.as_deref() == Some(section)
                    && line.path.as_deref() == Some(path)
            })
            .unwrap_or_else(|| panic!("no file header for {section}/{path}"));
        self.goto(line)
    }

    /// Move the cursor to the first node whose copy text is exactly `copy`.
    pub fn goto_copy(&mut self, copy: &str) -> usize {
        let line = self
            .tree()
            .nodes
            .iter()
            .find(|node| node.copy.as_deref() == Some(copy))
            .map(|node| node.start_line)
            .unwrap_or_else(|| panic!("no node copying {copy:?}"));
        self.goto(line)
    }

    /// The focused view's rendered tree.
    pub fn tree(&self) -> &Tree {
        self.tree_of(&self.view)
    }

    /// The rendered tree of `view_id`.
    pub fn tree_of(&self, view_id: &str) -> &Tree {
        self.session
            .views()
            .get(view_id)
            .unwrap_or_else(|| panic!("unknown view: {view_id}"))
            .tree()
    }

    /// The focused view's last acknowledged tree.
    pub fn acked_tree(&self) -> &Tree {
        self.session
            .views()
            .get(&self.view)
            .and_then(|view| view.acked_tree())
            .expect("acked tree")
    }

    /// The focused view's full rendered text.
    pub fn text(&self) -> Vec<String> {
        self.text_of(&self.view)
    }

    /// The full rendered text of `view_id`.
    pub fn text_of(&self, view_id: &str) -> Vec<String> {
        self.tree_of(view_id).lines.clone()
    }

    /// The tree node under the stored cursor.
    pub fn focused_node(&self) -> &Node {
        let tree = self.tree();
        let line = tree
            .line_nodes
            .get(self.cursor.line)
            .unwrap_or_else(|| panic!("cursor line {} is outside the tree", self.cursor.line));
        tree.node(&line.node).expect("cursor node")
    }

    /// The focused view's keymap notice, if its scope resolves.
    pub fn keymap_notice(&self) -> Option<magit_lsp::protocol::ViewKeymapParams> {
        self.session.keymap_notice(&self.view)
    }

    /// The structured menu model of `view_id`, when it is a menu.
    pub fn menu(&self, view_id: &str) -> Option<magit_lsp::menu::Menu> {
        self.session
            .views()
            .get(view_id)
            .and_then(|view| view.menu())
    }

    /// Every open view id, in stack order.
    pub fn view_ids(&self) -> Vec<String> {
        self.session
            .views()
            .iter()
            .map(|view| view.descriptor().id.clone())
            .collect()
    }

    /// The keymaps resolved from the session's config.
    pub fn keymaps(&self) -> Arc<BTreeMap<String, Keymap>> {
        Arc::new(keymap::validate(&self.config).expect("resolve keymaps"))
    }

    /// Replace the session's keymaps and remember the new config.
    pub fn set_keymaps(&mut self, config: &Config) {
        self.session
            .set_keymaps(Arc::new(keymap::validate(config).expect("resolve keymaps")));
        self.config = config.clone();
    }

    /// The session, for assertions the harness does not wrap.
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// The session for direct mutation, e.g. `resync` or `close`.
    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    /// The scripted UI that answers prompts.
    pub fn ui(&self) -> &ScriptedUi {
        &self.ui
    }

    /// Press every whitespace-separated key step in `keys` at the stored
    /// cursor and return the last step's result.
    pub fn press(&mut self, keys: &str) -> InvokeResult {
        let steps: Vec<&str> = keys.split_whitespace().collect();
        assert!(!steps.is_empty(), "press needs at least one key step");
        let mut last = None;
        for step in steps {
            last = Some(self.press_full(step, EditorMode::Normal, self.cursor, None));
        }
        last.expect("at least one step")
    }

    /// Press `key` at `cursor` with the given editor mode and selection.
    pub fn press_full(
        &mut self,
        key: &str,
        mode: EditorMode,
        cursor: Cursor,
        selection: Option<Selection>,
    ) -> InvokeResult {
        let view = self.view.clone();
        let version = self.version_of(&view);
        self.key_version = version;
        let result = self
            .session
            .key(&ViewKeyParams {
                view_id: view,
                version,
                key: key.to_string(),
                editor_mode: mode,
                cursor,
                selection,
            })
            .expect("view/key");
        self.track_result(&result);
        result
    }

    /// Press a key using an explicitly observed render version.
    pub fn press_at_version(&mut self, key: &str, version: u64) -> InvokeResult {
        let view = self.view.clone();
        self.key_version = version;
        let result = self
            .session
            .key(&ViewKeyParams {
                view_id: view,
                version,
                key: key.to_string(),
                editor_mode: EditorMode::Normal,
                cursor: self.cursor,
                selection: None,
            })
            .expect("view/key");
        self.track_result(&result);
        result
    }

    /// Press `key` over the inclusive line range `start..=end` in visual mode.
    pub fn press_visual(&mut self, start: usize, end: usize, key: &str) -> InvokeResult {
        self.press_full(
            key,
            EditorMode::Visual,
            Cursor {
                line: start,
                column: 0,
            },
            Some(Selection {
                start_line: start,
                end_line: end,
            }),
        )
    }

    /// Press `key` and answer any prompt from the harness's [`ScriptedUi`].
    pub fn press_ui(&mut self, key: &str) -> InvokeResult {
        let view = self.view.clone();
        let version = self.version_of(&view);
        self.key_version = version;
        let result = self
            .session
            .key_ui(
                &ViewKeyParams {
                    view_id: view,
                    version,
                    key: key.to_string(),
                    editor_mode: EditorMode::Normal,
                    cursor: self.cursor,
                    selection: None,
                },
                &self.ui,
            )
            .expect("view/key");
        self.track_result(&result);
        result
    }

    /// Invoke `command` on the focused view with no cursor or selection.
    pub fn invoke(&mut self, command: &str) -> InvokeResult {
        self.invoke_full(command, None, None, None)
    }

    /// Invoke `command` with JSON arguments and no cursor.
    pub fn invoke_args(&mut self, command: &str, args: Value) -> InvokeResult {
        self.invoke_full(command, None, None, Some(args))
    }

    /// Invoke `command` at `cursor`.
    pub fn invoke_at(&mut self, command: &str, cursor: Cursor) -> InvokeResult {
        self.invoke_full(command, Some(cursor), None, None)
    }

    /// Invoke `command` at `cursor` with JSON arguments.
    pub fn invoke_at_args(
        &mut self,
        command: &str,
        cursor: Option<Cursor>,
        args: Value,
    ) -> InvokeResult {
        self.invoke_full(command, cursor, None, Some(args))
    }

    /// Invoke `command` at the stored cursor.
    pub fn invoke_here(&mut self, command: &str) -> InvokeResult {
        self.invoke_full(command, Some(self.cursor), None, None)
    }

    /// Invoke `command` with a cursor and a visual selection.
    pub fn invoke_selection(
        &mut self,
        command: &str,
        cursor: Option<Cursor>,
        selection: Option<Selection>,
    ) -> InvokeResult {
        self.invoke_full(command, cursor, selection, None)
    }

    /// Invoke `command`, answering any prompt from the harness's [`ScriptedUi`].
    pub fn invoke_ui(&mut self, command: &str) -> InvokeResult {
        let view = self.view.clone();
        let result = self
            .session
            .invoke_ui(
                &ViewInvokeParams {
                    view_id: view,
                    command: command.to_string(),
                    version: None,
                    args: None,
                    cursor: None,
                    selection: None,
                },
                &self.ui,
            )
            .expect("invoke");
        self.track_result(&result);
        result
    }

    /// Invoke `command` and return the session's error instead of panicking.
    pub fn try_invoke(&mut self, command: &str) -> Result<InvokeResult, Error> {
        let view = self.view.clone();
        let result = self.session.invoke(&ViewInvokeParams {
            view_id: view,
            command: command.to_string(),
            version: None,
            args: None,
            cursor: None,
            selection: None,
        })?;
        self.track_result(&result);
        Ok(result)
    }

    /// The shared invoke path behind every `invoke*` helper.
    pub fn invoke_full(
        &mut self,
        command: &str,
        cursor: Option<Cursor>,
        selection: Option<Selection>,
        args: Option<Value>,
    ) -> InvokeResult {
        let version = self.version();
        self.invoke_at_version(command, version, cursor, selection, args)
    }

    /// Invoke using coordinates from an explicitly observed render version.
    pub fn invoke_at_version(
        &mut self,
        command: &str,
        version: u64,
        cursor: Option<Cursor>,
        selection: Option<Selection>,
        args: Option<Value>,
    ) -> InvokeResult {
        let view = self.view.clone();
        let result = self
            .session
            .invoke(&ViewInvokeParams {
                view_id: view,
                command: command.to_string(),
                version: Some(version),
                args,
                cursor,
                selection,
            })
            .expect("invoke");
        self.track_result(&result);
        result
    }

    /// Switch the focused view's mode through the same path as the toggle.
    pub fn set_mode(&mut self, mode: ViewMode) -> InvokeResult {
        let view = self.view.clone();
        let result = self
            .session
            .set_mode(&ViewSetModeParams {
                view_id: view,
                mode,
            })
            .expect("view/setMode");
        self.track_result(&result);
        result
    }

    /// Refresh the focused view, tracking a render when one is produced.
    pub fn refresh(&mut self) -> Option<ViewRender> {
        let render = self.session.refresh(&self.view).expect("refresh");
        if let Some(render) = &render {
            self.apply(render);
        }
        render
    }

    /// Resize the focused view through the real session entry point and track its emitted render.
    pub fn resize(&mut self, viewport: Viewport) -> Option<ViewRender> {
        let render = self
            .session
            .resize(&self.view, viewport)
            .expect("resize view");
        if let Some(render) = &render {
            self.apply(render);
        }
        render
    }

    /// The focused view's keymap scope, when it has one.
    pub fn keymap_scope(&self) -> Option<String> {
        self.session
            .views()
            .get(&self.view)
            .map(|view| view.descriptor().scope.as_str().to_string())
    }

    /// The repository root the session renders.
    pub fn repo_path(&self) -> &str {
        self.session.repo_path()
    }

    /// Validate a config's keymaps, e.g. after editing bindings in a test.
    pub fn validate_keymaps(config: &Config) -> Arc<BTreeMap<String, Keymap>> {
        Arc::new(keymap::validate(config).expect("resolve keymaps"))
    }

    /// Record a render as applied and focus a newly pushed view.
    fn track_result(&mut self, result: &InvokeResult) {
        if let Some(render) = &result.render {
            self.apply(render);
        }
        for (render, _keymap) in &result.opened {
            self.apply(render);
        }
        let focused = result
            .opened
            .last()
            .map(|(render, _)| render.view_id.clone())
            .or_else(|| {
                result.effects.iter().rev().find_map(|effect| match effect {
                    Effect::FocusView { view_id } => Some(view_id.clone()),
                    _ => None,
                })
            });
        if let Some(view_id) = focused {
            self.view = view_id;
        }
    }

    /// Record the render version applied for its view.
    fn apply(&mut self, render: &ViewRender) {
        self.versions.insert(render.view_id.clone(), render.version);
    }
}

/// A path-only constructor helper for tests that do not own a fixture.
impl TestSession {
    /// A session for a repository path string, e.g. a synthetic `/repo`.
    pub fn new_at(repo: impl AsRef<Path>, profile: Profile) -> Self {
        Self::new(repo.as_ref().to_string_lossy(), profile)
    }
}
