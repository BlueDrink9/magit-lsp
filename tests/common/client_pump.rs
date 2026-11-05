use crate::fake_editor::{allocate_request_id, FakeEditor, Profile};
use crate::stdio::TestServer;
use crate::editor_model::EditorModel;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

/// Bounded bidirectional LSP pump used by Plain-client integration tests.
pub struct ClientPump {
    pub server: TestServer,
    pub editor: FakeEditor,
    replies: HashMap<i64, Value>,
    pub cancel_on_edit: bool,
    strict: bool,
    capabilities: Value,
}
impl ClientPump {
    fn initialized(mut server: TestServer, capabilities: Value, mut editor: FakeEditor) -> Self {
        let init = server.initialize(capabilities.clone());
        assert!(init["result"]["capabilities"].is_object());
        editor.encoding = match init["result"]["capabilities"]["positionEncoding"].as_str().unwrap_or("utf-16") {
            "utf-8" => "utf-8", "utf-16" => "utf-16", "utf-32" => "utf-32",
            other => panic!("invalid negotiated encoding: {other}"),
        };
        Self { server, editor, replies: Default::default(), cancel_on_edit: false, strict: false, capabilities }
    }
    /// Start a client with custom initialize capabilities.
    pub fn new(mut server: TestServer, profile: Profile, capabilities: Value) -> Self {
        Self::initialized(server, capabilities, FakeEditor::new(profile))
    }
    /// Start a client using declarations composed from an editor model.
    pub fn with_model(server: TestServer, profile: Profile, model: &EditorModel) -> Self {
        let resolution = magit_lsp::quirks::Resolution { enabled: Default::default() };
        let mut pump = Self::initialized(server, model.capabilities(), FakeEditor::from_model(profile, model, &resolution));
        pump.strict = model.strict;
        pump
    }
    /// Start a client from composed model declarations and resolved quirk behavior.
    pub fn with_resolution(server: TestServer, profile: Profile, model: &EditorModel, resolution: &magit_lsp::quirks::Resolution) -> Self {
        let mut pump = Self::initialized(server, model.capabilities(), FakeEditor::from_model(profile, model, resolution));
        pump.strict = model.strict;
        pump
    }
    /// Send a message, assigning every request a unique ID.
    pub fn send(&mut self, mut message: Value) -> i64 {
        let id = if message.get("id").is_some() {
            let id = allocate_request_id();
            message["id"] = json!(id);
            id
        } else { 0 };
        self.server.send(message);
        id
    }
    /// Route an incoming server request through fake editor.
    pub fn dispatch(&mut self, request: &Value) { self.validate(request); self.dispatch_validated(request); }
    fn dispatch_validated(&mut self, request: &Value) { for frame in self.editor.dispatch(request) { self.server.send(frame); } }
    fn validate(&self, message: &Value) {
        if self.strict { super::editor_model::strict::check(&self.capabilities, message).unwrap_or_else(|e| panic!("{e}")); }
    }
    /// Read and route one server message.
    pub fn step(&mut self) {
        let message = self.server.recv();
        self.validate(&message);
        if message["method"].is_string() && !message["id"].is_null() { self.dispatch_validated(&message); return; }
        if let Some(id) = message["id"].as_i64() {
            assert!(self.replies.insert(id, message).is_none(), "duplicate response"); return;
        }
        if self.cancel_on_edit && message["method"] == "view/effects" && message["params"]["effects"].as_array().is_some_and(|e| e.iter().any(|e| e["type"] == "editDocument")) {
            let token = self.server.notifications.iter().rev().find(|m| m["method"] == "$/progress" && m["params"]["value"]["kind"] == "begin").expect("commit began")["params"]["token"].clone();
            self.send(json!({"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":token}}));
            self.cancel_on_edit = false;
        }
        self.server.notifications.push(message);
    }
    /// Pump until condition or bounded timeout.
    pub fn until(&mut self, condition: impl Fn(&Self) -> bool) {
        let deadline=std::time::Instant::now()+Duration::from_secs(15);
        while !condition(self) { assert!(std::time::Instant::now()<deadline,"pump condition timed out"); self.step(); }
    }
    /// Wait for and remove response by request ID.
    pub fn until_response(&mut self, id: i64) -> Value { self.until(|c|c.replies.contains_key(&id)); self.replies.remove(&id).unwrap() }
    /// Shut down server within bounded time.
    pub fn finish(mut self) {
        let id=self.send(json!({"jsonrpc":"2.0","id":0,"method":"shutdown","params":null}));
        assert_eq!(self.until_response(id)["result"],Value::Null);
        self.send(json!({"jsonrpc":"2.0","method":"exit","params":null}));
        assert!(self.server.finish(Duration::from_secs(10)).success());
    }
}
