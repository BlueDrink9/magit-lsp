//! Directional schema generation and committed-artifact drift checks.

#[path = "../scripts/generate_schema.rs"]
mod generate_schema;
#[path = "common/mod.rs"]
mod fixtures;

use fixtures::schema::{read_schema, schema_errors};
use serde_json::Value;
use std::path::{Path, PathBuf};

use magit_lsp::keymap::{Binding, EditorMode};
use magit_lsp::menu::{Args, Group, Item, Menu, MenuStatus, Reader};
use magit_lsp::presentation::{negotiate, PlainCapabilities, Presentation};
use magit_lsp::quirks;
use generate_schema::schema_luals;
use generate_schema::schema_typescript;
use generate_schema::{Direction, MethodKind, Payload};
use magit_lsp::presentation::{OPTIONAL_EFFECTS, OPTIONAL_PROMPT_KINDS};
use magit_lsp::protocol::{
    EditDocumentMode, Effect, MessageLevel, ViewCloseParams, ViewCloseRequest, ViewEffectsParams,
    ViewInvokeParams, ViewInvokeRequest, ViewInvokeResponse, ViewKeyParams, ViewKeyRequest,
    ViewKeymapParams, ViewOpenParams, ViewOpenRequest, ViewPickParams, ViewPickRespondParams,
    ViewPickRespondRequest, ViewPromptParams, ViewPromptRespondParams, ViewPromptRespondRequest,
    ViewRenderedParams, ViewRender, ViewResizeParams, ViewResyncParams, ViewResyncRequest,
    ViewSetModeParams, ViewSetModeRequest, VIEW_EFFECTS, VIEW_KEYMAP, VIEW_PICK, VIEW_PROMPT,
    VIEW_RENDER, VIEW_RENDERED, VIEW_RESIZE,
};
use magit_lsp::view::{Placement, Scope, ViewDescriptor, ViewKind, ViewMode, Viewport};

fn property_names(value: &Value) -> Vec<String> {
    value["properties"]
        .as_object()
        .expect("schema has properties")
        .keys()
        .cloned()
        .collect()
}

#[test]
fn schema_generation_is_deterministic() {
    let first: Vec<String> = generate_schema::committed_artifacts()
        .iter()
        .map(generate_schema::render)
        .collect();
    let second: Vec<String> = generate_schema::committed_artifacts()
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

fn assert_unconstrained_json(schema: &Value) {
    if let Some(object) = schema.as_object() {
        for restrictive in ["type", "anyOf", "oneOf", "$ref", "properties", "required"] {
            assert!(
                object.get(restrictive).is_none(),
                "expected unconstrained JSON, found {restrictive}: {schema}"
            );
        }
    }
}

#[test]
fn prompt_unions_stay_any_of() {
    let respond = read_schema("view-prompt-respond-params.json");
    let prompt_value = &respond["$defs"]["PromptValue"];
    assert!(
        prompt_value.get("oneOf").is_none(),
        "untagged PromptValue must not become oneOf: {prompt_value}"
    );
    let variants = prompt_value["anyOf"]
        .as_array()
        .expect("untagged PromptValue anyOf variants");
    assert_eq!(variants.len(), 4, "PromptValue keeps every variant: {variants:?}");
    assert_eq!(variants[0]["type"], "boolean");
    assert_eq!(variants[1]["type"], "string");

    let choice = &respond["$defs"]["PromptChoiceAnswer"];
    assert!(choice.get("oneOf").is_none());
    let choice_variants = choice["anyOf"]
        .as_array()
        .expect("untagged PromptChoiceAnswer anyOf variants");
    assert_eq!(choice_variants.len(), 2);
    assert_eq!(choice_variants[0]["properties"]["id"]["type"], "string");
    assert_eq!(choice_variants[1]["properties"]["custom"]["type"], "string");
}

#[test]
fn option_value_fields_stay_unconstrained_json() {
    let open = read_schema("view-open-params.json");
    assert_unconstrained_json(&open["properties"]["args"]);

    let invoke = read_schema("view-invoke-params.json");
    assert_eq!(
        invoke["properties"]["args"],
        Value::Bool(true),
        "Option<Value> args must stay any JSON: {}",
        invoke["properties"]["args"]
    );

    let prompt = read_schema("view-prompt-params.json");
    assert_unconstrained_json(&prompt["properties"]["default"]);
}

#[test]
fn custom_view_kinds_stay_strings() {
    let open = read_schema("view-open-params.json");
    assert_eq!(open["properties"]["kind"]["type"], "string");
    assert!(
        open["properties"]["kind"].get("enum").is_none(),
        "view/open kind must stay an arbitrary string: {}",
        open["properties"]["kind"]
    );

    let descriptor = read_schema("view-descriptor.json");
    assert_eq!(descriptor["properties"]["kind"]["type"], "string");
    assert!(
        descriptor["properties"]["kind"].get("enum").is_none(),
        "descriptor kind must stay an arbitrary string: {}",
        descriptor["properties"]["kind"]
    );
}

#[test]
fn every_custom_view_payload_has_a_directional_artifact() {
    let committed: Vec<String> = generate_schema::artifacts()
        .iter()
        .map(|artifact| artifact.file.to_string())
        .collect();

    let expected: Vec<(&str, Value)> = vec![
        (
            "view-open-params.json",
            generate_schema::directional_schema::<ViewOpenParams>(generate_schema::Direction::Input),
        ),
        (
            "view-descriptor.json",
            generate_schema::directional_schema::<ViewDescriptor>(
                generate_schema::Direction::Output,
            ),
        ),
        (
            "view-close-params.json",
            generate_schema::directional_schema::<ViewCloseParams>(
                generate_schema::Direction::Input,
            ),
        ),
        (
            "view-invoke-params.json",
            generate_schema::directional_schema::<ViewInvokeParams>(
                generate_schema::Direction::Input,
            ),
        ),
        (
            "view-invoke-response.json",
            generate_schema::directional_schema::<ViewInvokeResponse>(
                generate_schema::Direction::Output,
            ),
        ),
        (
            "view-key-params.json",
            generate_schema::directional_schema::<ViewKeyParams>(generate_schema::Direction::Input),
        ),
        (
            "view-set-mode-params.json",
            generate_schema::directional_schema::<ViewSetModeParams>(
                generate_schema::Direction::Input,
            ),
        ),
        (
            "view-resync-params.json",
            generate_schema::directional_schema::<ViewResyncParams>(
                generate_schema::Direction::Input,
            ),
        ),
        (
            "view-rendered-params.json",
            generate_schema::directional_schema::<ViewRenderedParams>(
                generate_schema::Direction::Input,
            ),
        ),
        (
            "view-resize-params.json",
            generate_schema::directional_schema::<ViewResizeParams>(
                generate_schema::Direction::Input,
            ),
        ),
        (
            "view-effects-params.json",
            generate_schema::directional_schema::<ViewEffectsParams>(
                generate_schema::Direction::Output,
            ),
        ),
        (
            "view-keymap-params.json",
            generate_schema::directional_schema::<ViewKeymapParams>(
                generate_schema::Direction::Output,
            ),
        ),
        (
            "view-prompt-params.json",
            generate_schema::directional_schema::<ViewPromptParams>(
                generate_schema::Direction::Output,
            ),
        ),
        (
            "view-prompt-respond-params.json",
            generate_schema::directional_schema::<ViewPromptRespondParams>(
                generate_schema::Direction::Input,
            ),
        ),
        (
            "view-pick-params.json",
            generate_schema::directional_schema::<ViewPickParams>(generate_schema::Direction::Output),
        ),
        (
            "view-pick-respond-params.json",
            generate_schema::directional_schema::<ViewPickRespondParams>(
                generate_schema::Direction::Input,
            ),
        ),
    ];

    for (file, fresh) in &expected {
        assert!(
            committed.iter().any(|name| name == file),
            "generator has no artifact for {file}"
        );
        assert_eq!(
            &read_schema(file),
            fresh,
            "{file} was not generated with its declared wire direction"
        );
    }

    let keymap = read_schema("view-keymap-params.json");
    assert_eq!(
        keymap["properties"]["bindings"]["items"]["$ref"],
        "#/$defs/Binding"
    );
    assert_eq!(
        keymap["$defs"]["Binding"]["properties"]["modes"]["items"]["$ref"],
        "#/$defs/EditorMode"
    );
    let binding = Binding {
        command: "common.quit".into(),
        keys: vec!["q".into()],
        modes: vec![EditorMode::Normal],
        label: "quit".into(),
        status: "implemented".into(),
    };
    let wire = serde_json::to_value(&binding).expect("serialize binding");
    for key in wire.as_object().expect("binding object").keys() {
        assert!(
            keymap["$defs"]["Binding"]["properties"].get(key).is_some(),
            "keymap schema lacks binding key {key}"
        );
    }
}

fn method_entry<'a>(
    methods: &'a [generate_schema::MethodEntry],
    name: &str,
) -> &'a generate_schema::MethodEntry {
    methods
        .iter()
        .find(|entry| entry.method == name)
        .unwrap_or_else(|| panic!("catalog lacks method {name}"))
}

