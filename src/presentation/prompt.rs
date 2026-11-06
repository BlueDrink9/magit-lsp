//! Server-owned document rendering for prompts that a client cannot show natively.
//!
//! A [`PromptDocument`] turns one [`ViewPromptParams`] into a text document with
//! stable node identities. Fields are [`NodeKind::Field`] nodes occupying exactly
//! one line; the line text is the value the server reads back from the document
//! store. Rows are [`NodeKind::Line`] nodes the client turns into edits or
//! actions.

use crate::protocol::{PromptKind, ViewPromptParams};
use crate::tree::{LineNode, Node, NodeKind, Tree};
use crate::view::{Ctx, Scope, View, ViewKind};
use std::collections::BTreeSet;

/// Maximum byte length of one document-prompt field.
///
/// A field occupies exactly one document line. Values longer than this are
/// refused rather than truncated, so the server never silently drops input.
pub const MAX_FIELD_BYTES: usize = 4096;

/// Message shown when a non-native client asks for secret input.
pub const SECRET_UNAVAILABLE: &str =
    "This client cannot ask for secret input; the operation was cancelled";

/// How a prompt request reaches a client that did or did not declare support.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptDelivery {
    /// The client declared the prompt kind (and, for secret input, `secret`).
    Native,
    /// The client cannot show this prompt natively; render it as document text.
    Document,
    /// The client cannot show secret input natively; refuse the operation.
    RefuseSecret,
}

/// Choose how `request` reaches a client whose prompt declarations are
/// `native_declared`.
///
/// Secret input is checked first: a client that cannot show it natively never
/// gets a document fallback, because document text would expose the value.
pub fn delivery(request: &ViewPromptParams, native_declared: bool) -> PromptDelivery {
    if request.secret == Some(true) && !native_declared {
        PromptDelivery::RefuseSecret
    } else if !native_declared {
        PromptDelivery::Document
    } else {
        PromptDelivery::Native
    }
}

/// Outcome of ingesting one client document revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reconcile {
    /// Whether the client changed lines outside prompt fields.
    ///
    /// A repaired document adopts no text at all; the caller should re-render
    /// the authoritative tree and push it back to the client.
    pub repaired: bool,
}

/// One editable field of a prompt document.
struct Field {
    id: String,
    label: String,
    value: String,
}

/// A validated prompt rendered as server-owned text and structure.
///
/// The document keeps field values and item selections in server state. Editing
/// a field or toggling an item changes that state without rebuilding the tree;
/// call [`PromptDocument::render`] when a fresh [`Tree`] is needed.
pub struct PromptDocument {
    prompt_id: String,
    kind: PromptKind,
    title: String,
    detail: Option<String>,
    yes_label: String,
    no_label: String,
    items: Vec<crate::protocol::PromptItem>,
    fields: Vec<Field>,
    selected: BTreeSet<String>,
    default_item: Option<String>,
    row_ids: Vec<String>,
}

