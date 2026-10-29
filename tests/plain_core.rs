use lsp_server::{Message, Notification};
use magit_lsp::presentation::{
    negotiate, CustomRender, PlainCapabilities, Presentation, Support, Ui,
};
use magit_lsp::protocol::*;
use serde_json::{json, Value};
use std::sync::Mutex;

#[test]
fn custom_delivery_reads_negotiated_state_and_secret_requires_text_too() {
    let Presentation::Rich(caps) = negotiate(&initialize(Some(rich_block()))).unwrap() else {
        panic!()
    };
    let state = std::sync::Arc::new(Mutex::new(caps));
    let source = state.clone();
    let ui = CustomRender::negotiated(move || Some(source.lock().unwrap().clone()));
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
    }
}

fn rich_block() -> Value {
    json!({"v":1,"keys":"dynamic","modifiers":"full","render":"edits",
        "layout":["main","bottom"],"effects":["visitFile","clipboard"],
        "prompts":["confirm","text"],"folds":"server"})
}

fn initialize(block: Option<Value>) -> Value {
    let mut caps = json!({});
    if let Some(block) = block {
        caps["experimental"] = json!({"magit":block});
    }
    json!({"capabilities":caps})
}

#[derive(Default)]
struct RecordingUi(Mutex<Vec<(&'static str, Value)>>);

impl Ui for RecordingUi {
    fn render(&self, value: ViewRender) -> Message {
        self.0
            .lock()
            .unwrap()
            .push(("render", serde_json::to_value(&value).unwrap()));
        Notification::new(VIEW_RENDER.into(), serde_json::to_value(value).unwrap()).into()
    }
    fn effects(&self, value: ViewEffectsParams) -> Message {
        self.0
            .lock()
            .unwrap()
            .push(("effects", serde_json::to_value(&value).unwrap()));
        Notification::new(VIEW_EFFECTS.into(), serde_json::to_value(value).unwrap()).into()
    }
    fn prompt(&self, value: ViewPromptParams) -> Message {
        self.0
            .lock()
            .unwrap()
            .push(("prompt", serde_json::to_value(&value).unwrap()));
        Notification::new(VIEW_PROMPT.into(), serde_json::to_value(value).unwrap()).into()
    }
}

#[test]
fn recording_ui_receives_typed_render_effect_and_prompt_independent_of_transport() {
    let ui = RecordingUi::default();
    let render = ViewRender {
        view_id: "v".into(),
        version: 3,
        base_version: None,
        text: Some(vec!["text".into()]),
        edits: None,
        cursor: None,
        mode: magit_lsp::view::ViewMode::Interactive,
        pending: String::new(),
        menu: None,
    };
    ui.render(render);
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
        "workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{}},
        "textDocument":{"codeAction":{"disabledSupport":true},"completion":{},"foldingRange":{}},
        "window":{"showDocument":{"support":true},"workDoneProgress":true},
        "general":{"positionEncodings":["utf-8","utf-16"]}
    }});
    let Presentation::Plain(caps) = negotiate(&params).unwrap() else {
        panic!("expected Plain")
    };
    assert!(caps.apply_edit && caps.document_changes && caps.disabled_actions);
    assert!(caps.code_actions && caps.execute_command && caps.show_document);
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
        Support::Missing("textDocument.codeAction and workspace.executeCommand are required")
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
    assert_eq!(caps.keys, "dynamic");
    assert_eq!(caps.modifiers, "full");
    assert_eq!(caps.render, "edits");
    assert_eq!(caps.folds, "server");
    assert!(caps.supports_placement("bottom"));
    assert!(!caps.supports_placement("side"));
    assert!(caps.supports_effect("visitFile"));
    assert!(!caps.supports_effect("editDocument"));
    assert!(caps.supports_prompt("confirm"));
    assert!(!caps.supports_prompt("choice"));
}

#[test]
fn reduced_key_render_and_fold_modes_do_not_disable_explicit_commands() {
    let mut block = rich_block();
    block["modifiers"] = json!("basic");
    block["render"] = json!("full");
    block["folds"] = json!("native");
    let Presentation::Rich(caps) = negotiate(&initialize(Some(block))).unwrap() else {
        panic!("expected rich")
    };
    for command in ["status.stage", "open.commit", "common.hide-all", "menu.save-args"] {
        assert_eq!(caps.command_requirement(command), None, "{command}");
    }
    assert_eq!(caps.command_requirement("commit.create"), Some("editDocument"));
}

#[test]
fn rich_v1_accepts_each_declared_enum_option() {
    for keys in ["dynamic", "static", "forwardAll"] {
        for modifiers in ["full", "basic"] {
            for render in ["full", "edits"] {
                for folds in ["server", "native"] {
                    let mut block = rich_block();
                    block["keys"] = json!(keys);
                    block["modifiers"] = json!(modifiers);
                    block["render"] = json!(render);
                    block["folds"] = json!(folds);
                    assert!(negotiate(&initialize(Some(block))).is_ok());
                }
            }
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
    ] {
        assert!(negotiate(&initialize(Some(block))).is_err());
    }
}
