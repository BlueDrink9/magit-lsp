use magit_lsp::quirks::{self, Capability, CapabilityTest, Client, Match, REGISTRY};
use serde_json::{json, Value};

mod common;
use common::stdio::TestServer;

#[path = "../scripts/generate_client_behaviors.rs"]
mod client_behaviors;

#[test]
fn known_client_appendix_matches_registry() {
    let path = "docs/CLIENT_SPEC.md";
    let text = std::fs::read_to_string(path).unwrap();
    let start = "<!-- BEGIN GENERATED KNOWN CLIENT BEHAVIORS -->";
    let end = "<!-- END GENERATED KNOWN CLIENT BEHAVIORS -->";
    let generated = client_behaviors::render();
    if std::env::var_os("UPDATE_CLIENT_BEHAVIORS").is_some() {
        let before = text.split_once(start).unwrap().0;
        let after = text.split_once(end).unwrap().1;
        std::fs::write(path, format!("{before}{generated}{after}")).unwrap();
    } else {
        let (before, rest) = text.split_once(start).unwrap();
        let (_, after) = rest.split_once(end).unwrap();
        assert_eq!(format!("{start}{}", rest.split_once(end).unwrap().0) + end, generated);
        assert!(!before.is_empty() && !after.is_empty());
    }
}

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

fn real_initialize(client: Value, capabilities: Value, config: Value) -> (TestServer, Value) {
    let mut server = TestServer::spawn(&[]);
    let mut request = common::stdio::initialize_message();
    request["params"]["clientInfo"] = client;
    request["params"]["capabilities"] = capabilities;
    request["params"]["initializationOptions"] = config;
    server.send(request);
    server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    let response = server.response_with_id(1);
    (server, response)
}

#[test]
fn real_server_reports_effective_plain_resolution_and_overrides() {
    let identity = json!({"name":"Neovim","version":"0.12.6"});
    let capabilities = json!({"workspace":{"applyEdit":true}});
    let (mut enabled, response) = real_initialize(
        identity.clone(),
        capabilities.clone(),
        json!({"quirks":{"assume-document-changes":true}}),
    );
    assert_eq!(
        response["result"]["capabilities"]["experimental"]["magit"]["effective"]["capabilities"]
            ["documentChanges"],
        true
    );
    assert!(
        response["result"]["capabilities"]["experimental"]["magit"]["effective"]["quirks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|q| q["id"] == "assume-document-changes")
    );
    enabled.shutdown_and_exit();
    assert!(enabled.finish(std::time::Duration::from_secs(10)).success());

    let (mut disabled, response) = real_initialize(
        identity,
        capabilities,
        json!({"quirks":{"assume-document-changes":false}}),
    );
    assert_eq!(
        response["result"]["capabilities"]["experimental"]["magit"]["effective"]["capabilities"]
            ["documentChanges"],
        false
    );
    assert!(
        !response["result"]["capabilities"]["experimental"]["magit"]["effective"]["quirks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|q| q["id"] == "assume-document-changes")
    );
    disabled.shutdown_and_exit();
    assert!(disabled
        .finish(std::time::Duration::from_secs(10))
        .success());
}

#[test]
fn real_server_accepts_valid_rich_and_recovers_invalid_config() {
    let mut rich = common::stdio::initialize_message();
    rich["params"]["capabilities"] = json!({"experimental":{"magit":{
        "v":1,"keys":"dynamic","modifiers":"full","layout":["main"],"effects":[],"prompts":[]
    }}});
    rich["params"]["clientInfo"] = json!({"name":"Helix","version":"25.07.1"});
    rich["params"]["initializationOptions"] = json!({"quirks":{"decomposed-changes":false}});
    let mut server = TestServer::spawn(&[]);
    server.send(rich);
    server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    let response = server.response_with_id(1);
    assert_eq!(
        response["result"]["capabilities"]["experimental"]["magit"]["effective"]["quirks"],
        json!([])
    );
    server.shutdown_and_exit();
    assert!(server.finish(std::time::Duration::from_secs(10)).success());

    let (mut invalid, response) = real_initialize(
        json!({"name":"Unknown","version":"1"}),
        json!({}),
        json!({"quirks":{"not-a-quirk":true}}),
    );
    assert!(response["result"]["capabilities"].is_object());
    assert!(invalid
        .drain_notifications()
        .iter()
        .any(|m| m["method"] == "window/showMessage"
            && m["params"]["message"]
                .as_str()
                .unwrap()
                .contains("not-a-quirk")));
    invalid.shutdown_and_exit();
    assert!(invalid.finish(std::time::Duration::from_secs(10)).success());
}

#[test]
fn real_server_rejects_invalid_rich_schema() {
    let mut request = common::stdio::initialize_message();
    request["params"]["capabilities"] = json!({"experimental":{"magit":{
        "v":1,"keys":"dynamic","modifiers":"full","layout":["main"],
        "effects":[],"prompts":[],"unexpected":true
    }}});
    let mut server = TestServer::spawn(&[]);
    server.send(request);
    let response = server.response_with_id(1);
    assert!(response.get("error").is_some(), "{response}");
    server.send(json!({"jsonrpc":"2.0","id":99,"method":"shutdown","params":null}));
    assert!(server.finish(std::time::Duration::from_secs(10)).success());
}
