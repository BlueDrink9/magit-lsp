use lsp_server::Message;
use magit_lsp::presentation::{
    negotiate, CustomRender, MagitCapabilities, PlainCapabilities, Presentation, PromptSupport,
    Support, Ui,
};
use magit_lsp::protocol::*;
use serde_json::{json, Value};
use std::sync::Mutex;

#[path = "common/recording_ui.rs"]
mod recording_ui;
use recording_ui::RecordingUi;

#[test]
fn custom_delivery_reads_negotiated_state_and_secret_requires_text_too() {
    let Presentation::Rich(caps) = negotiate(&initialize(Some(rich_block()))).unwrap() else {
        panic!()
    };
    let state = std::sync::Arc::new(Mutex::new(caps));
    let source = state.clone();
    let ui = CustomRender::negotiated(move || {
        PromptSupport::Rich(source.lock().unwrap().clone())
    });
    let effects = || ViewEffectsParams {
        effects: vec![Effect::Clipboard {
            text: "copied".into(),
        }],
    };
    let Message::Notification(allowed) = ui.effects(effects()) else {
        panic!()
    };
    assert_eq!(allowed.params["effects"][0]["type"], "clipboard");
    state.lock().unwrap().effects.clear();
    let Message::Notification(refused) = ui.effects(effects()) else {
        panic!()
    };
    assert_eq!(refused.params["effects"][0]["type"], "message");
    assert_eq!(refused.params["effects"][0]["level"], "warn");
    assert_eq!(
        refused.params["effects"][0]["text"],
        "Client does not support clipboard"
    );
    let prompt: ViewPromptParams = serde_json::from_value(json!({
        "promptId":"p", "kind":"text", "title":"Password", "secret":true
    }))
    .unwrap();
    for (declared, allowed) in [
        (vec![], false),
        (vec!["text"], false),
        (vec!["secret"], false),
        (vec!["text", "secret"], true),
    ] {
        state.lock().unwrap().prompts = declared.into_iter().map(str::to_owned).collect();
        assert_eq!(ui.allows_prompt(&prompt), allowed);
        let Message::Notification(message) = ui.prompt(prompt.clone()) else {
            panic!()
        };
        assert_eq!(
            message.method,
            if allowed { VIEW_PROMPT } else { VIEW_EFFECTS }
        );
        if !allowed {
            assert_eq!(message.params["effects"][0]["level"], "warn");
            assert_eq!(
                message.params["effects"][0]["text"],
                "Client does not support this prompt"
            );
        }
    }
}

#[test]
fn plain_denies_native_prompts_while_undeclared_and_declared_rich_are_unchanged() {
    let prompt = |kind: &str| -> ViewPromptParams {
        serde_json::from_value(json!({
            "promptId": "p", "kind": kind, "title": "Question"
        }))
        .unwrap()
    };
    let plain = CustomRender::negotiated(|| PromptSupport::Plain);
    for kind in ["confirm", "text", "choice", "multiChoice"] {
        assert!(!plain.allows_prompt(&prompt(kind)), "{kind}");
        let Message::Notification(message) = plain.prompt(prompt(kind)) else {
            panic!("plain prompt must warn, not emit view/prompt")
        };
        assert_eq!(message.method, VIEW_EFFECTS);
        assert_eq!(message.params["effects"][0]["level"], "warn");
        assert_eq!(
            message.params["effects"][0]["text"],
            "Client does not support this prompt"
        );
    }
    let mut secret = prompt("text");
    secret.secret = Some(true);
    assert!(!plain.allows_prompt(&secret));

    let unnegotiated = CustomRender::new();
    assert!(unnegotiated.allows_prompt(&prompt("multiChoice")));
    let Message::Notification(message) = unnegotiated.prompt(prompt("confirm")) else {
        panic!("undeclared client must still receive the native prompt")
    };
    assert_eq!(message.method, VIEW_PROMPT);

    let Presentation::Rich(caps) = negotiate(&initialize(Some(rich_block()))).unwrap() else {
        panic!()
    };
    let rich = CustomRender::negotiated(move || PromptSupport::Rich(caps.clone()));
    assert!(rich.allows_prompt(&prompt("confirm")));
    assert!(rich.allows_prompt(&prompt("text")));
    assert!(!rich.allows_prompt(&prompt("choice")));
    assert!(!rich.allows_prompt(&prompt("multiChoice")));
    let mut secret = prompt("text");
    secret.secret = Some(true);
    assert!(!rich.allows_prompt(&secret));
}

