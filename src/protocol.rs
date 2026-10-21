use crate::git;
use crate::tree::Document;
use crate::view::{Placement, ViewDescriptor, ViewKind, Viewport};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Severity of a message effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MessageLevel {
    Info,
    Warn,
    Error,
}

/// A user-facing consequence of a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Effect {
    /// Show a notification; `detail` and `processLogId` are optional extras.
    Message {
        level: MessageLevel,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        hint: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        #[serde(rename = "processLogId", skip_serializing_if = "Option::is_none")]
        process_log_id: Option<u64>,
    },
}

impl Effect {
    /// Create a plain message effect with no hint or detail.
    pub fn message(level: MessageLevel, text: impl Into<String>) -> Self {
        Effect::Message {
            level,
            text: text.into(),
            hint: None,
            detail: None,
            process_log_id: None,
        }
    }
}

/// The result of a mutation: fresh status, rendered view and user effects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationResponse {
    pub status: git::Status,
    pub effects: Vec<Effect>,
    pub document: Document,
}

/// `vcs/status`: fetch the structured repository status and rendered view.
pub enum StatusRequest {}
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct StatusParams {
    pub repo_path: String,
    /// Buffer URI; registering it lets later positional requests find the tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}
/// Status fields stay at the top level; the rendered view rides along.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    #[serde(flatten)]
    pub status: git::Status,
    pub document: Document,
}
impl lsp_types::request::Request for StatusRequest {
    type Params = StatusParams;
    type Result = StatusResponse;
    const METHOD: &'static str = "vcs/status";
}

/// `vcs/diff`: unified diff for a single file.
pub enum DiffRequest {}
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct DiffParams {
    pub repo_path: String,
    pub path: String,
    pub staged: bool,
}
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct DiffResponse {
    pub path: String,
    pub staged: bool,
    pub diff: String,
}
impl lsp_types::request::Request for DiffRequest {
    type Params = DiffParams;
    type Result = DiffResponse;
    const METHOD: &'static str = "vcs/diff";
}

macro_rules! mutation_request {
    ($name:ident, $method:literal, $params:ty) => {
        pub enum $name {}
        impl lsp_types::request::Request for $name {
            type Params = $params;
            type Result = MutationResponse;
            const METHOD: &'static str = $method;
        }
    };
}

/// Parameters for a whole-hunk stage/unstage.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct HunkMutationParams {
    pub repo_path: String,
    pub path: String,
    pub hunk_id: String,
}
mutation_request!(StageHunkRequest, "vcs/stageHunk", HunkMutationParams);
mutation_request!(UnstageHunkRequest, "vcs/unstageHunk", HunkMutationParams);

/// Parameters for a line-range stage/unstage within a hunk.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct LinesMutationParams {
    pub repo_path: String,
    pub path: String,
    pub hunk_id: String,
    pub start: usize,
    pub end: usize,
}
mutation_request!(StageLinesRequest, "vcs/stageLines", LinesMutationParams);
mutation_request!(UnstageLinesRequest, "vcs/unstageLines", LinesMutationParams);

/// Parameters for a whole-file stage/unstage.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct FileMutationParams {
    pub repo_path: String,
    pub path: String,
}
mutation_request!(StageFileRequest, "vcs/stageFile", FileMutationParams);
mutation_request!(UnstageFileRequest, "vcs/unstageFile", FileMutationParams);

/// Parameters for a repo-wide stage/unstage.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct RepoMutationParams {
    pub repo_path: String,
}
mutation_request!(StageAllRequest, "vcs/stageAll", RepoMutationParams);
mutation_request!(UnstageAllRequest, "vcs/unstageAll", RepoMutationParams);
mutation_request!(StageUntrackedRequest, "vcs/stageUntracked", RepoMutationParams);

/// A cursor position in the rendered view, 0-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    pub line: usize,
    pub column: usize,
}

/// An inclusive document-line selection, as produced by visual mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub start_line: usize,
    pub end_line: usize,
}

/// Replace `start_line..end_line` with `lines` (end exclusive).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineEdit {
    pub start_line: usize,
    pub end_line: usize,
    pub lines: Vec<String>,
}

/// `view/render`: a full document or a line-range update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewRender {
    pub view_id: String,
    pub version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_version: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edits: Option<Vec<LineEdit>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<Cursor>,
}

/// Method name of the server's render notification.
pub const VIEW_RENDER: &str = "view/render";
/// Method name of the client's ack notification.
pub const VIEW_RENDERED: &str = "view/rendered";
/// Method name of the client's viewport notification.
pub const VIEW_RESIZE: &str = "view/resize";

/// `view/open`: create a view and receive its descriptor.
pub enum ViewOpenRequest {}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewOpenParams {
    pub kind: ViewKind,
    /// Kind-specific arguments. A status view carries `repo_path` here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<Placement>,
    pub viewport: Viewport,
}
impl ViewOpenParams {
    /// Repository root carried by a status view's arguments.
    pub fn repo_path(&self) -> Option<&str> {
        self.args
            .as_ref()?
            .get("repo_path")
            .and_then(Value::as_str)
    }
}
impl lsp_types::request::Request for ViewOpenRequest {
    type Params = ViewOpenParams;
    type Result = ViewDescriptor;
    const METHOD: &'static str = "view/open";
}

/// `view/close`: drop a view from its session.
pub enum ViewCloseRequest {}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewCloseParams {
    pub view_id: String,
}
impl lsp_types::request::Request for ViewCloseRequest {
    type Params = ViewCloseParams;
    type Result = Value;
    const METHOD: &'static str = "view/close";
}

/// `view/invoke`: run a command against a view.
pub enum ViewInvokeRequest {}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewInvokeParams {
    pub view_id: String,
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<Cursor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<Selection>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewInvokeResponse {
    pub effects: Vec<Effect>,
}
impl lsp_types::request::Request for ViewInvokeRequest {
    type Params = ViewInvokeParams;
    type Result = ViewInvokeResponse;
    const METHOD: &'static str = "view/invoke";
}

/// `view/resync`: return the current full text after a lost update.
pub enum ViewResyncRequest {}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewResyncParams {
    pub view_id: String,
}
impl lsp_types::request::Request for ViewResyncRequest {
    type Params = ViewResyncParams;
    type Result = ViewRender;
    const METHOD: &'static str = "view/resync";
}

/// `view/rendered`: the client applied a version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewRenderedParams {
    pub view_id: String,
    pub version: u64,
}

/// `view/resize`: the client's text area changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewResizeParams {
    pub view_id: String,
    pub cols: u32,
    pub rows: u32,
}

/// `vcs/requestTransient`: reserved SDUI schema for commit-style menus (stub).
pub enum TransientMenuRequest {}
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct TransientMenuParams {
    pub trigger_key: String,
    pub document_uri: String,
    pub cursor_line: u32,
}
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct TransientMenuResponse {
    pub title: String,
    pub flags: Vec<TransientFlag>,
    pub actions: Vec<TransientAction>,
}
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct TransientFlag {
    pub flag: String,
    pub description: String,
    pub active: bool,
}
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct TransientAction {
    pub key: String,
    pub command: String,
    pub description: String,
}
impl lsp_types::request::Request for TransientMenuRequest {
    type Params = TransientMenuParams;
    type Result = TransientMenuResponse;
    const METHOD: &'static str = "vcs/requestTransient";
}