fn artifact_names() -> Vec<String> {
    generate_schema::artifacts()
        .iter()
        .map(|artifact| artifact.file.to_string())
        .collect()
}

#[test]
fn catalog_covers_every_custom_request_with_directional_payloads() {
    use lsp_types::request::Request;

    let methods = generate_schema::methods();
    let artifacts = artifact_names();
    let requests: [(&str, &str, &str, Option<(&str, &str)>); 8] = [
        (
            <ViewOpenRequest as Request>::METHOD,
            "ViewOpenParams",
            "view-open-params.json",
            Some(("ViewDescriptor", "view-descriptor.json")),
        ),
        (
            <ViewCloseRequest as Request>::METHOD,
            "ViewCloseParams",
            "view-close-params.json",
            None,
        ),
        (
            <ViewInvokeRequest as Request>::METHOD,
            "ViewInvokeParams",
            "view-invoke-params.json",
            Some(("ViewInvokeResponse", "view-invoke-response.json")),
        ),
        (
            <ViewKeyRequest as Request>::METHOD,
            "ViewKeyParams",
            "view-key-params.json",
            Some(("ViewInvokeResponse", "view-invoke-response.json")),
        ),
        (
            <ViewSetModeRequest as Request>::METHOD,
            "ViewSetModeParams",
            "view-set-mode-params.json",
            Some(("ViewInvokeResponse", "view-invoke-response.json")),
        ),
        (
            <ViewResyncRequest as Request>::METHOD,
            "ViewResyncParams",
            "view-resync-params.json",
            Some(("ViewRender", "view-render.json")),
        ),
        (
            <ViewPromptRespondRequest as Request>::METHOD,
            "ViewPromptRespondParams",
            "view-prompt-respond-params.json",
            None,
        ),
        (
            <ViewPickRespondRequest as Request>::METHOD,
            "ViewPickRespondParams",
            "view-pick-respond-params.json",
            None,
        ),
    ];

    assert_eq!(
        methods
            .iter()
            .filter(|entry| entry.kind == MethodKind::Request)
            .count(),
        requests.len(),
        "catalog must cover every custom request"
    );

    for (name, params_type, params_file, result) in requests {
        let entry = method_entry(&methods, name);
        assert_eq!(entry.kind, MethodKind::Request, "{name} kind");
        assert_eq!(
            entry.direction,
            Direction::Input,
            "{name} is client-initiated"
        );
        assert_eq!(entry.params.rust_type(), params_type, "{name} params type");
        assert_eq!(entry.params.file(), Some(params_file), "{name} params");
        assert!(
            artifacts.iter().any(|file| file == params_file),
            "{name} params must reference an artifact"
        );

        match result {
            Some((result_type, result_file)) => {
                let payload = entry
                    .result
                    .as_ref()
                    .unwrap_or_else(|| panic!("{name} needs an explicit result"));
                assert_eq!(payload.rust_type(), result_type, "{name} result type");
                assert_eq!(payload.file(), Some(result_file), "{name} result");
                assert!(
                    artifacts.iter().any(|file| file == result_file),
                    "{name} result must reference an artifact"
                );
            }
            None => assert_eq!(
                entry.result,
                Some(Payload::UnconstrainedJson),
                "{name} Value result must stay unconstrained"
            ),
        }
    }
}

#[test]
fn catalog_covers_every_custom_notification_with_its_direction() {
    let methods = generate_schema::methods();
    let artifacts = artifact_names();
    let notifications: [(&str, Direction, &str, &str); 7] = [
        (VIEW_RENDER, Direction::Output, "ViewRender", "view-render.json"),
        (
            VIEW_EFFECTS,
            Direction::Output,
            "ViewEffectsParams",
            "view-effects-params.json",
        ),
        (
            VIEW_KEYMAP,
            Direction::Output,
            "ViewKeymapParams",
            "view-keymap-params.json",
        ),
        (
            VIEW_PROMPT,
            Direction::Output,
            "ViewPromptParams",
            "view-prompt-params.json",
        ),
        (
            VIEW_PICK,
            Direction::Output,
            "ViewPickParams",
            "view-pick-params.json",
        ),
        (
            VIEW_RENDERED,
            Direction::Input,
            "ViewRenderedParams",
            "view-rendered-params.json",
        ),
        (
            VIEW_RESIZE,
            Direction::Input,
            "ViewResizeParams",
            "view-resize-params.json",
        ),
    ];

    assert_eq!(
        methods
            .iter()
            .filter(|entry| entry.kind == MethodKind::Notification)
            .count(),
        notifications.len(),
        "catalog must cover every custom notification"
    );

    for (name, direction, params_type, params_file) in notifications {
        let entry = method_entry(&methods, name);
        assert_eq!(entry.kind, MethodKind::Notification, "{name} kind");
        assert_eq!(entry.direction, direction, "{name} initiating direction");
        assert_eq!(entry.params.rust_type(), params_type, "{name} params type");
        assert_eq!(entry.params.file(), Some(params_file), "{name} params");
        assert!(entry.result.is_none(), "{name} notifications have no result");
        assert!(
            artifacts.iter().any(|file| file == params_file),
            "{name} params must reference an artifact"
        );
    }

    assert_eq!(methods.len(), 15, "eight requests and seven notifications");
    let mut names: Vec<&str> = methods.iter().map(|entry| entry.method).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), methods.len(), "method names must be unique");
}

