use magit_lsp::presentation::{negotiate, PlainCapabilities, Presentation, Support};
use serde_json::{json, Value};

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