impl PromptDocument {
    /// Validate `params` and build its prompt document model.
    ///
    /// The kind-specific shape is checked here: `confirm` and `text` carry no
    /// items, `choice` and `multiChoice` require an item list and known
    /// preselection ids, and secret prompts are refused because document text
    /// would expose the value.
    pub fn new(params: ViewPromptParams) -> Result<Self, String> {
        if params.secret == Some(true) {
            return Err(
                "secret prompts are native-only and cannot be rendered as document text".into(),
            );
        }

        let ViewPromptParams {
            prompt_id,
            kind,
            title,
            detail,
            yes_label,
            no_label,
            default,
            items,
            allow_custom,
            preselected,
            ..
        } = params;

        let supplied_items = items.is_some();
        let items = items.unwrap_or_default();
        match kind {
            PromptKind::Confirm | PromptKind::Text => {
                if !items.is_empty() {
                    return Err(format!("{kind:?} prompts cannot carry items"));
                }
            }
            PromptKind::Choice | PromptKind::MultiChoice => {
                if !supplied_items {
                    return Err(format!("{kind:?} prompts require items"));
                }
            }
        }

        let preselected = preselected.unwrap_or_default();
        for id in &preselected {
            if !items.iter().any(|item| &item.id == id) {
                return Err(format!(
                    "preselected item id {id:?} is not in the prompt items"
                ));
            }
        }

        let default_text = default
            .as_ref()
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        let default_item = match kind {
            PromptKind::Choice => default
                .as_ref()
                .and_then(|value| value.as_str())
                .filter(|id| items.iter().any(|item| item.id == **id))
                .map(str::to_owned),
            _ => None,
        };

        let allow_custom = allow_custom.unwrap_or(false);
        let mut fields = Vec::new();
        match kind {
            PromptKind::Text => fields.push(Field {
                id: field_id(&prompt_id),
                label: "Value".into(),
                value: default_text,
            }),
            PromptKind::Choice if allow_custom => {
                let prefill = default_item
                    .as_ref()
                    .and_then(|id| items.iter().find(|item| &item.id == id))
                    .map(|item| item.label.clone())
                    .unwrap_or_default();
                fields.push(Field {
                    id: custom_field_id(&prompt_id),
                    label: "Custom value".into(),
                    value: prefill,
                });
            }
            _ => {}
        }
        for field in &fields {
            validate_field(&field.label, &field.value)?;
        }

        let row_ids = match kind {
            PromptKind::Confirm => vec![row_id(&prompt_id, "yes"), row_id(&prompt_id, "no")],
            PromptKind::Text => vec![
                row_id(&prompt_id, "submit"),
                row_id(&prompt_id, "cancel"),
            ],
            PromptKind::Choice => items
                .iter()
                .map(|item| item_row_id(&prompt_id, &item.id))
                .collect(),
            PromptKind::MultiChoice => items
                .iter()
                .map(|item| item_row_id(&prompt_id, &item.id))
                .chain(std::iter::once(row_id(&prompt_id, "done")))
                .collect(),
        };

        let selected = if kind == PromptKind::MultiChoice {
            preselected.into_iter().collect()
        } else {
            BTreeSet::new()
        };

        Ok(Self {
            prompt_id,
            kind,
            title,
            detail,
            yes_label: yes_label.unwrap_or_else(|| "Yes".into()),
            no_label: no_label.unwrap_or_else(|| "No".into()),
            items,
            fields,
            selected,
            default_item,
            row_ids,
        })
    }

    /// The prompt id shared by every stable node id of this document.
    pub fn prompt_id(&self) -> &str {
        &self.prompt_id
    }

    /// The human-readable prompt title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The prompt kind this document renders.
    pub fn kind(&self) -> PromptKind {
        self.kind
    }

    /// The rendered field ids, in document order.
    pub fn field_ids(&self) -> Vec<&str> {
        self.fields.iter().map(|field| field.id.as_str()).collect()
    }

