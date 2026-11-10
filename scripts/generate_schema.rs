//! Directional JSON Schema generation for Plain wire payloads.

#[path = "schema_luals.rs"]
pub mod schema_luals;
#[path = "schema_typescript.rs"]
pub mod schema_typescript;

use lsp_types::request::Request;
use magit_lsp::protocol::{
    ViewCloseParams, ViewCloseRequest, ViewEffectsParams, ViewInvokeParams, ViewInvokeRequest,
    ViewInvokeResponse, ViewKeyParams, ViewKeyRequest, ViewKeymapParams, ViewOpenParams,
    ViewOpenRequest, ViewPickParams, ViewPickRespondParams, ViewPickRespondRequest, ViewPromptParams,
    ViewPromptRespondParams, ViewPromptRespondRequest, ViewRender, ViewRenderedParams,
    ViewResizeParams, ViewResyncParams, ViewResyncRequest, ViewSetModeParams, ViewSetModeRequest,
    VIEW_EFFECTS, VIEW_KEYMAP, VIEW_PICK, VIEW_PROMPT, VIEW_RENDER, VIEW_RENDERED, VIEW_RESIZE,
};
use magit_lsp::view::ViewDescriptor;
use schemars::generate::SchemaSettings;
use schemars::JsonSchema;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// Wire direction a payload schema describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Client-to-server input, generated from the deserialization contract.
    Input,
    /// Server-to-client output, generated from the serialization contract.
    Output,
}

/// One committed schema artifact under `docs/schema/`.
pub struct Artifact {
    /// File name relative to `docs/schema/`.
    pub file: &'static str,
    /// Generated root schema.
    pub schema: Value,
}

/// Generate the root schema for one payload using the requested wire direction.
pub fn directional_schema<T: JsonSchema>(direction: Direction) -> Value {
    let settings = match direction {
        Direction::Input => SchemaSettings::draft2020_12().for_deserialize(),
        Direction::Output => SchemaSettings::draft2020_12().for_serialize(),
    };
    let schema = settings.into_generator().into_root_schema_for::<T>();
    serde_json::to_value(schema).expect("generated schema must serialize")
}

/// Whether a cataloged method carries a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    /// A client-initiated request with an explicit result.
    Request,
    /// A one-way notification, which never has a result.
    Notification,
}

/// Reference from a catalog entry to a wire payload schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Payload {
    /// A Rust wire type with its generated artifact under `docs/schema/`.
    Schema {
        /// Rust type name carried in the catalog.
        rust_type: &'static str,
        /// Artifact file name relative to `docs/schema/`.
        file: &'static str,
    },
    /// `serde_json::Value`: unconstrained JSON with no generated artifact.
    UnconstrainedJson,
}

impl Payload {
    /// Rust type name recorded in the catalog.
    pub fn rust_type(&self) -> &'static str {
        match self {
            Payload::Schema { rust_type, .. } => rust_type,
            Payload::UnconstrainedJson => "Value",
        }
    }

    /// Artifact file name, or `None` for unconstrained JSON.
    pub fn file(&self) -> Option<&'static str> {
        match self {
            Payload::Schema { file, .. } => Some(file),
            Payload::UnconstrainedJson => None,
        }
    }
}

/// One custom protocol method in the hand-maintained catalog.
pub struct MethodEntry {
    /// Wire method or notification name.
    pub method: &'static str,
    /// Direction of the initiating message.
    pub direction: Direction,
    /// Request or one-way notification.
    pub kind: MethodKind,
    /// Payload carried by the initiating message.
    pub params: Payload,
    /// Request result schema; `None` for notifications.
    pub result: Option<Payload>,
}

/// Reference a generated artifact for a Rust wire type.
fn schema_payload<T: JsonSchema>(file: &'static str) -> Payload {
    Payload::Schema {
        rust_type: std::any::type_name::<T>()
            .rsplit("::")
            .next()
            .expect("type name has a segment"),
        file,
    }
}