#[test]
fn catalog_is_committed_and_participates_in_drift_detection() {
    assert_eq!(
        read_schema("methods.json"),
        generate_schema::methods_catalog(),
        "committed catalog must match fresh generation"
    );

    let dir = tempfile::tempdir().expect("tempdir");
    generate_schema::write_all(dir.path()).expect("write schemas");
    let path = dir.path().join("docs/schema/methods.json");
    std::fs::remove_file(&path).expect("remove catalog");
    assert_eq!(
        generate_schema::stale_files(dir.path()).expect("missing check"),
        vec![PathBuf::from("docs/schema/methods.json")],
        "a missing catalog is stale"
    );

    generate_schema::write_all(dir.path()).expect("rewrite schemas");
    let text = std::fs::read_to_string(&path).expect("read catalog");
    std::fs::write(&path, text.replace("\"view/open\"", "\"view/legacyOpen\""))
        .expect("tamper catalog");
    assert_eq!(
        generate_schema::stale_files(dir.path()).expect("tampered check"),
        vec![PathBuf::from("docs/schema/methods.json")],
        "a tampered catalog is stale"
    );
}

fn names(list: &[&str]) -> Value {
    Value::Array(list.iter().map(|name| Value::String((*name).into())).collect())
}

#[test]
fn shared_capability_lists_match_the_typed_constants() {
    assert_eq!(
        names(OPTIONAL_EFFECTS),
        serde_json::json!(["visitFile", "clipboard", "editDocument"]),
        "the typed optional effects are the wire contract"
    );
    assert_eq!(
        names(OPTIONAL_PROMPT_KINDS),
        serde_json::json!(["confirm", "text", "choice", "multiChoice", "secret"]),
        "the typed optional prompt kinds are the wire contract"
    );

    let committed = read_schema("shared-lists.json");
    assert_eq!(committed["optionalEffects"], names(OPTIONAL_EFFECTS));
    assert_eq!(
        committed["optionalPromptKinds"],
        names(OPTIONAL_PROMPT_KINDS)
    );
    assert_eq!(committed, generate_schema::shared_lists());

    assert!(
        artifact_names().iter().any(|file| file == "shared-lists.json"),
        "the shared lists participate in the generated artifact set"
    );
}

#[test]
fn shared_lists_participate_in_drift_detection() {
    let dir = tempfile::tempdir().expect("tempdir");
    generate_schema::write_all(dir.path()).expect("write schemas");
    assert!(generate_schema::stale_files(dir.path())
        .expect("clean check")
        .is_empty());

    let path = dir.path().join("docs/schema/shared-lists.json");
    std::fs::remove_file(&path).expect("remove shared lists");
    assert_eq!(
        generate_schema::stale_files(dir.path()).expect("missing check"),
        vec![PathBuf::from("docs/schema/shared-lists.json")],
        "a missing shared-lists artifact is stale"
    );

    generate_schema::write_all(dir.path()).expect("rewrite schemas");
    let text = std::fs::read_to_string(&path).expect("read shared lists");
    std::fs::write(&path, text.replace("\"visitFile\"", "\"openFile\""))
        .expect("tamper shared lists");
    assert_eq!(
        generate_schema::stale_files(dir.path()).expect("tampered check"),
        vec![PathBuf::from("docs/schema/shared-lists.json")],
        "a modified shared-lists artifact is stale"
    );
}

#[test]
fn shared_client_lists_are_generated_from_the_typed_constants() {
    let files = generate_schema::client_list_files();
    let paths: Vec<&str> = files.iter().map(|file| file.path).collect();
    assert_eq!(
        paths,
        vec![
            generate_schema::LUA_SHARED_PATH,
            generate_schema::VSCODE_SHARED_PATH
        ],
        "both clients consume a generated shared-list module"
    );
    let lua = &files[0].text;
    assert!(
        lua.contains("local optional_effects = { 'visitFile', 'clipboard', 'editDocument' }"),
        "the Lua module carries the typed optional effects"
    );
    assert!(
        lua.contains(
            "local optional_prompt_kinds = { 'confirm', 'text', 'choice', 'multiChoice', 'secret' }"
        ),
        "the Lua module carries the typed optional prompt kinds"
    );
    let javascript = &files[1].text;
    assert!(
        javascript.contains("\"visitFile\",\n  \"clipboard\",\n  \"editDocument\""),
        "the JavaScript module carries the typed optional effects"
    );
    assert!(
        javascript.contains(
            "\"confirm\",\n  \"text\",\n  \"choice\",\n  \"multiChoice\",\n  \"secret\""
        ),
        "the JavaScript module carries the typed optional prompt kinds"
    );
}

#[test]
fn shared_client_lists_participate_in_drift_detection() {
    let dir = tempfile::tempdir().expect("tempdir");
    generate_schema::write_all(dir.path()).expect("write schemas");
    assert!(
        generate_schema::stale_files(dir.path())
            .expect("clean check")
            .is_empty(),
        "freshly generated client modules are current"
    );

    for path in [
        generate_schema::LUA_SHARED_PATH,
        generate_schema::VSCODE_SHARED_PATH,
    ] {
        let file = dir.path().join(path);
        std::fs::remove_file(&file).expect("remove client module");
        assert_eq!(
            generate_schema::stale_files(dir.path()).expect("missing check"),
            vec![PathBuf::from(path)],
            "a missing {path} is stale"
        );

        generate_schema::write_all(dir.path()).expect("rewrite schemas");
        let text = std::fs::read_to_string(&file).expect("read client module");
        std::fs::write(&file, text.replace("clipboard", "clipboardLegacy"))
            .expect("tamper client module");
        assert_eq!(
            generate_schema::stale_files(dir.path()).expect("tampered check"),
            vec![PathBuf::from(path)],
            "a modified {path} is stale"
        );
        generate_schema::write_all(dir.path()).expect("restore schemas");
    }
}

