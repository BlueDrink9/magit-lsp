use magit_lsp::quirks::{self, Capability, CapabilityTest, Client, Match, REGISTRY};
use serde_json::{json, Value};

fn fixture(path: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}
fn resolve(v: &Value, overrides: Option<&Value>) -> quirks::Resolution {
    let info = v.get("clientInfo").unwrap();
    quirks::resolve(
        Client {
            name: info["name"].as_str(),
            version: info["version"].as_str(),
            capabilities: &v["capabilities"],
        },
        overrides,
    )
    .unwrap()
}

#[test]
fn captured_declarations_and_authentic_identity_match() {
    let plain = fixture("tests/fixtures/lsp_traces/initialize-plain-neovim-0.12.6.json");
    let bundled = fixture("tests/fixtures/lsp_traces/initialize-bundled-neovim-0.12.6.json");
    let helix = fixture("tests/fixtures/lsp_traces/initialize-helix-25.07.1.json");
    let p = resolve(&plain, None);
    let b = resolve(&bundled, None);
    let h = resolve(&helix, None);
    assert!(p.enabled.contains("assume-document-changes"));
    assert!(b.enabled.contains("apply-result-unreliable"));
    assert!(h.enabled.contains("decomposed-changes"));
    assert!(!h.enabled.contains("assume-show-document"));
    let caps = &plain["capabilities"];
    assert_eq!(
        quirks::resolve(
            Client {
                name: Some("Neovim"),
                version: Some("0.12.7"),
                capabilities: caps
            },
            None
        )
        .unwrap()
        .enabled
        .len(),
        0
    );
    assert_eq!(
        quirks::resolve(
            Client {
                name: Some("neovim"),
                version: Some("0.12.6"),
                capabilities: caps
            },
            None
        )
        .unwrap()
        .enabled
        .len(),
        0
    );
    assert_eq!(
        quirks::resolve(
            Client {
                name: Some("Other"),
                version: Some("0.12.6"),
                capabilities: caps
            },
            None
        )
        .unwrap()
        .enabled
        .len(),
        0
    );
}

#[test]
fn override_order_and_validation() {
    let helix = fixture("tests/fixtures/lsp_traces/initialize-helix-25.07.1.json");
    let info = &helix["clientInfo"];
    let client = Client {
        name: info["name"].as_str(),
        version: info["version"].as_str(),
        capabilities: &helix["capabilities"],
    };
    let r = quirks::resolve(
        client,
        Some(&json!({"decomposed-changes": false, "assume-show-document": true})),
    )
    .unwrap();
    assert!(!r.enabled.contains("decomposed-changes"));
    assert!(r.enabled.contains("assume-show-document"));
    assert!(quirks::resolve(client, Some(&json!({"bad-id": true}))).is_err());
    assert!(quirks::resolve(client, Some(&json!({"decomposed-changes": "on"}))).is_err());
}

#[test]
fn capability_match_requires_declared_magit_block() {
    let trace = fixture("tests/fixtures/lsp_traces/initialize-bundled-neovim-0.12.6.json");
    let info = &trace["clientInfo"];
    let client = Client {
        name: info["name"].as_str(),
        version: info["version"].as_str(),
        capabilities: &trace["capabilities"],
    };
    assert!(quirks::matches(
        client,
        Match::Capability(CapabilityTest::DeclaresMagit)
    ));
}

#[test]
fn registry_has_four_complete_entries_and_no_universal_safety_quirk() {
    assert_eq!(REGISTRY.len(), 4);
    assert!(REGISTRY
        .iter()
        .all(|q| q.capability.is_some() || q.workaround.is_some() || q.model.is_some()));
    assert_eq!(
        REGISTRY
            .iter()
            .find(|q| q.id == "assume-document-changes")
            .unwrap()
            .capability
            .unwrap()
            .capability,
        Capability::DocumentChanges
    );
    assert!(REGISTRY
        .iter()
        .find(|q| q.id == "assume-show-document")
        .unwrap()
        .matches
        .is_none());
}
