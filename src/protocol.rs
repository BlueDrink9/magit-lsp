use crate::git;
use serde::{Deserialize, Serialize};

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

/// The result of a mutation: fresh status plus anything the user should see.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationResponse {
    pub status: git::Status,
    pub effects: Vec<Effect>,
}

/// `vcs/status`: fetch the structured repository status.
pub enum StatusRequest {}
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct StatusParams {
    pub repo_path: String,
}
impl lsp_types::request::Request for StatusRequest {
    type Params = StatusParams;
    type Result = git::Status;
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
    pub hunk_index: usize,
}
mutation_request!(StageHunkRequest, "vcs/stageHunk", HunkMutationParams);
mutation_request!(UnstageHunkRequest, "vcs/unstageHunk", HunkMutationParams);

/// Parameters for a line-range stage/unstage within a hunk.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct LinesMutationParams {
    pub repo_path: String,
    pub path: String,
    pub hunk_index: usize,
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

/// A jump destination in the status document.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Target {
    pub kind: String,
    pub label: String,
    pub line: u32,
    pub section: Option<String>,
    pub path: Option<String>,
}

/// `vcs/targets`: navigation targets derived from the rendered document.
pub enum TargetsRequest {}
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct TargetsParams {
    pub uri: String,
}
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct TargetsResponse {
    pub targets: Vec<Target>,
}
impl lsp_types::request::Request for TargetsRequest {
    type Params = TargetsParams;
    type Result = TargetsResponse;
    const METHOD: &'static str = "vcs/targets";
}
