//! Directional schema generation and committed-artifact drift checks.

#[path = "../scripts/generate_schema.rs"]
mod generate_schema;

use serde_json::Value;
use std::path::{Path, PathBuf};

use magit_lsp::menu::{Args, Group, Item, Menu, MenuStatus, Reader};
use magit_lsp::protocol::{EditDocumentMode, Effect, MessageLevel, ViewRender};
use magit_lsp::view::{Placement, Scope, ViewDescriptor, ViewKind, ViewMode, Viewport};

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

fn variant_for_type<'a>(schema: &'a Value, tag: &str) -> &'a Value {
    schema["oneOf"]
        .as_array()
        .expect("oneOf variants")
        .iter()
        .find(|variant| variant["properties"]["type"]["const"] == tag)
        .unwrap_or_else(|| panic!("no variant for type {tag}: {schema}"))
}

fn assert_variant_covers(variant: &Value, serialized: &serde_json::Map<String, Value>) {
    let properties = variant["properties"]
        .as_object()
        .expect("variant properties");
    for key in serialized.keys() {
        assert!(
            properties.contains_key(key),
            "schema lacks serialized key {key}: {variant}"
        );
    }
    for required in variant["required"].as_array().expect("variant required") {
        let name = required.as_str().expect("required name");
        assert!(
            serialized.contains_key(name),
            "serialized value lacks required key {name}"
        );
    }
}

fn sample_menu() -> Menu {
    Menu {
        id: "menu.commit".into(),
        title: "Commit".into(),
        groups: vec![Group {
            title: "Arguments".into(),
            items: vec![
                Item::Switch {
                    key: "-a".into(),
                    flag: "--all".into(),
                    label: "all".into(),
                    on: true,
                    status: MenuStatus::Implemented,
                },
                Item::Option {
                    key: "=A".into(),
                    flag: "--author".into(),
                    label: "author".into(),
                    value: Some("Alice".into()),
                    reader: Reader::Text,
                    status: MenuStatus::Implemented,
                },
                Item::Action {
                    key: "c".into(),
                    command: "magit.commit".into(),
                    label: "Commit".into(),
                    status: MenuStatus::Implemented,
                },
            ],
        }],
        saved_args: Args::default(),
    }
}

#[test]
fn menu_output_requires_activation_steps_and_input_does_not() {
    let output = read_schema("menu.json");
    assert_eq!(output["title"], "Menu");
    assert_eq!(
        output["$defs"]["Group"]["properties"]["items"]["items"]["$ref"],
        "#/$defs/SerializedItem"
    );
    let required = output["$defs"]["SerializedItem"]["required"]
        .as_array()
        .expect("serialized item required");
    assert!(
        required.iter().any(|field| field == "activationSteps"),
        "output items must require activationSteps: {output}"
    );

    let input = generate_schema::directional_schema::<Menu>(generate_schema::Direction::Input);
    assert_eq!(input["title"], "Menu");
    assert_eq!(
        input["$defs"]["Group"]["properties"]["items"]["items"]["$ref"],
        "#/$defs/Item"
    );
    assert!(
        !serde_json::to_string(&input)
            .expect("render input schema")
            .contains("activationSteps"),
        "input items must not mention activationSteps: {input}"
    );
}

#[test]
fn menu_schema_describes_serialized_switch_option_and_action() {
    let schema = read_schema("menu.json");
    let item_schema = &schema["$defs"]["SerializedItem"];
    let root_properties = item_schema["properties"]
        .as_object()
        .expect("serialized item properties");
    let root_required: Vec<&str> = item_schema["required"]
        .as_array()
        .expect("serialized item required")
        .iter()
        .map(|name| name.as_str().expect("required name"))
        .collect();

    let wire = serde_json::to_value(sample_menu()).expect("serialize menu");
    let items = wire["groups"][0]["items"]
        .as_array()
        .expect("serialized group items");
    assert_eq!(items.len(), 3);

    for item in items {
        let kind = item["kind"].as_str().expect("item kind");
        let variant = item_schema["oneOf"]
            .as_array()
            .expect("serialized item variants")
            .iter()
            .find(|variant| variant["properties"]["kind"]["const"] == kind)
            .unwrap_or_else(|| panic!("no schema variant for kind {kind}: {item_schema}"));
        let properties = variant["properties"].as_object().expect("variant properties");
        for key in item.as_object().expect("serialized item").keys() {
            assert!(
                properties.contains_key(key) || root_properties.contains_key(key),
                "schema lacks serialized menu key {key} for kind {kind}"
            );
        }
        for required in variant["required"]
            .as_array()
            .expect("variant required")
            .iter()
            .map(|name| name.as_str().expect("required name"))
            .chain(root_required.iter().copied())
        {
            assert!(
                item.get(required).is_some(),
                "serialized {kind} lacks required key {required}: {item}"
            );
        }
        assert!(
            item["activationSteps"].is_array(),
            "serialized items carry activationSteps: {item}"
        );
    }
}

