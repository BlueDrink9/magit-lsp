//! Standard-LSP document presentation synchronization.

use crate::docstore::Encoding;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;

/// Lifecycle of a Plain document controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentState {
    /// No editor document is currently attached.
    Detached,
    /// The editor is synchronized and an edit may be sent.
    Ready,
    /// One workspace/applyEdit request is awaiting reconciliation.
    Applying,
}

/// One server-initiated edit request for `workspace/applyEdit`.
#[derive(Clone, Debug, PartialEq)]
pub struct ApplyEdit {
    /// Monotonic controller-local identifier used to discard late responses.
    pub id: u64,
    /// Standard LSP `WorkspaceEdit` payload.
    pub edit: Value,
}

/// Immutable line ending and EOF convention captured from the accepted didOpen text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocumentFormat {
    /// Whether serialized rows use CRLF rather than LF separators.
    pub crlf: bool,
    /// Whether serialized text ends with the captured final line terminator.
    pub final_terminator: bool,
}

impl DocumentFormat {
    /// Infer the document's newline convention without changing its accepted text.
    pub fn from_text(text: &str) -> Self {
        let crlf = text.contains("\r\n");
        Self { crlf, final_terminator: text.ends_with('\n') }
    }

    /// Serialize structured Tree rows using this document's immutable newline convention.
    pub fn serialize(self, tree: &crate::tree::Tree) -> String {
        let separator = if self.crlf { "\r\n" } else { "\n" };
        let mut text = tree.lines.join(separator);
        if self.final_terminator { text.push_str(separator); }
        text
    }
}

#[derive(Clone, Debug)]
struct Snapshot {
    hash: [u8; 32],
    text: String,
    version: i32,
}

#[derive(Clone, Debug)]
struct ActionSnapshot {
    hash: [u8; 32],
    view_id: String,
    generation: u64,
    scope: String,
    tree: crate::tree::Tree,
}

#[derive(Clone, Debug)]
struct InFlight {
    request: ApplyEdit,
    target: Snapshot,
    echoed: bool,
    diverged: bool,
    response: Option<bool>,
}

/// Serializes authoritative rendered text into standard workspace edits.
///
/// The controller owns synchronization state only; callers supply document
/// notifications and transport responses and send returned edits themselves.
pub struct PlainDocument {
    uri: String,
    encoding: Encoding,
    format: DocumentFormat,
    document_changes: bool,
    state: DocumentState,
    authoritative: Option<Snapshot>,
    client: Option<Snapshot>,
    snapshots: VecDeque<Snapshot>,
    action_snapshots: VecDeque<ActionSnapshot>,
    snapshot_limit: usize,
    in_flight: Option<InFlight>,
    /// A rejected target is not retried until the client or render advances.
    rejected: Option<([u8; 32], [u8; 32])>,
    next_id: u64,
}

impl PlainDocument {
    /// Create a detached controller for one document URI.
    pub fn new(uri: impl Into<String>, encoding: Encoding, document_changes: bool, format: DocumentFormat) -> Self {
        Self {
            uri: uri.into(), encoding, document_changes, format,
            state: DocumentState::Detached, authoritative: None, client: None,
            snapshots: VecDeque::new(), snapshot_limit: 6, in_flight: None, rejected: None, next_id: 0,
            action_snapshots: VecDeque::new(),
        }
    }

    /// Return the current lifecycle state.
    pub fn state(&self) -> DocumentState { self.state }

    /// Return a snapshot text when its exact-text hash is retained.
    pub fn snapshot(&self, hash: &[u8; 32]) -> Option<(&str, i32)> {
        self.snapshots.iter().find(|s| &s.hash == hash).map(|s| (s.text.as_str(), s.version))
    }

    /// Record an authoritative render and start an edit if the client is ready.
    pub fn render(&mut self, text: impl Into<String>) -> Option<ApplyEdit> {
        self.authoritative = Some(self.snapshot_for(text.into(), 0));
        self.rejected = None;
        self.prepare()
    }

    /// Retain the exact structured snapshot associated with an authoritative render.
    pub fn render_actions(&mut self, tree: crate::tree::Tree, view_id: String, generation: u64, scope: String) -> Option<ApplyEdit> {
        let text = self.format.serialize(&tree);
        let hash = hash_text(&text);
        self.action_snapshots.retain(|snapshot| snapshot.hash != hash);
        self.action_snapshots.push_back(ActionSnapshot { hash, view_id, generation, scope, tree });
        while self.action_snapshots.len() > self.snapshot_limit { self.action_snapshots.pop_front(); }
        self.render(text)
    }

    /// Find a retained action snapshot only when its exact document text matches.
    pub fn action_snapshot(&self, text: &str) -> Option<(&crate::tree::Tree, &str, u64, &str)> {
        let hash = hash_text(text);
        self.action_snapshots.iter().find(|snapshot| snapshot.hash == hash)
            .map(|snapshot| (&snapshot.tree, snapshot.view_id.as_str(), snapshot.generation, snapshot.scope.as_str()))
    }

    /// Attach/reopen the document with its exact current editor snapshot.
    pub fn open(&mut self, version: i32, text: impl Into<String>) -> Option<ApplyEdit> {
        self.in_flight = None;
        self.client = Some(self.snapshot_for(text.into(), version));
        self.rejected = None;
        self.state = DocumentState::Ready;
        self.prepare()
    }

    /// Observe a didChange notification, reconciling an exact applyEdit echo.
    pub fn change(&mut self, version: i32, text: impl Into<String>) -> Option<ApplyEdit> {
        if self.state == DocumentState::Detached { return None; }
        let text = text.into();
        let observed = self.snapshot_for(text, version);
        self.client = Some(observed.clone());
        self.rejected = None;
        if let Some(flight) = self.in_flight.as_mut() {
            flight.echoed = flight.target.hash == observed.hash;
            flight.diverged |= !flight.echoed;
            if flight.response == Some(true) {
                self.in_flight = None;
                self.state = DocumentState::Ready;
            }
            return self.prepare();
        }
        self.state = DocumentState::Ready;
        self.prepare()
    }