    /// The human label of a rendered field.
    pub fn field_label(&self, id: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|field| field.id == id)
            .map(|field| field.label.as_str())
    }

    /// The current text of a rendered field, as the server will read it back.
    pub fn field_text(&self, id: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|field| field.id == id)
            .map(|field| field.value.as_str())
    }

    /// The rendered row ids, in document order.
    pub fn row_ids(&self) -> Vec<&str> {
        self.row_ids.iter().map(String::as_str).collect()
    }

    /// The item ids currently selected in the document model.
    pub fn selected_ids(&self) -> Vec<&str> {
        self.items
            .iter()
            .filter(|item| self.selected.contains(&item.id))
            .map(|item| item.id.as_str())
            .collect()
    }

    /// Whether an item is currently selected.
    pub fn is_selected(&self, item_id: &str) -> bool {
        self.selected.contains(item_id)
    }

    /// Replace one field's text without rebuilding the tree.
    ///
    /// A value must stay on one line and within [`MAX_FIELD_BYTES`]; oversized
    /// values are refused instead of truncated.
    pub fn set_field_text(&mut self, id: &str, text: &str) -> Result<(), String> {
        let field = self
            .fields
            .iter_mut()
            .find(|field| field.id == id)
            .ok_or_else(|| format!("unknown prompt field {id:?}"))?;
        validate_field(&field.label, text)?;
        field.value = text.to_string();
        Ok(())
    }

    /// Ingest one client revision of the rendered document.
    ///
    /// Field positions come from the authoritative tree rendered at `cols`;
    /// text is never parsed to recover structure. A field's value is adopted
    /// only when every non-field line matches that render, the line count is
    /// unchanged, and each field is exactly one single-line `NodeKind::Field`
    /// node. Otherwise this returns [`Reconcile::repaired`] and keeps every
    /// stored value, so the caller re-renders instead of trusting the client.
    ///
    /// LF and CRLF line endings are accepted, with or without a final
    /// terminator. An over-limit value, or a field line that still contains a
    /// stray carriage return, is refused with a readable error and leaves all
    /// previous values unchanged.
    pub fn reconcile(&mut self, document: &str, cols: usize) -> Result<Reconcile, String> {
        let tree = self.render(cols);

        let mut lines: Vec<&str> = document.split('\n').collect();
        if document.ends_with('\n') {
            lines.pop();
        }
        for line in &mut lines {
            if let Some(stripped) = line.strip_suffix('\r') {
                *line = stripped;
            }
        }

        let mut repaired = false;

        let field_node_count = tree
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Field)
            .count();
        if field_node_count != self.fields.len() {
            repaired = true;
        }

        let mut field_lines = Vec::with_capacity(self.fields.len());
        for field in &self.fields {
            let matches: Vec<&Node> = tree
                .nodes
                .iter()
                .filter(|node| node.kind == NodeKind::Field && node.id == field.id)
                .collect();
            match matches.as_slice() {
                [node]
                    if node.start_line == node.end_line
                        && node.start_line < tree.lines.len() =>
                {
                    field_lines.push(node.start_line);
                }
                _ => {
                    repaired = true;
                    field_lines.push(0);
                }
            }
        }
        let mut seen_lines = BTreeSet::new();
        if field_lines.iter().any(|line| !seen_lines.insert(*line)) {
            repaired = true;
        }

        if lines.len() != tree.lines.len() {
            repaired = true;
        }

        if !repaired {
            for (index, expected) in tree.lines.iter().enumerate() {
                if field_lines.contains(&index) {
                    continue;
                }
                if lines[index] != expected.as_str() {
                    repaired = true;
                    break;
                }
            }
        }

        if repaired {
            return Ok(Reconcile { repaired: true });
        }

        let mut values = Vec::with_capacity(self.fields.len());
        for (field, line) in self.fields.iter().zip(field_lines.iter()) {
            let text = lines[*line];
            validate_field(&field.label, text)?;
            values.push(text.to_string());
        }
        for (field, value) in self.fields.iter_mut().zip(values) {
            field.value = value;
        }
        Ok(Reconcile { repaired: false })
    }

    /// Toggle one item row without rebuilding the tree, returning its new state.
    pub fn toggle_item(&mut self, item_id: &str) -> Result<bool, String> {
        if !self.items.iter().any(|item| item.id == item_id) {
            return Err(format!("unknown prompt item {item_id:?}"));
        }
        match self.kind {
            PromptKind::MultiChoice => {
                if !self.selected.remove(item_id) {
                    self.selected.insert(item_id.to_string());
                    return Ok(true);
                }
                Ok(false)
            }
            PromptKind::Choice => {
                if self.selected.remove(item_id) {
                    return Ok(false);
                }
                self.selected.clear();
                self.selected.insert(item_id.to_string());
                Ok(true)
            }
            _ => Err(format!("{:?} prompts have no item rows", self.kind)),
        }
    }

    /// Render the current model at `cols` columns.
    ///
    /// Layout is one title line, zero or more detail lines, one blank line and
    /// then fields and rows. Detail and label text is clipped to `cols`; field
    /// lines are never clipped because their text is the stored value.
    pub fn render(&self, cols: usize) -> Tree {
        let mut tree = empty_tree();
        self.push_node(
            &mut tree,
            title_id(&self.prompt_id),
            NodeKind::Header,
            self.title.clone(),
            clip(&self.title, cols),
        );

        if let Some(detail) = &self.detail {
            for (index, line) in detail.lines().filter(|line| !line.is_empty()).enumerate() {
                self.push_node(
                    &mut tree,
                    detail_id(&self.prompt_id, index),
                    NodeKind::Message,
                    line.to_string(),
                    clip(line, cols),
                );
            }
        }

        self.push_node(
            &mut tree,
            blank_id(&self.prompt_id),
            NodeKind::Blank,
            String::new(),
            String::new(),
        );

        if self.kind == PromptKind::Text {
            self.push_fields(&mut tree);
        }

        match self.kind {
            PromptKind::Confirm => {
                self.push_row(
                    &mut tree,
                    row_id(&self.prompt_id, "yes"),
                    &self.yes_label,
                    None,
                    cols,
                );
                self.push_row(
                    &mut tree,
                    row_id(&self.prompt_id, "no"),
                    &self.no_label,
                    None,
                    cols,
                );
            }
            PromptKind::Text => {
                self.push_row(
                    &mut tree,
                    row_id(&self.prompt_id, "submit"),
                    "Submit",
                    None,
                    cols,
                );
                self.push_row(
                    &mut tree,
                    row_id(&self.prompt_id, "cancel"),
                    "Cancel",
                    None,
                    cols,
                );
            }
            PromptKind::Choice => {
                for item in &self.items {
                    let is_default = self.default_item.as_deref() == Some(item.id.as_str());
                    let (line, marker) = if is_default {
                        (format!("> {}", clip(&item.label, cols.saturating_sub(2))), Some('>'))
                    } else {
                        (clip(&item.label, cols), None)
                    };
                    self.push_item(&mut tree, item, marker, line);
                }
                self.push_fields(&mut tree);
            }
            PromptKind::MultiChoice => {
                for item in &self.items {
                    let selected = self.selected.contains(&item.id);
                    let toggle = if selected { "[x] " } else { "[ ] " };
                    let marker = if selected { Some('x') } else { Some(' ') };
                    let line = format!("{toggle}{}", clip(&item.label, cols.saturating_sub(4)));
                    self.push_item(&mut tree, item, marker, line);
                }
                self.push_row(
                    &mut tree,
                    row_id(&self.prompt_id, "done"),
                    "Done",
                    None,
                    cols,
                );
            }
        }

        tree
    }

    fn push_fields(&self, tree: &mut Tree) {
        for field in &self.fields {
            self.push_node(
                tree,
                field.id.clone(),
                NodeKind::Field,
                field.label.clone(),
                field.value.clone(),
            );
        }
    }

    fn push_node(
        &self,
        tree: &mut Tree,
        id: String,
        kind: NodeKind,
        label: String,
        text: String,
    ) {
        let line = tree.lines.len();
        tree.lines.push(text);
        tree.line_nodes.push(line_node(&id, kind));
        tree.nodes.push(Node {
            id,
            kind,
            label,
            start_line: line,
            end_line: line,
            section: None,
            path: None,
            hunk_id: None,
            status: None,
            marker: None,
            line_index: None,
            new_line: None,
            copy: None,
            mutation: None,
            commit: None,
            children: Vec::new(),
        });
    }

    fn push_row(
        &self,
        tree: &mut Tree,
        id: String,
        label: &str,
        marker: Option<char>,
        cols: usize,
    ) {
        let line = tree.lines.len();
        tree.lines.push(clip(label, cols));
        tree.line_nodes.push(line_node(&id, NodeKind::Line));
        tree.nodes.push(Node {
            id,
            kind: NodeKind::Line,
            label: label.to_string(),
            start_line: line,
            end_line: line,
            section: None,
            path: None,
            hunk_id: None,
            status: None,
            marker,
            line_index: None,
            new_line: None,
            copy: None,
            mutation: None,
            commit: None,
            children: Vec::new(),
        });
    }

    fn push_item(
        &self,
        tree: &mut Tree,
        item: &crate::protocol::PromptItem,
        marker: Option<char>,
        text: String,
    ) {
        let line = tree.lines.len();
        let id = item_row_id(&self.prompt_id, &item.id);
        tree.lines.push(text);
        tree.line_nodes.push(line_node(&id, NodeKind::Line));
        tree.nodes.push(Node {
            id,
            kind: NodeKind::Line,
            label: item.label.clone(),
            start_line: line,
            end_line: line,
            section: None,
            path: None,
            hunk_id: None,
            status: None,
            marker,
            line_index: None,
            new_line: None,
            copy: None,
            mutation: None,
            commit: None,
            children: Vec::new(),
        });
    }

}