#[test]
fn unsupported_view_placement_falls_back_to_main_without_changing_descriptor() {
    let Presentation::Rich(caps) = negotiate(&initialize(Some(json!({
        "v": 1,
        "keys": "dynamic",
        "modifiers": "full",
        "layout": ["main"],
        "effects": [],
        "prompts": []
    }))))
    .unwrap() else {
        panic!()
    };
    let ui = CustomRender::negotiated(move || PromptSupport::Rich(caps.clone()));

    for placement in [
        magit_lsp::view::Placement::Side,
        magit_lsp::view::Placement::Bottom,
        magit_lsp::view::Placement::Float,
    ] {
        let descriptor = magit_lsp::view::ViewDescriptor {
            id: "view-id".into(),
            kind: magit_lsp::view::ViewKind::Other("custom.kind".into()),
            uri: "file:///view".into(),
            title: "Title".into(),
            placement,
            mode: magit_lsp::view::ViewMode::Text,
            scope: magit_lsp::view::Scope::new("scope"),
            capture: true,
            viewport: magit_lsp::view::Viewport { cols: 91, rows: 37 },
        };
        let mut expected = serde_json::to_value(&descriptor).unwrap();
        expected["placement"] = json!("main");
        let Message::Notification(message) = ui.effects(ViewEffectsParams {
            effects: vec![Effect::PushView { descriptor }],
        }) else {
            panic!()
        };
        assert_eq!(message.params["effects"][0]["descriptor"], expected);
    }
}

fn rich_block() -> Value {
    json!({"v":1,"keys":"dynamic","modifiers":"full",
        "layout":["main","bottom"],"effects":["visitFile","clipboard"],
        "prompts":["confirm","text"]})
}

fn initialize(block: Option<Value>) -> Value {
    let mut caps = json!({});
    if let Some(block) = block {
        caps["experimental"] = json!({"magit":block});
    }
    json!({"capabilities":caps})
}

#[test]
fn shared_recording_ui_captures_order_and_payloads() {
    let ui = RecordingUi::default();
    ui.render(ViewRender {
        view_id: "v".into(),
        version: 3,
        base_version: None,
        text: Some(vec!["text".into()]),
        edits: None,
        cursor: None,
        mode: magit_lsp::view::ViewMode::Interactive,
        pending: String::new(),
        menu: None,
    });
    ui.effects(ViewEffectsParams {
        effects: vec![Effect::message(MessageLevel::Info, "notice")],
    });
    ui.prompt(ViewPromptParams {
        prompt_id: "p1".into(),
        kind: PromptKind::Confirm,
        title: "Continue?".into(),
        detail: None,
        yes_label: None,
        no_label: None,
        destructive: None,
        default: None,
        placeholder: None,
        secret: None,
        items: None,
        allow_custom: None,
        preselected: None,
        min: None,
        max: None,
    });
    let rows = ui.0.lock().unwrap();
    assert_eq!(
        rows.iter().map(|row| row.0).collect::<Vec<_>>(),
        ["render", "effects", "prompt"]
    );
    assert_eq!(rows[0].1["version"], 3);
    assert_eq!(rows[1].1["effects"][0]["text"], "notice");
    assert_eq!(rows[2].1["promptId"], "p1");
}

#[test]
fn custom_render_keeps_existing_rich_notification_shapes() {
    let ui = CustomRender::new();
    let Message::Notification(render) = ui.render(ViewRender {
        view_id: "v".into(),
        version: 1,
        base_version: None,
        text: Some(vec!["x".into()]),
        edits: None,
        cursor: None,
        mode: magit_lsp::view::ViewMode::Interactive,
        pending: String::new(),
        menu: None,
    }) else {
        panic!()
    };
    assert_eq!(render.method, VIEW_RENDER);
    assert_eq!(render.params["viewId"], "v");
    assert_eq!(render.params["text"][0], "x");
    let Message::Notification(effects) = ui.effects(ViewEffectsParams {
        effects: vec![Effect::message(MessageLevel::Info, "ok")],
    }) else {
        panic!()
    };
    assert_eq!(effects.method, VIEW_EFFECTS);
    assert_eq!(effects.params["effects"][0]["text"], "ok");
    let Message::Notification(prompt) = ui.prompt(ViewPromptParams {
        prompt_id: "p".into(),
        kind: PromptKind::Confirm,
        title: "Question".into(),
        detail: None,
        yes_label: None,
        no_label: None,
        destructive: None,
        default: None,
        placeholder: None,
        secret: None,
        items: None,
        allow_custom: None,
        preselected: None,
        min: None,
        max: None,
    }) else {
        panic!()
    };
    assert_eq!(prompt.method, VIEW_PROMPT);
    assert_eq!(prompt.params["promptId"], "p");
}

