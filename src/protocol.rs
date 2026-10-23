use crate::keymap::{Binding, EditorMode};
use crate::menu::Menu;
use crate::view::{Placement, ViewDescriptor, ViewKind, ViewMode, Viewport};
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
    /// The key is not bound here; perform its normal editor behaviour.
    PassThrough { key: String },
    /// Put `text` on the system clipboard.
    Clipboard { text: String },
    /// Create a window/buffer/panel and show it.
    OpenView { descriptor: ViewDescriptor },
    /// As `OpenView`, remembering the previous view so `PopView` returns.
    PushView { descriptor: ViewDescriptor },
    /// Close that view and focus the previous one.
    PopView {
        #[serde(rename = "viewId")]
        view_id: String,
    },
    /// Focus an existing view.
    FocusView {
        #[serde(rename = "viewId")]
        view_id: String,
    },
    /// Close every view of the session.
    Quit,
    /// Open a Git-owned file in the client's own editor and keep the operation
    /// that spawned Git waiting until `common.confirm`/`common.cancel` answers
    /// with the same `edit_id`. The metadata is selected by the server: `scope`
    /// is the keymap/command scope for the edit, and the current bridge edits
    /// real files only.
    EditDocument {
        #[serde(rename = "editId")]
        edit_id: String,
        path: String,
        kind: String,
        title: String,
        mode: EditDocumentMode,
        scope: String,
    },
}

/// How an editor-bridge document is edited.
///
/// The bridge currently supports Git-owned real files; virtual edit documents
/// require a separate implementation before adding another mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditDocumentMode {
    RealFile,
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
    /// The view mode this render belongs to, so the client can show it.
    pub mode: ViewMode,
    /// The menu steps typed so far, e.g. `-` or `g`; empty otherwise.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pending: String,
    /// The structured menu model, for menu views only (§3.11).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub menu: Option<Menu>,
}

/// Method name of the server's render notification.
pub const VIEW_RENDER: &str = "view/render";
/// Method name of the server's asynchronous effect notification.
pub const VIEW_EFFECTS: &str = "view/effects";

/// `view/effects`: effects produced after a request already answered, such as
/// an `editDocument` from a detached Git process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewEffectsParams {
    pub effects: Vec<Effect>,
}
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

/// `view/key`: press one canonical key step in a view.
pub enum ViewKeyRequest {}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewKeyParams {
    pub view_id: String,
    /// The version the client's cursor and selection were taken from.
    pub version: u64,
    /// One canonical key step, e.g. `s`, `g`, `ctrl+c` (§3.8).
    pub key: String,
    pub editor_mode: EditorMode,
    pub cursor: Cursor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<Selection>,
}
impl lsp_types::request::Request for ViewKeyRequest {
    type Params = ViewKeyParams;
    type Result = ViewInvokeResponse;
    const METHOD: &'static str = "view/key";
}

/// How a client wants to receive keymaps (§3.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeymapDelivery {
    /// The server pushes `view/keymap` and the client applies it atomically.
    #[default]
    Dynamic,
    /// The client ships static bindings; no push is needed.
    Static,
    /// The client forwards every key; the server answers `PassThrough`.
    ForwardAll,
}

impl KeymapDelivery {
    /// Parse the `capabilities.keymapDelivery` value, defaulting to `dynamic`.
    pub fn parse(value: &str) -> Self {
        match value {
            "static" => KeymapDelivery::Static,
            "forwardAll" => KeymapDelivery::ForwardAll,
            _ => KeymapDelivery::Dynamic,
        }
    }
}

/// How faithfully a client can report key modifiers (§3.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyModifiers {
    #[default]
    Full,
    Basic,
}

impl KeyModifiers {
    /// Parse the `capabilities.keyModifiers` value, defaulting to `full`.
    pub fn parse(value: &str) -> Self {
        match value {
            "basic" => KeyModifiers::Basic,
            _ => KeyModifiers::Full,
        }
    }
}

/// Method name of the server's keymap notification.
pub const VIEW_KEYMAP: &str = "view/keymap";

/// `view/keymap`: the complete binding set for one view scope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewKeymapParams {
    pub view_id: String,
    pub scope: String,
    pub version: u64,
    pub capture: bool,
    pub bindings: Vec<Binding>,
    /// The view mode these bindings belong to (`text` has only the toggle).
    pub mode: ViewMode,
    /// The menu steps typed so far, e.g. `-`; empty for other views.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pending: String,
    /// The keys that do something right now, so clients need no chord logic.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub live_keys: Vec<String>,
}

/// `view/setMode`: switch a view between interactive and text mode.
pub enum ViewSetModeRequest {}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewSetModeParams {
    pub view_id: String,
    pub mode: ViewMode,
}
impl lsp_types::request::Request for ViewSetModeRequest {
    type Params = ViewSetModeParams;
    type Result = ViewInvokeResponse;
    const METHOD: &'static str = "view/setMode";
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

/// Method name of the server's prompt notification.
pub const VIEW_PROMPT: &str = "view/prompt";
/// Method name of the client's prompt answer request.
pub const VIEW_PROMPT_RESPOND: &str = "view/promptRespond";

/// The four kinds of question the server can ask (§3.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PromptKind {
    Confirm,
    Text,
    Choice,
    MultiChoice,
}

/// One selectable item of a `choice` or `multiChoice` prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptItem {
    /// The id the server receives back; the client displays `label`.
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// `view/prompt`: one question the server is waiting on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewPromptParams {
    pub prompt_id: String,
    pub kind: PromptKind,
    /// The question, shown prominently.
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yes_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destructive: Option<bool>,
    /// Kind-dependent default: a bool, a string or an item id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<PromptItem>>,
    #[serde(default, rename = "allowCustom", skip_serializing_if = "Option::is_none")]
    pub allow_custom: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preselected: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<usize>,
}

/// A `choice` answer: one item id, or the text the user typed instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PromptChoiceAnswer {
    Id { id: String },
    Custom { custom: String },
}

/// The value of a prompt answer; its shape follows the prompt kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PromptValue {
    Bool(bool),
    Text(String),
    Choice(PromptChoiceAnswer),
    Multi { ids: Vec<String> },
}

/// `view/promptRespond`: the client's answer to a prompt notification.
pub enum ViewPromptRespondRequest {}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewPromptRespondParams {
    pub prompt_id: String,
    pub cancelled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<PromptValue>,
}
impl lsp_types::request::Request for ViewPromptRespondRequest {
    type Params = ViewPromptRespondParams;
    type Result = Value;
    const METHOD: &'static str = VIEW_PROMPT_RESPOND;
}
