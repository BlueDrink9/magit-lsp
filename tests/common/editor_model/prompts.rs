//! Shared scripted prompt answers for client-contract vectors.
// anchor: tests/data/client-contracts.json prompts fixture.

use magit_lsp::protocol::{PromptKind, ViewPromptParams};
use serde_json::{json, Value};

/// User input scripted for one prompt.
#[derive(Clone, Debug)]
pub struct PromptInput {
    /// Selected or entered response; null means cancellation.
    pub response: Value,
    /// Text entered after selecting the custom choice.
    pub custom_response: Option<Value>,
}

/// Independently derived prompt answer and confirm focus order.
#[derive(Clone, Debug, PartialEq)]
pub struct PromptOutcome {
    /// Protocol answer object, including cancellation state.
    pub answer: Value,
    /// Confirm button labels in initial focus order, when applicable.
    pub focus: Option<Vec<String>>,
}

/// Derive protocol answer and confirm focus from prompt parameters and input.
pub fn answer(params: &ViewPromptParams, input: &PromptInput) -> PromptOutcome {
    let focus = (params.kind == PromptKind::Confirm && !input.response.is_null()).then(|| {
        let yes = params.yes_label.as_deref().unwrap_or("Yes").to_owned();
        let no = params.no_label.as_deref().unwrap_or("No").to_owned();
        if params.destructive == Some(true) || params.default == Some(json!(false)) {
            vec![no, yes]
        } else {
            vec![yes, no]
        }
    });
    let response = &input.response;
    let value = if response.is_null()
        || (params.kind == PromptKind::Text
            && params.secret == Some(true)
            && response.as_str() == Some(""))
    {
        None
    } else {
        Some(match params.kind {
            PromptKind::Confirm => json!(response == "yes"),
            PromptKind::Text => response.clone(),
            PromptKind::Choice if response == "__custom__" => {
                let custom = input.custom_response.as_ref().filter(|v| !v.is_null());
                if let Some(custom) = custom { json!({"custom":custom}) } else { Value::Null }
            }
            PromptKind::Choice => json!({"id":response}),
            PromptKind::MultiChoice => json!({"ids":response}),
        })
    };
    let cancelled = value.as_ref().is_none_or(Value::is_null);
    let mut answer = json!({"cancelled":cancelled});
    if !cancelled { answer["value"] = value.unwrap(); }
    PromptOutcome { answer, focus }
}
