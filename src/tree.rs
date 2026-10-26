//! The server-owned view tree.
//!
//! A tree holds the rendered text plus everything a client needs to display
//! and act on it: a line → node map, semantic tokens, a symbol tree, link
//! ranges and the node hierarchy itself. Structure comes from this tree, never
//! from parsing the rendered text.

use lsp_types::{
    DocumentLink, DocumentSymbol, FoldingRange, FoldingRangeKind, Location, Position, Range,
    SymbolKind, Uri,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

/// Stable identity of a node, derived from what the node is, never its position.
pub type NodeId = String;

/// Whether a node's body is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisibilityState {
    Expanded,
    Collapsed,
}

/// Per-view visibility, keyed by stable node id so it survives a refresh.
///
/// Unknown nodes are expanded: the server only records a state when the user
/// changes it, so a refresh that adds a file shows it expanded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Visibility {
    states: HashMap<NodeId, VisibilityState>,
}

impl Visibility {
    /// An empty map, where every node is expanded.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `id`'s body is shown.
    pub fn is_expanded(&self, id: &str) -> bool {
        self.states.get(id) != Some(&VisibilityState::Collapsed)
    }

    /// Record `state` for `id`.
    pub fn set(&mut self, id: impl Into<NodeId>, state: VisibilityState) {
        self.states.insert(id.into(), state);
    }

    /// Expand `id`.
    pub fn show(&mut self, id: impl Into<NodeId>) {
        self.set(id, VisibilityState::Expanded);
    }

    /// Collapse `id`.
    pub fn hide(&mut self, id: impl Into<NodeId>) {
        self.set(id, VisibilityState::Collapsed);
    }

    /// Flip `id` and return its new state.
    pub fn toggle(&mut self, id: &str) -> VisibilityState {
        let next = if self.is_expanded(id) {
            VisibilityState::Collapsed
        } else {
            VisibilityState::Expanded
        };
        self.set(id.to_string(), next);
        next
    }

    /// Forget every recorded state, expanding all nodes.
    pub fn show_all(&mut self) {
        self.states.clear();
    }
}

/// What a rendered line or tree node represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Header,
    Section,
    Message,
    File,
    Hunk,
    /// A line inside a hunk; the client knows this kind as `line`.
    Line,
    Blank,
    /// A selectable Git commit.
    Commit,
}

/// One element of a view tree. Lines are 0-based and inclusive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub label: String,
    pub start_line: usize,
    pub end_line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(rename = "hunk_id", skip_serializing_if = "Option::is_none")]
    pub hunk_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<char>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_index: Option<usize>,
    /// Line in the new (worktree) file this node visits, 1-based.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_line: Option<u32>,
    /// Exact text `common.copy-value` yields for this node, if it is copyable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copy: Option<String>,
    /// Full commit object ID for structured commit selection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Ids of the child nodes, in document order.
    pub children: Vec<NodeId>,
}

/// The node that owns one rendered line; this is the client's cursor map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineNode {
    /// Id of the owning tree node.
    pub node: NodeId,
    pub kind: NodeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(rename = "hunk_id", skip_serializing_if = "Option::is_none")]
    pub hunk_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<char>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_index: Option<usize>,
    /// Line in the new (worktree) file this node visits, 1-based.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_line: Option<u32>,
    /// Hunk line text without its marker, for clients that need to compare it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// A semantic-token range, using the `magit.*` type names of TASKS.md §3.15.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    pub line: usize,
    pub start: usize,
    pub length: usize,
    pub token_type: String,
    pub modifiers: Vec<String>,
}

/// One node of the symbol tree (sections, files and hunks).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    /// Extra outline text: a section's file count or a file's status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// `section`, `file` or `hunk`.
    pub kind: String,
    pub node: NodeId,
    pub start_line: usize,
    pub end_line: usize,
    pub children: Vec<Symbol>,
}

/// A clickable range, currently used for file paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub line: usize,
    pub start: usize,
    pub length: usize,
    pub path: String,
}

/// A rendered view: text plus its structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tree {
    pub version: u64,
    pub lines: Vec<String>,
    pub nodes: Vec<Node>,
    pub line_nodes: Vec<LineNode>,
    pub tokens: Vec<Token>,
    pub symbols: Vec<Symbol>,
    pub links: Vec<Link>,
}

/// The wire form of a rendered view: text, version and per-line node info.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub text: Vec<String>,
    pub version: u64,
    pub nodes: Vec<LineNode>,
}

