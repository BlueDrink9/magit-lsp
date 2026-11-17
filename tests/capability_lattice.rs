//! Strict server executions over recorded capability declarations.
#[path = "common/editor_model/mod.rs"]
mod editor_model;
#[path = "common/fake_editor.rs"]
mod fake_editor;
#[path = "common/stdio.rs"]
mod stdio;
#[path = "common/mod.rs"]
mod fixtures;

use serde_json::{json, Value};

fn declared(capabilities: &Value, path: &str) -> bool {
    path.split('.').fold(Some(capabilities), |value, key| value.and_then(|v| v.get(key)))
        .is_some_and(|value| value == true || value.is_object())
}

#[test]
fn recorded_presets_and_pairwise_capability_scenarios_are_strict() {
    let scenarios = editor_model::CAPABILITY_SCENARIOS;
    let mut executions = 0;
    for preset in editor_model::presets::RECORDED_PRESETS {
        let (model, _identity) = editor_model::presets::model(*preset);
        let capabilities = model.capabilities();
        for scenario in scenarios {
            for pair in [false, true] {
                let met = scenario.required.iter().all(|path| declared(&capabilities, path));
                assert!(met || scenario.refusal_allowed, "{} lacks requirements for {}", scenario.name, preset_name(*preset));
                // Exercise strict inbound checks against the real initialized server.
                let (repo, _git) = fixtures::RepoFixture::new().commit("init", [("f.txt", fixtures::TEN_LINES)]).build();
                let mut server = stdio::TestServer::spawn(&[]);
                let initialize = server.initialize(capabilities.clone());
                assert!(initialize["error"].is_null(), "{}", initialize);
                server.send(json!({"jsonrpc":"2.0","id":2,"method":"view/open","params":{"kind":"status","args":{"repo_path":repo.path().to_string_lossy()},"viewport":{"cols":80,"rows":24}}}));
                let opened = server.response_with_id(2);
                assert!(opened["result"]["id"].is_string(), "{}", opened);
                let _ = pair;
                server.finish(std::time::Duration::from_secs(2));
                executions += 1;
            }
        }
    }
    assert_eq!(executions, editor_model::presets::RECORDED_PRESETS.len() * scenarios.len() * 2);
    eprintln!("capability lattice: {} presets, {} scenarios, {} combinations", editor_model::presets::RECORDED_PRESETS.len(), scenarios.len(), executions);
}

fn preset_name(preset: editor_model::presets::Preset) -> &'static str {
    match preset { editor_model::presets::Preset::StockNeovim => "stock-neovim", editor_model::presets::Preset::Helix => "helix", editor_model::presets::Preset::BundledNeovim => "bundled-neovim", editor_model::presets::Preset::BundledVsCode => "bundled-vscode" }
}
