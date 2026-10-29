//! Validated client presentation capabilities read at the LSP initialize boundary.

use serde_json::Value;
use std::collections::BTreeSet;
use std::fmt;

mod custom;
pub use custom::{CustomRender, Ui};

/// Presentation selected by `capabilities.experimental.magit` during initialize.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Presentation {
    /// No custom selector was supplied; use the future standard-LSP presentation.
    Plain(PlainCapabilities),
    /// The client explicitly requested the versioned custom protocol.
    Rich(RichCapabilities),
}

/// A validation error in the experimental rich presentation declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityError(pub String);

impl fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CapabilityError {}

/// Parse the initialize params, selecting Plain when the selector is absent.
/// A present `experimental.magit` declaration is strictly validated as rich v1.
pub fn negotiate(params: &Value) -> Result<Presentation, CapabilityError> {
    let caps = params.get("capabilities").unwrap_or(&Value::Null);
    let Some(block) = caps.pointer("/experimental/magit") else {
        return Ok(Presentation::Plain(PlainCapabilities::from_lsp(caps)));
    };
    let object = block
        .as_object()
        .ok_or_else(|| invalid("must be an object"))?;
    let version = object.get("v").and_then(Value::as_u64);
    if version != Some(1) {
        return Err(invalid("v must be 1"));
    }
    let keys = parse_enum(object, "keys", &["dynamic", "static", "forwardAll"])?;
    let modifiers = parse_enum(object, "modifiers", &["full", "basic"])?;
    let render = parse_enum(object, "render", &["edits", "full"])?;
    let folds = parse_enum(object, "folds", &["server", "native"])?;
    let layout = parse_set(object, "layout", &["main", "side", "bottom", "float"])?;
    if !layout.contains("main") {
        return Err(invalid(
            "layout must include main (the view stack uses main)",
        ));
    }
    let effects = parse_set(
        object,
        "effects",
        &["visitFile", "editDocument", "clipboard"],
    )?;
    let prompts = parse_set(
        object,
        "prompts",
        &["confirm", "text", "choice", "multiChoice", "secret"],
    )?;
    Ok(Presentation::Rich(RichCapabilities {
        keys,
        modifiers,
        render,
        layout,
        effects,
        prompts,
        folds,
    }))
}

fn invalid(message: &str) -> CapabilityError {
    CapabilityError(format!("capabilities.experimental.magit {message}"))
}

fn parse_enum(
    object: &serde_json::Map<String, Value>,
    field: &str,
    allowed: &[&str],
) -> Result<String, CapabilityError> {
    let value = object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(&format!("{field} is required and must be a string")))?;
    if !allowed.contains(&value) {
        return Err(invalid(&format!("{field} has unsupported value {value:?}")));
    }
    Ok(value.to_owned())
}

fn parse_set(
    object: &serde_json::Map<String, Value>,
    field: &str,
    allowed: &[&str],
) -> Result<BTreeSet<String>, CapabilityError> {
    let values = object
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| invalid(&format!("{field} is required and must be an array")))?;
    let mut result = BTreeSet::new();
    for value in values {
        let item = value
            .as_str()
            .ok_or_else(|| invalid(&format!("{field} entries must be strings")))?;
        if !allowed.contains(&item) {
            return Err(invalid(&format!("{field} has unsupported value {item:?}")));
        }
        if !result.insert(item.to_owned()) {
            return Err(invalid(&format!("{field} contains duplicate {item:?}")));
        }
    }
    Ok(result)
}

/// Validated custom-render declarations. Core effects are mandatory protocol behavior;
/// `effects` and `prompts` contain only explicitly declared optional features.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichCapabilities {
    /// Key delivery strategy.
    pub keys: String,
    /// Modifier fidelity requested by the client.
    pub modifiers: String,
    /// Render transport selected by the client.
    pub render: String,
    /// Declared logical view placements; always includes `main`.
    pub layout: BTreeSet<String>,
    /// Optional effects the client can handle.
    pub effects: BTreeSet<String>,
    /// Optional prompt kinds the client can handle.
    pub prompts: BTreeSet<String>,
    /// Fold presentation strategy.
    pub folds: String,
}

impl RichCapabilities {
    /// Whether this declaration opts into a named optional effect.
    pub fn supports_effect(&self, effect: &str) -> bool {
        self.effects.contains(effect)
    }
    /// Whether this declaration opts into a named optional prompt kind.
    pub fn supports_prompt(&self, prompt: &str) -> bool {
        self.prompts.contains(prompt)
    }
    /// Whether a non-core placement was declared.
    pub fn supports_placement(&self, placement: &str) -> bool {
        self.layout.contains(placement)
    }
}

/// Standard LSP capability subset relevant to the Plain presentation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlainCapabilities {
    /// Client can accept workspace edits.
    pub apply_edit: bool,
    /// Client supports versioned `WorkspaceEdit.documentChanges`.
    pub document_changes: bool,
    /// Client supports code actions.
    pub code_actions: bool,
    /// Client supports execute-command requests.
    pub execute_command: bool,
    /// Client can represent disabled code actions and reasons.
    pub disabled_actions: bool,
    /// Client supports `window/showDocument`.
    pub show_document: bool,
    /// Client supports work-done progress.
    pub work_done_progress: bool,
    /// Client supports completion.
    pub completion: bool,
    /// Client supports folding ranges.
    pub folding_range: bool,
    /// Client's advertised position encodings.
    pub position_encodings: Vec<String>,
}

/// A standard capability with a human-readable absence reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Support {
    /// The required standard capability is advertised.
    Supported,
    /// The capability is absent; the string explains the resulting limitation.
    Missing(&'static str),
}

impl PlainCapabilities {
    fn from_lsp(caps: &Value) -> Self {
        let b = |path: &str| caps.pointer(path).and_then(Value::as_bool) == Some(true);
        let present = |path: &str| {
            caps.pointer(path)
                .is_some_and(|v| !v.is_null() && v != false)
        };
        Self {
            apply_edit: b("/workspace/applyEdit"),
            document_changes: b("/workspace/workspaceEdit/documentChanges"),
            code_actions: present("/textDocument/codeAction"),
            execute_command: present("/workspace/executeCommand"),
            disabled_actions: b("/textDocument/codeAction/disabledSupport"),
            show_document: b("/window/showDocument/support"),
            work_done_progress: b("/window/workDoneProgress"),
            completion: present("/textDocument/completion"),
            folding_range: present("/textDocument/foldingRange"),
            position_encodings: caps
                .pointer("/general/positionEncodings")
                .and_then(Value::as_array)
                .map(|xs| {
                    xs.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// Report whether workspace edits are available, with an explicit inert reason.
    pub fn edit_support(&self) -> Support {
        if self.apply_edit {
            Support::Supported
        } else {
            Support::Missing("workspace.applyEdit is not supported; presentation is inert")
        }
    }
    /// Report whether action invocation can be exposed.
    pub fn action_support(&self) -> Support {
        if self.code_actions && self.execute_command {
            Support::Supported
        } else {
            Support::Missing("textDocument.codeAction and workspace.executeCommand are required")
        }
    }
    /// Report whether file navigation and real-file commit editing can be exposed.
    pub fn show_document_support(&self) -> Support {
        if self.show_document {
            Support::Supported
        } else {
            Support::Missing("window.showDocument.support is not supported")
        }
    }
}
