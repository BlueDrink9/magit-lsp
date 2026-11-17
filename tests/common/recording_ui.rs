//! Test-only UI recorder for structured presentation delivery.

use lsp_server::{Message, Notification};
use magit_lsp::presentation::Ui;
use magit_lsp::protocol::{
    ViewEffectsParams, ViewPromptParams, ViewRender, VIEW_EFFECTS, VIEW_PROMPT, VIEW_RENDER,
};
use serde_json::Value;
use std::sync::Mutex;

/// Records typed presentation deliveries and their serialized payloads in order.
#[derive(Default)]
pub struct RecordingUi(pub Mutex<Vec<(&'static str, Value)>>);

impl RecordingUi {
    /// Return a snapshot of ordered UI events for contract assertions.
    pub fn events(&self) -> Vec<(&'static str, Value)> {
        self.0.lock().unwrap().clone()
    }
}

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
