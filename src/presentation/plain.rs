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
        Self {
            crlf,
            final_terminator: text.ends_with('\n'),
        }
    }

    /// Serialize structured Tree rows using this document's immutable newline convention.
    pub fn serialize(self, tree: &crate::tree::Tree) -> String {
        let separator = if self.crlf { "\r\n" } else { "\n" };
        let mut text = tree.lines.join(separator);
        if self.final_terminator {
            text.push_str(separator);
        }
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
    menu: Option<crate::menu::Menu>,
}

#[derive(Clone, Debug)]
struct InFlight {
    request: ApplyEdit,
    target: Snapshot,
    echoed: bool,
    diverged: bool,
    response: Option<bool>,
    format_drift: bool,
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
    navigation: Option<([u8; 32], lsp_types::Range)>,
    /// A rejected target is not retried until the client or render advances.
    rejected: Option<([u8; 32], [u8; 32])>,
    next_id: u64,
    format_suspended: bool,
    warning_pending: bool,
}

impl PlainDocument {
    /// Create a detached controller for one document URI.
    pub fn new(
        uri: impl Into<String>,
        encoding: Encoding,
        document_changes: bool,
        format: DocumentFormat,
    ) -> Self {
        Self {
            uri: uri.into(),
            encoding,
            document_changes,
            format,
            state: DocumentState::Detached,
            authoritative: None,
            client: None,
            snapshots: VecDeque::new(),
            snapshot_limit: 6,
            in_flight: None,
            navigation: None,
            rejected: None,
            next_id: 0,
            action_snapshots: VecDeque::new(),
            format_suspended: false,
            warning_pending: false,
        }
    }

    /// Return the current lifecycle state.
    pub fn state(&self) -> DocumentState {
        self.state
    }

    /// Return a snapshot text when its exact-text hash is retained.
    pub fn snapshot(&self, hash: &[u8; 32]) -> Option<(&str, i32)> {
        self.snapshots
            .iter()
            .find(|s| &s.hash == hash)
            .map(|s| (s.text.as_str(), s.version))
    }

    /// Record server-authoritative rendered text and start an edit if the client is ready.
    pub fn render(&mut self, text: impl Into<String>) -> Option<ApplyEdit> {
        self.navigation = None;
        let snapshot = self.snapshot_for(text.into(), 0);
        if self
            .authoritative
            .as_ref()
            .is_none_or(|old| old.hash != snapshot.hash)
        {
            self.rejected = None;
        }
        self.authoritative = Some(snapshot);
        self.prepare()
    }

    /// Associate transition navigation with the current authoritative render.
    pub fn navigate_after_edit(&mut self, selection: lsp_types::Range) {
        self.navigation = self
            .authoritative
            .as_ref()
            .map(|target| (target.hash, selection));
    }

    /// Consume navigation only for a successful, still-current edit response.
    pub fn take_navigation(&mut self, id: u64, applied: bool) -> Option<lsp_types::Range> {
        let flight = self.in_flight.as_ref()?;
        if flight.request.id != id || flight.response.is_some() {
            return None;
        }
        let (hash, _) = self.navigation.as_ref()?;
        if *hash != flight.target.hash {
            return None;
        }
        let (_, selection) = self.navigation.take()?;
        (applied
            && !flight.format_drift
            && (!flight.diverged || flight.echoed)
            && self
                .authoritative
                .as_ref()
                .is_some_and(|target| target.hash == flight.target.hash))
        .then_some(selection)
    }

    /// Retain the exact structured snapshot associated with an authoritative render.
    pub fn render_actions(
        &mut self,
        tree: crate::tree::Tree,
        view_id: String,
        generation: u64,
        scope: String,
        menu: Option<crate::menu::Menu>,
    ) -> Option<ApplyEdit> {
        let text = self.retain_action(tree, view_id, generation, scope, menu);
        self.render(text)
    }

