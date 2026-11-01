//! Scripted LSP editor model used to exercise Plain-client quirks.

use serde_json::{json, Value};
use std::collections::HashMap;

/// Client compatibility behavior toggled by a conformance test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Standard,
    DecomposedChanges,
    DebouncedDidChange,
    ReorderEcho,
    NoDocumentChanges,
    NoShowDocument,
    CrlfNormalise,
    Utf16,
}

/// Open editor document state.
#[derive(Clone, Debug)]
pub struct Document {
    pub text: String,
    pub version: i64,
}

/// Scriptable client-side editor model, independent of production server behavior.
pub struct FakeEditor {
    pub documents: HashMap<String, Document>,
    pub profile: Profile,
    pub encoding: &'static str,
    pending: Vec<Value>,
    pub outbound: Vec<Value>,
}

impl FakeEditor {
    /// Create a client model with the requested compatibility profile.
    pub fn new(profile: Profile) -> Self {
        Self {
            documents: HashMap::new(),
            profile,
            encoding: "utf-16",
            pending: Vec::new(),
            outbound: Vec::new(),
        }
    }
    /// Open a document and return its didOpen notification.
    pub fn open(&mut self, uri: &str, text: &str) -> Value {
        self.documents.insert(
            uri.into(),
            Document {
                text: text.into(),
                version: 1,
            },
        );
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"plaintext","version":1,"text":text}}})
    }
    /// Replace document text; debounce profile queues the notification until flush.
    pub fn edit(&mut self, uri: &str, text: &str) -> Value {
        let d = self.documents.get_mut(uri).expect("open document");
        let old = d.text.clone();
        d.version += 1;
        d.text = text.into();
        let changes = if matches!(self.profile, Profile::DecomposedChanges) {
            decomposed(&old, text, self.encoding)
        } else {
            vec![json!({"text":text})]
        };
        let m = json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":d.version},"contentChanges":changes}});
        if matches!(self.profile, Profile::DebouncedDidChange) {
            self.pending.push(m.clone());
        }
        m
    }
    /// Release deferred didChange notifications.
    pub fn flush(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.pending)
    }
    /// Return whether a debounced change is waiting to be sent.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
    /// Return the current editor text for a URI.
    pub fn text(&self, uri: &str) -> &str {
        &self.documents.get(uri).expect("open document").text
    }
    /// Create a save notification.
    pub fn save(&self, uri: &str) -> Value {
        json!({"jsonrpc":"2.0","method":"textDocument/didSave","params":{"textDocument":{"uri":uri}}})
    }
    /// Close an open document.
    pub fn close(&mut self, uri: &str) -> Value {
        self.documents.remove(uri);
        json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri}}})
    }
    /// Send a code-action request for a range.
    pub fn code_action(&self, uri: &str, range: Value) -> Value {
        json!({"jsonrpc":"2.0","id":71,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":range,"context":{"diagnostics":[]}}})
    }
    /// Send an execute-command request.
    pub fn execute_command(&self, command: &str, arguments: Value) -> Value {
        json!({"jsonrpc":"2.0","id":72,"method":"workspace/executeCommand","params":{"command":command,"arguments":arguments}})
    }
    /// Apply an entire workspace edit atomically, rejecting stale versions and invalid ranges.
    pub fn apply_edit(&mut self, edit: &Value) -> Value {
        if !edit.is_object()
            || (edit.get("documentChanges").is_some() && edit.get("changes").is_some())
        {
            return rejected("malformed workspace edit");
        }
        if edit.get("documentChanges").is_some()
            && matches!(self.profile, Profile::NoDocumentChanges)
        {
            return rejected("documentChanges is unsupported");
        }
        if edit.get("documentChanges").is_some_and(|v| !v.is_array()) {
            return rejected("malformed documentChanges");
        }
        let Some(changes) = edit["documentChanges"].as_array() else {
            let Some(changes) = edit["changes"].as_object() else {
                return rejected("malformed workspace edit");
            };
            let mut replacements = Vec::new();
            for (uri, edits) in changes {
                let Some(doc) = self.documents.get(uri) else {
                    return rejected("document is not open");
                };
                let Some(edits) = edits.as_array() else {
                    return rejected("malformed edits");
                };
                let Some(result) = apply_edits(&doc.text, edits, self.encoding) else {
                    return rejected("invalid or overlapping edits");
                };
                replacements.push((uri.clone(), result));
            }
            return self.commit(replacements);
        };
        let mut replacements = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for change in changes {
            if change.get("kind").is_some() {
                return rejected("resource operations are unsupported");
            }
            let Some(uri) = change["textDocument"]["uri"].as_str() else {
                return rejected("malformed document change");
            };
            if !seen.insert(uri) {
                return rejected("duplicate document change");
            }
            let Some(doc) = self.documents.get(uri) else {
                return rejected("document is not open");
            };
            let Some(version) = change["textDocument"].get("version") else {
                return rejected("missing version");
            };
            if !version.is_null() && version.as_i64() != Some(doc.version) {
                return rejected("document version is stale or malformed");
            }
            let Some(edits) = change["edits"].as_array() else {
                return rejected("missing edits");
            };
            let Some(result) = apply_edits(&doc.text, edits, self.encoding) else {
                return rejected("invalid or overlapping edits");
            };
            replacements.push((uri.to_owned(), result));
        }
        self.commit(replacements)
    }

    fn commit(&mut self, replacements: Vec<(String, String)>) -> Value {
        for (uri, text) in replacements {
            let d = self.documents.get_mut(&uri).unwrap();
            d.text = if matches!(self.profile, Profile::CrlfNormalise) {
                text.replace("\r\n", "\n")
            } else {
                text
            };
            d.version += 1;
        }
        json!({"applied":true})
    }
    /// Respond to supported server requests while retaining outbound order for assertions.
    pub fn respond(&mut self, request: &Value) -> Value {
        self.outbound.push(request.clone());
        match request["method"].as_str().unwrap_or("") {
            "workspace/applyEdit" => {
                json!({"jsonrpc":"2.0","id":request["id"],"result":self.apply_edit(&request["params"]["edit"])})
            }
            "window/showDocument" => {
                json!({"jsonrpc":"2.0","id":request["id"],"result":{"success":!matches!(self.profile,Profile::NoShowDocument)}})
            }
            "window/workDoneProgress/create" => {
                json!({"jsonrpc":"2.0","id":request["id"],"result":null})
            }
            _ => {
                json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32601,"message":"Method not found"}})
            }
        }
    }

    /// Dispatch a server request and return client-to-server frames in wire order.
    pub fn dispatch(&mut self, request: &Value) -> Vec<Value> {
        let before = self.documents.clone();
        let response = self.respond(request);
        let mut frames = vec![response];
        if request["method"] == "workspace/applyEdit" && frames[0]["result"]["applied"] == true {
            let mut uris: Vec<_> = self.documents.keys().cloned().collect();
            uris.sort();
            let mut echoes = Vec::new();
            for uri in uris {
                let doc = &self.documents[&uri];
                if doc.version != before[&uri].version {
                    let changes = if matches!(self.profile, Profile::DecomposedChanges) {
                        decomposed(&before[&uri].text, &doc.text, self.encoding)
                    } else {
                        vec![json!({"text":doc.text})]
                    };
                    let echo = json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":doc.version},"contentChanges":changes}});
                    if matches!(self.profile, Profile::DebouncedDidChange) {
                        self.pending.push(echo);
                    } else {
                        echoes.push(echo);
                    }
                }
            }
            if matches!(self.profile, Profile::ReorderEcho) {
                frames.extend(echoes);
            } else {
                echoes.extend(frames);
                frames = echoes;
            }
        }
        frames
    }
}