fn read_declarations() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(schema_typescript::DECLARATION_PATH);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn namespace_block<'a>(text: &'a str, name: &str) -> &'a str {
    let header = format!("export namespace {name} {{");
    let start = text
        .find(&header)
        .unwrap_or_else(|| panic!("declarations lack namespace {name}"));
    let rest = &text[start..];
    let end = rest
        .find("\n}\n")
        .unwrap_or_else(|| panic!("namespace {name} is unterminated"));
    &rest[..end]
}

#[test]
fn typescript_declaration_generation_is_deterministic() {
    let first = schema_typescript::protocol_declarations().expect("generate declarations");
    let second = schema_typescript::protocol_declarations().expect("regenerate declarations");
    assert_eq!(first.path, "clients/vscode/types/protocol.d.ts");
    assert_eq!(
        first.text, second.text,
        "declaration generation must be deterministic"
    );
}

#[test]
fn typescript_declarations_cover_every_payload_root() {
    let text = read_declarations();
    for artifact in generate_schema::artifacts() {
        if artifact.file == "methods.json" || artifact.file == "shared-lists.json" {
            continue;
        }
        let title = artifact.schema["title"]
            .as_str()
            .expect("artifact has a title");
        assert!(
            text.contains(&format!("export namespace {title} {{")),
            "declarations lack namespace {title} for {}",
            artifact.file
        );
    }
}

#[test]
fn typescript_declarations_preserve_directional_contracts() {
    let text = read_declarations();

    let plain_action = namespace_block(&text, "PlainActionParams");
    assert!(plain_action.contains("    command: string;\n"));
    assert!(plain_action.contains("    viewGeneration: number;\n"));
    assert!(plain_action.contains("    selection?: SelectionIntent | null;\n"));
    assert!(plain_action.contains("    target: PlainActionTarget;\n"));
    for tag in ["content", "view", "node", "menuItem"] {
        assert!(
            plain_action.contains(&format!("kind: \"{tag}\";")),
            "PlainActionTarget loses literal tag {tag}: {plain_action}"
        );
    }

    let render = namespace_block(&text, "ViewRender");
    assert!(render.contains("    viewId: string;\n"), "required field");
    assert!(
        render.contains("    baseVersion?: number | null;\n"),
        "optional nullable field"
    );
    assert!(render.contains("    edits?: LineEdit[] | null;\n"), "array");
    assert!(render.contains("    menu?: Menu | null;\n"), "local ref");
    assert_eq!(
        render.matches("export type Args = {").count(),
        1,
        "same-named local definitions stay inside their namespace"
    );

    let prompt = namespace_block(&text, "ViewPromptRespondParams");
    assert!(prompt.contains("    value?: PromptValue | null;\n"));
    assert!(
        prompt.contains("  export type PromptValue = boolean\n  | string\n  | PromptChoiceAnswer\n"),
        "untagged unions stay unions: {prompt}"
    );

    let open = namespace_block(&text, "ViewOpenParams");
    assert!(
        open.contains("    kind: string;\n"),
        "custom view kinds stay arbitrary strings: {open}"
    );
    assert!(open.contains("    args?: unknown;\n"), "unconstrained JSON");
    assert!(!open.contains("    args?: any;"), "unconstrained JSON is not any");

    let invoke = namespace_block(&text, "ViewInvokeParams");
    assert!(invoke.contains("    args?: unknown;\n"));
}

#[test]
fn typescript_declarations_preserve_all_of_intersections() {
    let schema = serde_json::json!({
        "title": "Synthetic",
        "allOf": [
            {
                "type": "object",
                "properties": { "a": { "type": "string" } },
                "required": ["a"]
            },
            {
                "type": "object",
                "properties": { "b": { "type": "number" } },
                "required": ["b"]
            }
        ]
    });
    let rendered =
        schema_typescript::render_root(&schema, "Synthetic").expect("render intersection");
    assert!(
        rendered.contains("}\n  & {"),
        "allOf must become an intersection: {rendered}"
    );
    assert!(rendered.contains("a: string;"));
    assert!(rendered.contains("b: number;"));
}

#[test]
fn typescript_declarations_fail_on_unsupported_schema_constructs() {
    let schemas = [
        (
            serde_json::json!({ "title": "Synthetic", "not": { "type": "string" } }),
            "not",
        ),
        (
            serde_json::json!({
                "title": "Synthetic",
                "type": "object",
                "patternProperties": { "^a": { "type": "string" } }
            }),
            "patternProperties",
        ),
        (
            serde_json::json!({
                "title": "Synthetic",
                "type": "array",
                "prefixItems": [{ "type": "string" }]
            }),
            "prefixItems",
        ),
    ];
    for (schema, keyword) in schemas {
        let error = schema_typescript::render_root(&schema, "Synthetic")
            .expect_err("unsupported construct must fail");
        assert!(
            error.contains(keyword),
            "error must name {keyword}: {error}"
        );
    }
}

#[test]
fn typescript_declarations_preserve_combinator_siblings() {
    let schema = serde_json::json!({
        "title": "Synthetic",
        "description": "Item with activation steps.",
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "kind": { "const": "switch", "type": "string" }
                },
                "required": ["kind"]
            },
            {
                "type": "object",
                "properties": {
                    "kind": { "const": "option", "type": "string" }
                },
                "required": ["kind"]
            }
        ],
        "type": "object",
        "properties": {
            "activationSteps": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["activationSteps"]
    });
    let rendered = schema_typescript::render_root(&schema, "Synthetic")
        .expect("render combinator with siblings");
    assert!(
        rendered.contains(") & {\n    activationSteps: string[];\n  };"),
        "oneOf sibling constraints must survive: {rendered}"
    );
    assert!(
        rendered.contains("kind: \"switch\";") && rendered.contains("kind: \"option\";"),
        "combinator variants must survive: {rendered}"
    );

    let any_of = serde_json::json!({
        "title": "Synthetic",
        "anyOf": [{ "type": "string" }, { "type": "number" }],
        "properties": { "note": { "type": "string" } },
        "required": ["note"]
    });
    let rendered = schema_typescript::render_root(&any_of, "Synthetic")
        .expect("anyOf sibling constraints are representable");
    assert!(
        rendered.contains("(string | number) & {"),
        "anyOf stays anyOf and keeps siblings: {rendered}"
    );
}