    /// Retain a structured snapshot without rendering or starting an edit.
    ///
    /// Used when a client edit is adopted as authoritative: the snapshot must
    /// resolve the edited text while no server render is pending.
    pub fn retain_actions(
        &mut self,
        tree: crate::tree::Tree,
        view_id: &str,
        generation: u64,
        scope: &str,
        menu: Option<crate::menu::Menu>,
    ) {
        let _ = self.retain_action(tree, view_id.to_owned(), generation, scope.to_owned(), menu);
    }

    /// Adopt client text as both the observed and authoritative snapshot.
    ///
    /// Any pending or rejected edit is discarded and the controller becomes
    /// ready, so a following `change` reports no divergence.
    pub fn adopt(&mut self, version: i32, text: String) {
        self.navigation = None;
        let snapshot = self.snapshot_for(text, version);
        self.authoritative = Some(snapshot.clone());
        self.client = Some(snapshot);
        self.in_flight = None;
        self.rejected = None;
        self.state = DocumentState::Ready;
    }

    /// The newest retained action snapshot, for routing client edits by scope.
    pub fn latest_action_meta(
        &self,
    ) -> Option<(
        &crate::tree::Tree,
        &str,
        u64,
        &str,
        Option<&crate::menu::Menu>,
    )> {
        self.action_snapshots.back().map(|snapshot| {
            (
                &snapshot.tree,
                snapshot.view_id.as_str(),
                snapshot.generation,
                snapshot.scope.as_str(),
                snapshot.menu.as_ref(),
            )
        })
    }

    /// Find a retained action snapshot only when its exact document text matches.
    pub fn action_snapshot(
        &self,
        text: &str,
    ) -> Option<(
        &crate::tree::Tree,
        &str,
        u64,
        &str,
        Option<&crate::menu::Menu>,
    )> {
        if self.format_suspended {
            return None;
        }
        let hash = hash_text(text);
        self.action_snapshots
            .iter()
            .find(|snapshot| snapshot.hash == hash)
            .map(|snapshot| {
                (
                    &snapshot.tree,
                    snapshot.view_id.as_str(),
                    snapshot.generation,
                    snapshot.scope.as_str(),
                    snapshot.menu.as_ref(),
                )
            })
    }

    /// Consume the one user-facing warning caused by persistent format drift.
    pub fn take_format_warning(&mut self) -> bool {
        std::mem::replace(&mut self.warning_pending, false)
    }

    /// Attach/reopen with the exact observed editor snapshot; it does not become server-authoritative.
    pub fn open(&mut self, version: i32, text: impl Into<String>) -> Option<ApplyEdit> {
        self.navigation = None;
        self.in_flight = None;
        self.format_suspended = false;
        self.warning_pending = false;
        self.client = Some(self.snapshot_for(text.into(), version));
        self.rejected = None;
        self.state = DocumentState::Ready;
        self.prepare()
    }

    /// Record an observed didChange snapshot and reconcile an exact applyEdit echo.
    pub fn change(&mut self, version: i32, text: impl Into<String>) -> Option<ApplyEdit> {
        self.observe(version, text);
        self.prepare()
    }

    /// Whether the controller has already observed this exact editor snapshot.
    ///
    /// A delivered render observes the snapshot it replaced before rendering new
    /// authoritative text; a refused key's typed snapshot is never observed.
    /// That distinguishes an acknowledged render from text that still needs an
    /// authoritative repair.
    pub fn has_observed(&self, version: i32, text: &str) -> bool {
        self.client
            .as_ref()
            .is_some_and(|client| client.version == version && client.hash == hash_text(text))
    }

    /// Retry the retained authoritative render against an observed snapshot.
    ///
    /// Unlike [`Self::render_actions`], this never re-renders, so navigation
    /// attached to the delivered render survives the retry.
    pub fn repair(&mut self, version: i32, text: impl Into<String>) -> Option<ApplyEdit> {
        self.observe(version, text);
        self.prepare()
    }

