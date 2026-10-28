//! Scripted LSP editor model used to exercise Plain-client quirks.

use serde_json::{json, Value};
use std::collections::HashMap;

/// Client compatibility behavior toggled by a conformance test.
#[derive(Clone, Copy, Debug)]
pub enum Profile { Standard, DecomposedChanges, DebouncedDidChange, ReorderEcho, NoDocumentChanges, NoShowDocument, CrlfNormalise, Utf16 }

/// Open editor document state.
#[derive(Clone, Debug)]
pub struct Document { pub text: String, pub version: i64 }

/// Scriptable client-side editor model, independent of production server behavior.
pub struct FakeEditor { pub documents: HashMap<String, Document>, pub profile: Profile, pub encoding: &'static str, pending: Vec<Value>, pub outbound: Vec<Value> }

impl FakeEditor {
    /// Create a client model with the requested compatibility profile.
    pub fn new(profile: Profile) -> Self { Self { documents: HashMap::new(), profile, encoding: if matches!(profile, Profile::Utf16) { "utf-16" } else { "utf-16" }, pending: Vec::new(), outbound: Vec::new() } }
    /// Open a document and return its didOpen notification.
    pub fn open(&mut self, uri: &str, text: &str) -> Value { self.documents.insert(uri.into(), Document { text: text.into(), version: 1 }); json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"plaintext","version":1,"text":text}}}) }
    /// Replace document text; debounce profile queues the notification until flush.
    pub fn edit(&mut self, uri: &str, text: &str) -> Value { let d=self.documents.get_mut(uri).expect("open document"); d.version+=1; d.text=text.into(); let m=json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":d.version},"contentChanges":[{"text":text}]}}); if matches!(self.profile,Profile::DebouncedDidChange) { self.pending.push(m.clone()); } m }
    /// Release deferred didChange notifications.
    pub fn flush(&mut self) -> Vec<Value> { std::mem::take(&mut self.pending) }
    /// Return the current editor text for a URI.
    pub fn text(&self, uri: &str) -> &str { &self.documents.get(uri).expect("open document").text }
    /// Create a save notification.
    pub fn save(&self, uri: &str) -> Value { json!({"jsonrpc":"2.0","method":"textDocument/didSave","params":{"textDocument":{"uri":uri}}}) }
    /// Close an open document.
    pub fn close(&mut self, uri: &str) -> Value { self.documents.remove(uri); json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri}}}) }
    /// Send a code-action request for a range.
    pub fn code_action(&self, uri: &str, range: Value) -> Value { json!({"jsonrpc":"2.0","id":71,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":range,"context":{"diagnostics":[]}}}) }
    /// Send an execute-command request.
    pub fn execute_command(&self, command: &str, arguments: Value) -> Value { json!({"jsonrpc":"2.0","id":72,"method":"workspace/executeCommand","params":{"command":command,"arguments":arguments}}) }
    /// Apply an entire workspace edit atomically, rejecting stale versions and invalid ranges.
    pub fn apply_edit(&mut self, edit: &Value) -> Value {
        let mut replacements=Vec::new();
        if let Some(changes)=edit["documentChanges"].as_array() { for change in changes { let uri=change["textDocument"]["uri"].as_str().unwrap_or(""); let d=match self.documents.get(uri){Some(d)=>d,None=>return json!({"applied":false,"failureReason":"document is not open"})}; if change["textDocument"]["version"].as_i64().is_some_and(|v|v!=d.version){return json!({"applied":false,"failureReason":"document version is stale"})} let Some(edits)=change["edits"].as_array() else {return json!({"applied":false,"failureReason":"missing edits"})}; let mut text=d.text.clone(); for e in edits.iter().rev(){let Some(start)=offset(&text,&e["range"]["start"],self.encoding) else{return json!({"applied":false,"failureReason":"invalid range"})}; let Some(end)=offset(&text,&e["range"]["end"],self.encoding) else{return json!({"applied":false,"failureReason":"invalid range"})}; if start>end{return json!({"applied":false,"failureReason":"invalid range"})} text.replace_range(start..end,e["newText"].as_str().unwrap_or(""));} replacements.push((uri.to_owned(),text)); } }
        else if let Some(changes)=edit["changes"].as_object(){ for (uri,edits) in changes { let Some(d)=self.documents.get(uri) else{return json!({"applied":false,"failureReason":"document is not open"})}; let mut text=d.text.clone(); for e in edits.as_array().into_iter().flatten().rev(){let Some(a)=offset(&text,&e["range"]["start"],self.encoding) else{return json!({"applied":false,"failureReason":"invalid range"})};let Some(b)=offset(&text,&e["range"]["end"],self.encoding) else{return json!({"applied":false,"failureReason":"invalid range"})};if a>b{return json!({"applied":false,"failureReason":"invalid range"})}text.replace_range(a..b,e["newText"].as_str().unwrap_or(""));} replacements.push((uri.clone(),text)); } }
        for (uri,text) in replacements {let d=self.documents.get_mut(&uri).unwrap();d.text=if matches!(self.profile,Profile::CrlfNormalise){text.replace("\r\n","\n")}else{text};d.version+=1;} json!({"applied":true})
    }
    /// Respond to supported server requests while retaining outbound order for assertions.
    pub fn respond(&mut self, request: &Value) -> Value { self.outbound.push(request.clone()); match request["method"].as_str().unwrap_or("") {"window/showDocument"=>json!({"jsonrpc":"2.0","id":request["id"],"result":{"success":!matches!(self.profile,Profile::NoShowDocument)}}),"workDoneProgress/create"=>json!({"jsonrpc":"2.0","id":request["id"],"result":null}),_=>json!({"jsonrpc":"2.0","id":request["id"],"result":null})} }
}

fn offset(text:&str, position:&Value, encoding:&str)->Option<usize>{let line=position["line"].as_u64()? as usize;let character=position["character"].as_u64()? as usize;let start=if line==0{0}else{text.match_indices('\n').nth(line-1)?.0+1};let end=text[start..].find('\n').map(|n|start+n).unwrap_or(text.len());let slice=&text[start..end];let mut units=0;for (i,c) in slice.char_indices(){if units==character{return Some(start+i)}units+=match encoding{"utf-8"=>c.len_utf8(),"utf-32"=>1,_=>c.len_utf16()};if units>character{return None}}(units==character).then_some(end)}
