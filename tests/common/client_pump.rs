use crate::editor_model::EditorModel;
use crate::fake_editor::{allocate_request_id, FakeEditor, Profile};
use crate::stdio::TestServer;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::time::Duration;

/// Bounded bidirectional LSP pump used by Plain-client integration tests.
pub struct ClientPump {
    pub server: TestServer,
    pub editor: FakeEditor,
    replies: HashMap<i64, Value>,
    pub cancel_on_edit: bool,
    strict: bool,
    capabilities: Value,
    /// Rich rendered state, present when initialize declares the rich protocol.
    pub rich: Option<super::editor_model::rich::RichEditor>,
    resyncs: HashMap<i64, String>,
    prompt_responds: HashMap<i64, String>,
    prompt_inputs: VecDeque<super::editor_model::prompts::PromptInput>,
}
impl ClientPump {
    fn initialized(server: TestServer, capabilities: Value, editor: FakeEditor) -> Self {
        let mut params = crate::stdio::initialize_message()["params"].take();
        params["capabilities"] = capabilities;
        Self::initialized_params(server, params, editor)
    }
    fn initialized_params(mut server: TestServer, params: Value, mut editor: FakeEditor) -> Self {
        let capabilities = params["capabilities"].clone();
        let init = server.initialize_params(params);
        assert!(init["result"]["capabilities"].is_object());
        editor.encoding = match init["result"]["capabilities"]["positionEncoding"]
            .as_str()
            .unwrap_or("utf-16")
        {
            "utf-8" => "utf-8",
            "utf-16" => "utf-16",
            "utf-32" => "utf-32",
            other => panic!("invalid negotiated encoding: {other}"),
        };
        let rich = capabilities
            .get("experimental")
            .and_then(|v| v.get("magit"))
            .is_some();
        Self {
            server,
            editor,
            replies: Default::default(),
            cancel_on_edit: false,
            strict: false,
            capabilities,
            rich: rich.then(Default::default),
            resyncs: Default::default(),
            prompt_responds: Default::default(),
            prompt_inputs: Default::default(),
        }
    }
    /// Start a client with custom initialize capabilities.
    pub fn new(mut server: TestServer, profile: Profile, capabilities: Value) -> Self {
        Self::initialized(server, capabilities, FakeEditor::new(profile))
    }
    /// Start a client using declarations composed from an editor model.
    pub fn with_model(server: TestServer, profile: Profile, model: &EditorModel) -> Self {
        let resolution = magit_lsp::quirks::Resolution {
            enabled: Default::default(),
        };
        let mut pump = Self::initialized(
            server,
            model.capabilities(),
            FakeEditor::from_model(profile, model, &resolution),
        );
        pump.strict = model.strict;
        pump
    }
    /// Start a modeled client with additional initialize parameters such as roots.
    pub fn with_model_params(
        server: TestServer,
        profile: Profile,
        model: &EditorModel,
        mut params: Value,
    ) -> Self {
        params["capabilities"] = model.capabilities();
        let resolution = magit_lsp::quirks::Resolution {
            enabled: Default::default(),
        };
        let mut pump = Self::initialized_params(
            server,
            params,
            FakeEditor::from_model(profile, model, &resolution),
        );
        pump.strict = model.strict;
        pump
    }
    /// Start a client from composed model declarations and resolved quirk behavior.
    pub fn with_resolution(
        server: TestServer,
        profile: Profile,
        model: &EditorModel,
        resolution: &magit_lsp::quirks::Resolution,
    ) -> Self {
        let mut pump = Self::initialized(
            server,
            model.capabilities(),
            FakeEditor::from_model(profile, model, resolution),
        );
        pump.strict = model.strict;
        pump
    }
    /// Send a message, assigning every request a unique ID.
    pub fn send(&mut self, mut message: Value) -> i64 {
        let id = if message.get("id").is_some() {
            let id = allocate_request_id();
            message["id"] = json!(id);
            id
        } else {
            0
        };
        self.server.send(message);
        id
    }
    /// Route an incoming server request through fake editor.
    pub fn dispatch(&mut self, request: &Value) {
        self.validate(request);
        self.dispatch_validated(request);
    }
    fn dispatch_validated(&mut self, request: &Value) {
        for frame in self.editor.dispatch(request) {
            self.server.send(frame);
        }
    }
    fn cancel_pending_edit(&mut self) {
        if !self.cancel_on_edit {
            return;
        }
        let token = self
            .server
            .notifications
            .iter()
            .rev()
            .find(|m| m["method"] == "$/progress" && m["params"]["value"]["kind"] == "begin")
            .expect("commit began")["params"]["token"]
            .clone();
        self.send(json!({"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":token}}));
        self.cancel_on_edit = false;
    }
    fn validate(&self, message: &Value) {
        if self.strict {
            super::editor_model::strict::check(&self.capabilities, message)
                .unwrap_or_else(|e| panic!("{e}"));
        }
    }
    /// Queue one answer for the next incoming scripted prompt.
    pub fn queue_prompt(&mut self, input: super::editor_model::prompts::PromptInput) {
        self.prompt_inputs.push_back(input);
    }
    /// Route a declared prompt only when scripted input is queued.
    pub fn route_scripted_prompt(&mut self, message: &Value) -> Option<i64> {
        self.validate(message);
        if message["method"] != "view/prompt" || self.prompt_inputs.is_empty() {
            return None;
        }
        let input = self.prompt_inputs.pop_front().unwrap();
        let params: magit_lsp::protocol::ViewPromptParams =
            serde_json::from_value(message["params"].clone()).expect("valid prompt");
        let outcome = super::editor_model::prompts::answer(&params, &input);
        let id = allocate_request_id();
        assert!(self
            .prompt_responds
            .insert(id, params.prompt_id.clone())
            .is_none());
        let mut response = json!({"jsonrpc":"2.0","id":id,"method":"view/promptRespond","params":{"promptId":params.prompt_id,"cancelled":outcome.answer["cancelled"]}});
        if !outcome.answer["cancelled"].as_bool().unwrap() {
            response["params"]["value"] = outcome.answer["value"].clone();
        }
        self.server.send(response);
        Some(id)
    }
    /// Read and route one server message.
    pub fn step(&mut self) {
        let message = self.server.recv();
        self.validate(&message);
        if message["method"] == "view/prompt" && self.route_scripted_prompt(&message).is_some() {
            return;
        }
        if message["method"] == "view/render" {
            if self.rich.is_none() {
                self.server.notifications.push(message);
                return;
            }
            let render: magit_lsp::protocol::ViewRender =
                serde_json::from_value(message["params"].clone()).expect("valid rich render");
            let view_id = render.view_id.clone();
            match self.rich.as_mut().expect("rich render requires rich model").apply(render) {
                super::editor_model::rich::Intent::Ack(version) => self.server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view_id,"version":version}})),
                super::editor_model::rich::Intent::Resync => {
                    let id = allocate_request_id();
                    assert!(self.resyncs.insert(id, view_id.clone()).is_none());
                    self.server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/resync","params":{"viewId":view_id}}));
                }
            }
            self.server.notifications.push(message);
            return;
        }
        if message["method"].is_string() && !message["id"].is_null() {
            let responses = self.editor.dispatch(&message);
            for response in responses.iter().cloned() {
                self.server.send(response);
            }
            if message["method"] == "window/showDocument"
                && message["params"]["uri"]
                    .as_str()
                    .is_some_and(|uri| uri.ends_with("/.git/COMMIT_EDITMSG"))
                && responses
                    .iter()
                    .any(|response| response["result"]["success"] == true)
            {
                self.cancel_pending_edit();
            }
            return;
        }
        if let Some(id) = message["id"].as_i64() {
            if let Some(prompt_id) = self.prompt_responds.remove(&id) {
                assert!(
                    message["error"].is_null(),
                    "promptRespond {prompt_id} failed: {message}"
                );
                assert_eq!(
                    message["result"],
                    Value::Null,
                    "promptRespond {prompt_id} must return null: {message}"
                );
                assert!(
                    self.replies.insert(id, message).is_none(),
                    "duplicate prompt response"
                );
                return;
            }
            if let Some(view_id) = self.resyncs.remove(&id) {
                let render: magit_lsp::protocol::ViewRender =
                    serde_json::from_value(message["result"].clone()).expect("valid resync render");
                assert_eq!(render.view_id, view_id);
                let version = match self.rich.as_mut().expect("rich model").apply(render) {
                    super::editor_model::rich::Intent::Ack(version) => version,
                    super::editor_model::rich::Intent::Resync => {
                        panic!("full resync response was stale")
                    }
                };
                self.server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view_id,"version":version}}));
                return;
            }
            assert!(
                self.replies.insert(id, message).is_none(),
                "duplicate response"
            );
            return;
        }
        if self.cancel_on_edit
            && ((message["method"] == "view/effects"
                && message["params"]["effects"]
                    .as_array()
                    .is_some_and(|e| e.iter().any(|e| e["type"] == "editDocument")))
                || (message["method"] == "window/showMessage"
                    && message["params"]["message"]
                        .as_str()
                        .is_some_and(|s| s.contains("/.git/COMMIT_EDITMSG"))))
        {
            self.cancel_pending_edit();
        }
        self.server.notifications.push(message);
    }
    /// Pump until condition or bounded timeout.
    pub fn until(&mut self, condition: impl Fn(&Self) -> bool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while !condition(self) {
            assert!(
                std::time::Instant::now() < deadline,
                "pump condition timed out"
            );
            self.step();
        }
    }
    /// Wait for and remove response by request ID.
    pub fn until_response(&mut self, id: i64) -> Value {
        self.until(|c| c.replies.contains_key(&id));
        self.replies.remove(&id).unwrap()
    }
    /// Return IDs of outstanding scripted prompt response requests.
    pub fn prompt_response_ids(&self) -> Vec<i64> {
        self.prompt_responds.keys().copied().collect()
    }
    /// Shut down server within bounded time.
    pub fn finish(mut self) {
        let id = self.send(json!({"jsonrpc":"2.0","id":0,"method":"shutdown","params":null}));
        assert_eq!(self.until_response(id)["result"], Value::Null);
        self.send(json!({"jsonrpc":"2.0","method":"exit","params":null}));
        assert!(self.server.finish(Duration::from_secs(10)).success());
    }
}