    /// Record an observed didChange snapshot and reconcile an in-flight edit echo.
    ///
    /// This is the observation half of [`PlainDocument::change`]: it updates the
    /// client snapshot and flight bookkeeping but never prepares or returns an
    /// edit, so a caller can render new authoritative text afterwards.
    pub fn observe(&mut self, version: i32, text: impl Into<String>) {
        if self.state == DocumentState::Detached {
            return;
        }
        let text = text.into();
        let observed = self.snapshot_for(text, version);
        self.client = Some(observed.clone());
        self.rejected = None;
        if let Some(flight) = self.in_flight.as_mut() {
            flight.echoed = flight.target.hash == observed.hash;
            flight.format_drift =
                !flight.echoed && format_equivalent(&flight.target.text, &observed.text);
            flight.diverged |= !flight.echoed;
            if flight.response == Some(true) {
                if flight.format_drift {
                    self.suspend_for_format_drift();
                    return;
                }
                self.in_flight = None;
                self.state = DocumentState::Ready;
            }
            return;
        }
        self.state = DocumentState::Ready;
    }

    /// Resolve an applyEdit result; late responses after close/reopen are ignored.
    pub fn response(&mut self, id: u64, applied: bool) -> Option<ApplyEdit> {
        let flight = self.in_flight.as_mut()?;
        if flight.request.id != id {
            return None;
        }
        flight.response = Some(applied);
        if !applied {
            if !flight.diverged {
                self.rejected = Some((
                    flight.target.hash,
                    self.client.as_ref().map_or([0; 32], |client| client.hash),
                ));
            }
            self.in_flight = None;
            self.state = DocumentState::Ready;
            return self.prepare();
        }
        if flight.format_drift {
            self.suspend_for_format_drift();
            return None;
        }
        if flight.echoed || flight.diverged {
            self.in_flight = None;
            self.state = DocumentState::Ready;
            return self.prepare();
        }
        None
    }

    /// Expire an accepted applyEdit that has not been confirmed by a document echo.
    ///
    /// The correlated request ID prevents stale timers from affecting newer flights.
    /// The observed client snapshot remains distinct from server-authoritative rendered text.
    pub fn expire(&mut self, id: u64) -> Option<ApplyEdit> {
        let flight = self.in_flight.as_ref()?;
        if flight.request.id != id || flight.response != Some(true) || flight.echoed {
            return None;
        }
        let target_hash = flight.target.hash;
        let client_hash = self.client.as_ref().map_or([0; 32], |client| client.hash);
        self.in_flight = None;
        self.rejected = Some((target_hash, client_hash));
        self.state = DocumentState::Ready;
        self.prepare()
    }

    /// Detach and invalidate pending responses so no late result can resurrect it.
    pub fn close(&mut self) {
        self.navigation = None;
        self.state = DocumentState::Detached;
        self.client = None;
        self.in_flight = None;
        self.rejected = None;
        self.format_suspended = false;
        self.warning_pending = false;
    }

    fn retain_action(
        &mut self,
        tree: crate::tree::Tree,
        view_id: String,
        generation: u64,
        scope: String,
        menu: Option<crate::menu::Menu>,
    ) -> String {
        let text = self.format.serialize(&tree);
        let hash = hash_text(&text);
        self.action_snapshots
            .retain(|snapshot| snapshot.hash != hash);
        self.action_snapshots.push_back(ActionSnapshot {
            hash,
            view_id,
            generation,
            scope,
            tree,
            menu,
        });
        while self.action_snapshots.len() > self.snapshot_limit {
            self.action_snapshots.pop_front();
        }
        text
    }

