//! Sessions and the view stack.
//!
//! One [`Session`] per repository root. It owns the open views, their rendered
//! trees and the last version each client acknowledged. Rendering is full on
//! the server; the wire form is a line-range update unless the client has not
//! acked the base version.

use crate::commands::{self, CommandStatus};
use crate::diff_text;
use crate::protocol::{Cursor, Effect, MessageLevel, ViewInvokeParams, ViewRender};
use crate::status_view::StatusView;
use crate::tree::{Node, NodeKind, Tree};
use crate::view::{Ctx, Placement, View, ViewDescriptor, ViewKind, Viewport, VisibilityCommand};
use crate::{Error, Result};

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
        let mut tree = self.view.render(&Ctx {
            viewport: self.descriptor.viewport,
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

    /// Whether no views are open.
    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }
}

/// One repository's open views and render versions.
pub struct Session {
    repo_path: String,
    views: ViewStack,
}

/// What a `view/invoke` produced: effects plus the render that follows it.
#[derive(Debug, Clone, PartialEq)]
pub struct InvokeResult {
    pub effects: Vec<Effect>,
    pub render: Option<ViewRender>,
}

impl Session {
    /// Create a session for a repository root.
    pub fn new(repo_path: impl Into<String>) -> Self {
        Self {
            repo_path: repo_path.into(),
            views: ViewStack::new(),
        }
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
        let id = id.into();
        if self.views.get(&id).is_some() {
            return Err(Error::new(format!("view already open: {id}")));
        }

        let mut view = create_view(&self.repo_path, &kind)?;
        let mut tree = view.render(&Ctx { viewport });
        let version = 1;
        tree.version = version;

        let descriptor = ViewDescriptor {
            id: id.clone(),
            kind: view.kind(),
            uri: status_uri(&self.repo_path),
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
        };
        self.views.push(OpenView {
            descriptor: descriptor.clone(),
            view,
            tree,
            version,
            acked: None,
            acked_version: None,
        });
        Ok((descriptor, render))
    }

    /// Close a view.
    pub fn close(&mut self, view_id: &str) -> Result<()> {
        self.views
            .remove(view_id)
            .map(|_| ())
            .ok_or_else(|| Error::new(format!("unknown view: {view_id}")))
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
    pub fn resize(&mut self, view_id: &str, viewport: Viewport) -> Result<()> {
        let view = self
            .views
            .get_mut(view_id)
            .ok_or_else(|| Error::new(format!("unknown view: {view_id}")))?;
        view.descriptor.viewport = viewport;
        Ok(())
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
        })
    }

    /// Run a registered command against a view.
    ///
    /// The command table lives in [`crate::commands`]; this method only looks
    /// it up, reports `Planned`/`Unavailable`/scope mismatches, and renders.
    pub fn invoke(&mut self, params: &ViewInvokeParams) -> Result<InvokeResult> {
        let Some(command) = commands::registry().get(&params.command) else {
            return Err(Error::new(format!("unknown command: {}", params.command)));
        };
        let repo_path = self.repo_path.clone();
        let view = self
            .views
            .get_mut(&params.view_id)
            .ok_or_else(|| Error::new(format!("unknown view: {}", params.view_id)))?;

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
                    render: None,
                });
            }
            CommandStatus::Unavailable { reason } => {
                return Ok(InvokeResult {
                    effects: vec![Effect::message(MessageLevel::Info, *reason)],
                    render: None,
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
                render: None,
            });
        }

        let mut ctx = commands::CommandCtx::new(
            view,
            &repo_path,
            params.cursor,
            params.selection,
            params.args.as_ref(),
        );
        let outcome = (command.run)(&mut ctx)?;
        let effects = ctx.into_effects();
        let render = if outcome.refresh {
            view.refresh_and_render(params.cursor)?
        } else {
            view.render(params.cursor, outcome.cursor)?
        };
        Ok(InvokeResult {
            effects,
            render: Some(render),
        })
    }

    /// The last acknowledged tree for the view showing `uri`.
    pub fn acked_tree_for_uri(&self, uri: &str) -> Option<Tree> {
        self.views
            .iter()
            .find(|view| view.descriptor.uri == uri)
            .map(|view| view.acked.clone().unwrap_or_else(|| view.tree.clone()))
    }
}

fn create_view(repo_path: &str, kind: &ViewKind) -> Result<Box<dyn View>> {
    match kind {
        ViewKind::Status => Ok(Box::new(StatusView::open(repo_path)?)),
        ViewKind::Other(name) => Err(Error::new(format!(
            "The `{name}` view is not implemented yet."
        ))),
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
