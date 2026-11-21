//! Sessions and the view stack.
//!
//! One [`Session`] per repository root. It owns the open views, their rendered
//! trees and the last version each client acknowledged. Rendering is full on
//! the server; the wire form is a line-range update unless the client has not
//! acked the base version.

use crate::commands::{self, SelectionOperation, SelectionRequest};
use crate::diff_text;
use crate::git::{GitLogOptions, GitRepo};
use crate::keyhandler::{self, MenuResolution, PendingChord, Resolution};
use crate::keymap::Keymap;
use crate::log_view::CommitSelection;
use crate::log_view::LogView;
use crate::menu::{Menu, MenuStatus, MenuView, Reader};
use crate::presentation::prompt::{self, PromptAction, PromptDelivery, PromptDocument, PromptView};
use crate::process_log::ProcessLogView;
use crate::protocol::{
    Cursor, Effect, MessageLevel, PromptChoiceAnswer, PromptItem, PromptKind, Selection,
    ViewInvokeParams, ViewKeyParams, ViewKeymapParams, ViewPickParams, ViewPromptParams,
    ViewRender, ViewSetModeParams,
};
use crate::status_view::StatusView;
use crate::tree::{Node, NodeKind, Tree};
use crate::ui::PromptUi;
use crate::view::{
    Ctx, Placement, View, ViewDescriptor, ViewKind, ViewMode, Viewport, VisibilityCommand,
};
use crate::{Error, Result};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[path = "session/commands.rs"]
mod command_execution;
mod menu;

/// The command that leaves or enters text mode in every scope.
const TOGGLE_TEXT_MODE: &str = "common.toggle-text-mode";

fn unsupported_menu_item(status: MenuStatus, label: &str) -> Option<InvokeResult> {
    let message = match status {
        MenuStatus::Planned { roadmap_id } => {
            format!("Not implemented yet: {label} (roadmap {roadmap_id})")
        }
        MenuStatus::Implemented => return None,
    };
    Some(InvokeResult {
        effects: vec![Effect::message(MessageLevel::Info, message)],
        ..InvokeResult::empty()
    })
}

/// Preflight one resolved command without running it or touching session state.
fn preflight_command(
    id: &str,
    key: &str,
    menu: bool,
    profile_bound: bool,
) -> crate::presentation::keys::KeyPreflight {
    use crate::presentation::keys::KeyPreflight;
    let Some(command) = commands::registry().get(id) else {
        return KeyPreflight::Refused {
            id: Some(id.to_owned()),
            message: "key command is unavailable".into(),
        };
    };
    if !matches!(command.status, commands::CommandStatus::Implemented) {
        return KeyPreflight::Refused {
            id: Some(id.to_owned()),
            message: "command is planned and cannot run from a key".into(),
        };
    }
    if command.destructive {
        return KeyPreflight::Refused {
            id: Some(id.to_owned()),
            message: "destructive command; use the code action".into(),
        };
    }
    if !menu && !profile_bound {
        return KeyPreflight::Refused {
            id: Some(id.to_owned()),
            message: "command is not bound in the active profile".into(),
        };
    }
    KeyPreflight::Allowed {
        command: Some(id.to_owned()),
        key: key.to_owned(),
        menu,
    }
}

/// The URI a status view registers, e.g. `magit://status/home/me/repo`.
pub fn status_uri(repo_path: &str) -> String {
    format!("magit://status/{}", repo_path.trim_start_matches('/'))
}

/// The immutable state of one view as last published by its session.
struct PublishedView {
    uri: String,
    acked: Option<Tree>,
    version: u64,
    lines: Arc<Vec<String>>,
    tree: Tree,
    scope: String,
    mode: ViewMode,
    pending: String,
    menu: Option<Menu>,
    /// A resync handed out this state; the next render must be a full replace.
    resynced: bool,
}

/// Per-session snapshots of every open view, shared outside the session lock.
///
/// The session publishes after each state change and before its caller can
/// notify the client, so a read answered from here is always a state the
/// session really rendered, even while a command holds the session inside Git.
#[derive(Clone, Default)]
pub struct PublishedViews(
    Arc<Mutex<HashMap<String, PublishedView>>>,
    Arc<std::sync::atomic::AtomicBool>,
);

impl PublishedViews {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, PublishedView>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Replace the snapshot with the session's current views.
    ///
    /// An unchanged version shares its previous text; a pending resync marker
    /// survives until a render consumes it.
    fn publish(&self, views: &ViewStack) {
        let mut map = self.lock();
        let mut next = HashMap::with_capacity(views.views.len());
        for view in views.iter() {
            let id = &view.descriptor.id;
            let previous = map.remove(id);
            let lines = match &previous {
                Some(old) if old.version == view.version => old.lines.clone(),
                _ => Arc::new(view.tree.lines.clone()),
            };
            next.insert(
                id.clone(),
                PublishedView {
                    uri: view.descriptor.uri.clone(),
                    acked: (view.mode != ViewMode::Text)
                        .then(|| view.acked.clone().unwrap_or_else(|| view.tree.clone())),
                    version: view.version,
                    lines,
                    tree: view.tree.clone(),
                    scope: view.descriptor.scope.as_str().to_string(),
                    mode: view.mode,
                    pending: view.pending(),
                    menu: view.menu(),
                    resynced: previous.is_some_and(|old| old.resynced),
                },
            );
        }
        *map = next;
    }

    /// The latest published full text of `view_id`, without any session lock.
    pub fn rendered_text(&self, view_id: &str) -> Option<String> {
        self.lock().get(view_id).map(|view| view.lines.join("\n"))
    }

    /// Return an atomic current render snapshot, including its structured tree.
    pub fn current_snapshot(&self, view_id: &str) -> Option<(Tree, u64, String, Option<Menu>)> {
        let map = self.lock();
        let view = map.get(view_id)?;
        Some((
            view.tree.clone(),
            view.version,
            view.scope.clone(),
            view.menu.clone(),
        ))
    }

    /// The latest published full text of `view_id`, without any session lock.
    ///
    /// Marks the view so the session's next render replaces the client's text
    /// instead of diffing against content this resync replaced.
    pub fn resync(&self, view_id: &str) -> Result<ViewRender> {
        let mut map = self.lock();
        let view = map
            .get_mut(view_id)
            .ok_or_else(|| Error::new(format!("unknown view: {view_id}")))?;
        view.resynced = true;
        Ok(ViewRender {
            view_id: view_id.to_string(),
            version: view.version,
            base_version: None,
            text: Some(view.lines.as_ref().clone()),
            edits: None,
            cursor: None,
            mode: view.mode,
            pending: view.pending.clone(),
            menu: view.menu.clone(),
        })
    }

    /// Whether a published view owns this document URI.
    pub fn contains_uri(&self, uri: &str) -> bool {
        self.lock().values().any(|view| view.uri == uri)
    }

    /// The last acknowledged tree for this URI, without waiting on Git.
    pub fn acked_tree_for_uri(&self, uri: &str) -> Option<Tree> {
        self.lock()
            .values()
            .find(|view| view.uri == uri)?
            .acked
            .clone()
    }

    /// Whether the published document is in text mode.
    pub fn is_text_uri(&self, uri: &str) -> bool {
        self.lock()
            .values()
            .any(|view| view.uri == uri && view.mode == ViewMode::Text)
    }

    /// Whether the session is waiting for a prompt answer.
    pub fn has_pending_prompt(&self) -> bool {
        self.1.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Consume the resync marker of `view_id`.
    fn take_resynced(&self, view_id: &str) -> bool {
        self.lock()
            .get_mut(view_id)
            .is_some_and(|view| std::mem::take(&mut view.resynced))
    }
}

/// One open view plus the versions the client has and has not acknowledged.
pub struct OpenView {
    descriptor: ViewDescriptor,
    view: Box<dyn View>,
    tree: Tree,
    version: u64,
    acked: Option<Tree>,
    /// A bounded dispatch-only clone; never changes the acknowledgement snapshot.
    request_tree: Option<Tree>,
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
    /// Global detail level, including levels with no visible children.
    visibility_level: u8,
    /// Where the session publishes this view; consulted for resync markers.
    published: PublishedViews,
    repo_path: String,
    draft_owner: Option<String>,
}

impl OpenView {
    /// The descriptor sent to the client when the view was opened.
    pub fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    /// The view behind this entry, for server-owned state the session reads.
    pub fn view(&self) -> &dyn View {
        self.view.as_ref()
    }

    /// Mutate the paused rebase todo owned by this view.
    pub fn rebase_todo_mut(&mut self) -> Option<&mut crate::rebase_todo_view::RebaseTodoView> {
        self.view_mut().as_any_mut()?.downcast_mut()
    }