    /// Resolve an applyEdit result; late responses after close/reopen are ignored.
    pub fn response(&mut self, id: u64, applied: bool) -> Option<ApplyEdit> {
        let Some(flight) = self.in_flight.as_mut() else { return None; };
        if flight.request.id != id { return None; }
        flight.response = Some(applied);
        if !applied {
            if !flight.diverged {
                self.rejected = Some((flight.target.hash, self.client.as_ref().map_or([0; 32], |client| client.hash)));
            }
            self.in_flight = None;
            self.state = DocumentState::Ready;
            return self.prepare();
        }
        if flight.echoed || flight.diverged {
            self.in_flight = None;
            self.state = DocumentState::Ready;
            return self.prepare();
        }
        None
    }

    /// Detach and invalidate pending responses so no late result can resurrect it.
    pub fn close(&mut self) {
        self.state = DocumentState::Detached;
        self.client = None;
        self.in_flight = None;
        self.rejected = None;
    }

    fn snapshot_for(&mut self, text: String, version: i32) -> Snapshot {
        let hash = hash_text(&text);
        let snapshot = Snapshot { hash, text, version };
        self.snapshots.retain(|s| s.hash != hash);
        self.snapshots.push_back(snapshot.clone());
        while self.snapshots.len() > self.snapshot_limit { self.snapshots.pop_front(); }
        snapshot
    }

    fn prepare(&mut self) -> Option<ApplyEdit> {
        if self.state == DocumentState::Detached || self.in_flight.is_some() { return None; }
        let (Some(authoritative), Some(client)) = (&self.authoritative, &self.client) else { return None; };
        if authoritative.hash == client.hash {
            self.state = DocumentState::Ready;
            return None;
        }
        if self.rejected == Some((authoritative.hash, client.hash)) {
            self.state = DocumentState::Ready;
            return None;
        }
        self.next_id += 1;
        let edits = text_edits(&client.text, &authoritative.text, self.encoding);
        let edit = if self.document_changes {
            json!({"documentChanges":[{"textDocument":{"uri":self.uri,"version":client.version},"edits":edits}]})
        } else {
            json!({"changes":{self.uri.clone():edits}})
        };
        let request = ApplyEdit { id: self.next_id, edit };
        let target = authoritative.clone();
        self.in_flight = Some(InFlight { request: request.clone(), target, echoed: false, diverged: false, response: None });
        self.state = DocumentState::Applying;
        Some(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::Tree;

    fn tree(text: &str) -> Tree {
        Tree { version: 0, lines: vec![text.to_owned()], nodes: vec![], line_nodes: vec![], tokens: vec![], symbols: vec![], links: vec![] }
    }

    #[test]
    fn action_snapshot_retention_is_bounded_and_keeps_generation() {
        let mut document = PlainDocument::new("file:///test", Encoding::Utf16, false, DocumentFormat::from_text(""));
        for generation in 0..7 {
            document.render_actions(tree(&format!("render-{generation}")), "view".into(), generation, "scope".into());
        }
        assert!(document.action_snapshot("render-0").is_none());
        let (_, _, generation, _) = document.action_snapshot("render-6").unwrap();
        assert_eq!(generation, 6);
    }
}

/// Hash exact UTF-8 text for collision-resistant snapshot matching.
pub fn hash_text(text: &str) -> [u8; 32] { Sha256::digest(text.as_bytes()).into() }

/// Produce a single minimal valid edit using negotiated LSP position units.
pub fn text_edits(old: &str, new: &str, encoding: Encoding) -> Vec<Value> {
    if old == new { return Vec::new(); }
    let mut prefix = old.bytes().zip(new.bytes()).take_while(|(a,b)| a == b).count();
    while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) { prefix -= 1; }
    while splits_crlf(old, prefix) || splits_crlf(new, prefix) { prefix -= 1; }
    let mut suffix = 0;
    let max_suffix = old.len().saturating_sub(prefix).min(new.len().saturating_sub(prefix));
    while suffix < max_suffix && old.as_bytes()[old.len()-1-suffix] == new.as_bytes()[new.len()-1-suffix] { suffix += 1; }
    while !old.is_char_boundary(old.len()-suffix) || !new.is_char_boundary(new.len()-suffix) { suffix -= 1; }
    while splits_crlf(old, old.len()-suffix) || splits_crlf(new, new.len()-suffix) { suffix -= 1; }
    let old_end = old.len() - suffix;
    let new_end = new.len() - suffix;
    let start = position(old, prefix, encoding);
    let end = position(old, old_end, encoding);
    vec![json!({"range":{"start":start,"end":end},"newText":&new[prefix..new_end]})]
}

fn splits_crlf(text: &str, offset: usize) -> bool {
    offset > 0 && offset < text.len()
        && text.as_bytes()[offset - 1] == b'\r'
        && text.as_bytes()[offset] == b'\n'
}

fn position(text: &str, offset: usize, encoding: Encoding) -> Value {
    let before = &text[..offset];
    let line = before.bytes().filter(|b| *b == b'\n').count();
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let segment = &before[line_start..];
    let segment = segment.strip_suffix('\r').unwrap_or(segment);
    let character = match encoding {
        Encoding::Utf8 => segment.len(),
        Encoding::Utf16 => segment.encode_utf16().count(),
        Encoding::Utf32 => segment.chars().count(),
    };
    json!({"line":line,"character":character})
}
