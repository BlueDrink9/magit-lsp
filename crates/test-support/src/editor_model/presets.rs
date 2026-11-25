//! Recorded client declarations and stable test profile mappings.

use super::{EditorModel, Module, PLAIN_MIN, RICH_MIN};
use serde_json::{json, Value};
use std::borrow::Cow;

/// Named harness profile. These labels do not describe protocol presentations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NamedProfile {
    Plain,
    RichNative,
    RichDoc,
}

/// Recorded preset inputs, bounded by retained captures or current capability fixture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    StockNeovim,
    Helix,
    BundledNeovim,
    BundledVsCode,
}

/// Every recorded initialize declaration used by the capability lattice.
pub const RECORDED_PRESETS: &[Preset] = &[
    Preset::StockNeovim,
    Preset::Helix,
    Preset::BundledNeovim,
    Preset::BundledVsCode,
];

/// Build a composable model from captured capability declarations and identity.
pub fn model(preset: Preset) -> (EditorModel, Value) {
    let (base, capabilities, identity) = match preset {
        Preset::StockNeovim => {
            // anchor: retained initialize capability and clientInfo capture.
            let capture: Value = serde_json::from_str(include_str!(
                "../../../../tests/fixtures/lsp_traces/initialize-plain-neovim-0.12.6.json"
            ))
            .unwrap();
            (
                PLAIN_MIN,
                capture["capabilities"].clone(),
                capture["clientInfo"].clone(),
            )
        }
        Preset::Helix => {
            // anchor: retained initialize capability and clientInfo capture.
            let capture: Value = serde_json::from_str(include_str!(
                "../../../../tests/fixtures/lsp_traces/initialize-helix-25.07.1.json"
            ))
            .unwrap();
            (
                PLAIN_MIN,
                capture["capabilities"].clone(),
                capture["clientInfo"].clone(),
            )
        }
        Preset::BundledNeovim | Preset::BundledVsCode => {
            // anchor: current-client-capabilities.json current rich declaration rows.
            let (identity, rich) = current(preset);
            (RICH_MIN, json!({"experimental":{"magit":rich}}), identity)
        }
    };
    let model = EditorModel::new(base).with_module(Module {
        declares: Cow::Owned(capabilities.to_string()),
        behaves: &[],
    });
    (model, identity)
}

/// Resolve the explicit stable model assigned to a named test profile.
pub fn profile_model(profile: NamedProfile) -> EditorModel {
    match profile {
        NamedProfile::Plain => EditorModel::new(PLAIN_MIN),
        NamedProfile::RichNative => current_model(Preset::BundledNeovim),
        NamedProfile::RichDoc => current_model(Preset::BundledVsCode),
    }
}

fn current(preset: Preset) -> (Value, Value) {
    let all: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/current-client-capabilities.json"
    ))
    .expect("current capability fixture is JSON");
    let expected_name = match preset {
        Preset::BundledNeovim => "Neovim",
        Preset::BundledVsCode => "VSCodium",
        _ => unreachable!(),
    };
    let row = all
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["client"]["name"] == expected_name)
        .expect("current fixture row exists");
    (row["client"].clone(), row["magit"].clone())
}
fn current_model(preset: Preset) -> EditorModel {
    let (_, rich) = current(preset);
    EditorModel::new(RICH_MIN).with_module(Module {
        declares: Cow::Owned(json!({"experimental":{"magit":rich}}).to_string()),
        behaves: &[],
    })
}
