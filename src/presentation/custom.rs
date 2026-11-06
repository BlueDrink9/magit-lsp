//! Typed transport for the existing custom view protocol.

use super::RichCapabilities;
use crate::protocol::{
    Effect, MessageLevel, ViewEffectsParams, ViewPromptParams, ViewRender, VIEW_EFFECTS,
    VIEW_PROMPT, VIEW_RENDER,
};
use lsp_server::{Message, Notification};
use serde::Serialize;
use std::sync::Arc;

/// Negotiated native-prompt support.
///
/// Rich clients receive validated capability declarations; Plain clients have
/// no native prompt transport; a connection that has not declared a
/// presentation yet keeps the permissive default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptSupport {
    /// No declaration has been seen; native prompts stay permissive.
    Unnegotiated,
    /// A validated rich declaration, including its optional prompt kinds.
    Rich(RichCapabilities),
    /// A Plain client with no native prompt transport.
    Plain,
}

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
    prompt_support: Arc<dyn Fn() -> PromptSupport + Send + Sync>,
}

impl Default for CustomRender {
    fn default() -> Self {
        Self::new()
    }
}

impl CustomRender {
    /// Construct the custom-protocol adapter.
    pub fn new() -> Self {
        Self::negotiated(|| PromptSupport::Unnegotiated)
    }

    /// Bind delivery to the connection's current negotiated prompt support.
    pub fn negotiated(
        source: impl Fn() -> PromptSupport + Send + Sync + 'static,
    ) -> Self {
        Self {
            prompt_support: Arc::new(source),
        }
    }