fn rejected(reason: &str) -> Value {
    json!({"applied":false,"failureReason":reason})
}
fn decomposed(old: &str, new: &str, encoding: &str) -> Vec<Value> {
    let line = old.bytes().filter(|b| *b == b'\n').count();
    let last = old.rsplit('\n').next().unwrap().trim_end_matches('\r');
    let character = match encoding {
        "utf-8" => last.len(),
        "utf-32" => last.chars().count(),
        _ => last.encode_utf16().count(),
    };
    let mut changes = vec![
        json!({"range":{"start":{"line":0,"character":0},"end":{"line":line,"character":character}},"text":""}),
    ];
    // Each insertion is relative to the preceding result, not the old snapshot.
    for (line, text) in new.split_inclusive('\n').enumerate() {
        changes.push(json!({"range":{"start":{"line":line,"character":0},"end":{"line":line,"character":0}},"text":text}));
    }
    changes
}
fn apply_edits(text: &str, edits: &[Value], encoding: &str) -> Option<String> {
    let mut ranges = Vec::new();
    for e in edits {
        let start = offset(text, &e["range"]["start"], encoding)?;
        let end = offset(text, &e["range"]["end"], encoding)?;
        let replacement = e["newText"].as_str()?.to_owned();
        if start > end {
            return None;
        }
        ranges.push((start, end, replacement));
    }
    ranges.sort_by_key(|(a, b, _)| (*a, *b));
    if ranges.windows(2).any(|w| w[0].1 > w[1].0) {
        return None;
    }
    let mut result = text.to_owned();
    for (a, b, repl) in ranges.into_iter().rev() {
        result.replace_range(a..b, &repl);
    }
    Some(result)
}
fn offset(text: &str, position: &Value, encoding: &str) -> Option<usize> {
    let line = position["line"].as_u64()? as usize;
    let character = position["character"].as_u64()? as usize;
    let start = if line == 0 {
        0
    } else {
        text.match_indices('\n').nth(line - 1)?.0 + 1
    };
    let end = text[start..]
        .find('\n')
        .map(|n| start + n)
        .unwrap_or(text.len());
    let slice = text
        .get(start..end)?
        .strip_suffix('\r')
        .unwrap_or(&text[start..end]);
    let mut units = 0;
    for (i, c) in slice.char_indices() {
        if units == character {
            return Some(start + i);
        }
        units += match encoding {
            "utf-8" => c.len_utf8(),
            "utf-32" => 1,
            _ => c.len_utf16(),
        };
        if units > character {
            return None;
        }
    }
    (units == character).then_some(start + slice.len())
}