#[test]
fn menu_item_declarations_require_activation_steps() {
    let text = read_declarations();
    let menu = namespace_block(&text, "Menu");
    let start = menu
        .find("export type SerializedItem =")
        .expect("menu declarations define SerializedItem");
    let declaration = &menu[start..];
    let declaration = &declaration[..declaration.find("  };").expect("SerializedItem ends")];
    assert!(
        declaration.contains("activationSteps: string[];"),
        "serialized items require activationSteps: {declaration}"
    );
    assert!(
        !declaration.contains("activationSteps?:"),
        "activationSteps must stay required: {declaration}"
    );
    assert!(
        declaration.contains(") & {"),
        "menu item declarations keep combinator sibling constraints: {declaration}"
    );
}

#[test]
fn typescript_declarations_fail_on_unsupported_combinator_siblings() {
    let schemas = [
        (
            serde_json::json!({
                "title": "Synthetic",
                "oneOf": [{ "type": "string" }],
                "type": "array",
                "items": { "type": "string" }
            }),
            "beside `oneOf`",
        ),
        (
            serde_json::json!({
                "title": "Synthetic",
                "oneOf": [{ "type": "string" }],
                "$ref": "#/$defs/Args"
            }),
            "oneOf",
        ),
        (
            serde_json::json!({
                "title": "Synthetic",
                "allOf": [{ "type": "string" }],
                "const": "fixed"
            }),
            "const",
        ),
    ];
    for (schema, reason) in schemas {
        let error = schema_typescript::render_root(&schema, "Synthetic")
            .expect_err("unsupported sibling must fail loudly");
        assert!(
            error.contains(reason),
            "error must name {reason}: {error}"
        );
    }
}

#[test]
fn typescript_method_map_mirrors_the_catalog() {
    let text = read_declarations();
    assert!(text.contains("export interface MethodMap {\n"));
    assert!(text.contains("export type MethodName = keyof MethodMap;\n"));

    assert!(text.contains(
        "  \"view/open\": {\n    kind: \"request\";\n    direction: \"clientToServer\";\n\
         \x20   params: ViewOpenParams.Root;\n    result: ViewDescriptor.Root;\n  };\n"
    ));
    assert!(text.contains(
        "  \"view/close\": {\n    kind: \"request\";\n    direction: \"clientToServer\";\n\
         \x20   params: ViewCloseParams.Root;\n    result: unknown;\n  };\n"
    ));
    assert!(text.contains(
        "  \"view/render\": {\n    kind: \"notification\";\n    direction: \"serverToClient\";\n\
         \x20   params: ViewRender.Root;\n  };\n"
    ));

    for entry in generate_schema::methods() {
        assert!(
            text.contains(&format!("\"{}\": {{", entry.method)),
            "method map lacks {}",
            entry.method
        );
    }
}

#[test]
fn typescript_declaration_drift_is_reported() {
    let dir = tempfile::tempdir().expect("tempdir");
    generate_schema::write_all(dir.path()).expect("write schemas");
    assert!(generate_schema::stale_files(dir.path())
        .expect("clean check")
        .is_empty());

    let path = dir.path().join(schema_typescript::DECLARATION_PATH);
    std::fs::remove_file(&path).expect("remove declarations");
    assert_eq!(
        generate_schema::stale_files(dir.path()).expect("missing check"),
        vec![PathBuf::from(schema_typescript::DECLARATION_PATH)],
        "a missing declaration file is stale"
    );

    generate_schema::write_all(dir.path()).expect("rewrite schemas");
    let text = std::fs::read_to_string(&path).expect("read declarations");
    std::fs::write(
        &path,
        text.replace("export namespace Menu {", "export namespace LegacyMenu {"),
    )
    .expect("tamper declarations");
    assert_eq!(
        generate_schema::stale_files(dir.path()).expect("tampered check"),
        vec![PathBuf::from(schema_typescript::DECLARATION_PATH)],
        "a modified declaration file is stale"
    );
}

fn read_annotations() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(schema_luals::ANNOTATION_PATH);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// Return the declaration block starting at `header` and ending at the next
/// blank line, which is how generated LuaLS declarations are separated.
fn lua_declaration<'a>(text: &'a str, header: &str) -> &'a str {
    let start = text
        .find(header)
        .unwrap_or_else(|| panic!("annotations lack declaration {header}"));
    let rest = &text[start..];
    let end = rest.find("\n\n").unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn luals_annotation_generation_is_deterministic() {
    let first = schema_luals::protocol_annotations().expect("generate annotations");
    let second = schema_luals::protocol_annotations().expect("regenerate annotations");
    assert_eq!(first.path, "lua/magit-lsp/types.lua");
    assert_eq!(
        first.text, second.text,
        "annotation generation must be deterministic"
    );
    assert!(
        first.text.starts_with("---@meta\n"),
        "annotations are a LuaLS meta file"
    );
}

#[test]
fn luals_annotations_cover_every_payload_root() {
    let text = read_annotations();
    for artifact in generate_schema::artifacts() {
        if artifact.file == "methods.json" || artifact.file == "shared-lists.json" {
            continue;
        }
        let title = artifact.schema["title"]
            .as_str()
            .expect("artifact has a title");
        assert!(
            text.contains(&format!("---@class {title}.Root\n"))
                || text.contains(&format!("---@alias {title}.Root ")),
            "annotations lack root {title} for {}",
            artifact.file
        );
    }
}

#[test]
fn luals_annotations_preserve_directional_contracts() {
    let text = read_annotations();

    let plain_action = lua_declaration(&text, "---@class PlainActionParams.Root");
    assert!(plain_action.contains("---@field command string"));
    assert!(plain_action.contains("---@field viewGeneration integer"));
    assert!(plain_action.contains(
        "---@field target PlainActionParams.PlainActionTarget"
    ));
    let selection_variant = lua_declaration(
        &text,
        "---@class PlainActionParams.PlainActionTarget.Variant1",
    );
    assert!(
        selection_variant.contains(
            "---@field selection? PlainActionParams.SelectionIntent|nil"
        ),
        "optional nullable local ref: {selection_variant}"
    );
    for (variant, tag) in [
        ("Variant1", "content"),
        ("Variant2", "view"),
        ("Variant3", "node"),
        ("Variant4", "menuItem"),
    ] {
        let block = lua_declaration(
            &text,
            &format!("---@class PlainActionParams.PlainActionTarget.{variant}"),
        );
        assert!(
            block.contains(&format!("---@field kind \"{tag}\"")),
            "target variant {variant} loses literal tag {tag}: {block}"
        );
    }

    let render = lua_declaration(&text, "---@class ViewRender.Root");
    assert!(render.contains("---@field viewId string"), "required field");
    assert!(
        render.contains("---@field baseVersion? integer|nil"),
        "optional nullable field: {render}"
    );
    assert!(
        render.contains("---@field edits? ViewRender.LineEdit[]|nil"),
        "array of local refs: {render}"
    );
    assert!(
        render.contains("---@field menu? ViewRender.Menu|nil"),
        "local ref: {render}"
    );
    assert_eq!(
        text.matches("---@class ViewRender.Menu\n").count(),
        1,
        "same-named local definitions stay inside their prefix"
    );

    let prompt = lua_declaration(&text, "---@class ViewPromptRespondParams.Root");
    assert!(prompt.contains("---@field value? ViewPromptRespondParams.PromptValue|nil"));
    let prompt_value = lua_declaration(
        &text,
        "---@alias ViewPromptRespondParams.PromptValue ",
    );
    assert!(
        prompt_value.contains("boolean|string|ViewPromptRespondParams.PromptChoiceAnswer|"),
        "untagged unions stay unions: {prompt_value}"
    );
    assert!(
        !prompt_value.contains("&"),
        "anyOf must not become an intersection: {prompt_value}"
    );

    let open = lua_declaration(&text, "---@class ViewOpenParams.Root");
    assert!(
        open.contains("---@field kind string"),
        "custom view kinds stay arbitrary strings: {open}"
    );
    assert!(
        open.contains("---@field args? any"),
        "unconstrained JSON is any: {open}"
    );
    assert!(
        open.contains("---@field placement? ViewOpenParams.Placement|nil"),
        "nullable ref union: {open}"
    );
    assert!(
        text.contains("---@field items ViewPickParams.PickItem[]"),
        "arrays of local refs"
    );
}