impl From<&Tree> for Document {
    fn from(tree: &Tree) -> Self {
        Self {
            text: tree.lines.clone(),
            version: tree.version,
            nodes: tree.line_nodes.clone(),
        }
    }
}

impl Tree {
    /// Find a node by its stable id.
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }

    /// The tree's symbols as hierarchical `DocumentSymbol`s.
    ///
    /// A section is a `NAMESPACE`, a file is a `FILE` and a hunk is a `METHOD`:
    /// hunks are the smallest navigable unit inside a file, like a method body.
    pub fn document_symbols(&self) -> Vec<DocumentSymbol> {
        self.symbols
            .iter()
            .map(|symbol| self.document_symbol(symbol))
            .collect()
    }

    #[allow(deprecated)]
    fn document_symbol(&self, symbol: &Symbol) -> DocumentSymbol {
        DocumentSymbol {
            name: symbol.name.clone(),
            detail: symbol.detail.clone(),
            kind: symbol_kind(&symbol.kind),
            tags: None,
            deprecated: None,
            range: self.line_range(symbol.start_line, symbol.end_line),
            selection_range: self.selection_range(symbol),
            children: if symbol.children.is_empty() {
                None
            } else {
                Some(
                    symbol
                        .children
                        .iter()
                        .map(|child| self.document_symbol(child))
                        .collect(),
                )
            },
        }
    }

    /// The file-path links of the rendered document, as `DocumentLink`s.
    pub fn document_links(&self, repo_root: &str) -> Vec<DocumentLink> {
        self.links
            .iter()
            .filter_map(|link| {
                Some(DocumentLink {
                    range: self.link_range(link),
                    target: Some(file_uri(repo_root, &link.path)?),
                    tooltip: Some(link.path.clone()),
                    data: None,
                })
            })
            .collect()
    }

    /// Where the real file for `position` is, as a `definition` location.
    ///
    /// File lines visit line 1; hunk and diff lines visit the new-side line
    /// they belong to. Deleted files have no worktree file, so they return no
    /// location; untracked files exist and are visited normally.
    pub fn definition(&self, repo_root: &str, position: Position) -> Option<Location> {
        let line = self.line_nodes.get(position.line as usize)?;
        let path = line.path.as_deref()?;
        if line.status.as_deref() == Some("D") {
            return None;
        }
        let new_line = match line.kind {
            NodeKind::File => 0,
            NodeKind::Hunk | NodeKind::Line => {
                u32::from(line.new_line.unwrap_or(1).saturating_sub(1))
            }
            _ => return None,
        };
        Some(Location {
            uri: file_uri(repo_root, path)?,
            range: Range::new(Position::new(new_line, 0), Position::new(new_line, 0)),
        })
    }

    /// Fold ranges for each section and file block (0-based, LSP convention).
    pub fn folding_ranges(&self) -> Vec<FoldingRange> {
        let sections: Vec<&Node> = self
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Section)
            .collect();
        let files: Vec<&Node> = self
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::File)
            .collect();
        let mut ranges = Vec::new();

        for (index, section) in sections.iter().enumerate() {
            let start = section.start_line;
            let next = sections
                .get(index + 1)
                .map(|next| next.start_line)
                .unwrap_or(self.lines.len());
            if let Some(end) = last_nonblank(&self.lines, start, next) {
                if end > start {
                    ranges.push(fold(start, end));
                }
            }
        }

        for (index, file) in files.iter().enumerate() {
            let start = file.start_line;
            let next_file = files.get(index + 1).map(|next| next.start_line);
            let next_section = sections
                .iter()
                .find(|section| section.start_line > start)
                .map(|section| section.start_line);
            let next = match (next_file, next_section) {
                (Some(a), Some(b)) => a.min(b),
                (Some(a), None) => a,
                (None, Some(b)) => b,
                (None, None) => self.lines.len(),
            };
            if let Some(end) = last_nonblank(&self.lines, start, next) {
                if end > start {
                    ranges.push(fold(start, end));
                }
            }
        }

        ranges
    }

    /// The text `common.copy-value` yields for a line or selection.
    ///
    /// Resolution uses the node behind each line and the copy text recorded
    /// when the tree was built; rendered line shapes are never parsed.
    pub fn copy_value(
        &self,
        line: Option<usize>,
        selection: Option<(usize, usize)>,
    ) -> Option<String> {
        if let Some((a, b)) = selection {
            let (start, end) = if a <= b { (a, b) } else { (b, a) };
            if start != end {
                let parts: Vec<String> = (start..=end)
                    .filter_map(|line| self.selected_line(line))
                    .collect();
                if !parts.is_empty() {
                    return Some(parts.join("\n"));
                }
            }
        }
        self.copy_node(line?)
    }

    /// The copy text of the node owning `line`.
    fn copy_node(&self, line: usize) -> Option<String> {
        let node = self.node(self.line_nodes.get(line)?.node.as_str())?;
        match node.kind {
            NodeKind::Commit => node.commit.clone(),
            NodeKind::File => node.path.clone(),
            NodeKind::Hunk => node.copy.clone(),
            NodeKind::Line => Some(node.label.clone()),
            NodeKind::Section => Some(node.label.clone()),
            // Commit nodes (R-5.1) will copy their hash from here.
            _ => None,
        }
    }

    /// One line of a visual selection, keeping diff markers.
    fn selected_line(&self, line: usize) -> Option<String> {
        let node = self.node(self.line_nodes.get(line)?.node.as_str())?;
        match node.kind {
            NodeKind::Commit => node.commit.clone(),
            NodeKind::Line => Some(format!("{}{}", node.marker.unwrap_or(' '), node.label)),
            NodeKind::Hunk => node.copy.clone(),
            NodeKind::File => node.path.clone(),
            NodeKind::Section => Some(node.label.clone()),
            _ => None,
        }
    }

    /// A full-line LSP range from the first to the last line of a symbol.
    fn line_range(&self, start_line: usize, end_line: usize) -> Range {        let last = self.lines.len().saturating_sub(1);
        let end = end_line.min(last);
        let start = start_line.min(end);
        Range::new(
            Position::new(start as u32, 0),
            Position::new(end as u32, self.line_utf16_len(end)),
        )
    }

    /// The range to reveal when a symbol is picked: its name or path.
    fn selection_range(&self, symbol: &Symbol) -> Range {
        let line = symbol.start_line;
        if symbol.kind == "file" {
            if let Some(link) = self
                .links
                .iter()
                .find(|link| link.line == line && link.path == symbol.name)
            {
                return self.link_range(link);
            }
        }
        if symbol.kind == "section" {
            let start = Position::new(line as u32, 0);
            let end = Position::new(
                line as u32,
                self.line_utf16_len(line).min(utf16_len(&symbol.name)),
            );
            return Range::new(start, end);
        }
        self.line_range(line, line)
    }

    /// A link's byte offsets as an LSP range.
    fn link_range(&self, link: &Link) -> Range {
        let text = self.lines.get(link.line).map(String::as_str).unwrap_or("");
        let start = utf16_col(text, link.start);
        let end = utf16_col(text, link.start.saturating_add(link.length));
        Range::new(
            Position::new(link.line as u32, start),
            Position::new(link.line as u32, end),
        )
    }

    /// Length of line `line` in UTF-16 code units.
    fn line_utf16_len(&self, line: usize) -> u32 {
        self.lines
            .get(line)
            .map(|text| utf16_len(text))
            .unwrap_or(0)
    }
}

