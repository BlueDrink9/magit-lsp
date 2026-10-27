//! View model shared by the session layer and the protocol.
//!
//! A view is one screen the server renders: a status, a log, a menu. The
//! session owns instances; the protocol sends a [`ViewDescriptor`] so a client
//! can create the matching buffer.

use crate::menu::Menu;
use crate::log_view::CommitSelection;
use crate::tree::Tree;
use crate::Result;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// The type of a view, e.g. `status`, `log` or `menu.commit`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ViewKind {
    Status,
    /// The server-rendered Git commit history.
    Log,
    /// The read-only log of finished Git invocations.
    ProcessLog,
    /// A menu (Magit transient), stored by its wire name, e.g. `menu.commit`.
    Menu(String),
    /// A known-but-unimplemented or future kind, kept verbatim on the wire.
    Other(String),
}

impl ViewKind {
    /// The wire name of this kind.
    pub fn as_str(&self) -> &str {
        match self {
            ViewKind::Status => "status",
            ViewKind::Log => "log",
            ViewKind::ProcessLog => "process-log",
            ViewKind::Menu(name) => name,
            ViewKind::Other(name) => name,
        }
    }

    /// The full wire name, e.g. `menu.commit`.
    pub fn wire_name(&self) -> String {
        self.as_str().to_string()
    }

    /// The bare menu id, for `ViewKind::Menu`.
    pub fn menu_name(&self) -> Option<&str> {
        match self {
            ViewKind::Menu(name) => Some(name.trim_start_matches("menu.")),
            _ => None,
        }
    }
}

impl fmt::Display for ViewKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.wire_name())
    }
}

impl Serialize for ViewKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.wire_name())
    }
}

impl<'de> Deserialize<'de> for ViewKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Ok(match name.as_str() {
            "status" => ViewKind::Status,
            "log" => ViewKind::Log,
            "process-log" => ViewKind::ProcessLog,
            other if other.starts_with("menu.") => ViewKind::Menu(other.to_string()),
            _ => ViewKind::Other(name),
        })
    }
}

/// Where the server suggests the client put a view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Placement {
    Main,
    Side,
    Bottom,
    Float,
}

impl Default for Placement {
    fn default() -> Self {
        Placement::Main
    }
}

/// Whether the client may edit the view or only display it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewMode {
    Interactive,
    Text,
}

impl Default for ViewMode {
    fn default() -> Self {
        ViewMode::Interactive
    }
}

/// A named group of key bindings, e.g. `common` or `status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Scope(String);

impl Scope {
    /// Create a scope name.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// The scope name as sent on the wire.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The client's text area, needed to lay out multi-column views.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Viewport {
    pub cols: u32,
    pub rows: u32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self { cols: 80, rows: 24 }
    }
}

/// Everything a client needs to create the buffer for a view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewDescriptor {
    pub id: String,
    pub kind: ViewKind,
    pub uri: String,
    pub title: String,
    pub placement: Placement,
    pub mode: ViewMode,
    pub scope: Scope,
    pub capture: bool,
    /// keep-for: TASKS §3.2 records viewport in the descriptor; clients may use it when creating the view.
    pub viewport: Viewport,
}

/// Render context passed to a view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ctx<'a> {
    pub viewport: Viewport,
    /// The menu steps typed so far; empty for every other view.
    pub pending: &'a [String],
}

/// A visibility change requested by one of the `common.*` fold commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisibilityCommand {
    /// Show or hide the node under the cursor.
    Toggle { recursive: bool },
    /// Expand the node under the cursor.
    Show { recursive: bool },
    /// Collapse the node under the cursor.
    Hide { recursive: bool },
    /// Expand every node.
    ShowAll,
    /// Collapse every section.
    HideAll,
    /// Show one more level of detail.
    MoreDetail,
    /// Show one less level of detail.
    LessDetail,
    /// Set the status view to an absolute detail level (1–4).
    SetDetail(u8),
}

/// One server-owned screen.
pub trait View: Send {
    /// The view kind this implementation renders.
    fn kind(&self) -> ViewKind;

    /// Human-readable title for the client's buffer or window.
    fn title(&self) -> String;

    /// Interactive unless the view is a read-only text document.
    fn mode(&self) -> ViewMode {
        ViewMode::Interactive
    }

    /// Key-binding scope for this view.
    fn scope(&self) -> Scope;

    /// Whether the client must forward every key while this view is focused.
    fn capture(&self) -> bool {
        false
    }

    /// Render the current state. Must not touch the repository; refresh first.
    fn render(&mut self, ctx: &Ctx<'_>) -> Tree;

    /// The structured menu model, for clients that prefer native widgets.
    fn menu(&self) -> Option<Menu> {
        None
    }

    /// The menu model for server-side key handling and argument changes.
    fn menu_mut(&mut self) -> Option<&mut Menu> {
        None
    }

    /// Selection settings attached to this view, if any.
    fn commit_selection(&self) -> Option<&CommitSelection> {
        None
    }

    /// Selected commit IDs in view order, optionally using the cursor commit.
    fn selected_commits(&self, _cursor_oid: Option<&str>) -> Vec<String> {
        Vec::new()
    }

    /// Toggle a mark for a commit identified by its full object ID.
    fn toggle_commit_mark(&mut self, _oid: &str) -> bool {
        false
    }

    /// Apply a visibility command to the node at `line` of `tree`.
    ///
    /// Returns `false` when the view has no visibility model.
    fn set_visibility(
        &mut self,
        _tree: &Tree,
        _line: Option<usize>,
        _command: VisibilityCommand,
    ) -> bool {
        false
    }

    /// Re-read whatever backs this view before the next render.
    fn on_refresh(&mut self) -> Result<()> {
        Ok(())
    }

    /// Extend a lazy view by one page, when the kind supports it.
    /// keep-for: TASKS §3.2 view contract; LogView implements page extension (TASKS §7 R-5.1).
    fn load_more(&mut self) -> Result<()> {
        Ok(())
    }
}