#[test]
fn absent_selector_selects_plain_and_extracts_standard_capabilities() {
    let params = json!({"capabilities":{
        "workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true}},
        "textDocument":{"codeAction":{"disabledSupport":true},"completion":{},"foldingRange":{}},
        "window":{"showDocument":{"support":true},"workDoneProgress":true},
        "general":{"positionEncodings":["utf-8","utf-16"]}
    }});
    let Presentation::Plain(caps) = negotiate(&params).unwrap() else {
        panic!("expected Plain")
    };
    assert!(caps.apply_edit && caps.document_changes && caps.disabled_actions);
    assert!(caps.code_actions && caps.show_document);
    assert!(caps.work_done_progress && caps.completion && caps.folding_range);
    assert_eq!(caps.position_encodings, ["utf-8", "utf-16"]);
    assert_eq!(caps.edit_support(), Support::Supported);
    assert_eq!(caps.action_support(), Support::Supported);
    assert_eq!(caps.show_document_support(), Support::Supported);
}

#[test]
fn plain_support_reports_missing_capability_reasons() {
    let caps = PlainCapabilities::default();
    assert_eq!(
        caps.edit_support(),
        Support::Missing("workspace.applyEdit is not supported; presentation is inert")
    );
    assert_eq!(
        caps.action_support(),
        Support::Missing("textDocument.codeAction is not supported")
    );
    assert_eq!(
        caps.show_document_support(),
        Support::Missing("window.showDocument.support is not supported")
    );
}

#[test]
fn valid_rich_block_selects_rich_with_optional_features_separate() {
    let Presentation::Rich(caps) = negotiate(&initialize(Some(rich_block()))).unwrap() else {
        panic!("expected rich")
    };
    assert_eq!(caps.keys, KeymapDelivery::Dynamic);
    assert_eq!(caps.modifiers, KeyModifiers::Full);
    assert!(caps.supports_placement("bottom"));
    assert!(!caps.supports_placement("side"));
    assert!(caps.supports_effect("visitFile"));
    assert!(!caps.supports_effect("editDocument"));
    assert!(caps.supports_prompt("confirm"));
    assert!(!caps.supports_prompt("choice"));
}

#[test]
fn rich_command_requirements_come_from_registry_metadata() {
    let mut block = rich_block();
    block["effects"] = json!([]);
    let Presentation::Rich(caps) = negotiate(&initialize(Some(block))).unwrap() else {
        panic!("expected rich")
    };
    let required = [
        ("common.visit", "visitFile"),
        ("common.copy-value", "clipboard"),
        ("log.select-and-copy", "clipboard"),
        ("commit.create", "editDocument"),
        ("commit.amend", "editDocument"),
        ("commit.extend", "editDocument"),
        ("commit.reword", "editDocument"),
    ];
    for (command, effect) in required {
        assert_eq!(caps.command_requirement(command), Some(effect));
    }
    assert_eq!(caps.command_requirement("common.confirm"), None);
    assert_eq!(caps.command_requirement("missing.command"), None);
    let Presentation::Rich(declared) = negotiate(&initialize(Some(rich_block()))).unwrap() else {
        panic!("expected rich")
    };
    assert_eq!(declared.command_requirement("common.visit"), None);
}

#[test]
fn rich_capabilities_deserialize_roundtrip_supported_modes_and_reject_invalid_schema() {
    assert!(negotiate(&initialize(Some(rich_block()))).is_ok());

    for (keys, modifiers) in [("dynamic", "full"), ("static", "basic")] {
        let mut value = rich_block();
        value["keys"] = json!(keys);
        value["modifiers"] = json!(modifiers);
        let declaration: MagitCapabilities = serde_json::from_value(value).unwrap();
        let encoded = serde_json::to_value(&declaration).unwrap();
        assert_eq!(
            serde_json::from_value::<MagitCapabilities>(encoded).unwrap(),
            declaration
        );
    }

    let valid = r#"{"v":1,"keys":"dynamic","modifiers":"full","layout":["main"],"effects":[],"prompts":[]}"#;
    // Duplicate JSON object fields are distinct from duplicate values in sets.
    for duplicate in [
        r#""layout":["main"],"layout":["main"]"#,
        r#""effects":[],"effects":[]"#,
        r#""prompts":[],"prompts":[]"#,
    ] {
        let (field, value) = duplicate.split_once(',').unwrap();
        let input = valid.replace(field, &format!("{field},{value}"));
        assert!(
            serde_json::from_str::<MagitCapabilities>(&input).is_err(),
            "{input}"
        );
    }
    for (field, duplicate_values) in [
        ("layout", json!(["main", "main"])),
        ("effects", json!(["visitFile", "visitFile"])),
        ("prompts", json!(["confirm", "confirm"])),
    ] {
        let mut block = rich_block();
        block[field] = duplicate_values;
        assert!(
            negotiate(&initialize(Some(block))).is_err(),
            "duplicate values in {field} must be rejected"
        );
    }
    for invalid in [
        valid.replace("\"v\":1", "\"v\":2"),
        valid.replace("\"prompts\":[]", "\"prompts\":[],\"render\":\"full\""),
        valid.replace("\"prompts\":[]", "\"prompts\":[],\"folds\":\"native\""),
        valid.replace("[\"main\"]", "[\"bottom\"]"),
        valid.replace("\"layout\":[\"main\"],", "\"layout\":[\"bottom\"],"),
        valid.replace(",\"effects\":[]", ""),
        valid.replace(",\"prompts\":[]", ""),
        valid.replace(",\"modifiers\":\"full\"", ""),
        valid.replace(",\"keys\":\"dynamic\"", ""),
        valid.replace("\"v\":1,", ""),
    ] {
        assert!(
            serde_json::from_str::<MagitCapabilities>(&invalid).is_err(),
            "{invalid}"
        );
    }
    let mut unsupported_version = rich_block();
    unsupported_version["v"] = json!(2);
    assert!(negotiate(&initialize(Some(unsupported_version))).is_err());
}