/// One prompt rendered as server-owned document text.
///
/// The view owns its [`PromptDocument`] and renders it verbatim; it never parses
/// the rendered text back into state.
pub struct PromptView {
    document: PromptDocument,
}

impl PromptView {
    /// Create a view around a validated prompt document.
    pub fn new(document: PromptDocument) -> Self {
        Self { document }
    }

    /// The document this view renders.
    pub fn document(&self) -> &PromptDocument {
        &self.document
    }

    /// The document this view renders, for reconciliation and field updates.
    pub fn document_mut(&mut self) -> &mut PromptDocument {
        &mut self.document
    }
}

impl View for PromptView {
    fn kind(&self) -> ViewKind {
        ViewKind::Prompt
    }

    fn title(&self) -> String {
        self.document.title().to_string()
    }

    fn scope(&self) -> Scope {
        Scope::new("prompt")
    }

    fn render(&mut self, ctx: &Ctx<'_>) -> Tree {
        self.document.render(ctx.viewport.cols as usize)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

fn validate_field(label: &str, value: &str) -> Result<(), String> {
    if value.contains('\n') || value.contains('\r') {
        return Err(format!("{label} must be a single line"));
    }
    if value.len() > MAX_FIELD_BYTES {
        return Err(format!(
            "{label} exceeds the {MAX_FIELD_BYTES}-byte document field limit"
        ));
    }
    Ok(())
}

fn field_id(prompt_id: &str) -> String {
    format!("prompt:{prompt_id}:field")
}

fn custom_field_id(prompt_id: &str) -> String {
    format!("prompt:{prompt_id}:custom")
}

fn row_id(prompt_id: &str, row: &str) -> String {
    format!("prompt:{prompt_id}:{row}")
}

fn item_row_id(prompt_id: &str, item_id: &str) -> String {
    format!("prompt:{prompt_id}:item:{item_id}")
}

fn title_id(prompt_id: &str) -> String {
    format!("prompt:{prompt_id}:title")
}

fn detail_id(prompt_id: &str, index: usize) -> String {
    format!("prompt:{prompt_id}:detail:{index}")
}

fn blank_id(prompt_id: &str) -> String {
    format!("prompt:{prompt_id}:blank")
}

fn clip(text: &str, cols: usize) -> String {
    text.chars().take(cols).collect()
}

fn empty_tree() -> Tree {
    Tree {
        version: 0,
        lines: Vec::new(),
        nodes: Vec::new(),
        line_nodes: Vec::new(),
        tokens: Vec::new(),
        symbols: Vec::new(),
        links: Vec::new(),
    }
}

fn line_node(id: &str, kind: NodeKind) -> LineNode {
    LineNode {
        node: id.to_string(),
        kind,
        section: None,
        path: None,
        hunk_id: None,
        status: None,
        marker: None,
        line_index: None,
        new_line: None,
        text: None,
    }
}
