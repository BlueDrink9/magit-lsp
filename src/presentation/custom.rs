//! Typed transport for the existing custom view protocol.

use super::RichCapabilities;
use crate::protocol::{
    Effect, MessageLevel, ViewEffectsParams, ViewPromptParams, ViewRender, VIEW_EFFECTS,
    VIEW_PROMPT, VIEW_RENDER,
};
use lsp_server::{Message, Notification};
use serde::Serialize;
use std::sync::Arc;

/// Client-facing typed presentation delivery seam.
pub trait Ui: Send + Sync {
    /// Build the existing wire notification for one rendered view.
    fn render(&self, render: ViewRender) -> Message;
    /// Build the existing wire notification for asynchronous effects.
    fn effects(&self, effects: ViewEffectsParams) -> Message;
    /// Build the prompt notification; the caller owns its response lifecycle.
    fn prompt(&self, prompt: ViewPromptParams) -> Message;
}

/// Existing rich/custom wire transport, retained as the initial adapter.
#[derive(Clone)]
pub struct CustomRender {
    capabilities: Arc<dyn Fn() -> Option<RichCapabilities> + Send + Sync>,
}

impl Default for CustomRender {
    fn default() -> Self {
        Self::new()
    }
}

impl CustomRender {
    /// Construct the custom-protocol adapter.
    pub fn new() -> Self {
        Self::negotiated(|| None)
    }

    /// Bind delivery to the connection's current negotiated declaration.
    pub fn negotiated(
        source: impl Fn() -> Option<RichCapabilities> + Send + Sync + 'static,
    ) -> Self {
        Self {
            capabilities: Arc::new(source),
        }
    }

    /// Enforce optional effects and placement on notifications and response payloads.
    pub fn prepare_effects(&self, mut effects: ViewEffectsParams) -> ViewEffectsParams {
        if let Some(caps) = (self.capabilities)() {
            for effect in &mut effects.effects {
                let required = match effect {
                    Effect::VisitFile { .. } => Some("visitFile"),
                    Effect::EditDocument { .. } => Some("editDocument"),
                    Effect::Clipboard { .. } => Some("clipboard"),
                    Effect::OpenView { descriptor } | Effect::PushView { descriptor } => {
                        descriptor.placement = caps.placement(descriptor.placement);
                        None
                    }
                    _ => None,
                };
                if let Some(required) = required.filter(|required| !caps.supports_effect(required))
                {
                    *effect = Effect::message(
                        MessageLevel::Warn,
                        format!("Client does not support {required}"),
                    );
                }
            }
        }
        effects
    }

    /// Check before registering a waiter or entering an editor-helper transition.
    pub fn allows_prompt(&self, prompt: &ViewPromptParams) -> bool {
        (self.capabilities)().is_none_or(|caps| caps.allows_prompt(prompt))
    }

    fn notify<T: Serialize>(method: &str, params: &T) -> Message {
        let value = serde_json::to_value(params).expect("protocol params serialize");
        Message::Notification(Notification::new(method.to_owned(), value))
    }
}

impl Ui for CustomRender {
    fn render(&self, render: ViewRender) -> Message {
        Self::notify(VIEW_RENDER, &render)
    }
    fn effects(&self, effects: ViewEffectsParams) -> Message {
        Self::notify(VIEW_EFFECTS, &self.prepare_effects(effects))
    }
    fn prompt(&self, prompt: ViewPromptParams) -> Message {
        if !self.allows_prompt(&prompt) {
            return self.effects(ViewEffectsParams {
                effects: vec![Effect::message(
                    MessageLevel::Warn,
                    "Client does not support this prompt",
                )],
            });
        }
        Self::notify(VIEW_PROMPT, &prompt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Cursor, Effect, MessageLevel};
    use crate::view::ViewMode;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingUi(Mutex<Vec<(&'static str, serde_json::Value)>>);

    impl Ui for RecordingUi {
        fn render(&self, value: ViewRender) -> Message {
            self.0
                .lock()
                .unwrap()
                .push(("render", serde_json::to_value(&value).unwrap()));
            Message::Notification(Notification::new(
                VIEW_RENDER.to_owned(),
                serde_json::to_value(value).unwrap(),
            ))
        }
        fn effects(&self, value: ViewEffectsParams) -> Message {
            self.0
                .lock()
                .unwrap()
                .push(("effects", serde_json::to_value(&value).unwrap()));
            Message::Notification(Notification::new(
                VIEW_EFFECTS.to_owned(),
                serde_json::to_value(value).unwrap(),
            ))
        }
        fn prompt(&self, value: ViewPromptParams) -> Message {
            self.0
                .lock()
                .unwrap()
                .push(("prompt", serde_json::to_value(&value).unwrap()));
            Message::Notification(Notification::new(
                VIEW_PROMPT.to_owned(),
                serde_json::to_value(value).unwrap(),
            ))
        }
    }

    #[test]
    fn recording_ui_receives_typed_delivery_without_transport() {
        let ui = RecordingUi::default();
        let _ = ui.render(ViewRender {
            view_id: "v".into(),
            version: 1,
            base_version: None,
            text: Some(vec!["status".into()]),
            edits: None,
            cursor: Some(Cursor { line: 0, column: 0 }),
            mode: ViewMode::default(),
            pending: String::new(),
            menu: None,
        });
        let _ = ui.effects(ViewEffectsParams {
            effects: vec![Effect::message(MessageLevel::Info, "ok")],
        });
        let _ = ui.prompt(ViewPromptParams {
            prompt_id: "p1".into(),
            kind: crate::protocol::PromptKind::Confirm,
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
        let got = ui.0.lock().unwrap();
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].0, "render");
        assert_eq!(got[1].0, "effects");
        assert_eq!(got[2].0, "prompt");
        assert_eq!(got[2].1["promptId"], "p1");
    }

    #[test]
    fn custom_adapter_preserves_notification_wire_shape() {
        let ui = CustomRender::new();
        let Message::Notification(render) = ui.render(ViewRender {
            view_id: "v".into(),
            version: 1,
            base_version: None,
            text: Some(vec!["hello".into()]),
            edits: None,
            cursor: None,
            mode: ViewMode::default(),
            pending: String::new(),
            menu: None,
        }) else {
            panic!("notification expected")
        };
        assert_eq!(render.method, VIEW_RENDER);
        assert_eq!(render.params["viewId"], "v");
        assert_eq!(render.params["text"][0], "hello");
        let Message::Notification(note) = ui.effects(ViewEffectsParams {
            effects: vec![Effect::message(MessageLevel::Info, "ok")],
        }) else {
            panic!("notification expected")
        };
        assert_eq!(note.method, VIEW_EFFECTS);
        assert_eq!(note.params["effects"][0]["type"], "message");
        assert_eq!(note.params["effects"][0]["text"], "ok");
        let Message::Notification(prompt) = ui.prompt(ViewPromptParams {
            prompt_id: "p".into(),
            kind: crate::protocol::PromptKind::Confirm,
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
            panic!("notification expected")
        };
        assert_eq!(prompt.method, VIEW_PROMPT);
        assert_eq!(prompt.params["promptId"], "p");
        assert_eq!(prompt.params["title"], "Question");
    }
}