    /// Enforce optional effects and placement on notifications and response payloads.
    ///
    /// Test builds assert on undeclared emissions before production fallbacks run.
    /// Production builds replace unsupported optional effects with warnings and
    /// unsupported placements with `main` while preserving descriptor identity.
    pub fn prepare_effects(&self, mut effects: ViewEffectsParams) -> ViewEffectsParams {
        if let PromptSupport::Rich(caps) = (self.prompt_support)() {
            for effect in &mut effects.effects {
                let required = match effect {
                    Effect::VisitFile { .. } => Some("visitFile"),
                    Effect::EditDocument { .. } => Some("editDocument"),
                    Effect::Clipboard { .. } => Some("clipboard"),
                    Effect::PushView { descriptor } => {
                        #[cfg(test)]
                        {
                            let name = match descriptor.placement {
                                crate::view::Placement::Main => "main",
                                crate::view::Placement::Side => "side",
                                crate::view::Placement::Bottom => "bottom",
                                crate::view::Placement::Float => "float",
                            };
                            assert!(
                                caps.supports_placement(name),
                                "undeclared placement emitted: {name}"
                            );
                        }
                        descriptor.placement = caps.placement(descriptor.placement);
                        None
                    }
                    _ => None,
                };
                if let Some(required) = required.filter(|required| !caps.supports_effect(required))
                {
                    #[cfg(test)]
                    assert!(false, "undeclared optional effect emitted: {required}");
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
    /// This is a non-panicking preflight probe; strict emission checks run separately.
    pub fn allows_prompt(&self, prompt: &ViewPromptParams) -> bool {
        match (self.prompt_support)() {
            PromptSupport::Unnegotiated => true,
            PromptSupport::Plain => false,
            PromptSupport::Rich(caps) => caps.allows_prompt(prompt),
        }
    }

    /// Whether the client's declared pick shapes can present this request.
    pub fn allows_pick(&self, pick: &crate::protocol::ViewPickParams) -> bool {
        match (self.prompt_support)() {
            PromptSupport::Rich(caps) => caps.allows_pick(pick),
            PromptSupport::Plain | PromptSupport::Unnegotiated => false,
        }
    }

    /// The maximum item count this client declared, if any.
    pub fn pick_limit(&self) -> Option<usize> {
        match (self.prompt_support)() {
            PromptSupport::Rich(caps) => caps.pick.and_then(|pick| pick.max_items),
            PromptSupport::Plain | PromptSupport::Unnegotiated => None,
        }
    }

    /// Build the `view/pick` notification; the caller owns its response lifecycle.
    pub fn pick(&self, pick: &crate::protocol::ViewPickParams) -> Message {
        Self::notify(crate::protocol::VIEW_PICK, pick)
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
        #[cfg(test)]
        if let PromptSupport::Rich(caps) = (self.prompt_support)() {
            if !caps.allows_prompt(&prompt) {
                panic!("undeclared prompt emitted: {:?}", prompt.kind);
            }
        }
        if !self.allows_prompt(&prompt) {
            let text = if prompt.secret == Some(true) {
                super::prompt::SECRET_UNAVAILABLE
            } else {
                "Client does not support this prompt"
            };
            return self.effects(ViewEffectsParams {
                effects: vec![Effect::message(MessageLevel::Warn, text)],
            });
        }
        Self::notify(VIEW_PROMPT, &prompt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Effect, MessageLevel, PromptItem, PromptKind};
    use crate::view::{Placement, Scope, ViewDescriptor, ViewKind, ViewMode, Viewport};
    use std::collections::BTreeSet;

    fn caps(effects: &[&str], prompts: &[&str], layout: &[&str]) -> RichCapabilities {
        RichCapabilities {
            keys: crate::protocol::KeymapDelivery::Dynamic,
            modifiers: crate::protocol::KeyModifiers::Full,
            layout: layout
                .iter()
                .map(|value| (*value).to_owned())
                .collect::<BTreeSet<_>>(),
            effects: effects.iter().map(|value| (*value).to_owned()).collect(),
            prompts: prompts.iter().map(|value| (*value).to_owned()).collect(),
            pick: None,
        }
    }

    fn prompt(kind: PromptKind) -> ViewPromptParams {
        ViewPromptParams {
            prompt_id: "p".into(),
            kind,
            title: "Question".into(),
            detail: None,
            yes_label: None,
            no_label: None,
            destructive: None,
            default: None,
            placeholder: None,
            secret: None,
            items: Some(vec![PromptItem {
                id: "x".into(),
                label: "X".into(),
                description: None,
                detail: None,
            }]),
            allow_custom: None,
            preselected: None,
            min: None,
            max: None,
        }
    }

    fn descriptor(placement: Placement) -> ViewDescriptor {
        ViewDescriptor {
            id: "view-id".into(),
            kind: ViewKind::Other("custom.kind".into()),
            uri: "file:///view".into(),
            title: "Title".into(),
            placement,
            mode: ViewMode::Text,
            scope: Scope::new("scope"),
            capture: true,
            viewport: Viewport { cols: 91, rows: 37 },
        }
    }

    fn assert_rejected(action: impl FnOnce()) {
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(action))
            .expect_err("undeclared emission must panic");
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("");
        assert!(
            message.starts_with("undeclared optional effect emitted:")
                || message.starts_with("undeclared prompt emitted:")
                || message.starts_with("undeclared placement emitted:"),
            "unexpected panic: {message}"
        );
    }

    #[test]
    fn strict_emitter_rejects_undeclared_effects_prompts_and_placements() {
        let ui = CustomRender::negotiated(|| PromptSupport::Rich(caps(&[], &[], &["main"])));
        for effect in [
            Effect::VisitFile {
                uri: "file:///a".into(),
                line: 1,
                column: 2,
            },
            Effect::EditDocument {
                run_id: "r".into(),
                uri: "file:///a".into(),
                text: "x".into(),
                version: 1,
                kind: "commit".into(),
                title: "t".into(),
                mode: crate::protocol::EditDocumentMode::RealFile,
                scope: "s".into(),
            },
            Effect::Clipboard { text: "x".into() },
        ] {
            assert_rejected(|| {
                ui.effects(ViewEffectsParams {
                    effects: vec![effect],
                });
            });
        }
        for kind in [
            PromptKind::Confirm,
            PromptKind::Text,
            PromptKind::Choice,
            PromptKind::MultiChoice,
        ] {
            let ui = CustomRender::negotiated(|| PromptSupport::Rich(caps(&[], &[], &["main"])));
            assert!(!ui.allows_prompt(&prompt(kind)));
            assert_rejected(|| {
                ui.prompt(prompt(kind));
            });
        }
        let ui = CustomRender::negotiated(|| PromptSupport::Rich(caps(&[], &["text", "confirm"], &["main"])));
        let mut value = prompt(PromptKind::Text);
        value.secret = Some(true);
        assert!(!ui.allows_prompt(&value));
        assert_rejected(|| {
            ui.prompt(value);
        });
        let mut secret_only = prompt(PromptKind::Text);
        secret_only.secret = Some(true);
        let secret_only_caps = caps(&[], &["secret"], &["main"]);
        assert!(secret_only_caps.supports_prompt("secret"));
        assert!(!secret_only_caps.supports_prompt("text"));
        let ui = CustomRender::negotiated(move || PromptSupport::Rich(secret_only_caps.clone()));
        assert!(!ui.allows_prompt(&secret_only));
        assert_rejected(|| {
            ui.prompt(secret_only);
        });
        for placement in [Placement::Side, Placement::Bottom, Placement::Float] {
            let ui = CustomRender::negotiated(|| PromptSupport::Rich(caps(&[], &[], &["main"])));
            assert_rejected(|| {
                ui.effects(ViewEffectsParams {
                    effects: vec![Effect::PushView {
                        descriptor: descriptor(placement),
                    }],
                });
            });
        }
    }

    #[test]
    fn strict_emitter_accepts_declared_and_core_emissions() {
        let ui = CustomRender::negotiated(|| {
            PromptSupport::Rich(caps(
                &["visitFile", "editDocument", "clipboard"],
                &["confirm", "text", "choice", "multiChoice", "secret"],
                &["main", "side", "bottom", "float"],
            ))
        });
        assert!(ui.allows_prompt(&prompt(PromptKind::Confirm)));
        for kind in [
            PromptKind::Confirm,
            PromptKind::Text,
            PromptKind::Choice,
            PromptKind::MultiChoice,
        ] {
            assert!(matches!(ui.prompt(prompt(kind)), Message::Notification(_)));
        }
        let mut secret = prompt(PromptKind::Text);
        secret.secret = Some(true);
        assert!(matches!(ui.prompt(secret), Message::Notification(_)));
        assert!(matches!(
            ui.effects(ViewEffectsParams {
                effects: vec![
                    Effect::VisitFile {
                        uri: "file:///a".into(),
                        line: 0,
                        column: 0
                    },
                    Effect::message(MessageLevel::Info, "ok")
                ]
            }),
            Message::Notification(_)
        ));
        let effects = ViewEffectsParams {
            effects: vec![
                Effect::EditDocument {
                    run_id: "r".into(),
                    uri: "file:///a".into(),
                    text: "body".into(),
                    version: 7,
                    kind: "commit".into(),
                    title: "Edit".into(),
                    mode: crate::protocol::EditDocumentMode::RealFile,
                    scope: "s".into(),
                },
                Effect::Clipboard {
                    text: "copied".into(),
                },
            ],
        };
        let Message::Notification(note) = ui.effects(effects) else {
            panic!("notification expected")
        };
        assert_eq!(note.params["effects"][0]["type"], "editDocument");
        assert_eq!(note.params["effects"][0]["text"], "body");
        assert_eq!(note.params["effects"][0]["runId"], "r");
        assert_eq!(note.params["effects"][1]["type"], "clipboard");
        assert_eq!(note.params["effects"][1]["text"], "copied");
        for placement in [
            Placement::Main,
            Placement::Side,
            Placement::Bottom,
            Placement::Float,
        ] {
            assert!(matches!(
                ui.effects(ViewEffectsParams {
                    effects: vec![Effect::PushView {
                        descriptor: descriptor(placement)
                    }]
                }),
                Message::Notification(_)
            ));
        }
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
