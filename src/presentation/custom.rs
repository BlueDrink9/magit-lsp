//! Typed transport for the existing custom view protocol.

use crate::protocol::{
    ViewEffectsParams, ViewPromptParams, ViewRender, VIEW_EFFECTS, VIEW_PROMPT, VIEW_RENDER,
};
use lsp_server::{Message, Notification};
use serde::Serialize;

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
#[derive(Clone, Copy, Default)]
pub struct CustomRender;

impl CustomRender {
    /// Construct the custom-protocol adapter.
    pub fn new() -> Self {
        Self
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
        Self::notify(VIEW_EFFECTS, &effects)
    }
    fn prompt(&self, prompt: ViewPromptParams) -> Message {
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
            effects: vec![Effect::message(MessageLevel::Info, "ok".into())],
        });
        let got = ui.0.lock().unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].0, "render");
        assert_eq!(got[1].0, "effects");
    }

    #[test]
    fn custom_adapter_preserves_notification_wire_shape() {
        let ui = CustomRender::new();
        let Message::Notification(note) = ui.effects(ViewEffectsParams {
            effects: vec![Effect::message(MessageLevel::Info, "ok".into())],
        }) else {
            panic!("notification expected")
        };
        assert_eq!(note.method, VIEW_EFFECTS);
        assert_eq!(note.params["effects"][0]["type"], "message");
        assert_eq!(note.params["effects"][0]["message"], "ok");
    }
}