#[test]
fn luals_annotations_merge_all_of_intersections() {
    let schema = serde_json::json!({
        "title": "Synthetic",
        "allOf": [
            {
                "type": "object",
                "properties": { "a": { "type": "string" } },
                "required": ["a"]
            },
            {
                "type": "object",
                "properties": { "b": { "type": "number" } },
                "required": ["b"]
            }
        ]
    });
    let rendered = schema_luals::render_root(&schema, "Synthetic").expect("render intersection");
    assert!(
        rendered.contains("---@class Synthetic.Root\n"),
        "object allOf becomes one class: {rendered}"
    );
    assert!(rendered.contains("---@field a string"));
    assert!(rendered.contains("---@field b number"));
    assert!(
        !rendered.contains(" & "),
        "LuaLS has no intersection operator: {rendered}"
    );
}

#[test]
fn luals_annotations_preserve_combinator_siblings() {
    let schema = serde_json::json!({
        "title": "Synthetic",
        "description": "Item with activation steps.",
        "oneOf": [
            {
                "type": "object",
                "properties": { "kind": { "const": "switch", "type": "string" } },
                "required": ["kind"]
            },
            {
                "type": "object",
                "properties": { "kind": { "const": "option", "type": "string" } },
                "required": ["kind"]
            }
        ],
        "type": "object",
        "properties": {
            "activationSteps": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["activationSteps"]
    });
    let rendered =
        schema_luals::render_root(&schema, "Synthetic").expect("render combinator with siblings");
    assert!(
        rendered.contains("---@alias Synthetic.Root Synthetic.Root.Variant1|Synthetic.Root.Variant2"),
        "oneOf stays a union: {rendered}"
    );
    for (variant, tag) in [("Variant1", "switch"), ("Variant2", "option")] {
        let block = lua_declaration(
            &rendered,
            &format!("---@class Synthetic.Root.{variant}"),
        );
        assert!(
            block.contains(&format!("---@field kind \"{tag}\"")),
            "variant tag survives: {block}"
        );
        assert!(
            block.contains("---@field activationSteps string[]"),
            "sibling constraints are distributed into variants: {block}"
        );
        assert!(
            !block.contains("activationSteps?"),
            "activationSteps must stay required: {block}"
        );
    }

    let any_of = serde_json::json!({
        "title": "Synthetic",
        "anyOf": [{ "type": "string" }, { "type": "number" }],
    });
    let rendered = schema_luals::render_root(&any_of, "Synthetic").expect("render anyOf");
    assert!(
        rendered.contains("---@alias Synthetic.Root string|number"),
        "anyOf stays an honest union: {rendered}"
    );
}

#[test]
fn luals_annotations_fail_on_unsupported_constructs() {
    let schemas = [
        (
            serde_json::json!({ "title": "Synthetic", "not": { "type": "string" } }),
            "not",
        ),
        (
            serde_json::json!({
                "title": "Synthetic",
                "type": "object",
                "patternProperties": { "^a": { "type": "string" } }
            }),
            "patternProperties",
        ),
        (
            serde_json::json!({
                "title": "Synthetic",
                "type": "array",
                "prefixItems": [{ "type": "string" }]
            }),
            "prefixItems",
        ),
        (
            serde_json::json!({
                "title": "Synthetic",
                "allOf": [{ "type": "string" }]
            }),
            "intersection",
        ),
        (
            serde_json::json!({
                "title": "Synthetic",
                "oneOf": [{ "type": "string" }],
                "properties": { "note": { "type": "string" } }
            }),
            "sibling constraints",
        ),
        (
            serde_json::json!({
                "title": "Synthetic",
                "properties": { "value": {
                    "$ref": "#/$defs/Args",
                    "properties": { "note": { "type": "string" } }
                } }
            }),
            "$ref",
        ),
    ];
    for (schema, reason) in schemas {
        let error = schema_luals::render_root(&schema, "Synthetic")
            .expect_err("unsupported construct must fail loudly");
        assert!(
            error.contains(reason),
            "error must name {reason}: {error}"
        );
    }
}

#[test]
fn standalone_payload_tables_detect_tamper_and_missing_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    generate_schema::write_all(dir.path()).expect("write schemas");
    assert!(generate_schema::stale_files(dir.path())
        .expect("clean check")
        .is_empty());

    let doc = generate_schema::schema_markdown::DOC_PATH;
    let path = dir.path().join(doc);
    let text = std::fs::read_to_string(&path).expect("read generated payload tables");
    std::fs::write(&path, text.replace("Unconstrained JSON", "Anything goes"))
        .expect("tamper payload tables");
    assert_eq!(
        generate_schema::stale_files(dir.path()).expect("tampered check"),
        vec![PathBuf::from(doc)],
        "modified payload tables are stale"
    );

    generate_schema::write_all(dir.path()).expect("rewrite schemas");
    std::fs::remove_file(&path).expect("remove payload tables");
    assert_eq!(
        generate_schema::stale_files(dir.path()).expect("missing check"),
        vec![PathBuf::from(doc)],
        "missing payload tables are stale"
    );
}

#[test]
fn standalone_worked_exchanges_include_provenance_and_examples() {
    let block = generate_schema::schema_markdown::worked_exchanges_block()
        .expect("render worked exchanges");
    assert_eq!(
        block,
        generate_schema::schema_markdown::worked_exchanges_block()
            .expect("render again"),
        "worked exchanges must render deterministically"
    );
    for name in [
        "commit-edit-abort.jsonl",
        "commit-edit-confirm.jsonl",
        "plain-code-action.jsonl",
        "plain-document-prompt.jsonl",
        "plain-open.jsonl",
        "rich-lifecycle.jsonl",
    ] {
        assert!(
            block.contains(&format!("##### `{name}`")),
            "worked exchanges must include {name}"
        );
    }
    assert_eq!(
        block.matches("```jsonl").count(),
        6,
        "each example file gets one fenced jsonl block"
    );
}

