use crate::fake_editor::{allocate_request_id, FakeEditor, Profile};
use crate::stdio::TestServer;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

/// Bounded bidirectional LSP pump used by Plain-client integration tests.
pub struct ClientPump {
    pub server: TestServer,
    pub editor: FakeEditor,
    replies: HashMap<i64, Value>,
    pub cancel_on_edit: bool,
}
impl ClientPump {
    /// Start a client with custom initialize capabilities.
    pub fn new(mut server: TestServer, profile: Profile, capabilities: Value) -> Self {
        let init=server.initialize(capabilities);
        assert!(init["result"]["capabilities"].is_object());
        let mut editor = FakeEditor::new(profile);
        editor.encoding = match init["result"]["capabilities"]["positionEncoding"]
            .as_str().unwrap_or("utf-16") {
            "utf-8" => "utf-8", "utf-16" => "utf-16", "utf-32" => "utf-32",
            other => panic!("invalid negotiated encoding: {other}"),
        };
        Self { server, editor, replies: Default::default(), cancel_on_edit: false }
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
    pub fn dispatch(&mut self, request: &Value) { for frame in self.editor.dispatch(request) { self.server.send(frame); } }
    /// Read and route one server message.
    pub fn step(&mut self) {
        let message = self.server.recv();
        if message["method"].is_string() && !message["id"].is_null() { self.dispatch(&message); return; }
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
