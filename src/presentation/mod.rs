//! Validated client presentation capabilities read at the LSP initialize boundary.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fmt;

mod custom;
pub mod plain;
pub mod prompt;
pub mod resolve;
pub use custom::{CustomRender, PromptSupport, Ui};

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
    let declaration: MagitCapabilities =
        serde_json::from_value(block.clone()).map_err(|error| invalid(&error.to_string()))?;
    Ok(Presentation::Rich(RichCapabilities {
        keys: declaration.keys,
        modifiers: declaration.modifiers,
        layout: declaration.layout,
        effects: declaration.effects,
        prompts: declaration.prompts,
        pick: declaration.pick,
    }))
}

/// Strict version-1 rich client declaration accepted during initialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MagitCapabilities {
    /// Rich protocol version.
    #[serde(deserialize_with = "deserialize_v1")]
    pub v: u8,
    /// Client keymap delivery strategy.
    pub keys: crate::protocol::KeymapDelivery,
    /// Client key modifier fidelity.
    pub modifiers: crate::protocol::KeyModifiers,
    /// Supported view placements.
    #[serde(deserialize_with = "deserialize_layout")]
    pub layout: BTreeSet<String>,
    /// Supported optional effects.
    #[serde(deserialize_with = "deserialize_effects")]
    pub effects: BTreeSet<String>,
    /// Supported prompt kinds.
    #[serde(deserialize_with = "deserialize_prompts")]
    pub prompts: BTreeSet<String>,
    /// Optional native picker shapes the client can present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pick: Option<PickCapabilities>,
}

/// Optional native picker shapes a rich client opts into.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct PickCapabilities {
    /// The client can select more than one item.
    pub multi: bool,
    /// The client accepts an answer that is not an item id.
    pub free_text: bool,
    /// The client displays item descriptions.
    pub description: bool,
    /// The client displays item details.
    pub detail: bool,
    /// The client groups items under their group label.
    pub group: bool,
    /// The client honours pre-selected item ids.
    pub preselection: bool,
    /// The client can open the server-rendered fallback view.
    pub fallback_view: bool,
    /// The client can present at most this many items; `None` means no limit.
    pub max_items: Option<usize>,
}

fn deserialize_v1<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u8, D::Error> {
    let version = u8::deserialize(deserializer)?;
    if version != 1 {
        return Err(serde::de::Error::custom(
            "unsupported rich protocol version",
        ));
    }
    Ok(version)
}

fn deserialize_names<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
    allowed: &[&str],
    main: bool,
) -> Result<BTreeSet<String>, D::Error> {
    let values = Vec::<String>::deserialize(deserializer)?;
    let mut result = BTreeSet::new();
    for value in values {
        if !allowed.contains(&value.as_str()) {
            return Err(serde::de::Error::custom(format!(
                "unsupported value {value:?}"
            )));
        }
        if !result.insert(value.clone()) {
            return Err(serde::de::Error::custom(format!(
                "duplicate value {value:?}"
            )));
        }
    }
    if main && !result.contains("main") {
        return Err(serde::de::Error::custom("layout must include main"));
    }
    Ok(result)
}

fn deserialize_layout<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<BTreeSet<String>, D::Error> {
    deserialize_names(d, &["main", "bottom", "side", "float"], true)
}
fn deserialize_effects<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<BTreeSet<String>, D::Error> {
    deserialize_names(d, &["visitFile", "clipboard", "editDocument"], false)
}
fn deserialize_prompts<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<BTreeSet<String>, D::Error> {
    deserialize_names(
        d,
        &["confirm", "text", "choice", "multiChoice", "secret"],
        false,
    )
}

fn invalid(message: &str) -> CapabilityError {
    CapabilityError(format!("capabilities.experimental.magit {message}"))
}

/// Validated custom-render declarations. Core effects are mandatory protocol behavior;
/// `effects` and `prompts` contain only explicitly declared optional features.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichCapabilities {
    /// Key delivery strategy.
    pub keys: crate::protocol::KeymapDelivery,
    /// Modifier fidelity requested by the client.
    pub modifiers: crate::protocol::KeyModifiers,
    /// Declared logical view placements; always includes `main`.
    pub layout: BTreeSet<String>,
    /// Optional effects the client can handle.
    pub effects: BTreeSet<String>,
    /// Optional prompt kinds the client can handle.
    pub prompts: BTreeSet<String>,
    /// Optional native picker shapes the client declared.
    pub pick: Option<PickCapabilities>,
}

impl RichCapabilities {
    /// First declared effect a command needs that this client lacks.
    pub fn command_requirement(&self, command: &str) -> Option<crate::commands::EffectKind> {
        crate::commands::required_effects(command)
            .iter()
            .copied()
            .find(|effect| !self.supports_effect(effect.name()))
    }

    /// Whether both the prompt kind and any secret-text requirement are declared.
    pub fn allows_prompt(&self, prompt: &crate::protocol::ViewPromptParams) -> bool {
        use crate::protocol::PromptKind;
        let kind = match prompt.kind {
            PromptKind::Confirm => "confirm",
            PromptKind::Text => "text",
            PromptKind::Choice => "choice",
            PromptKind::MultiChoice => "multiChoice",
        };
        self.supports_prompt(kind)
            && (prompt.secret != Some(true) || self.supports_prompt("secret"))
    }