#[test]
fn standalone_worked_exchanges_detect_tamper_and_missing_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    generate_schema::write_all(dir.path()).expect("write schemas");
    assert!(generate_schema::stale_files(dir.path())
        .expect("clean check")
        .is_empty());

    let doc = generate_schema::schema_markdown::EXCHANGES_DOC_PATH;
    let path = dir.path().join(doc);
    let text = std::fs::read_to_string(&path).expect("read client spec");
    let provenance = "temporary paths are normalized to `/repo`";
    assert!(text.contains(provenance), "worked exchanges carry provenance");
    std::fs::write(&path, text.replace(provenance, "temporary paths are not normalized"))
        .expect("tamper worked exchanges");
    assert_eq!(
        generate_schema::stale_files(dir.path()).expect("tampered check"),
        vec![PathBuf::from(doc)],
        "a modified worked-exchange block is stale"
    );

    generate_schema::write_all(dir.path()).expect("rewrite schemas");
    std::fs::remove_file(&path).expect("remove worked exchanges");
    assert_eq!(
        generate_schema::stale_files(dir.path()).expect("missing check"),
        vec![PathBuf::from(doc)],
        "a missing worked-exchange block is stale"
    );
}

#[test]
fn capability_levels_pin_the_plain_capability_fields() {
    let documented: Vec<&str> = generate_schema::schema_markdown::CAPABILITY_LEVELS
        .iter()
        .map(|row| row.field)
        .collect();
    let mut implemented = plain_capability_fields();
    // Runtime state, not a client declaration; the table documents declarations.
    implemented.retain(|field| field != "commit_draft_expired");
    assert_eq!(
        implemented, documented,
        "capability table must document every PlainCapabilities declaration field exactly"
    );
    for row in generate_schema::schema_markdown::CAPABILITY_LEVELS {
        let mut caps = Value::Object(Default::default());
        let probe = if row.field == "position_encodings" {
            serde_json::json!(["utf-8"])
        } else {
            Value::Bool(true)
        };
        set_json_pointer(&mut caps, row.pointer, probe);
        let Presentation::Plain(plain) = negotiate(&serde_json::json!({ "capabilities": caps }))
            .expect("absent selector must negotiate Plain")
        else {
            panic!("absent selector must not negotiate rich");
        };
        if row.field == "position_encodings" {
            assert_eq!(
                plain.position_encodings,
                vec!["utf-8".to_string()],
                "{} must populate position_encodings",
                row.pointer
            );
        } else {
            assert!(
                plain_flag(&plain, row.field),
                "{} must set PlainCapabilities::{}",
                row.pointer,
                row.field
            );
        }
    }
}

#[test]
fn plain_classification_follows_negotiate_and_recorded_traces() {
    assert_eq!(
        negotiate(&serde_json::json!({ "capabilities": {} })),
        Ok(Presentation::Plain(PlainCapabilities::default())),
        "an absent experimental.magit selects Plain with nothing declared"
    );

    let Presentation::Plain(neovim) = negotiate(&trace_params("initialize-plain-neovim-0.12.6.json"))
        .expect("recorded plain Neovim trace negotiates Plain")
    else {
        panic!("plain Neovim trace must not negotiate rich");
    };
    assert!(neovim.apply_edit);
    assert!(
        !neovim.document_changes,
        "the recorded plain Neovim trace omits documentChanges"
    );
    assert!(neovim.show_document);
    assert!(neovim.work_done_progress);
    assert!(neovim.folding_range);
    assert!(neovim.code_actions);

    let Presentation::Plain(helix) = negotiate(&trace_params("initialize-helix-25.07.1.json"))
        .expect("recorded Helix trace negotiates Plain")
    else {
        panic!("Helix trace must not negotiate rich");
    };
    assert!(helix.apply_edit);
    assert!(helix.document_changes);
    assert!(
        !helix.show_document,
        "the recorded Helix trace omits window.showDocument"
    );
    assert!(helix.work_done_progress);
    assert!(
        !helix.folding_range,
        "the recorded Helix trace omits textDocument.foldingRange"
    );

    assert!(
        matches!(
            negotiate(&serde_json::json!({
                "capabilities": { "experimental": { "magit": {} } }
            })),
            Err(_)
        ),
        "an invalid rich declaration is rejected, never downgraded to Plain"
    );
    let neovim_declaration = current_client_magit("Neovim");
    assert!(
        matches!(
            negotiate(&serde_json::json!({
                "clientInfo": neovim_declaration["client"],
                "capabilities": { "experimental": { "magit": neovim_declaration["magit"] } }
            })),
            Ok(Presentation::Rich(_))
        ),
        "the current bundled Neovim declaration selects the rich protocol"
    );

    let quirk = quirks::REGISTRY
        .iter()
        .find(|quirk| quirk.id == "assume-document-changes")
        .expect("neovim documentChanges quirk is registered");
    assert_eq!(
        quirk.model,
        Some("neovim-accepts-undeclared-documentChanges"),
        "the quirk names the recorded fake-editor behavior"
    );
    let override_ = quirk.capability.expect("quirk carries a capability override");
    assert_eq!(override_.capability, quirks::Capability::DocumentChanges);
    assert!(override_.enabled);
}

#[test]
fn standalone_capability_levels_detect_tamper_and_missing_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    generate_schema::write_all(dir.path()).expect("write schemas");
    assert!(generate_schema::stale_files(dir.path())
        .expect("clean check")
        .is_empty());

    let doc = generate_schema::schema_markdown::CAPABILITY_DOC_PATH;
    let path = dir.path().join(doc);
    let text = std::fs::read_to_string(&path).expect("read client spec");
    let expected = "| `apply_edit` | `workspace.applyEdit` | L1 |";
    assert!(
        text.contains(expected),
        "capability table must carry the generated applyEdit row"
    );
    std::fs::write(&path, text.replace(expected, "| `apply_edit` | edit | L9 |"))
        .expect("tamper capability levels");
    assert_eq!(
        generate_schema::stale_files(dir.path()).expect("tampered check"),
        vec![PathBuf::from(doc)],
        "a modified capability-level block is stale"
    );

    generate_schema::write_all(dir.path()).expect("rewrite schemas");
    std::fs::remove_file(&path).expect("remove capability levels");
    assert_eq!(
        generate_schema::stale_files(dir.path()).expect("missing check"),
        vec![PathBuf::from(doc)],
        "a missing capability-level block is stale"
    );
}