/// Hand-maintained catalog of the project's custom protocol methods.
///
/// Method names come from the request trait constants and notification
/// constants in `src/protocol.rs`; payloads reference the directional roots.
pub fn methods() -> Vec<MethodEntry> {
    vec![
        MethodEntry {
            method: <ViewOpenRequest as Request>::METHOD,
            direction: Direction::Input,
            kind: MethodKind::Request,
            params: schema_payload::<ViewOpenParams>("view-open-params.json"),
            result: Some(schema_payload::<ViewDescriptor>("view-descriptor.json")),
        },
        MethodEntry {
            method: <ViewCloseRequest as Request>::METHOD,
            direction: Direction::Input,
            kind: MethodKind::Request,
            params: schema_payload::<ViewCloseParams>("view-close-params.json"),
            result: Some(Payload::UnconstrainedJson),
        },
        MethodEntry {
            method: <ViewInvokeRequest as Request>::METHOD,
            direction: Direction::Input,
            kind: MethodKind::Request,
            params: schema_payload::<ViewInvokeParams>("view-invoke-params.json"),
            result: Some(schema_payload::<ViewInvokeResponse>("view-invoke-response.json")),
        },
        MethodEntry {
            method: <ViewKeyRequest as Request>::METHOD,
            direction: Direction::Input,
            kind: MethodKind::Request,
            params: schema_payload::<ViewKeyParams>("view-key-params.json"),
            result: Some(schema_payload::<ViewInvokeResponse>("view-invoke-response.json")),
        },
        MethodEntry {
            method: <ViewSetModeRequest as Request>::METHOD,
            direction: Direction::Input,
            kind: MethodKind::Request,
            params: schema_payload::<ViewSetModeParams>("view-set-mode-params.json"),
            result: Some(schema_payload::<ViewInvokeResponse>("view-invoke-response.json")),
        },
        MethodEntry {
            method: <ViewResyncRequest as Request>::METHOD,
            direction: Direction::Input,
            kind: MethodKind::Request,
            params: schema_payload::<ViewResyncParams>("view-resync-params.json"),
            result: Some(schema_payload::<ViewRender>("view-render.json")),
        },
        MethodEntry {
            method: <ViewPromptRespondRequest as Request>::METHOD,
            direction: Direction::Input,
            kind: MethodKind::Request,
            params: schema_payload::<ViewPromptRespondParams>("view-prompt-respond-params.json"),
            result: Some(Payload::UnconstrainedJson),
        },
        MethodEntry {
            method: <ViewPickRespondRequest as Request>::METHOD,
            direction: Direction::Input,
            kind: MethodKind::Request,
            params: schema_payload::<ViewPickRespondParams>("view-pick-respond-params.json"),
            result: Some(Payload::UnconstrainedJson),
        },
        MethodEntry {
            method: VIEW_RENDER,
            direction: Direction::Output,
            kind: MethodKind::Notification,
            params: schema_payload::<ViewRender>("view-render.json"),
            result: None,
        },
        MethodEntry {
            method: VIEW_EFFECTS,
            direction: Direction::Output,
            kind: MethodKind::Notification,
            params: schema_payload::<ViewEffectsParams>("view-effects-params.json"),
            result: None,
        },
        MethodEntry {
            method: VIEW_KEYMAP,
            direction: Direction::Output,
            kind: MethodKind::Notification,
            params: schema_payload::<ViewKeymapParams>("view-keymap-params.json"),
            result: None,
        },
        MethodEntry {
            method: VIEW_PROMPT,
            direction: Direction::Output,
            kind: MethodKind::Notification,
            params: schema_payload::<ViewPromptParams>("view-prompt-params.json"),
            result: None,
        },
        MethodEntry {
            method: VIEW_PICK,
            direction: Direction::Output,
            kind: MethodKind::Notification,
            params: schema_payload::<ViewPickParams>("view-pick-params.json"),
            result: None,
        },
        MethodEntry {
            method: VIEW_RENDERED,
            direction: Direction::Input,
            kind: MethodKind::Notification,
            params: schema_payload::<ViewRenderedParams>("view-rendered-params.json"),
            result: None,
        },
        MethodEntry {
            method: VIEW_RESIZE,
            direction: Direction::Input,
            kind: MethodKind::Notification,
            params: schema_payload::<ViewResizeParams>("view-resize-params.json"),
            result: None,
        },
    ]
}

/// Render one payload reference as catalog JSON.
fn payload_json(payload: &Payload) -> Value {
    match payload {
        Payload::Schema { rust_type, file } => serde_json::json!({
            "type": rust_type,
            "schema": file,
        }),
        Payload::UnconstrainedJson => serde_json::json!({
            "type": "Value",
            "unconstrained": true,
        }),
    }
}

/// Build the deterministic custom-method catalog committed as `methods.json`.
pub fn methods_catalog() -> Value {
    let methods: Vec<Value> = methods()
        .iter()
        .map(|entry| {
            serde_json::json!({
                "method": entry.method,
                "kind": match entry.kind {
                    MethodKind::Request => "request",
                    MethodKind::Notification => "notification",
                },
                "direction": match entry.direction {
                    Direction::Input => "clientToServer",
                    Direction::Output => "serverToClient",
                },
                "params": payload_json(&entry.params),
                "result": entry.result.as_ref().map(payload_json),
            })
        })
        .collect();
    serde_json::json!({
        "$comment": "Hand-maintained custom protocol method catalog; method names come from src/protocol.rs.",
        "methods": methods,
    })
}