    /// Whether the client's declared pick shapes can present this request.
    /// A missing declaration, or any requested shape it does not opt into,
    /// means the client must use the document fallback instead.
    pub fn allows_pick(&self, pick: &crate::protocol::ViewPickParams) -> bool {
        self.allows_pick_with_limit(pick, self.pick.as_ref().and_then(|p| p.max_items))
    }

    /// Whether the client's declared pick shapes can present this request when
    /// `limit` overrides the declared item maximum. Every other shape check
    /// still applies; `limit` of `None` means no item-count restriction.
    pub fn allows_pick_with_limit(
        &self,
        pick: &crate::protocol::ViewPickParams,
        limit: Option<usize>,
    ) -> bool {
        let Some(caps) = self.pick.as_ref() else {
            return false;
        };
        if pick.multi && !caps.multi {
            return false;
        }
        if pick.allow_free_text && !caps.free_text {
            return false;
        }
        let needs_item = pick.items.iter().any(|item| {
            (item.description.is_some() && !caps.description)
                || (item.detail.is_some() && !caps.detail)
                || (item.group.is_some() && !caps.group)
        });
        if needs_item {
            return false;
        }
        if !pick.preselection.is_empty() && !caps.preselection {
            return false;
        }
        if pick.fallback_view.is_some() && !caps.fallback_view {
            return false;
        }
        if limit.is_some_and(|max| pick.items.len() > max) {
            return false;
        }
        true
    }

    /// Fall back to the mandatory main placement without changing view identity.
    pub fn placement(&self, placement: crate::view::Placement) -> crate::view::Placement {
        use crate::view::Placement;
        let name = match placement {
            Placement::Main => "main",
            Placement::Side => "side",
            Placement::Bottom => "bottom",
            Placement::Float => "float",
        };
        if self.supports_placement(name) {
            placement
        } else {
            Placement::Main
        }
    }

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

/// Whether a command starts a commit draft the client must open to proceed.
pub fn commit_draft_command(id: &str) -> bool {
    matches!(
        id,
        "commit.create" | "commit.amend" | "commit.extend" | "commit.reword"
    )
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
    /// The last commit run expired because the client never opened its draft.
    pub commit_draft_expired: bool,
}

/// A standard capability with a human-readable absence reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Support {
    /// The required standard capability is advertised.
    Supported,
    /// The capability is absent; the string explains the resulting limitation.
    Missing(&'static str),
}

/// Whether a command can run under the Plain presentation, and why not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlainAvailability {
    /// The command may be offered and dispatched.
    Available,
    /// The client lacks a capability the command needs.
    Unavailable {
        /// Human-readable explanation of the missing capability.
        reason: &'static str,
    },
}

impl PlainCapabilities {
    /// Decide whether a registered command can run in Plain mode.
    ///
    /// The caller decides `Planned` status; this only folds declared effects
    /// and the extra requirement of the status hunk actions.
    pub fn plain_availability(&self, command: &crate::commands::Command) -> PlainAvailability {
        use crate::commands::EffectKind;
        if self.commit_draft_expired && commit_draft_command(command.id) {
            return PlainAvailability::Unavailable {
                reason: "the commit draft was not opened in time; retry the commit",
            };
        }
        for effect in command.effects {
            let reason = match effect {
                EffectKind::Clipboard => {
                    Some("the clipboard effect is not available in Plain mode")
                }
                EffectKind::EditDocument if !self.apply_edit => {
                    Some("workspace.applyEdit is required")
                }
                EffectKind::VisitFile if !self.show_document => {
                    Some("window.showDocument.support is required")
                }
                _ => None,
            };
            if let Some(reason) = reason {
                return PlainAvailability::Unavailable { reason };
            }
        }
        if matches!(
            command.id,
            "status.stage" | "status.unstage" | "status.discard"
        ) && (!self.apply_edit || !self.code_actions)
        {
            return PlainAvailability::Unavailable {
                reason: "workspace.applyEdit and textDocument.codeAction are required",
            };
        }
        PlainAvailability::Available
    }

    fn from_lsp(caps: &Value) -> Self {
        let b = |path: &str| caps.pointer(path).and_then(Value::as_bool) == Some(true);
        let present = |path: &str| {
            caps.pointer(path)
                .is_some_and(|v| !v.is_null() && v != false)
        };
        Self {
            // Declaration is the feature: the client can receive workspace edits.
            apply_edit: b("/workspace/applyEdit"),
            // Declaration describes the versioned-edit sub-feature, not action support.
            document_changes: b("/workspace/workspaceEdit/documentChanges"),
            // Declaration is the feature: the client can request and invoke code actions.
            code_actions: present("/textDocument/codeAction"),
            // Declaration describes the disabled-action sub-feature only.
            disabled_actions: b("/textDocument/codeAction/disabledSupport"),
            // Declaration is the feature: the client can open documents by URI.
            show_document: b("/window/showDocument/support"),
            // Declaration describes progress reporting sub-feature.
            work_done_progress: b("/window/workDoneProgress"),
            // Declaration is the feature: the client can request completions.
            completion: present("/textDocument/completion"),
            // Declaration is the feature: the client can request folding ranges.
            folding_range: present("/textDocument/foldingRange"),
            // Declaration lists supported position-encoding sub-features.
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
            // Runtime state, not a client declaration: no draft has expired yet.
            commit_draft_expired: false,
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
        if self.code_actions {
            Support::Supported
        } else {
            Support::Missing("textDocument.codeAction is not supported")
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
