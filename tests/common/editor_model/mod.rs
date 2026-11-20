//! Composable fake-editor declarations and behaviors used by conformance tests.

use serde_json::{json, Value};
#[path = "strict.rs"]
pub mod strict;
#[path = "rich.rs"]
pub mod rich;
#[path = "prompts.rs"]
pub mod prompts;
#[path = "presets.rs"]
pub mod presets;
#[path = "edit_shapes.rs"]
pub mod edit_shapes;

/// Supported exact minimum declaration bases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base { PlainMin, RichMin }

/// Translate a registry behavior name to its fake-editor model.
pub fn registry_behavior(name: &str) -> Option<crate::fake_editor::Model> {
    match name {
        "neovim-accepts-undeclared-documentChanges" => Some(crate::fake_editor::Model::UndeclaredDocumentChanges),
        "silent-stale-apply" => Some(crate::fake_editor::Model::SilentStaleApply),
        "decomposed-changes" => Some(crate::fake_editor::Model::DecomposedChanges),
        _ => None,
    }
}

/// Minimum standard LSP features needed by a Plain client.
pub const PLAIN_MIN: &str = "plain-min";
/// Minimum rich protocol features needed by a rendered client.
pub const RICH_MIN: &str = "rich-min";

/// A named scenario's declared requirements and permitted fallback contract.
#[derive(Clone, Copy, Debug)]
pub struct ScenarioRequirement {
    /// Stable scenario label used in lattice diagnostics.
    pub name: &'static str,
    /// LSP capability paths required for direct delivery.
    pub required: &'static [&'static str],
    /// An alternative rich delivery contract for the same operation.
    pub alternative: &'static [&'static str],
}

/// Explicit requirements for the core plain-mode capability contract.
pub const CAPABILITY_SCENARIOS: &[ScenarioRequirement] = &[
    ScenarioRequirement { name: "stage-file", required: &["workspace.applyEdit", "textDocument.codeAction"], alternative: &["experimental.magit"] },
];

/// Contractual fallback when an optional draft delivery channel is undeclared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DraftFallback {
    /// Keep preopened text intact and reopen Git's disk template explicitly.
    ReopenFromDisk,
    /// Read the instruction message and open the named real file manually.
    ManualOpen,
    /// Use the synchronized draft's Abort code action instead of progress cancel.
    DocumentAbort,
}

/// An optional draft capability and its independently specified fallback.
pub struct DraftRequirement {
    /// Capability whose absence selects the fallback.
    pub capability: &'static str,
    /// Expected client path, not inferred from a server response.
    pub fallback: DraftFallback,
}

/// Plain draft contracts: versioned refresh, manual open, and semantic Abort.
// anchor: src/server/plain.rs send_plain_edit_document; docs/CLIENT_SPEC.md real-file commit editing.
pub const DRAFT_REQUIREMENTS: &[DraftRequirement] = &[
    DraftRequirement { capability: "workspace.workspaceEdit.documentChanges", fallback: DraftFallback::ReopenFromDisk },
    DraftRequirement { capability: "window.showDocument.support", fallback: DraftFallback::ManualOpen },
    DraftRequirement { capability: "window.workDoneProgress", fallback: DraftFallback::DocumentAbort },
];

/// Neovim's explicitly modeled documentChanges acceptance behavior.
// anchor: Neovim 0.12.6 trace contract in tests/fixtures/lsp_traces/README.md.
pub const NEOVIM_UNDECLARED_CHANGES: Module = Module { declares: std::borrow::Cow::Borrowed("{}"), behaves: &["neovim-accepts-undeclared-documentChanges"] };
/// Neovim's observed stale-first/later-entry apply behavior.
// anchor: T005 contract and retained rapid-repair evidence in tests/fixtures/lsp_traces/README.md.
pub const SILENT_STALE_APPLY: Module = Module { declares: std::borrow::Cow::Borrowed("{}"), behaves: &["silent-stale-apply"] };
/// The test-only decomposed didChange behavior.
// anchor: tests/plain_render.rs decomposed-change profile assertions.
pub const DECOMPOSED_CHANGES: Module = Module { declares: std::borrow::Cow::Borrowed("{}"), behaves: &["decomposed-changes"] };
/// Declaration contribution for optional workspace and window features.
// anchor: tests/plain_harness.rs minimum model composition contract.
pub const EXTRA_DECLARATIONS: Module = Module { declares: std::borrow::Cow::Borrowed(r#"{"workspace":{"workspaceEdit":{"documentChanges":true}},"window":{"showDocument":{"support":true}}}"#), behaves: &[] };

/// One independently anchored model contribution.
#[derive(Clone, Debug)]
pub struct Module {
    /// LSP declarations contributed by this module.
    pub declares: std::borrow::Cow<'static, str>,
    /// Fake-editor behaviors contributed by this module.
    pub behaves: &'static [&'static str],
}

/// A composed minimum base and explicit modules.
#[derive(Clone, Debug)]
pub struct EditorModel {
    /// Selected base contract.
    pub base: Base,
    /// Explicit declaration and behavior modules.
    pub modules: Vec<Module>,
    /// Reject inbound server features absent from declarations.
    pub strict: bool,
}

impl EditorModel {
    /// Create a model with the exact requested minimum declaration base.
    pub fn new(base: &str) -> Self {
        let base = match base { PLAIN_MIN => Base::PlainMin, RICH_MIN => Base::RichMin, _ => panic!("unknown editor model base: {base}") };
        Self { base, modules: Vec::new(), strict: false }
    }

    /// Add a module whose declarations and behaviors remain independently inspectable.
    pub fn with_module(mut self, module: Module) -> Self {
        self.modules.push(module);
        self
    }

    /// Enable inbound server feature validation.
    pub fn strict(mut self) -> Self { self.strict = true; self }

    /// Build initialize capabilities from base plus declared module contributions.
    pub fn capabilities(&self) -> Value {
        let mut value = if self.base == Base::PlainMin {
            json!({"workspace":{"applyEdit":true},"textDocument":{"codeAction":{}}})
        } else {
            json!({"experimental":{"magit":{"v":1,"keys":"dynamic","modifiers":"basic","layout":["main"],"effects":[],"prompts":[]}}})
        };
        for module in &self.modules {
            let declarations: Value = serde_json::from_str(&module.declares).expect("module declarations are JSON");
            merge(&mut value, declarations);
        }
        value
    }

    /// Return all explicitly modeled behaviors, excluding declaration-only content.
    pub fn behaviors(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.modules.iter().flat_map(|module| module.behaves.iter().copied())
    }
}

fn merge(target: &mut Value, source: Value) {
    match (target, source) {
        (Value::Object(dst), Value::Object(src)) => for (key, value) in src { merge(dst.entry(key).or_insert(Value::Null), value); },
        (dst, src) => *dst = src,
    }
}