    fn snapshot_for(&mut self, text: String, version: i32) -> Snapshot {
        let hash = hash_text(&text);
        let snapshot = Snapshot {
            hash,
            text,
            version,
        };
        self.snapshots.retain(|s| s.hash != hash);
        self.snapshots.push_back(snapshot.clone());
        while self.snapshots.len() > self.snapshot_limit {
            self.snapshots.pop_front();
        }
        snapshot
    }

    fn prepare(&mut self) -> Option<ApplyEdit> {
        if self.state == DocumentState::Detached
            || self.in_flight.is_some()
            || self.format_suspended
        {
            return None;
        }
        let (Some(authoritative), Some(client)) = (&self.authoritative, &self.client) else {
            return None;
        };
        if authoritative.hash == client.hash {
            self.state = DocumentState::Ready;
            self.rejected = None;
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
        let request = ApplyEdit {
            id: self.next_id,
            edit,
        };
        let target = authoritative.clone();
        self.in_flight = Some(InFlight {
            request: request.clone(),
            target,
            echoed: false,
            diverged: false,
            response: None,
            format_drift: false,
        });
        self.state = DocumentState::Applying;
        Some(request)
    }

    fn suspend_for_format_drift(&mut self) {
        self.in_flight = None;
        self.state = DocumentState::Ready;
        self.format_suspended = true;
        self.warning_pending = true;
    }
}

fn format_equivalent(expected: &str, observed: &str) -> bool {
    fn logical(text: &str) -> String {
        let normalized = text.replace("\r\n", "\n");
        normalized
            .strip_suffix('\n')
            .unwrap_or(&normalized)
            .to_owned()
    }
    expected != observed && logical(expected) == logical(observed)
}

fn splits_crlf(text: &str, offset: usize) -> bool {
    offset > 0
        && offset < text.len()
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

/// Hash exact UTF-8 text for collision-resistant snapshot matching.
pub fn hash_text(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}

/// Build a versioned `WorkspaceEdit` that replaces a whole document.
///
/// `version` is the document's currently synchronized version and `current`
/// supplies the end position of the existing text. The caller sends the result
/// as the `edit` parameter of `workspace/applyEdit`.
pub fn whole_document_edit(
    uri: &str,
    version: i32,
    current: &str,
    text: &str,
    encoding: Encoding,
) -> Value {
    let end = position(current, current.len(), encoding);
    json!({"documentChanges":[{
        "textDocument":{"uri":uri,"version":version},
        "edits":[{"range":{"start":{"line":0,"character":0},"end":end},"newText":text}]
    }]})
}

/// Produce one valid edit using negotiated LSP position units. When both
/// strings end in LF, preserve their shared terminal newline in the suffix so
/// the edit does not insert at virtual EOF.
pub fn text_edits(old: &str, new: &str, encoding: Encoding) -> Vec<Value> {
    if old == new {
        return Vec::new();
    }
    let mut prefix = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
        prefix -= 1;
    }
    // Keep a shared final newline in the suffix, rather than inserting at
    // virtual EOF. Native editors may expose an intermediate extra line while
    // applying an EOF edit, then echo its removal after the applyEdit result.
    if old.ends_with('\n') && new.ends_with('\n') {
        prefix = prefix.min(old.len() - 1).min(new.len() - 1);
    }
    while splits_crlf(old, prefix) || splits_crlf(new, prefix) {
        prefix -= 1;
    }
    let mut suffix = 0;
    let max_suffix = old
        .len()
        .saturating_sub(prefix)
        .min(new.len().saturating_sub(prefix));
    while suffix < max_suffix
        && old.as_bytes()[old.len() - 1 - suffix] == new.as_bytes()[new.len() - 1 - suffix]
    {
        suffix += 1;
    }
    while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
        suffix -= 1;
    }
    while splits_crlf(old, old.len() - suffix) || splits_crlf(new, new.len() - suffix) {
        suffix -= 1;
    }
    let old_end = old.len() - suffix;
    let new_end = new.len() - suffix;
    let start = position(old, prefix, encoding);
    let end = position(old, old_end, encoding);
    vec![json!({"range":{"start":start,"end":end},"newText":&new[prefix..new_end]})]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::Tree;