fn sample_effects() -> Vec<Effect> {
    vec![
        Effect::Message {
            level: MessageLevel::Info,
            text: "ready".into(),
            hint: None,
            detail: None,
        },
        Effect::PassThrough { key: "x".into() },
        Effect::Clipboard {
            text: "copied".into(),
        },
        Effect::PushView {
            descriptor: ViewDescriptor {
                id: "view-1".into(),
                kind: ViewKind::Status,
                uri: "magit://status".into(),
                title: "Status".into(),
                placement: Placement::Main,
                mode: ViewMode::Interactive,
                scope: Scope::new("status"),
                capture: false,
                viewport: Viewport::default(),
            },
        },
        Effect::PopView {
            view_id: "view-1".into(),
        },
        Effect::FocusView {
            view_id: "view-1".into(),
        },
        Effect::Quit,
        Effect::VisitFile {
            uri: "file:///tmp/f.txt".into(),
            line: 3,
            column: 0,
        },
        Effect::EditDocument {
            run_id: "run-1".into(),
            uri: "file:///tmp/COMMIT_EDITMSG".into(),
            text: "message".into(),
            version: 1,
            kind: "commit".into(),
            title: "Commit message".into(),
            mode: EditDocumentMode::RealFile,
            scope: "commit".into(),
        },
    ]
}

#[test]
fn effect_schema_describes_every_serialized_effect() {
    let schema = read_schema("effect.json");
    assert_eq!(schema["title"], "Effect");
    for effect in sample_effects() {
        let wire = serde_json::to_value(&effect).expect("serialize effect");
        let object = wire.as_object().expect("effect object");
        let tag = object["type"].as_str().expect("effect type tag");
        let variant = variant_for_type(&schema, tag);
        assert_variant_covers(variant, object);
    }

    let descriptor = &schema["$defs"]["ViewDescriptor"];
    assert_eq!(descriptor["properties"]["kind"]["type"], "string");
    assert!(
        descriptor["properties"]["kind"].get("enum").is_none(),
        "ViewKind must stay an arbitrary string: {descriptor}"
    );
}

#[test]
fn view_render_schema_describes_the_render_payload() {
    let schema = read_schema("view-render.json");
    assert_eq!(schema["title"], "ViewRender");
    assert_eq!(
        property_names(&schema),
        ["baseVersion", "cursor", "edits", "menu", "mode", "pending", "text", "version", "viewId"]
    );
    assert_eq!(
        schema["required"],
        serde_json::json!(["viewId", "version", "mode"])
    );
    assert_eq!(
        schema["properties"]["menu"]["anyOf"][0]["$ref"],
        "#/$defs/Menu"
    );
    assert_eq!(schema["properties"]["menu"]["anyOf"][1]["type"], "null");
    assert_eq!(
        schema["properties"]["cursor"]["anyOf"][0]["$ref"],
        "#/$defs/Cursor"
    );
    assert_eq!(
        schema["properties"]["edits"]["items"]["$ref"],
        "#/$defs/LineEdit"
    );
    assert_eq!(schema["properties"]["mode"]["$ref"], "#/$defs/ViewMode");

    let render = ViewRender {
        view_id: "view-1".into(),
        version: 4,
        base_version: Some(3),
        text: None,
        edits: None,
        cursor: None,
        mode: ViewMode::Interactive,
        pending: "-".into(),
        menu: Some(sample_menu()),
    };
    let wire = serde_json::to_value(&render).expect("serialize render");
    let object = wire.as_object().expect("render object");
    let properties = schema["properties"].as_object().expect("render properties");
    for key in object.keys() {
        assert!(properties.contains_key(key), "schema lacks render key {key}");
    }
    for required in schema["required"].as_array().expect("render required") {
        let name = required.as_str().expect("required name");
        assert!(
            object.contains_key(name),
            "serialized render lacks required key {name}"
        );
    }
}