    /// The view behind this entry, for server-owned state the session drives.
    fn view_mut(&mut self) -> &mut dyn View {
        self.view.as_mut()
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

    /// Selection settings attached to this view, if any.
    pub fn commit_selection(&self) -> Option<&CommitSelection> {
        self.view.commit_selection()
    }

    /// Selected commit IDs in view order, optionally using the cursor commit.
    pub fn selected_commits(&self, cursor_oid: Option<&str>) -> Vec<String> {
        self.view.selected_commits(cursor_oid)
    }

    /// Toggle a mark for a commit identified by its full object ID.
    pub fn toggle_commit_mark(&mut self, oid: &str) -> bool {
        self.view.toggle_commit_mark(oid)
    }

    /// Load the next cached page for a paginated view.
    pub(crate) fn load_more(&mut self) -> Result<()> {
        self.view.load_more()
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
        self.render_with_delivery(cursor, None, RenderDelivery::Full)
    }

    /// The tree of the last acknowledged version, if any.
    pub fn acked_tree(&self) -> Option<&Tree> {
        self.acked.as_ref()
    }

    /// The exact request tree while dispatching a positional command; otherwise
    /// the acknowledged snapshot used by non-positional visibility commands.
    pub(crate) fn command_source_tree(&self) -> Option<&Tree> {
        self.request_tree.as_ref().or(self.acked.as_ref())
    }

    /// Apply a visibility command, resolving against the caller's tree.
    pub(crate) fn set_visibility(
        &mut self,
        tree: &Tree,
        line: Option<usize>,
        command: VisibilityCommand,
    ) -> bool {
        let changed = self.view.set_visibility(tree, line, command);
        if changed {
            self.visibility_level = match command {
                VisibilityCommand::ShowAll => 4,
                VisibilityCommand::HideAll => 1,
                VisibilityCommand::MoreDetail => (self.visibility_level + 1).min(4),
                VisibilityCommand::LessDetail => self.visibility_level.saturating_sub(1).max(1),
                VisibilityCommand::SetDetail(level) => level.clamp(1, 4),
                _ => self.visibility_level,
            };
        }
        changed
    }

    /// Cycle global detail even when an empty tree cannot expose its level.
    pub(crate) fn cycle_visibility(&mut self) {
        let level = self.visibility_level % 4 + 1;
        let tree = self.tree.clone();
        self.set_visibility(&tree, None, VisibilityCommand::SetDetail(level));
    }

    /// The current version, incremented on every render.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Whether a refresh arrived while the view was not rendering.
    pub fn dirty(&self) -> bool {
        self.dirty
    }

    /// The last version the client acknowledged.
    pub fn acked_version(&self) -> Option<u64> {
        self.acked_version
    }

    /// Re-read the backing state, then render and diff against the last tree.
    fn refresh_and_render(&mut self, cursor: Option<Cursor>) -> Result<ViewRender> {
        self.view.on_refresh()?;
        self.render_with_delivery(cursor, None, RenderDelivery::Acknowledged)
    }
    /// Render without touching the repository, diffing against the last tree.
    ///
    /// `forced` replaces the identity-based successor cursor; section
    /// navigation uses it to move somewhere the old node cannot describe.
    fn render_with_delivery(
        &mut self,
        cursor: Option<Cursor>,
        forced: Option<Cursor>,
        delivery: RenderDelivery,
    ) -> Result<ViewRender> {
        let tree = self.render_tree();
        self.render_tree_with_delivery(tree, cursor, forced, delivery)
    }

    /// Render authoritative structure without advancing the delivery version.
    fn render_tree(&mut self) -> Tree {
        let pending = self.menu_pending.clone();
        let mut tree = self.view.render(&Ctx {
            viewport: self.descriptor.viewport,
            pending: &pending,
        });
        #[cfg(unix)]
        if let Some(owner) = &self.draft_owner {
            let drafts = crate::editor::registry().pending_for(owner, &self.repo_path);
            tree.append_pending_drafts(&drafts);
        }
        tree
    }

    /// Deliver an already rendered tree through the normal diff/ack lifecycle.
    fn render_tree_with_delivery(
        &mut self,
        mut tree: Tree,
        cursor: Option<Cursor>,
        forced: Option<Cursor>,
        delivery: RenderDelivery,
    ) -> Result<ViewRender> {
        let old = self.tree.clone();
        let old_version = self.version;
        let version = old_version + 1;
        tree.version = version;

        let successor = forced.or_else(|| {
            cursor.map(|cursor| {
                successor_cursor(self.request_tree.as_ref().unwrap_or(&old), &tree, cursor)
            })
        });
        // Edits are only safe when the client holds exactly the previous
        // version; otherwise fall back to a full replace.
        let resynced = self.published.take_resynced(&self.descriptor.id);
        let base_version = (delivery == RenderDelivery::Acknowledged
            && !resynced
            && self.acked_version == Some(old_version))
        .then_some(old_version);
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
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RenderDelivery {
    Acknowledged,
    Full,
}

fn initialize_open_view(
    repo_path: &str,
    id: String,
    kind: ViewKind,
    placement: Placement,
    viewport: Viewport,
    mut view: Box<dyn View>,
    draft_owner: Option<String>,
) -> (ViewDescriptor, ViewRender, OpenView) {
    let repo_path_owned = repo_path.to_string();
    let mut tree = view.render(&Ctx {
        viewport,
        pending: &[],
    });
    #[cfg(unix)]
    if let Some(owner) = &draft_owner {
        let drafts = crate::editor::registry().pending_for(owner, &repo_path_owned);
        tree.append_pending_drafts(&drafts);
    }
    let version = 1;
    tree.version = version;
    let descriptor = ViewDescriptor {
        id: id.clone(),
        kind: view.kind(),
        uri: view_uri(repo_path, &kind, &id),
        title: view.title(),
        placement,
        mode: view.mode(),
        scope: view.scope(),
        capture: view.capture(),
        viewport,
    };
    let render = ViewRender {
        view_id: id,
        version,
        base_version: None,
        text: Some(tree.lines.clone()),
        edits: None,
        cursor: None,
        mode: descriptor.mode,
        pending: String::new(),
        menu: view.menu(),
    };
    let open_view = OpenView {
        descriptor: descriptor.clone(),
        view,
        tree,
        version,
        acked: None,
        request_tree: None,
        acked_version: None,
        pending: PendingChord::default(),
        menu_pending: Vec::new(),
        key_cursor: None,
        mode: descriptor.mode,
        dirty: false,
        visibility_level: 4,
        published: PublishedViews::default(),
        repo_path: repo_path_owned,
        draft_owner,
    };
    (descriptor, render, open_view)
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
    rich_capabilities: Option<crate::presentation::RichCapabilities>,
    views: ViewStack,
    keymaps: Arc<BTreeMap<String, Keymap>>,
    /// The next prompt id handed to a prompt request.
    next_prompt: u64,
    pending_prompt: Option<PendingPrompt>,
    pending_selections: BTreeMap<String, PendingSelection>,
    published: PublishedViews,
    draft_owner: Option<String>,
}

struct PendingPrompt {
    invalidated: bool,
    prepared: Option<InvokeResult>,
    request: ViewPromptParams,
    view_id: String,
    prompt_view: Option<String>,
    cursor: Option<Cursor>,
    cancel: Option<crate::runner::CancelHandle>,
    action: SessionContinuation,
    /// The pick waiting on this prompt, when the prompt came from a pick.
    pick: Option<ViewPickParams>,
}

#[derive(Clone)]
enum SessionContinuation {
    Option {
        key: String,
        menu: crate::menu::Menu,
        cursor: Cursor,
        reader: Reader,
    },
    Command {
        command_id: String,
        action: commands::PromptAction,
        effects: Vec<Effect>,
    },
}

/// A completed operation or a question whose continuation remains session-owned.
#[derive(Debug)]
pub enum Execution {
    /// Effects and renders ready for delivery.
    Done(InvokeResult),
    /// Wait outside session ownership, then resume with this prompt identity.
    Prompt(ViewPromptParams),
    /// Wait for a pick answer; a document prompt carries the same choice.
    Pick(ViewPickParams),
}

/// The result of answering a document prompt.
#[derive(Debug)]
pub struct PromptCompletion {
    /// Pop/focus effects and any render produced while closing the prompt.
    pub result: InvokeResult,
    /// The continuation step after the answer; drive it before returning to the client.
    pub next: Execution,
}

/// The result of ingesting an edited Plain prompt document.
#[derive(Debug)]
pub struct PromptReconcile {
    /// The authoritative prompt tree the client text was reconciled against.
    pub tree: Tree,
    /// Whether a non-field edit was repaired instead of adopted.
    pub repaired: bool,
    /// The re-render to deliver when the document was repaired.
    pub render: Option<ViewRender>,
}

struct PendingSelection {
    caller_id: String,
    operation: SelectionOperation,
    /// Where the picked commit lands; absent for a copy-only selection.
    continuation: Option<SessionContinuation>,
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
    /// Captured operation to run on the repository's detached worker.
    pub commit: Option<crate::git::RepositoryOperation>,
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

fn unsupported_feature(feature: &str) -> InvokeResult {
    InvokeResult {
        effects: vec![Effect::message(
            MessageLevel::Warn,
            format!("Client does not support {feature}"),
        )],
        ..InvokeResult::empty()
    }
}

fn prompt_busy() -> InvokeResult {
    InvokeResult {
        effects: vec![Effect::message(
            MessageLevel::Info,
            "Answer or cancel the pending prompt before running another command",
        )],
        ..InvokeResult::empty()
    }
}

impl Session {
    /// Create a session for a repository root.
    pub fn new(repo_path: impl Into<String>) -> Self {
        Self {
            repo_path: repo_path.into(),
            rich_capabilities: None,
            views: ViewStack::new(),
            keymaps: Arc::new(BTreeMap::new()),
            next_prompt: 0,
            pending_prompt: None,
            pending_selections: BTreeMap::new(),
            published: PublishedViews::default(),
            draft_owner: None,
        }
    }

    /// Create a session with optional Plain draft ownership for registry-backed rows.
    pub fn new_with_draft_owner(repo_path: impl Into<String>, owner: Option<String>) -> Self {
        let mut session = Self::new(repo_path);
        session.draft_owner = owner;
        session
    }

    /// Apply the connection's negotiated rich feature set to all session transitions.
    pub fn set_rich_capabilities(&mut self, caps: Option<crate::presentation::RichCapabilities>) {
        self.rich_capabilities = caps;
    }

    fn placement(&self, placement: Placement) -> Placement {
        self.rich_capabilities
            .as_ref()
            .map_or(placement, |caps| caps.placement(placement))
    }

    fn unsupported_prompt(&self, request: &ViewPromptParams) -> Option<InvokeResult> {
        let native_declared = self
            .rich_capabilities
            .as_ref()
            .is_some_and(|caps| caps.allows_prompt(request));
        match prompt::delivery(request, native_declared) {
            PromptDelivery::RefuseSecret => Some(InvokeResult {
                effects: vec![Effect::message(
                    MessageLevel::Warn,
                    prompt::SECRET_UNAVAILABLE.to_string(),
                )],
                ..InvokeResult::empty()
            }),
            PromptDelivery::Native | PromptDelivery::Document => None,
        }
    }

    /// The shared snapshot handle that answers reads without the session lock.
    pub fn published(&self) -> PublishedViews {
        self.published.clone()
    }

    /// Publish every open view's current state.
    fn publish(&self) {
        self.published.1.store(
            self.pending_prompt.is_some(),
            std::sync::atomic::Ordering::SeqCst,
        );
        self.published.publish(&self.views);
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
        let keymap = keymap.clone().with_basic_modifiers(
            self.rich_capabilities
                .as_ref()
                .is_some_and(|caps| caps.modifiers == crate::protocol::KeyModifiers::Basic),
        );
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

    /// Return the current full rendered document text for a logical view.
    pub fn rendered_text(&self, view_id: &str) -> Option<String> {
        self.views
            .get(view_id)
            .map(|view| view.tree.lines.join("\n"))
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

    /// Open a view with a section line limit applied before its initial render.
    pub fn open_with_section_line_limit(
        &mut self,
        id: impl Into<String>,
        kind: ViewKind,
        placement: Placement,
        viewport: Viewport,
        limit: usize,
    ) -> Result<(ViewDescriptor, ViewRender)> {
        self.open_internal(id.into(), kind, placement, viewport, None, Some(limit))
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
        self.open_internal(id.into(), kind, placement, viewport, args, None)
    }

    /// Open a view using the repository already discovered by the server.
    pub fn open_with_discovered_repo(
        &mut self,
        id: impl Into<String>,
        kind: ViewKind,
        placement: Placement,
        viewport: Viewport,
        args: Option<&Value>,
        repo: GitRepo,
    ) -> Result<(ViewDescriptor, ViewRender)> {
        self.open_internal_with_repo(id.into(), kind, placement, viewport, args, None, Some(repo))
    }

    fn open_internal(
        &mut self,
        id: String,
        kind: ViewKind,
        placement: Placement,
        viewport: Viewport,
        args: Option<&Value>,
        section_line_limit: Option<usize>,
    ) -> Result<(ViewDescriptor, ViewRender)> {
        self.open_internal_with_repo(
            id,
            kind,
            placement,
            viewport,
            args,
            section_line_limit,
            None,
        )
    }

    fn open_internal_with_repo(
        &mut self,
        id: String,
        kind: ViewKind,
        placement: Placement,
        viewport: Viewport,
        args: Option<&Value>,
        section_line_limit: Option<usize>,
        repo: Option<GitRepo>,
    ) -> Result<(ViewDescriptor, ViewRender)> {
        if self.views.get(&id).is_some() {
            return Err(Error::new(format!("view already open: {id}")));
        }
        let mut view = create_view(&self.repo_path, &kind, args, &self.keymaps, repo)?;
        if let Some(limit) = section_line_limit {
            view.set_section_line_limit(Some(limit));
        }
        self.open_provider_view(id, placement, viewport, view)
    }

    /// Open a provider-owned view through the normal render and dispatch lifecycle.
    pub fn open_provider_view(
        &mut self,
        id: impl Into<String>,
        placement: Placement,
        viewport: Viewport,
        view: Box<dyn View>,
    ) -> Result<(ViewDescriptor, ViewRender)> {
        let id = id.into();
        if self.views.get(&id).is_some() {
            return Err(Error::new(format!("view already open: {id}")));
        }
        let kind = view.kind();
        let placement = self.placement(placement);
        let (descriptor, render, mut open_view) = initialize_open_view(
            &self.repo_path,
            id,
            kind,
            placement,
            viewport,
            view,
            self.draft_owner.clone(),
        );
        open_view.published = self.published.clone();
        self.views.push(open_view);
        self.publish();
        Ok((descriptor, render))
    }

    /// Close a view; closing one the client already closed is a no-op.
    ///
    /// Closing the prompt view or its origin also clears the staged
    /// continuation, so a later answer cannot apply to a closed view. Both
    /// close paths return the resumed continuation and every sibling refresh
    /// queued while the prompt owned the session, for delivery by the caller.
    pub fn close(&mut self, view_id: &str) -> Result<InvokeResult> {
        let prompt = self.pending_prompt.as_ref().map(|pending| {
            (
                pending.request.prompt_id.clone(),
                pending.view_id.clone(),
                pending.prompt_view.clone(),
            )
        });
        if let Some((prompt_id, origin, prompt_view)) = prompt {
            if prompt_view.as_deref() == Some(view_id) {
                self.views.remove(view_id);
                self.pending_selections.remove(view_id);
                let result = self.resume_closed_prompt(&prompt_id)?;
                self.publish();
                return Ok(result);
            }
            if origin == view_id {
                if let Some(prompt_view) = prompt_view {
                    self.views.remove(&prompt_view);
                    self.pending_selections.remove(&prompt_view);
                }
                if let Some(pending) = self.pending_prompt.as_mut() {
                    pending.invalidated = true;
                }
                self.views.remove(view_id);
                self.pending_selections.remove(view_id);
                self.pending_selections
                    .retain(|_, pending| pending.caller_id != view_id);
                let result = self.resume_closed_prompt(&prompt_id)?;
                self.publish();
                return Ok(result);
            }
        }
        self.views.remove(view_id);
        self.pending_selections.remove(view_id);
        self.pending_selections
            .retain(|_, pending| pending.caller_id != view_id);
        self.publish();
        Ok(InvokeResult::empty())
    }

    /// Resume a prompt that a close cancelled and drain its queued refreshes.
    ///
    /// A continuation that starts another prompt keeps the queue: only the
    /// surviving normal-mode views of a finished prompt resume now.
    fn resume_closed_prompt(&mut self, prompt_id: &str) -> Result<InvokeResult> {
        let mut result = match self.resume_prompt(prompt_id, None)? {
            Execution::Done(result) => result,
            Execution::Prompt(_) | Execution::Pick(_) => InvokeResult::empty(),
        };
        if self.pending_prompt.is_none() {
            self.resume_deferred_refreshes(&mut result);
        }
        Ok(result)
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
        self.publish();
        Ok(())
    }

    /// Record a new viewport size from the client.
    ///
    /// Menus re-lay out their columns immediately; other views keep their
    /// current render until the next refresh.
    pub fn resize(&mut self, view_id: &str, viewport: Viewport) -> Result<Option<ViewRender>> {
        let result = self.resize_view(view_id, viewport);
        self.publish();
        result
    }

    fn resize_view(&mut self, view_id: &str, viewport: Viewport) -> Result<Option<ViewRender>> {
        let view = self
            .views
            .get_mut(view_id)
            .ok_or_else(|| Error::new(format!("unknown view: {view_id}")))?;
        view.descriptor.viewport = viewport;
        if view.is_menu() {
            Ok(Some(view.render_with_delivery(
                None,
                None,
                RenderDelivery::Acknowledged,
            )?))
        } else {
            Ok(None)
        }
    }

    /// The latest published full text, for a client that lost an update.
    ///
    /// Reads the published snapshot, so it never waits for a running command.
    pub fn resync(&self, view_id: &str) -> Result<ViewRender> {
        self.published.resync(view_id)
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
        let step = self.start_invoke(params)?;
        self.drive(step, ui)
    }

    /// Prepare an invocation without waiting for user input.
    pub fn start_invoke(&mut self, params: &ViewInvokeParams) -> Result<Execution> {
        let step = self.begin_invoke(params);
        self.publish();
        step
    }

    fn begin_invoke(&mut self, params: &ViewInvokeParams) -> Result<Execution> {
        if self.pending_prompt.is_some() && params.command != "common.refresh" {
            return Ok(Execution::Done(prompt_busy()));
        }
        let already_pending = self.pending_prompt.is_some();
        let result = self.dispatch(
            crate::dispatch::DispatchRequest {
                action_target: None,
                command: params.command.clone(),
                target: crate::dispatch::DispatchTarget {
                    view_id: params.view_id.clone(),
                    version: params.version,
                    cursor: params.cursor,
                    selection: params.selection,
                    key: None,
                },
                args: params.args.clone(),
            },
            &crate::ui::NullUi,
        )?;
        Ok(if already_pending {
            Execution::Done(result)
        } else {
            self.execution(result)
        })
    }

    /// Start a typed action through the same dispatcher and prompt lifecycle as
    /// rich input. Later Plain actions can reuse this without executeCommand.
    pub fn start_dispatch(
        &mut self,
        request: crate::dispatch::DispatchRequest,
    ) -> Result<Execution> {
        let already_pending = self.pending_prompt.is_some();
        let result = self.dispatch(request, &crate::ui::NullUi)?;
        self.publish();
        Ok(if already_pending {
            Execution::Done(result)
        } else {
            self.execution(result)
        })
    }

    /// Execute a semantic command using its captured args and displayed target.
    pub fn dispatch(
        &mut self,
        request: crate::dispatch::DispatchRequest,
        ui: &dyn PromptUi,
    ) -> Result<InvokeResult> {
        if self.pending_prompt.is_some() && request.command != "common.refresh" {
            return Ok(prompt_busy());
        }
        let mut target = request.target;
        if let Some(offered) = request.action_target.as_ref() {
            let view = self
                .views
                .get(&target.view_id)
                .ok_or_else(|| Error::new(format!("unknown view: {}", target.view_id)))?;
            let source = match target.version {
                Some(version) if Some(version) == view.acked_version => view.acked.as_ref(),
                Some(version) if version == view.version => Some(&view.tree),
                _ => None,
            };
            let valid = source.is_some_and(|tree| {
                if let Some(selection) = offered.selection {
                    crate::dispatch::offer_selection_target(
                        tree,
                        &request.command,
                        selection.start_line,
                        selection.end_line,
                    )
                    .as_ref()
                        == Some(offered)
                } else {
                    crate::dispatch::offer_target(tree, &request.command, &offered.node_id).as_ref()
                        == Some(offered)
                }
            });
            let node = source.and_then(|tree| tree.node(&offered.node_id));
            if valid {
                if let Some(selection) = offered.selection {
                    target.selection = Some(Selection {
                        start_line: selection.start_line as usize,
                        end_line: selection.end_line as usize,
                    });
                } else if let Some(node) = node {
                    target.cursor = Some(Cursor {
                        line: node.start_line,
                        column: 0,
                    });
                }
            } else {
                return Ok(InvokeResult {
                    effects: vec![Effect::message(
                        MessageLevel::Warn,
                        crate::errors::HUNK_CHANGED,
                    )],
                    render: Some(
                        self.views
                            .get_mut(&target.view_id)
                            .expect("target view was checked above")
                            .refresh_and_render(None)?,
                    ),
                    ..InvokeResult::empty()
                });
            }
        }
        self.run_versioned_command(
            &target.view_id,
            &request.command,
            target.version,
            target.cursor,
            target.selection,
            request.action_target.is_some(),
            request.args.as_ref(),
            target.key,
            ui,
        )
    }

    /// Handle one key step: resolve it through the view's keymap and either run
    /// the matched command or answer `PassThrough`.
    ///
    /// Cursor and selection come from the request, so the command path is
    /// identical to `view/invoke`; the node is resolved by identity against the
    /// exact request-version tree, never a newer ack. Menus use their own two-step
    /// matcher instead of the profile keymap.
    pub fn key(&mut self, params: &ViewKeyParams) -> Result<InvokeResult> {
        self.key_ui(params, &crate::ui::NullUi)
    }

    /// Handle one key step, asking the user through `ui` when the command does.
    pub fn key_ui(&mut self, params: &ViewKeyParams, ui: &dyn PromptUi) -> Result<InvokeResult> {
        let step = self.start_key(params)?;
        self.drive(step, ui)
    }

    /// Prepare a key operation without waiting for user input.
    pub fn start_key(&mut self, params: &ViewKeyParams) -> Result<Execution> {
        let step = self.begin_key(params);
        self.publish();
        step
    }

    /// Preflight a key against cloned pending state without mutating the session.
    ///
    /// Mirrors [`Session::key`] resolution (profile keymap or active menu) but
    /// runs no command and changes no pending chord or menu state.
    pub fn preflight_key(
        &self,
        params: &ViewKeyParams,
    ) -> Result<crate::presentation::keys::KeyPreflight> {
        let view = self
            .views
            .get(&params.view_id)
            .ok_or_else(|| Error::new(format!("unknown view: {}", params.view_id)))?;
        let cursor_moved = view.key_cursor.is_some() && view.key_cursor != Some(params.cursor);
        if view.is_menu() {
            let mut pending = view.menu_pending.clone();
            if cursor_moved {
                pending.clear();
            }
            let menu = view.menu();
            let keys: Vec<String> = menu
                .as_ref()
                .into_iter()
                .flat_map(|menu| &menu.groups)
                .flat_map(|group| &group.items)
                .map(|item| item.key().to_string())
                .collect();
            return Ok(
                match keyhandler::resolve_menu(&mut pending, &params.key, &keys) {
                    MenuResolution::Pending => crate::presentation::keys::KeyPreflight::Pending,
                    MenuResolution::Complete(key) => {
                        match menu.as_ref().and_then(|menu| menu.item(&key)) {
                            Some(item) if matches!(item.status(), MenuStatus::Implemented) => {
                                match item {
                                    crate::menu::Item::Action { command, .. } => {
                                        preflight_command(command, &key, true, false)
                                    }
                                    crate::menu::Item::Switch { .. }
                                    | crate::menu::Item::Option { .. } => {
                                        crate::presentation::keys::KeyPreflight::Allowed {
                                            command: None,
                                            key,
                                            menu: true,
                                        }
                                    }
                                }
                            }
                            Some(crate::menu::Item::Action { command, .. }) => {
                                crate::presentation::keys::KeyPreflight::Refused {
                                    id: Some(command.clone()),
                                    message: "command is planned and cannot run from a key".into(),
                                }
                            }
                            Some(_) => crate::presentation::keys::KeyPreflight::Refused {
                                id: None,
                                message: "menu item is planned and cannot run from a key".into(),
                            },
                            None => crate::presentation::keys::KeyPreflight::Unbound,
                        }
                    }
                    _ => crate::presentation::keys::KeyPreflight::Unbound,
                },
            );
        }
        let keymap = self
            .keymap_for(view.descriptor().scope.as_str())
            .cloned()
            .map(|keymap| {
                keymap.with_basic_modifiers(
                    self.rich_capabilities
                        .as_ref()
                        .is_some_and(|caps| caps.modifiers == crate::protocol::KeyModifiers::Basic),
                )
            });
        let mut pending = view.pending.clone();
        if cursor_moved {
            pending.clear();
        }
        Ok(
            match keyhandler::resolve(
                keymap.as_ref(),
                &mut pending,
                &params.key,
                params.editor_mode,
                view.descriptor().capture,
            )? {
                Resolution::Pending => crate::presentation::keys::KeyPreflight::Pending,
                Resolution::Run { command, .. } => {
                    preflight_command(&command, &params.key, false, true)
                }
                Resolution::PassThrough { .. } | Resolution::Swallow { .. } => {
                    crate::presentation::keys::KeyPreflight::Refused {
                        id: None,
                        message: "key is not bound in the active profile".into(),
                    }
                }
            },
        )
    }

    /// Resolve a key without changing pending chords, returning its command when complete.
    pub fn resolved_key_command(&self, params: &ViewKeyParams) -> Result<Option<String>> {
        let view = self
            .views
            .get(&params.view_id)
            .ok_or_else(|| Error::new(format!("unknown view: {}", params.view_id)))?;
        let cursor_moved = view.key_cursor.is_some() && view.key_cursor != Some(params.cursor);
        if view.is_menu() {
            let mut pending = view.menu_pending.clone();
            if cursor_moved {
                pending.clear();
            }
            let menu = view.menu();
            let keys: Vec<String> = menu
                .as_ref()
                .into_iter()
                .flat_map(|menu| &menu.groups)
                .flat_map(|group| &group.items)
                .map(|item| item.key().to_string())
                .collect();
            let MenuResolution::Complete(key) =
                keyhandler::resolve_menu(&mut pending, &params.key, &keys)
            else {
                return Ok(None);
            };
            return Ok(menu.and_then(|menu| {
                menu.groups
                    .iter()
                    .flat_map(|group| &group.items)
                    .find_map(|item| match item {
                        crate::menu::Item::Action {
                            key: item_key,
                            command,
                            ..
                        } if item_key == &key => Some(command.clone()),
                        _ => None,
                    })
            }));
        }

        let keymap = self
            .keymap_for(view.descriptor().scope.as_str())
            .cloned()
            .map(|keymap| {
                keymap.with_basic_modifiers(
                    self.rich_capabilities
                        .as_ref()
                        .is_some_and(|caps| caps.modifiers == crate::protocol::KeyModifiers::Basic),
                )
            });
        let mut pending = view.pending.clone();
        if cursor_moved {
            pending.clear();
        }
        Ok(
            match keyhandler::resolve(
                keymap.as_ref(),
                &mut pending,
                &params.key,
                params.editor_mode,
                view.descriptor().capture,
            )? {
                Resolution::Run { command, .. } => Some(command),
                _ => None,
            },
        )
    }

    fn begin_key(&mut self, params: &ViewKeyParams) -> Result<Execution> {
        let already_pending = self.pending_prompt.is_some();
        let result = self.key_start(params, &crate::ui::NullUi)?;
        Ok(if already_pending {
            Execution::Done(result)
        } else {
            self.execution(result)
        })
    }

    fn key_start(&mut self, params: &ViewKeyParams, ui: &dyn PromptUi) -> Result<InvokeResult> {
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
            if self.pending_prompt.is_some() {
                return Ok(prompt_busy());
            }
            return self.menu_key(params, cursor_moved, ui);
        }

        let keymap = self.keymap_for(&scope).cloned().map(|keymap| {
            keymap.with_basic_modifiers(
                self.rich_capabilities
                    .as_ref()
                    .is_some_and(|caps| caps.modifiers == crate::protocol::KeyModifiers::Basic),
            )
        });

        let resolution = {
            let view = self
                .views
                .get_mut(&params.view_id)
                .expect("view checked above");
            if cursor_moved {
                view.pending.clear();
            }
            keyhandler::resolve(
                keymap.as_ref(),
                &mut view.pending,
                &params.key,
                mode,
                capture,
            )?
        };

        match resolution {
            Resolution::Pending => Ok(InvokeResult::empty()),
            Resolution::Run { command, chord } => self.dispatch(
                crate::dispatch::DispatchRequest {
                    action_target: None,
                    command,
                    target: crate::dispatch::DispatchTarget {
                        view_id: params.view_id.clone(),
                        version: Some(params.version),
                        cursor: Some(params.cursor),
                        selection: params.selection,
                        key: Some(chord),
                    },
                    args: None,
                },
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

    fn prepare_prompt(
        &mut self,
        view_id: &str,
        cursor: Option<Cursor>,
        mut request: ViewPromptParams,
        action: SessionContinuation,
    ) {
        self.next_prompt += 1;
        request.prompt_id = format!("session-prompt-{}", self.next_prompt);
        self.pending_prompt = Some(PendingPrompt {
            invalidated: false,
            prepared: None,
            request,
            view_id: view_id.to_string(),
            prompt_view: None,
            cursor,
            action,
            cancel: crate::runner::current_cancel(),
            pick: None,
        });
    }

    /// Stage a pick and the derived choice prompt that shows the same items.
    fn prepare_pick(
        &mut self,
        view_id: &str,
        cursor: Option<Cursor>,
        mut pick: ViewPickParams,
        action: SessionContinuation,
    ) {
        let items = pick
            .items
            .iter()
            .map(|item| PromptItem {
                id: item.id.clone(),
                label: item.label.clone(),
                description: item.description.clone(),
                detail: item.detail.clone(),
            })
            .collect();
        let request = ViewPromptParams {
            prompt_id: String::new(),
            kind: PromptKind::Choice,
            title: pick.prompt.clone(),
            detail: None,
            yes_label: None,
            no_label: None,
            destructive: None,
            default: None,
            placeholder: None,
            secret: None,
            items: Some(items),
            allow_custom: Some(pick.allow_free_text),
            preselected: Some(pick.preselection.clone()),
            min: None,
            max: None,
        };
        self.prepare_prompt(view_id, cursor, request, action);
        if let Some(pending) = self.pending_prompt.as_mut() {
            pick.pick_id = pending.request.prompt_id.clone();
            pending.pick = Some(pick);
        }
    }

    fn execution(&mut self, result: InvokeResult) -> Execution {
        match &mut self.pending_prompt {
            Some(pending) => {
                pending.prepared = Some(result);
                match &pending.pick {
                    Some(pick) => Execution::Pick(pick.clone()),
                    None => Execution::Prompt(pending.request.clone()),
                }
            }
            None => Execution::Done(result),
        }
    }

    /// The staged pick awaiting delivery, if any.
    pub fn pending_pick_request(&self) -> Option<ViewPickParams> {
        self.pending_prompt
            .as_ref()
            .and_then(|pending| pending.pick.clone())
    }

    /// Truncate a pending pick and its prompt request to `limit` items.
    ///
    /// Preselected ids whose items were dropped are removed. Returns whether
    /// any item was removed.
    pub fn truncate_pending_pick(&mut self, limit: usize) -> bool {
        let Some(pending) = self.pending_prompt.as_mut() else {
            return false;
        };
        let Some(pick) = pending.pick.as_mut() else {
            return false;
        };
        let removed = pick.items.len() > limit;
        pick.items.truncate(limit);
        pick.preselection
            .retain(|id| pick.items.iter().any(|item| item.id == *id));
        if let Some(items) = pending.request.items.as_mut() {
            items.truncate(limit);
        }
        if let Some(preselected) = pending.request.preselected.as_mut() {
            if let Some(items) = pending.request.items.as_ref() {
                preselected.retain(|id| items.iter().any(|item| item.id == *id));
            }
        }
        removed
    }

    /// Whether a staged operation currently owns this session's prompt slot.
    pub fn has_pending_prompt(&self) -> bool {
        self.pending_prompt.is_some()
    }

    /// The staged prompt request awaiting delivery, if any.
    pub fn pending_prompt_request(&self) -> Option<ViewPromptParams> {
        self.pending_prompt
            .as_ref()
            .map(|pending| pending.request.clone())
    }

    fn drive(&mut self, mut step: Execution, ui: &dyn PromptUi) -> Result<InvokeResult> {
        loop {
            match step {
                Execution::Done(result) => return Ok(result),
                Execution::Pick(_) => {
                    let Some(request) = self.pending_prompt_request() else {
                        return Err(Error::new("no pick is pending"));
                    };
                    step = Execution::Prompt(request);
                }
                Execution::Prompt(request) => {
                    let answer = match crate::ui::answer(ui, &request) {
                        Ok(answer) => answer,
                        Err(error) => {
                            let Execution::Done(mut result) =
                                self.resume_prompt(&request.prompt_id, None)?
                            else {
                                unreachable!("cancellation cannot prompt");
                            };
                            result
                                .effects
                                .push(Effect::message(MessageLevel::Info, error.to_string()));
                            return Ok(result);
                        }
                    };
                    step = self.resume_prompt(&request.prompt_id, answer)?;
                }
            }
        }
    }

    /// Resume exactly the pending operation; cancelled, closed or replaced targets do nothing.
    pub fn resume_prompt(
        &mut self,
        prompt_id: &str,
        answer: Option<crate::protocol::PromptValue>,
    ) -> Result<Execution> {
        let step = self.resume_pending(prompt_id, answer);
        self.publish();
        step
    }

    /// Push the server-rendered prompt for the staged continuation.
    ///
    /// The request must match the staged prompt and a prompt view must not
    /// already be open. The new view is pushed above the staged origin view.
    pub fn open_document_prompt(&mut self, request: &ViewPromptParams) -> Result<InvokeResult> {
        let pending = self
            .pending_prompt
            .as_ref()
            .ok_or_else(|| Error::new("no prompt is pending"))?;
        if pending.request.prompt_id != request.prompt_id {
            return Err(Error::new(format!(
                "prompt {} is no longer active",
                request.prompt_id
            )));
        }
        if pending.prompt_view.is_some() {
            return Err(Error::new("a prompt view is already open"));
        }
        let origin = pending.view_id.clone();
        let document = PromptDocument::new(request.clone()).map_err(Error::new)?;
        let (descriptor, render) = self.push_view(
            &origin,
            ViewKind::Prompt,
            Placement::Main,
            Box::new(PromptView::new(document)),
        )?;
        let prompt_view = descriptor.id.clone();
        if let Some(pending) = self.pending_prompt.as_mut() {
            pending.prompt_view = Some(prompt_view.clone());
        }
        self.publish();
        Ok(InvokeResult {
            effects: vec![Effect::PushView { descriptor }],
            opened: vec![(render, self.keymap_notice(&prompt_view))],
            ..InvokeResult::empty()
        })
    }

    /// Reconcile an edited Plain prompt document against the open prompt view.
    ///
    /// Returns `None` when no prompt is open. A document whose non-field lines
    /// were edited is repaired by re-rendering the prompt view; otherwise the
    /// edited field values are adopted and the authoritative tree is returned
    /// with no render.
    pub fn reconcile_prompt_document(&mut self, document: &str) -> Result<Option<PromptReconcile>> {
        if !self.prompt_active() {
            return Ok(None);
        }
        let view_id = self.prompt_view_id().expect("checked prompt").to_string();
        let cols = self
            .views
            .get(&view_id)
            .expect("checked prompt")
            .descriptor()
            .viewport
            .cols as usize;
        let repaired = {
            let open = self.views.get_mut(&view_id).expect("checked prompt");
            let prompt_view = open
                .view_mut()
                .as_any_mut()
                .and_then(|any| any.downcast_mut::<PromptView>())
                .ok_or_else(|| Error::new("no prompt is active"))?;
            prompt_view
                .document_mut()
                .reconcile(document, cols)
                .map_err(Error::new)?
                .repaired
        };
        if repaired {
            let render = self
                .views
                .get_mut(&view_id)
                .expect("checked prompt")
                .render_with_delivery(None, None, RenderDelivery::Acknowledged)?;
            self.publish();
            let tree = self
                .views
                .get(&view_id)
                .expect("checked prompt")
                .tree()
                .clone();
            return Ok(Some(PromptReconcile {
                tree,
                repaired: true,
                render: Some(render),
            }));
        }
        let open = self.views.get_mut(&view_id).expect("checked prompt");
        let prompt_view = open
            .view_mut()
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<PromptView>())
            .ok_or_else(|| Error::new("no prompt is active"))?;
        let tree = prompt_view.document().render(cols);
        Ok(Some(PromptReconcile {
            tree,
            repaired: false,
            render: None,
        }))
    }

    /// Reconcile an edited Plain `log.select` document against its open view.
    ///
    /// Returns `None` when `view_id` is not a commit-selection view accepting
    /// free text, leaving the edit to ordinary document synchronization. A
    /// document whose non-field lines were edited is repaired by re-rendering
    /// the log view; otherwise the edited revision is adopted and the
    /// authoritative tree is returned with no render.
    pub fn reconcile_log_select_document(
        &mut self,
        view_id: &str,
        document: &str,
    ) -> Result<Option<PromptReconcile>> {
        if !self.pending_selections.contains_key(view_id) {
            return Ok(None);
        }
        let (reconcile, tree) = {
            let open = self.views.get_mut(view_id).expect("checked log select");
            let Some(log_view) = open
                .view_mut()
                .as_any_mut()
                .and_then(|any| any.downcast_mut::<LogView>())
            else {
                return Ok(None);
            };
            if log_view.revision().is_none() {
                return Ok(None);
            }
            log_view.reconcile(document).map_err(Error::new)?
        };
        if reconcile.repaired {
            let render = self
                .views
                .get_mut(view_id)
                .expect("checked log select")
                .render_with_delivery(None, None, RenderDelivery::Acknowledged)?;
            self.publish();
            let tree = self
                .views
                .get(view_id)
                .expect("checked log select")
                .tree()
                .clone();
            return Ok(Some(PromptReconcile {
                tree,
                repaired: true,
                render: Some(render),
            }));
        }
        Ok(Some(PromptReconcile {
            tree,
            repaired: false,
            render: None,
        }))
    }

    /// Whether a document prompt is the active top view.
    pub fn prompt_active(&self) -> bool {
        self.prompt_view_id()
            .is_some_and(|view_id| self.views.get(view_id).is_some())
    }

    /// The open document prompt view id, if any.
    pub fn prompt_view_id(&self) -> Option<&str> {
        self.pending_prompt
            .as_ref()
            .and_then(|pending| pending.prompt_view.as_deref())
    }

    /// The pending request id while a prompt view is open.
    pub fn active_prompt_id(&self) -> Option<&str> {
        if !self.prompt_active() {
            return None;
        }
        self.pending_prompt
            .as_ref()
            .map(|pending| pending.request.prompt_id.as_str())
    }

    /// The row a Rich client's Enter key answers, with the prompt's text.
    ///
    /// Returns `None` unless the prompt view is active at the client's version
    /// and `line` resolves to a confirm/text answer row. The text is the
    /// prompt's current rendered document, so reconciled field edits are used.
    pub fn prompt_key_target(
        &self,
        view_id: &str,
        version: u64,
        line: usize,
    ) -> Result<Option<(String, String)>> {
        if !self.prompt_active() || self.prompt_view_id() != Some(view_id) {
            return Ok(None);
        }
        let prompt_id = self
            .pending_prompt
            .as_ref()
            .expect("checked prompt")
            .request
            .prompt_id
            .clone();
        let Some(open) = self.views.get(view_id) else {
            return Ok(None);
        };
        if open.version != version {
            return Ok(None);
        }
        let Some(line_node) = open.tree.line_nodes.get(line) else {
            return Ok(None);
        };
        let Some(node) = open.tree.node(&line_node.node) else {
            return Ok(None);
        };
        if node.kind != NodeKind::Line {
            return Ok(None);
        }
        let Some((prompt, row)) = node
            .id
            .strip_prefix("prompt:")
            .and_then(|rest| rest.split_once(':'))
        else {
            return Ok(None);
        };
        if prompt != prompt_id {
            return Ok(None);
        }
        if !matches!(row, "yes" | "no" | "submit" | "cancel" | "done") && !row.starts_with("item:")
        {
            return Ok(None);
        }
        let Some(prompt_view) = open
            .view()
            .as_any()
            .and_then(|any| any.downcast_ref::<PromptView>())
        else {
            return Ok(None);
        };
        let cols = open.descriptor().viewport.cols as usize;
        let text = prompt_view.document().render(cols).lines.join("\n");
        Ok(Some((node.id.clone(), text)))
    }

    /// Answer the open document prompt from a rendered row and document text.
    ///
    /// The document is reconciled against the authoritative tree first. A
    /// document repaired by the client is re-rendered and stays open; a
    /// multi-choice item row toggles the stored selection and re-renders
    /// without finishing; otherwise `row_id` selects the answer and the prompt
    /// finishes normally.
    pub fn answer_document_prompt(
        &mut self,
        row_id: &str,
        document: &str,
    ) -> Result<PromptCompletion> {
        if !self.prompt_active() {
            return Err(Error::new("no prompt is active"));
        }
        let view_id = self.prompt_view_id().expect("checked prompt").to_string();
        let prompt_id = self
            .pending_prompt
            .as_ref()
            .expect("checked prompt")
            .request
            .prompt_id
            .clone();
        let cols = self
            .views
            .get(&view_id)
            .expect("checked prompt")
            .descriptor()
            .viewport
            .cols as usize;

        let (repaired, answer) = {
            let open = self.views.get_mut(&view_id).expect("checked prompt");
            let prompt_view = open
                .view_mut()
                .as_any_mut()
                .and_then(|any| any.downcast_mut::<PromptView>())
                .ok_or_else(|| Error::new("no prompt is active"))?;
            let document_state = prompt_view.document_mut();
            let reconciled = document_state
                .reconcile(document, cols)
                .map_err(Error::new)?;
            if reconciled.repaired {
                (true, None)
            } else {
                let action = document_state.answer(row_id).map_err(Error::new)?;
                let answer = match action {
                    PromptAction::Toggle(item_id) => {
                        document_state.toggle_item(&item_id).map_err(Error::new)?;
                        None
                    }
                    PromptAction::Answer(answer) => Some(answer),
                };
                (false, answer)
            }
        };

        if repaired || answer.is_none() {
            let effects = if repaired {
                vec![Effect::message(
                    MessageLevel::Warn,
                    crate::errors::STALE_VIEW.to_string(),
                )]
            } else {
                Vec::new()
            };
            let render = self
                .views
                .get_mut(&view_id)
                .expect("checked prompt")
                .render_with_delivery(None, None, RenderDelivery::Acknowledged)?;
            self.publish();
            return Ok(PromptCompletion {
                result: InvokeResult {
                    effects,
                    render: Some(render),
                    ..InvokeResult::empty()
                },
                next: Execution::Done(InvokeResult::empty()),
            });
        }

        self.finish_document_prompt(&prompt_id, answer.flatten())
    }

    /// Pop the active prompt, resume its continuation and return both steps.
    ///
    /// Validation happens before any mutation, so a mismatched or stale id
    /// leaves the view stack and the pending prompt unchanged.
    pub fn finish_document_prompt(
        &mut self,
        prompt_id: &str,
        answer: Option<crate::protocol::PromptValue>,
    ) -> Result<PromptCompletion> {
        let pending = self
            .pending_prompt
            .as_ref()
            .ok_or_else(|| Error::new("no prompt is active"))?;
        if pending.request.prompt_id != prompt_id {
            return Err(Error::new(format!(
                "prompt {prompt_id} is no longer active"
            )));
        }
        let prompt_view = pending
            .prompt_view
            .clone()
            .ok_or_else(|| Error::new("prompt is not open"))?;
        if self.views.get(&prompt_view).is_none() {
            return Err(Error::new("prompt is no longer active"));
        }
        let mut result = self.pop_view(&prompt_view)?;
        if let Some(pending) = self.pending_prompt.as_mut() {
            pending.prompt_view = None;
        }
        self.resume_deferred_refreshes(&mut result);
        let next = self.resume_prompt(prompt_id, answer)?;
        Ok(PromptCompletion { result, next })
    }

    /// Apply every refresh queued while the prompt owned the session.
    ///
    /// `refresh` and `refresh_all` queue each view instead of rendering while a
    /// document prompt is active, so finishing or cancelling the prompt resumes
    /// all of them, not only its origin. Text-mode views keep their queued
    /// refresh so the user's edits survive; a view whose backing state cannot
    /// be re-read is skipped, matching `refresh_all`.
    fn resume_deferred_refreshes(&mut self, result: &mut InvokeResult) {
        let ids: Vec<String> = self
            .views
            .iter()
            .filter(|view| view.dirty && view.mode() != ViewMode::Text)
            .map(|view| view.descriptor().id.clone())
            .collect();
        for id in ids {
            let Some(view) = self.views.get_mut(&id) else {
                continue;
            };
            let Ok(render) = view.refresh_and_render(None) else {
                // Keep `dirty`: a failed re-read must not discard the queue.
                continue;
            };
            view.dirty = false;
            let keymap = self.keymap_notice(&id);
            result.opened.push((render, keymap));
        }
    }

    fn resume_pending(
        &mut self,
        prompt_id: &str,
        answer: Option<crate::protocol::PromptValue>,
    ) -> Result<Execution> {
        if !self
            .pending_prompt
            .as_ref()
            .is_some_and(|pending| pending.request.prompt_id == prompt_id)
        {
            return Ok(Execution::Done(InvokeResult::empty()));
        }
        let pending = self.pending_prompt.take().expect("checked pending prompt");
        if pending.invalidated
            || pending
                .cancel
                .as_ref()
                .is_some_and(|cancel| cancel.is_cancelled())
            || self.views.get(&pending.view_id).is_none()
        {
            return Ok(Execution::Done(InvokeResult::empty()));
        }
        let mut result = match pending.action {
            SessionContinuation::Option {
                key,
                menu,
                cursor,
                reader,
            } => {
                if self
                    .views
                    .get(&pending.view_id)
                    .and_then(|view| view.menu())
                    != Some(menu)
                {
                    return Ok(Execution::Done(InvokeResult::empty()));
                }
                let value = match answer {
                    Some(crate::protocol::PromptValue::Text(text)) => Some(text),
                    Some(crate::protocol::PromptValue::Choice(PromptChoiceAnswer::Id { id })) => {
                        Some(id)
                    }
                    Some(crate::protocol::PromptValue::Choice(PromptChoiceAnswer::Custom {
                        custom,
                    })) => Some(custom),
                    None => None,
                    _ => return Err(Error::new("Unexpected option answer")),
                };
                if let Some(value) = value {
                    if let Err(message) = self.validate_option_value(reader, &value) {
                        return Ok(Execution::Done(InvokeResult {
                            effects: vec![Effect::message(MessageLevel::Warn, message)],
                            ..InvokeResult::empty()
                        }));
                    }
                    self.views
                        .get_mut(&pending.view_id)
                        .and_then(|view| view.menu_mut())
                        .expect("checked menu")
                        .set_option(&key, Some(value));
                    self.menu_rerender(&pending.view_id, cursor)?
                } else {
                    InvokeResult {
                        keymap: self.keymap_notice(&pending.view_id),
                        ..InvokeResult::empty()
                    }
                }
            }
            SessionContinuation::Command {
                command_id,
                action,
                mut effects,
            } => {
                let view = self.views.get_mut(&pending.view_id).expect("checked view");
                let mut ctx = commands::CommandCtx::new(
                    view,
                    &self.repo_path,
                    &self.keymaps,
                    pending.cursor,
                    None,
                    None,
                    None,
                    &crate::ui::NullUi,
                );
                let outcome = commands::resume_prompt(&mut ctx, action, answer)?;
                effects.extend(ctx.into_effects());
                self.finish_command(
                    &pending.view_id,
                    &command_id,
                    pending.cursor,
                    effects,
                    outcome,
                )?
            }
        };
        if let Some(mut prepared) = pending.prepared {
            prepared.effects.extend(result.effects);
            result.effects = prepared.effects;
            if result.keymap.is_none() {
                result.keymap = prepared.keymap;
            }
        }
        Ok(self.execution(result))
    }

    /// Pop the exact retained concrete log view and focus its caller.
    pub fn back_from_plain_log(&mut self, view_id: &str, generation: u64) -> Result<InvokeResult> {
        let valid = self.views.get(view_id).is_some_and(|view| {
            view.version() == generation && view.descriptor().scope.as_str() == "log"
        });
        if !valid {
            return Ok(unbound("common.cancel"));
        }
        let result = self.pop_view(view_id)?;
        self.publish();
        Ok(result)
    }

    /// The nearest ancestor matching the command's target kind and scope.
    fn matching_ancestor(&self, view_id: &str, command: &commands::Command) -> Option<String> {
        let mut current = self.views.previous_of(view_id);
        while let Some(view) = current {
            let id = view.descriptor().id.clone();
            if (command.target == commands::CommandTarget::View || !view.is_menu())
                && command.applies_to(view.descriptor().scope.as_str())
            {
                return Some(id);
            }
            current = self.views.previous_of(&id);
        }
        None
    }

    /// Remove a view and return focus to the one below it.
    fn pop_view(&mut self, view_id: &str) -> Result<InvokeResult> {
        self.views.remove(view_id);
        self.pending_selections.remove(view_id);
        self.pending_selections
            .retain(|_, pending| pending.caller_id != view_id);
        let mut effects = vec![Effect::PopView {
            view_id: view_id.to_string(),
        }];
        let keymap = match self.views.iter().last() {
            Some(previous) => {
                let id = previous.descriptor().id.clone();
                effects.push(Effect::FocusView {
                    view_id: id.clone(),
                });
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
            .render_with_delivery(cursor, None, RenderDelivery::Acknowledged)
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
        args: Option<&Value>,
    ) -> Result<(ViewDescriptor, ViewRender)> {
        let view = create_view(&self.repo_path, &kind, args, &self.keymaps, None)?;
        self.push_view(parent_id, kind, placement, view)
    }

    /// Push `view` above `parent_id` under a fresh id and render it once.
    fn push_view(
        &mut self,
        parent_id: &str,
        kind: ViewKind,
        placement: Placement,
        view: Box<dyn View>,
    ) -> Result<(ViewDescriptor, ViewRender)> {
        let placement = self.placement(placement);
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

        let (descriptor, render, mut open_view) = initialize_open_view(
            &self.repo_path,
            id,
            kind,
            placement,
            viewport,
            view,
            self.draft_owner.clone(),
        );
        open_view.published = self.published.clone();
        self.views.push(open_view);
        Ok((descriptor, render))
    }

    /// Push the commit-selection log for `request` above `caller_id`.
    ///
    /// Records the caller and optional continuation so the selection result can
    /// either copy the commits or write one back to the caller's menu option.
    fn push_selection(
        &mut self,
        caller_id: &str,
        request: SelectionRequest,
        continuation: Option<SessionContinuation>,
        effects: Vec<Effect>,
    ) -> Result<InvokeResult> {
        let view = match LogView::new(
            PathBuf::from(&self.repo_path),
            request.options,
            Some(request.settings),
        ) {
            Ok(view) => Box::new(view),
            Err(err) => {
                if let Some(effect) = Effect::from_error(&err) {
                    return Ok(InvokeResult {
                        effects: [effects, vec![effect]].concat(),
                        ..InvokeResult::empty()
                    });
                }
                return Err(err);
            }
        };
        let (descriptor, mut render) =
            self.push_view(caller_id, ViewKind::Log, Placement::Main, view)?;
        if let Some(node) = self.views.get(&descriptor.id).and_then(|view| {
            view.tree()
                .nodes
                .iter()
                .find(|node| node.kind == NodeKind::Commit)
        }) {
            render.cursor = Some(Cursor {
                line: node.start_line,
                column: 0,
            });
        }
        self.pending_selections.insert(
            descriptor.id.clone(),
            PendingSelection {
                caller_id: caller_id.to_string(),
                operation: request.operation,
                continuation,
            },
        );
        let mut all = effects;
        all.push(Effect::PushView { descriptor });
        let keymap = self.keymap_notice(&render.view_id);
        Ok(InvokeResult {
            effects: all,
            render: None,
            keymap: None,
            opened: vec![(render, keymap)],
            commit: None,
        })
    }

    /// Re-read the backing state and render, unless the view is in text mode.
    ///
    /// Text mode queues the refresh instead: returning `None` keeps the client's
    /// edited text in place. An active document prompt also queues it, so the
    /// origin is rendered once the prompt finishes. The file watcher (R-3.5) and
    /// `common.refresh` both route through here.
    pub fn refresh(&mut self, view_id: &str) -> Result<Option<ViewRender>> {
        let result = self.refresh_view(view_id);
        self.publish();
        result
    }

    fn refresh_view(&mut self, view_id: &str) -> Result<Option<ViewRender>> {
        let prompt_active = self.prompt_active();
        let view = self
            .views
            .get_mut(view_id)
            .ok_or_else(|| Error::new(format!("unknown view: {view_id}")))?;
        if prompt_active {
            view.queue_refresh();
            return Ok(None);
        }
        if view.mode() == ViewMode::Text {
            view.queue_refresh();
            return Ok(None);
        }
        let menu = view.view.menu();
        view.view.on_refresh()?;
        let mut tree = view.render_tree();
        tree.version = view.version;
        // A watcher burst may change another open view without changing this
        // one's text or action metadata. Do not invalidate outstanding offers
        // for an identical authoritative tree (including mutation identities).
        if tree == view.tree && menu == view.view.menu() {
            return Ok(None);
        }
        Ok(Some(view.render_tree_with_delivery(
            tree,
            None,
            None,
            RenderDelivery::Acknowledged,
        )?))
    }

    /// Re-render and publish the active view after a pending draft is admitted.
    ///
    /// A repository operation creates its draft after the command published its
    /// render, so the published snapshot predates the pending row. Rendering the
    /// active view again makes that snapshot and the retained tree include the
    /// row, so an unchanged watcher refresh sees no difference and emits no
    /// second status edit.
    pub fn publish_pending_draft(&mut self) -> Result<()> {
        let Some(view_id) = self
            .views
            .iter()
            .last()
            .map(|view| view.descriptor().id.clone())
        else {
            return Ok(());
        };
        self.refresh(&view_id)?;
        Ok(())
    }

    /// Re-read every open view's backing state and collect the new renders.
    ///
    /// This is the single refresh entry for the file watcher and detached
    /// workers: one lock acquisition refreshes the whole session, and text-mode
    /// views queue their refresh instead of replacing the client's text. Views
    /// whose backing state cannot be re-read (for example a deleted hunk source)
    /// are skipped rather than failing the whole refresh.
    pub fn refresh_all(&mut self) -> Vec<ViewRender> {
        let ids: Vec<String> = self
            .views
            .iter()
            .map(|view| view.descriptor().id.clone())
            .collect();
        ids.into_iter()
            .filter_map(|id| self.refresh(&id).ok().flatten())
            .collect()
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
        let result = self.switch_mode(params, ui);
        self.publish();
        result
    }

    fn switch_mode(
        &mut self,
        params: &ViewSetModeParams,
        ui: &dyn PromptUi,
    ) -> Result<InvokeResult> {
        if self.pending_prompt.is_some() {
            return Ok(prompt_busy());
        }
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
        let view = self.views.iter().find(|view| view.descriptor.uri == uri)?;
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
    repo: Option<GitRepo>,
) -> Result<Box<dyn View>> {
    match kind {
        ViewKind::Status => Ok(Box::new(match repo {
            Some(repo) => StatusView::open_repo(repo)?,
            None => StatusView::open(repo_path)?,
        })),
        ViewKind::Log => {
            let options = match args {
                Some(args) => {
                    if args.get("options").is_some() || args.get("switches").is_some() {
                        log_options_from_args(Some(args))?
                    } else {
                        serde_json::from_value::<crate::git::GitLogOptions>(args.clone())?
                    }
                }
                None => crate::git::GitLogOptions::default(),
            };
            Ok(Box::new(crate::log_view::LogView::new(
                repo_path.into(),
                options,
                None,
            )?))
        }
        ViewKind::RebaseTodo => Ok(Box::new(crate::rebase_todo_view::RebaseTodoView::load(
            GitRepo::open(repo_path)?,
        )?)),
        ViewKind::ProcessLog => Ok(Box::new(ProcessLogView::new())),
        ViewKind::Prompt => Err(Error::new("The `prompt` view is not client-openable.")),
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

/// Convert menu arguments or a typed log-options payload into Git query options.
pub(crate) fn log_options_from_args(args: Option<&Value>) -> Result<GitLogOptions> {
    let Some(args) = args else {
        return Ok(GitLogOptions::default());
    };
    if args.get("options").is_none() && args.get("switches").is_none() {
        let mut log_args = args.clone();
        if let Some(object) = log_args.as_object_mut() {
            for key in ["purpose", "prompt", "multi", "allowFreeText"] {
                object.remove(key);
            }
        }
        return Ok(serde_json::from_value(log_args)?);
    }
    let values: crate::menu::Args = serde_json::from_value(args.clone())?;
    let mut options = GitLogOptions::default();
    for (key, value) in values.options {
        match key.as_str() {
            "=n" => {
                options.page_size = value
                    .parse()
                    .ok()
                    .filter(|size| *size > 0)
                    .ok_or_else(|| Error::new("Log page size must be a positive integer."))?
            }
            "=a" => options.author = Some(value),
            "=s" => options.since = Some(value),
            "=u" => options.until = Some(value),
            "=g" => options.grep = Some(value),
            "=S" => options.pickaxe = Some(value),
            "=G" => options.pickaxe_regex = Some(value),
            "=f" => options.paths.push(value),
            _ => return Err(Error::new(format!("Unsupported log option: {key}"))),
        }
    }
    options.merges_only = values.switches.iter().any(|s| s == "=M");
    options.no_merges = values.switches.iter().any(|s| s == "=m");
    options.first_parent = values.switches.iter().any(|s| s == "=p");
    options.graph = values.switches.iter().any(|s| s == "-g");
    options.decorate = values.switches.iter().any(|s| s == "-d");
    Ok(options)
}

/// A URI unique to one view, so positional requests resolve the right tree.
fn view_uri(repo_path: &str, kind: &ViewKind, id: &str) -> String {
    match kind {
        ViewKind::Status => status_uri(repo_path),
        ViewKind::Log => format!("magit://log/{id}"),
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
