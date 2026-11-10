//! Directional schema generation and committed-artifact drift checks.

#[path = "../scripts/generate_schema.rs"]
mod generate_schema;

use serde_json::Value;
use std::path::{Path, PathBuf};

use magit_lsp::keymap::{Binding, EditorMode};
use magit_lsp::menu::{Args, Group, Item, Menu, MenuStatus, Reader};
use generate_schema::schema_typescript;
use generate_schema::{Direction, MethodKind, Payload};
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
fn committed_typescript_declarations_match_generation() {
    let first = schema_typescript::protocol_declarations().expect("generate declarations");
    let second = schema_typescript::protocol_declarations().expect("regenerate declarations");
    assert_eq!(first.path, "clients/vscode/types/protocol.d.ts");
    assert_eq!(
        first.text, second.text,
        "declaration generation must be deterministic"
    );
    assert_eq!(
        read_declarations(),
        first.text,
        "committed declarations are stale (run make update-schema)"
    );
}

#[test]
fn typescript_declarations_cover_every_payload_root() {
    let text = read_declarations();
    for artifact in generate_schema::artifacts() {
        if artifact.file == "methods.json" {
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
