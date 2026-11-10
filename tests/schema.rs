//! Directional schema generation and committed-artifact drift checks.

#[path = "../scripts/generate_schema.rs"]
mod generate_schema;

use serde_json::Value;
use std::path::{Path, PathBuf};

fn read_schema(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("docs/schema")
        .join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

fn property_names(value: &Value) -> Vec<String> {
    value["properties"]
        .as_object()
        .expect("schema has properties")
        .keys()
        .cloned()
        .collect()
}

#[test]
fn committed_schemas_match_fresh_deterministic_generation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let stale = generate_schema::stale_files(root).expect("drift check runs");
    assert!(
        stale.is_empty(),
        "stale or missing schemas (run make update-schema): {stale:?}"
    );

    let first: Vec<String> = generate_schema::artifacts()
        .iter()
        .map(generate_schema::render)
        .collect();
    let second: Vec<String> = generate_schema::artifacts()
        .iter()
        .map(generate_schema::render)
        .collect();
    assert_eq!(first, second, "generation must be deterministic");
}

#[test]
fn drift_check_reports_every_changed_artifact() {
    let dir = tempfile::tempdir().expect("tempdir");
    generate_schema::write_all(dir.path()).expect("write schemas");
    assert!(generate_schema::stale_files(dir.path())
        .expect("clean check")
        .is_empty());

    let path = dir.path().join("docs/schema/edit-confirmation.json");
    let text = std::fs::read_to_string(&path).expect("read artifact");
    std::fs::write(&path, text.replace("\"confirmed\"", "\"legacy\""))
        .expect("tamper artifact");

    assert_eq!(
        generate_schema::stale_files(dir.path()).expect("drift check"),
        vec![PathBuf::from("docs/schema/edit-confirmation.json")]
    );
}

#[test]
fn plain_action_input_requires_exact_kind_tags() {
    let schema = read_schema("plain-action-params.json");
    assert_eq!(schema["title"], "PlainActionParams");
    assert_eq!(
        schema["required"],
        serde_json::json!(["command", "documentUri", "target", "viewGeneration"])
    );
    assert_eq!(
        schema["properties"]["target"]["$ref"],
        "#/$defs/PlainActionTarget"
    );

    let variants = schema["$defs"]["PlainActionTarget"]["oneOf"]
        .as_array()
        .expect("tagged union variants");
    let tags: Vec<&str> = variants
        .iter()
        .map(|variant| {
            variant["properties"]["kind"]["const"]
                .as_str()
                .expect("kind tag")
        })
        .collect();
    assert_eq!(tags, ["content", "view", "node", "menuItem"]);
    for variant in variants {
        let required = variant["required"].as_array().expect("variant required");
        assert!(
            required.iter().any(|field| field == "kind"),
            "every variant must require kind: {variant}"
        );
    }
}

#[test]
fn confirmation_output_uses_exact_camel_case_names() {
    let confirmation = read_schema("edit-confirmation.json");
    assert_eq!(confirmation["title"], "EditConfirmation");
    let variants = confirmation["oneOf"]
        .as_array()
        .expect("confirmation variants");
    assert_eq!(variants.len(), 1);
    let confirmed = &variants[0];
    assert_eq!(confirmed["properties"]["type"]["const"], "confirmed");
    assert_eq!(
        property_names(confirmed),
        ["runId", "type", "uri", "version"]
    );
    assert_eq!(
        confirmed["required"],
        serde_json::json!(["type", "runId", "uri", "version"])
    );

    let abort = read_schema("edit-abort-result.json");
    assert_eq!(abort["title"], "EditAbortResult");
    let aborted = &abort["oneOf"][0];
    assert_eq!(aborted["properties"]["type"]["const"], "aborted");
    assert_eq!(property_names(aborted), ["runId", "type", "uri", "version"]);
}

#[test]
fn directional_contracts_keep_their_own_shapes() {
    let confirm_input = read_schema("edit-confirm-params.json");
    assert_eq!(confirm_input["title"], "EditConfirmParams");
    assert_eq!(
        property_names(&confirm_input),
        ["command", "runId", "source", "uri", "version"]
    );
    assert_eq!(confirm_input["additionalProperties"], false);
    assert!(confirm_input["properties"].get("type").is_none());

    let confirmation = read_schema("edit-confirmation.json");
    let confirmed = &confirmation["oneOf"][0];
    assert!(confirmed["properties"].get("source").is_none());
    assert!(confirmed["properties"].get("command").is_none());

    let abort_input = read_schema("edit-abort-params.json");
    assert_eq!(abort_input["title"], "EditAbortParams");
    assert_eq!(
        abort_input["required"],
        serde_json::json!(["command", "runId", "uri", "version"])
    );
}