/// Generate every committed artifact in stable file order.
pub fn artifacts() -> Vec<Artifact> {
    vec![
        Artifact {
            file: "plain-action-params.json",
            schema: directional_schema::<magit_lsp::dispatch::PlainActionParams>(Direction::Input),
        },
        Artifact {
            file: "edit-confirm-params.json",
            schema: directional_schema::<magit_lsp::protocol::EditConfirmParams>(Direction::Input),
        },
        Artifact {
            file: "edit-abort-params.json",
            schema: directional_schema::<magit_lsp::protocol::EditAbortParams>(Direction::Input),
        },
        Artifact {
            file: "edit-confirmation.json",
            schema: directional_schema::<magit_lsp::protocol::EditConfirmation>(Direction::Output),
        },
        Artifact {
            file: "edit-abort-result.json",
            schema: directional_schema::<magit_lsp::protocol::EditAbortResult>(Direction::Output),
        },
        Artifact {
            file: "effect.json",
            schema: directional_schema::<magit_lsp::protocol::Effect>(Direction::Output),
        },
        Artifact {
            file: "menu.json",
            schema: directional_schema::<magit_lsp::menu::Menu>(Direction::Output),
        },
        Artifact {
            file: "view-render.json",
            schema: directional_schema::<magit_lsp::protocol::ViewRender>(Direction::Output),
        },
        Artifact {
            file: "view-open-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewOpenParams>(Direction::Input),
        },
        Artifact {
            file: "view-descriptor.json",
            schema: directional_schema::<magit_lsp::view::ViewDescriptor>(Direction::Output),
        },
        Artifact {
            file: "view-close-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewCloseParams>(Direction::Input),
        },
        Artifact {
            file: "view-invoke-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewInvokeParams>(Direction::Input),
        },
        Artifact {
            file: "view-invoke-response.json",
            schema: directional_schema::<magit_lsp::protocol::ViewInvokeResponse>(Direction::Output),
        },
        Artifact {
            file: "view-key-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewKeyParams>(Direction::Input),
        },
        Artifact {
            file: "view-set-mode-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewSetModeParams>(Direction::Input),
        },
        Artifact {
            file: "view-resync-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewResyncParams>(Direction::Input),
        },
        Artifact {
            file: "view-rendered-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewRenderedParams>(Direction::Input),
        },
        Artifact {
            file: "view-resize-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewResizeParams>(Direction::Input),
        },
        Artifact {
            file: "view-effects-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewEffectsParams>(Direction::Output),
        },
        Artifact {
            file: "view-keymap-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewKeymapParams>(Direction::Output),
        },
        Artifact {
            file: "view-prompt-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewPromptParams>(Direction::Output),
        },
        Artifact {
            file: "view-prompt-respond-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewPromptRespondParams>(Direction::Input),
        },
        Artifact {
            file: "view-pick-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewPickParams>(Direction::Output),
        },
        Artifact {
            file: "view-pick-respond-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewPickRespondParams>(Direction::Input),
        },
        Artifact {
            file: "methods.json",
            schema: methods_catalog(),
        },
    ]
}

/// Render one artifact as deterministic pretty JSON with a trailing newline.
pub fn render(artifact: &Artifact) -> String {
    let mut out = serde_json::to_string_pretty(&artifact.schema)
        .expect("generated schema must render as JSON");
    out.push('\n');
    out
}

/// Write every artifact under `root/docs/schema` plus the generated client
/// declarations and LuaLS annotations.
pub fn write_all(root: &Path) -> std::io::Result<()> {
    let dir = root.join("docs/schema");
    fs::create_dir_all(&dir)?;
    for artifact in artifacts() {
        fs::write(dir.join(artifact.file), render(&artifact))?;
    }
    let declarations = schema_typescript::protocol_declarations()
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    let path = root.join(declarations.path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, declarations.text)?;
    let annotations = schema_luals::protocol_annotations()
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    let path = root.join(annotations.path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, annotations.text)?;
    Ok(())
}

/// Compare fresh generation with every committed artifact.
///
/// Returns one relative path per stale or missing file.
pub fn stale_files(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let dir = root.join("docs/schema");
    let mut stale = Vec::new();
    for artifact in artifacts() {
        let path = dir.join(artifact.file);
        let fresh = render(&artifact);
        let committed = fs::read_to_string(&path).unwrap_or_default();
        if committed != fresh {
            stale.push(PathBuf::from("docs/schema").join(artifact.file));
        }
    }
    let declarations = schema_typescript::protocol_declarations()
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    let path = root.join(declarations.path);
    let committed = fs::read_to_string(&path).unwrap_or_default();
    if committed != declarations.text {
        stale.push(PathBuf::from(declarations.path));
    }
    let annotations = schema_luals::protocol_annotations()
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    let path = root.join(annotations.path);
    let committed = fs::read_to_string(&path).unwrap_or_default();
    if committed != annotations.text {
        stale.push(PathBuf::from(annotations.path));
    }
    Ok(stale)
}