/// The LSP symbol kind for a tree symbol kind.
fn symbol_kind(kind: &str) -> SymbolKind {
    match kind {
        "section" => SymbolKind::NAMESPACE,
        "file" => SymbolKind::FILE,
        "hunk" => SymbolKind::METHOD,
        _ => SymbolKind::OBJECT,
    }
}

/// A `file://` URI for `path` inside `repo_root`, percent-encoding as needed.
fn file_uri(repo_root: &str, path: &str) -> Option<Uri> {
    let mut full = String::from(repo_root.trim_end_matches('/'));
    full.push('/');
    full.push_str(path);

    let mut encoded = String::from("file://");
    for byte in full.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                encoded.push(char::from(byte));
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    Uri::from_str(&encoded).ok()
}

/// Length of `text` in UTF-16 code units, the unit LSP columns use.
fn utf16_len(text: &str) -> u32 {
    text.encode_utf16().count() as u32
}

/// UTF-16 column of a byte offset into `text`.
fn utf16_col(text: &str, byte: usize) -> u32 {
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    utf16_len(&text[..byte])
}

/// The last non-blank line in `start..end`, if any.
pub(crate) fn last_nonblank(lines: &[String], start: usize, end: usize) -> Option<usize> {
    (start..end.min(lines.len()))
        .rev()
        .find(|&index| !lines[index].trim().is_empty())
}

fn fold(start: usize, end: usize) -> FoldingRange {
    FoldingRange {
        start_line: start as u32,
        end_line: end as u32,
        start_character: None,
        end_character: None,
        kind: Some(FoldingRangeKind::Region),
        collapsed_text: None,
    }
}
