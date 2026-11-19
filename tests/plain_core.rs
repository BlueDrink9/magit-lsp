use lsp_server::Message;
use magit_lsp::commands::EffectKind;
use magit_lsp::presentation::prompt::SECRET_UNAVAILABLE;
use magit_lsp::presentation::{
    negotiate, CustomRender, MagitCapabilities, PlainAvailability, PlainCapabilities, Presentation,
    PromptSupport, RichCapabilities, Support, Ui,
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
            assert_eq!(message.params["effects"][0]["text"], SECRET_UNAVAILABLE);
        }
    }
}

#[test]
fn runtime_unsupported_effect_is_a_readable_message() {
    let Presentation::Rich(caps) = negotiate(&initialize(Some(json!({
        "v": 1,
        "keys": "dynamic",
        "modifiers": "full",
        "layout": ["main"],
        "effects": ["visitFile"],
        "prompts": ["confirm"]
    }))))
    .unwrap() else {
        panic!()
    };
    let ui = CustomRender::negotiated(move || PromptSupport::Rich(caps.clone()));
    let prepared = ui.prepare_effects(ViewEffectsParams {
        effects: vec![Effect::Clipboard {
            text: "copied".into(),
        }],
    });
    assert_eq!(prepared.effects.len(), 1);
    let Effect::Message { level, text, .. } = &prepared.effects[0] else {
        panic!("expected a message effect, got {:?}", prepared.effects[0]);
    };
    assert_eq!(*level, MessageLevel::Warn);
    assert_eq!(text, "Client does not support clipboard");
    assert!(!text.contains('{'), "raw JSON leaked: {text}");
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
fn plain_secret_prompt_refusal_uses_the_readable_constant() {
    let secret: ViewPromptParams = serde_json::from_value(json!({
        "promptId": "p", "kind": "text", "title": "Secret",
        "secret": true
    }))
    .unwrap();
    let plain = CustomRender::negotiated(|| PromptSupport::Plain);
    assert!(!plain.allows_prompt(&secret));
    let Message::Notification(message) = plain.prompt(secret) else {
        panic!("plain secret prompt must warn, not emit view/prompt")
    };
    assert_eq!(message.method, VIEW_EFFECTS);
    assert_eq!(message.params["effects"][0]["level"], "warn");
    assert_eq!(
        message.params["effects"][0]["text"],
        magit_lsp::presentation::prompt::SECRET_UNAVAILABLE
    );
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

fn pick_block() -> Value {
    let mut block = rich_block();
    block["pick"] = json!({
        "multi": true, "freeText": true, "description": true, "detail": true,
        "group": true, "preselection": true, "fallbackView": true, "maxItems": 3
    });
    block
}

fn pick_params() -> ViewPickParams {
    ViewPickParams {
        pick_id: "p".into(),
        purpose: "revision".into(),
        prompt: "Select a revision".into(),
        multi: false,
        allow_free_text: false,
        preselection: Vec::new(),
        items: vec![PickItem {
            id: "a".into(),
            label: "A".into(),
            description: None,
            detail: None,
            group: None,
        }],
        fallback_view: None,
    }
}

fn declared_pick(pick: Value) -> RichCapabilities {
    let mut block = rich_block();
    block["pick"] = pick;
    let Presentation::Rich(caps) = negotiate(&initialize(Some(block))).unwrap() else {
        panic!("expected rich")
    };
    caps
}

#[test]
fn absent_pick_declaration_negotiates_none() {
    let Presentation::Rich(caps) = negotiate(&initialize(Some(rich_block()))).unwrap() else {
        panic!("expected rich")
    };
    assert!(caps.pick.is_none());
    let declaration: MagitCapabilities = serde_json::from_value(rich_block()).unwrap();
    assert!(declaration.pick.is_none());
}

#[test]
fn valid_and_empty_pick_declarations_parse() {
    let Presentation::Rich(caps) = negotiate(&initialize(Some(pick_block()))).unwrap() else {
        panic!("expected rich")
    };
    let pick = caps.pick.expect("declared pick");
    assert!(pick.multi && pick.free_text && pick.description && pick.detail);
    assert!(pick.group && pick.preselection && pick.fallback_view);
    assert_eq!(pick.max_items, Some(3));

    let empty = declared_pick(json!({})).pick.expect("declared pick");
    assert!(!empty.multi && !empty.free_text && !empty.description && !empty.detail);
    assert!(!empty.group && !empty.preselection && !empty.fallback_view);
    assert_eq!(empty.max_items, None);
}

#[test]
fn pick_declaration_rejects_unknown_keys_and_wrong_types() {
    for pick in [
        json!({"unknown": true}),
        json!({"multi": "yes"}),
        json!({"maxItems": -1}),
        json!({"maxItems": "3"}),
    ] {
        let mut block = rich_block();
        block["pick"] = pick.clone();
        assert!(negotiate(&initialize(Some(block))).is_err(), "{pick}");
    }
}

#[test]
fn pick_payloads_serialize_camel_case() {
    let mut params = pick_params();
    params.allow_free_text = true;
    params.preselection = vec!["a".into()];
    params.items[0].description = Some("short".into());
    params.items[0].detail = Some("long".into());
    params.items[0].group = Some("heads".into());
    params.fallback_view = Some("magit://pick/p".into());
    let value = serde_json::to_value(&params).unwrap();
    assert_eq!(value["pickId"], "p");
    assert_eq!(value["allowFreeText"], true);
    assert_eq!(value["fallbackView"], "magit://pick/p");
    assert_eq!(value["preselection"][0], "a");
    assert_eq!(value["items"][0]["description"], "short");
    assert_eq!(value["items"][0]["detail"], "long");
    assert_eq!(value["items"][0]["group"], "heads");

    let bare = PickItem {
        id: "b".into(),
        label: "B".into(),
        description: None,
        detail: None,
        group: None,
    };
    let value = serde_json::to_value(&bare).unwrap();
    assert_eq!(value["label"], "B");
    assert!(value.get("description").is_none());

    let respond = ViewPickRespondParams {
        pick_id: "p".into(),
        cancelled: false,
        selected: vec!["a".into()],
        free_text: Some("HEAD~1".into()),
    };
    let value = serde_json::to_value(&respond).unwrap();
    assert_eq!(value["pickId"], "p");
    assert_eq!(value["freeText"], "HEAD~1");
    assert_eq!(value["selected"][0], "a");
}

#[test]
fn allows_pick_rejects_each_undeclared_shape() {
    let base = pick_params();
    assert!(declared_pick(json!({})).allows_pick(&base));
    let Presentation::Rich(no_pick) = negotiate(&initialize(Some(rich_block()))).unwrap() else {
        panic!("expected rich")
    };
    assert!(!no_pick.allows_pick(&base));

    let mut multi = base.clone();
    multi.multi = true;
    assert!(!declared_pick(json!({})).allows_pick(&multi));
    assert!(declared_pick(json!({"multi": true})).allows_pick(&multi));

    let mut free_text = base.clone();
    free_text.allow_free_text = true;
    assert!(!declared_pick(json!({})).allows_pick(&free_text));
    assert!(declared_pick(json!({"freeText": true})).allows_pick(&free_text));

    for (field, flag) in [
        ("description", "description"),
        ("detail", "detail"),
        ("group", "group"),
    ] {
        let mut shaped = base.clone();
        let item = &mut shaped.items[0];
        match field {
            "description" => item.description = Some("short".into()),
            "detail" => item.detail = Some("long".into()),
            _ => item.group = Some("heads".into()),
        }
        assert!(!declared_pick(json!({})).allows_pick(&shaped), "{field}");
        let mut declared = serde_json::Map::new();
        declared.insert(flag.into(), json!(true));
        assert!(
            declared_pick(Value::Object(declared)).allows_pick(&shaped),
            "{field}"
        );
    }

    let mut preselected = base.clone();
    preselected.preselection = vec!["a".into()];
    assert!(!declared_pick(json!({})).allows_pick(&preselected));
    assert!(declared_pick(json!({"preselection": true})).allows_pick(&preselected));

    let mut fallback = base.clone();
    fallback.fallback_view = Some("magit://pick/p".into());
    assert!(!declared_pick(json!({})).allows_pick(&fallback));
    assert!(declared_pick(json!({"fallbackView": true})).allows_pick(&fallback));

    let mut many = base.clone();
    many.items.push(PickItem {
        id: "b".into(),
        label: "B".into(),
        description: None,
        detail: None,
        group: None,
    });
    assert!(!declared_pick(json!({"maxItems": 1})).allows_pick(&many));
    assert!(declared_pick(json!({"maxItems": 2})).allows_pick(&many));
}

#[test]
fn rich_command_requirements_come_from_registry_metadata() {
    let mut block = rich_block();
    block["effects"] = json!([]);
    let Presentation::Rich(caps) = negotiate(&initialize(Some(block))).unwrap() else {
        panic!("expected rich")
    };
    let required = [
        ("common.visit", EffectKind::VisitFile),
        ("common.copy-value", EffectKind::Clipboard),
        ("log.select-and-copy", EffectKind::Clipboard),
        ("commit.create", EffectKind::EditDocument),
        ("commit.amend", EffectKind::EditDocument),
        ("commit.extend", EffectKind::EditDocument),
        ("commit.reword", EffectKind::EditDocument),
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
fn rich_command_requirement_uses_every_declared_effect() {
    let mut declared: Vec<EffectKind> = magit_lsp::commands::registry()
        .iter()
        .flat_map(|command| command.effects.iter().copied())
        .collect();
    declared.sort_by_key(|effect| effect.name());
    declared.dedup();
    assert_eq!(
        declared,
        [EffectKind::Clipboard, EffectKind::EditDocument, EffectKind::VisitFile]
    );
    for effect in declared {
        let mut block = rich_block();
        let declared: Vec<&str> = ["clipboard", "editDocument", "visitFile"]
            .into_iter()
            .filter(|name| *name != effect.name())
            .collect();
        block["effects"] = json!(declared);
        let Presentation::Rich(caps) = negotiate(&initialize(Some(block))).unwrap() else {
            panic!("expected rich")
        };
        for command in magit_lsp::commands::registry().iter() {
            let Some(first) = magit_lsp::commands::required_effects(command.id).first() else {
                assert_eq!(caps.command_requirement(command.id), None);
                continue;
            };
            let expected = if *first == effect { Some(effect) } else { None };
            assert_eq!(caps.command_requirement(command.id), expected, "{}", command.id);
        }
    }
}

#[test]
fn plain_availability_matrix() {
    let minimal = PlainCapabilities::default();
    let edits = PlainCapabilities {
        apply_edit: true,
        code_actions: true,
        ..PlainCapabilities::default()
    };
    let full = PlainCapabilities {
        apply_edit: true,
        code_actions: true,
        show_document: true,
        ..PlainCapabilities::default()
    };
    let reason = |availability| match availability {
        PlainAvailability::Unavailable { reason } => reason,
        PlainAvailability::Available => panic!("expected unavailability"),
    };
    let cmd = |id| {
        magit_lsp::commands::registry()
            .get(id)
            .unwrap_or_else(|| panic!("missing command {id}"))
    };
    assert_eq!(
        minimal.plain_availability(cmd("status.stage")),
        PlainAvailability::Unavailable {
            reason: "workspace.applyEdit and textDocument.codeAction are required"
        }
    );
    assert_eq!(
        minimal.plain_availability(cmd("commit.create")),
        PlainAvailability::Unavailable {
            reason: "workspace.applyEdit is required"
        }
    );
    assert_eq!(
        minimal.plain_availability(cmd("common.visit")),
        PlainAvailability::Unavailable {
            reason: "window.showDocument.support is required"
        }
    );
    assert_eq!(
        reason(minimal.plain_availability(cmd("common.copy-value"))),
        "the clipboard effect is not available in Plain mode"
    );
    assert_eq!(
        minimal.plain_availability(cmd("common.confirm")),
        PlainAvailability::Available
    );
    let apply_edit_only = PlainCapabilities {
        apply_edit: true,
        ..PlainCapabilities::default()
    };
    assert_eq!(
        reason(apply_edit_only.plain_availability(cmd("status.stage"))),
        "workspace.applyEdit and textDocument.codeAction are required"
    );
    assert_eq!(
        apply_edit_only.plain_availability(cmd("commit.create")),
        PlainAvailability::Available
    );
    for command in ["status.stage", "status.unstage", "status.discard"] {
        assert_eq!(edits.plain_availability(cmd(command)), PlainAvailability::Available);
        assert_eq!(full.plain_availability(cmd(command)), PlainAvailability::Available);
    }
    assert_eq!(
        edits.plain_availability(cmd("common.visit")),
        PlainAvailability::Unavailable {
            reason: "window.showDocument.support is required"
        }
    );
    assert_eq!(
        full.plain_availability(cmd("common.visit")),
        PlainAvailability::Available
    );
    for command in ["commit.create", "commit.amend", "commit.extend", "commit.reword"] {
        assert_eq!(full.plain_availability(cmd(command)), PlainAvailability::Available);
    }
    assert_eq!(
        reason(full.plain_availability(cmd("common.copy-value"))),
        "the clipboard effect is not available in Plain mode"
    );
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

#[test]
fn rich_protocol_version_minimum_is_validated_without_selecting_below_it() {
    let accepted = |block: Value| match negotiate(&initialize(Some(block))) {
        Ok(Presentation::Rich(_)) => {}
        other => panic!("expected rich acceptance: {other:?}"),
    };
    // Omission selects `v`, and a matching minimum of 1 is accepted.
    accepted(rich_block());
    let mut matching = rich_block();
    matching["minimumVersion"] = json!(1);
    accepted(matching);

    let rejected = |block: Value| negotiate(&initialize(Some(block))).expect_err("rejection").0;

    let mut unsupported = rich_block();
    unsupported["v"] = json!(2);
    assert!(
        rejected(unsupported).contains("unsupported rich protocol version"),
        "an unsupported selected version keeps the existing readable error"
    );

    for invalid in [json!(0), json!(-1), json!(1.5), json!("2")] {
        let mut block = rich_block();
        block["minimumVersion"] = invalid.clone();
        let message = rejected(block);
        assert!(
            message.contains("capabilities.experimental.magit")
                && message.contains("minimumVersion must be a positive integer"),
            "{invalid}: {message}"
        );
    }

    // A minimum above the selected version names both numbers and is never clamped.
    let mut too_high = rich_block();
    too_high["minimumVersion"] = json!(2);
    let message = rejected(too_high);
    assert!(
        message.contains("minimumVersion 2")
            && message.contains("selected rich protocol version 1"),
        "{message}"
    );
}
