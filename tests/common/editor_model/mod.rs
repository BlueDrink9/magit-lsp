//! Composable fake-editor declarations and behaviors used by conformance tests.

use serde_json::{json, Value};

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

/// Neovim's explicitly modeled documentChanges acceptance behavior.
// anchor: Neovim 0.12.6 trace contract in tests/fixtures/lsp_traces/README.md.
pub const NEOVIM_UNDECLARED_CHANGES: Module = Module { declares: "{}", behaves: &["neovim-accepts-undeclared-documentChanges"] };
/// Neovim's observed stale-first/later-entry apply behavior.
// anchor: T005 contract and retained rapid-repair evidence in tests/fixtures/lsp_traces/README.md.
pub const SILENT_STALE_APPLY: Module = Module { declares: "{}", behaves: &["silent-stale-apply"] };
/// The test-only decomposed didChange behavior.
// anchor: tests/plain_render.rs decomposed-change profile assertions.
pub const DECOMPOSED_CHANGES: Module = Module { declares: "{}", behaves: &["decomposed-changes"] };
/// Declaration contribution for optional workspace and window features.
// anchor: tests/plain_harness.rs minimum model composition contract.
pub const EXTRA_DECLARATIONS: Module = Module { declares: r#"{"workspace":{"workspaceEdit":{"documentChanges":true}},"window":{"showDocument":{"support":true}}}"#, behaves: &[] };

/// One independently anchored model contribution.
#[derive(Clone, Copy, Debug)]
pub struct Module {
    /// LSP declarations contributed by this module.
    pub declares: &'static str,
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
}

impl EditorModel {
    /// Create a model with the exact requested minimum declaration base.
    pub fn new(base: &str) -> Self {
        let base = match base { PLAIN_MIN => Base::PlainMin, RICH_MIN => Base::RichMin, _ => panic!("unknown editor model base: {base}") };
        Self { base, modules: Vec::new() }
    }

    /// Add a module whose declarations and behaviors remain independently inspectable.
    pub fn with_module(mut self, module: Module) -> Self {
        self.modules.push(module);
        self
    }

    /// Build initialize capabilities from base plus declared module contributions.
    pub fn capabilities(&self) -> Value {
        let mut value = if self.base == Base::PlainMin {
            json!({"workspace":{"applyEdit":true},"textDocument":{"codeAction":{}}})
        } else {
            json!({"experimental":{"magit":{"v":1,"keys":"dynamic","modifiers":"basic","layout":["main"],"effects":[],"prompts":[]}}})
        };
        for module in &self.modules {
            let declarations: Value = serde_json::from_str(module.declares).expect("module declarations are JSON");
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