/// Parse the field names `PlainCapabilities` Debug output exposes.
fn plain_capability_fields() -> Vec<String> {
    let debug = format!("{:?}", PlainCapabilities::default());
    let inner = debug
        .strip_prefix("PlainCapabilities { ")
        .and_then(|rest| rest.strip_suffix(" }"))
        .expect("PlainCapabilities Debug keeps the struct shape");
    inner
        .split(", ")
        .map(|entry| {
            entry
                .split(':')
                .next()
                .expect("Debug entry starts with a field name")
                .to_owned()
        })
        .collect()
}

/// Read one boolean `PlainCapabilities` declaration field by name.
fn plain_flag(plain: &PlainCapabilities, field: &str) -> bool {
    match field {
        "apply_edit" => plain.apply_edit,
        "document_changes" => plain.document_changes,
        "code_actions" => plain.code_actions,
        "disabled_actions" => plain.disabled_actions,
        "show_document" => plain.show_document,
        "work_done_progress" => plain.work_done_progress,
        "completion" => plain.completion,
        "folding_range" => plain.folding_range,
        other => panic!("no boolean accessor for PlainCapabilities::{other}"),
    }
}

/// Build a nested JSON value at an LSP capability pointer.
fn set_json_pointer(root: &mut Value, pointer: &str, value: Value) {
    let mut target = root;
    for segment in pointer.trim_start_matches('/').split('/') {
        target = target
            .as_object_mut()
            .expect("capability pointer parents are objects")
            .entry(segment.to_owned())
            .or_insert_with(|| Value::Object(Default::default()));
    }
    *target = value;
}

/// Load one recorded initialize trace as the params object it records.
fn trace_params(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/lsp_traces")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(path).expect("read initialize trace"))
        .expect("parse initialize trace")
}

/// Load one current client declaration entry by client name.
fn current_client_magit(name: &str) -> Value {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/current-client-capabilities.json");
    let entries: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(path).expect("read current capabilities"))
            .expect("parse current capabilities");
    entries
        .into_iter()
        .find(|entry| entry["client"]["name"] == name)
        .unwrap_or_else(|| panic!("current capabilities name {name}"))
}

#[test]
fn luals_annotation_drift_is_reported() {
    let dir = tempfile::tempdir().expect("tempdir");
    generate_schema::write_all(dir.path()).expect("write schemas");
    assert!(generate_schema::stale_files(dir.path())
        .expect("clean check")
        .is_empty());

    let path = dir.path().join(schema_luals::ANNOTATION_PATH);
    std::fs::remove_file(&path).expect("remove annotations");
    assert_eq!(
        generate_schema::stale_files(dir.path()).expect("missing check"),
        vec![PathBuf::from(schema_luals::ANNOTATION_PATH)],
        "a missing annotation file is stale"
    );

    generate_schema::write_all(dir.path()).expect("rewrite schemas");
    let text = std::fs::read_to_string(&path).expect("read annotations");
    std::fs::write(&path, text.replace("---@class Menu.Root", "---@class LegacyMenu.Root"))
        .expect("tamper annotations");
    assert_eq!(
        generate_schema::stale_files(dir.path()).expect("tampered check"),
        vec![PathBuf::from(schema_luals::ANNOTATION_PATH)],
        "a modified annotation file is stale"
    );
}

/// Every recorded fixture `magit` declaration with its source label.
fn recorded_magit_declarations() -> Vec<(String, Value)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let rows: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("tests/fixtures/current-client-capabilities.json"))
            .expect("read current capability fixture"),
    )
    .expect("current capability fixture is JSON");
    let mut declarations = Vec::new();
    for (index, row) in rows
        .as_array()
        .expect("current capability fixture is an array")
        .iter()
        .enumerate()
    {
        declarations.push((
            format!("current-client-capabilities.json row {index}"),
            row["magit"].clone(),
        ));
    }

    let traces = root.join("tests/fixtures/lsp_traces");
    for entry in std::fs::read_dir(&traces).expect("read trace fixtures") {
        let path = entry.expect("trace entry").path();
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");
        if !(name.starts_with("initialize-") && name.ends_with(".json")) {
            continue;
        }
        let trace: Value = serde_json::from_str(
            &std::fs::read_to_string(&path).expect("read initialize trace"),
        )
        .expect("initialize trace is JSON");
        if let Some(declaration) = trace.pointer("/capabilities/experimental/magit") {
            declarations.push((name.to_string(), declaration.clone()));
        }
    }
    declarations
}

#[test]
fn recorded_magit_declarations_validate_against_capability_schema() {
    let schema = read_schema("rich-capabilities.json");
    assert_eq!(schema["title"], "RichCapabilityDeclaration");
    assert_eq!(schema["properties"]["v"]["const"], 1);

    let declarations = recorded_magit_declarations();
    assert!(
        declarations.len() >= 3,
        "expected the two current rows and the bundled initialize trace: {declarations:?}"
    );
    for (source, declaration) in declarations {
        let errors = schema_errors(&schema, &schema, &declaration, "$");
        assert!(
            errors.is_empty(),
            "{source} must validate against the capability schema: {errors:?}"
        );
    }
}

#[test]
fn corrupted_magit_declaration_copy_fails_capability_schema() {
    let schema = read_schema("rich-capabilities.json");
    let rows: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/current-client-capabilities.json"),
        )
        .expect("read current capability fixture"),
    )
    .expect("current capability fixture is JSON");
    let original = rows[0]["magit"].clone();
    assert!(
        schema_errors(&schema, &schema, &original, "$").is_empty(),
        "the uncorrupted declaration must validate"
    );

    let mut unknown_effect = original.clone();
    unknown_effect["effects"] = serde_json::json!(["visitFile", "teleport"]);
    let errors = schema_errors(&schema, &schema, &unknown_effect, "$");
    assert!(
        errors
            .iter()
            .any(|error| error.contains("$.effects[1]") && error.contains("is not one of")),
        "an unknown effect must fail the enum check: {errors:?}"
    );

    let mut unknown_keys = original.clone();
    unknown_keys["keys"] = serde_json::json!("teleport");
    let errors = schema_errors(&schema, &schema, &unknown_keys, "$");
    assert!(
        errors
            .iter()
            .any(|error| error.contains("$.keys") && error.contains("oneOf")),
        "an unknown keymap delivery must fail the oneOf check: {errors:?}"
    );

    let mut bad_version = original;
    bad_version["v"] = serde_json::json!(2);
    let errors = schema_errors(&schema, &schema, &bad_version, "$");
    assert!(
        errors
            .iter()
            .any(|error| error.contains("$.v") && error.contains("const 1")),
        "an unsupported version must fail the const check: {errors:?}"
    );
}