    fn tree(text: &str) -> Tree {
        Tree {
            version: 0,
            lines: vec![text.to_owned()],
            nodes: vec![],
            line_nodes: vec![],
            tokens: vec![],
            symbols: vec![],
            links: vec![],
        }
    }

    #[test]
    fn plain_navigation_is_correlated_and_invalidated_by_render_or_lifecycle() {
        let mut document = PlainDocument::new(
            "file:///test",
            Encoding::Utf16,
            false,
            DocumentFormat::from_text(""),
        );
        document.open(1, "status");
        let selection = lsp_types::Range::new(
            lsp_types::Position::new(0, 0),
            lsp_types::Position::new(2, 0),
        );
        let ordinary = document.render("menu").unwrap();
        assert_eq!(
            document.take_navigation(ordinary.id, true),
            None,
            "ordinary edits have no navigation"
        );
        document.observe(2, "menu");
        document.response(ordinary.id, true);

        let rejected = document.render("selection").unwrap();
        document.navigate_after_edit(selection);
        assert_eq!(
            document.take_navigation(rejected.id + 1, true),
            None,
            "unrelated replies cannot consume navigation"
        );
        assert_eq!(
            document.take_navigation(rejected.id, false),
            None,
            "rejected edits cannot navigate"
        );
        document.response(rejected.id, false);

        let superseded = document.render("selection 2").unwrap();
        document.navigate_after_edit(selection);
        assert!(document.render("selection 2").is_none());
        assert_eq!(
            document.take_navigation(superseded.id, true),
            None,
            "even identical newer renders invalidate navigation"
        );
        document.navigate_after_edit(selection);
        assert!(document.render("newer view").is_none());
        assert_eq!(
            document.take_navigation(superseded.id, true),
            None,
            "new renders invalidate old navigation"
        );
        document.observe(3, "selection 2");
        let next = document.response(superseded.id, true).unwrap();
        assert_eq!(
            document.take_navigation(next.id, true),
            None,
            "ordinary queued edits cannot inherit navigation"
        );
        document.observe(4, "newer view");
        document.response(next.id, true);

        let previous = document.render("older target").unwrap();
        assert!(document.render("queued selection").is_none());
        document.navigate_after_edit(selection);
        assert_eq!(document.take_navigation(previous.id, true), None);
        document.observe(5, "older target");
        let queued = document.response(previous.id, true).unwrap();
        assert_eq!(
            document.take_navigation(queued.id, true),
            Some(selection),
            "queued navigation follows its own successful edit"
        );
        assert_eq!(
            document.take_navigation(queued.id, true),
            None,
            "navigation is one-shot"
        );
        document.observe(6, "queued selection");
        document.response(queued.id, true);

        let edited = document.render("selection 3").unwrap();
        document.navigate_after_edit(selection);
        document.observe(7, "user edit");
        assert_eq!(
            document.take_navigation(edited.id, true),
            None,
            "divergent user edits cannot navigate"
        );
        document.close();
        document.open(8, "reopened");
        assert_eq!(
            document.take_navigation(edited.id, true),
            None,
            "late replies cannot navigate a reopened document"
        );
    }

    #[test]
    fn action_snapshot_retention_is_bounded_and_keeps_generation() {
        let mut document = PlainDocument::new(
            "file:///test",
            Encoding::Utf16,
            false,
            DocumentFormat::from_text(""),
        );
        for generation in 0..7 {
            document.render_actions(
                tree(&format!("render-{generation}")),
                "view".into(),
                generation,
                "scope".into(),
                None,
            );
        }
        assert!(document.action_snapshot("render-0").is_none());
        let (_, _, generation, _, _) = document.action_snapshot("render-6").unwrap();
        assert_eq!(generation, 6);
    }
}