#[test]
fn current_native_client_capabilities_are_strictly_negotiated() {
    let records: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/current-client-capabilities.json")).unwrap();
    assert_eq!(records.len(), 2);
    for record in records {
        let block = record["magit"].clone();
        let declaration: MagitCapabilities = serde_json::from_value(block.clone()).unwrap();
        let Presentation::Rich(caps) = negotiate(&initialize(Some(block.clone()))).unwrap() else {
            panic!("native client must negotiate rich presentation")
        };
        assert_eq!(caps.keys, declaration.keys);
        assert_eq!(caps.modifiers, declaration.modifiers);
        assert_eq!(caps.layout, declaration.layout);
        assert!(!caps.layout.is_empty());
        match record["client"]["name"].as_str().unwrap() {
            "Neovim" => {
                assert_eq!(caps.keys, KeymapDelivery::Dynamic);
                assert_eq!(caps.modifiers, KeyModifiers::Basic);
                assert!(caps.supports_placement("bottom"));
            }
            "VSCodium" => {
                assert_eq!(caps.keys, KeymapDelivery::Static);
                assert_eq!(caps.modifiers, KeyModifiers::Full);
                assert_eq!(
                    caps.layout,
                    ["main"].into_iter().map(str::to_owned).collect()
                );
            }
            name => panic!("unexpected recorded client {name}"),
        }
        let mut mutated = block;
        mutated["removedOrUnknownField"] = json!(true);
        assert!(serde_json::from_value::<MagitCapabilities>(mutated.clone()).is_err());
        assert!(negotiate(&initialize(Some(mutated))).is_err());
    }
}

#[test]
fn unsupported_render_and_fold_modes_are_rejected() {
    let mut block = rich_block();
    block["render"] = json!("full");
    assert!(negotiate(&initialize(Some(block))).is_err());
    let mut block = rich_block();
    block["folds"] = json!("native");
    assert!(negotiate(&initialize(Some(block))).is_err());
}

#[test]
fn rich_v1_accepts_each_declared_enum_option() {
    for keys in ["dynamic", "static"] {
        for modifiers in ["full", "basic"] {
            let mut block = rich_block();
            block["keys"] = json!(keys);
            block["modifiers"] = json!(modifiers);
            assert!(negotiate(&initialize(Some(block))).is_ok());
        }
    }
    let mut block = rich_block();
    block["layout"] = json!(["main", "side", "bottom", "float"]);
    block["effects"] = json!(["visitFile", "editDocument", "clipboard"]);
    block["prompts"] = json!(["confirm", "text", "choice", "multiChoice", "secret"]);
    assert!(negotiate(&initialize(Some(block))).is_ok());
}

#[test]
fn malformed_or_unknown_rich_declarations_are_rejected() {
    for block in [
        json!(null),
        json!({"v":2}),
        json!({"v":1}),
        {
            let mut b = rich_block();
            b["keys"] = json!("automatic");
            b
        },
        {
            let mut b = rich_block();
            b["modifiers"] = json!("none");
            b
        },
        {
            let mut b = rich_block();
            b["render"] = json!("delta");
            b
        },
        {
            let mut b = rich_block();
            b["layout"] = json!(["side"]);
            b
        },
        {
            let mut b = rich_block();
            b["effects"] = json!(["unknown"]);
            b
        },
        {
            let mut b = rich_block();
            b["prompts"] = json!(["secretText"]);
            b
        },
        {
            let mut b = rich_block();
            b["folds"] = json!("client");
            b
        },
        {
            let mut b = rich_block();
            b["keys"] = json!("forwardAll");
            b
        },
        {
            let mut b = rich_block();
            b["render"] = json!("full");
            b
        },
        {
            let mut b = rich_block();
            b["folds"] = json!("native");
            b
        },
    ] {
        assert!(negotiate(&initialize(Some(block))).is_err());
    }
}
