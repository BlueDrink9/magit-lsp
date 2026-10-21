//! The server-owned view tree.
//!
//! A tree holds the rendered text plus everything a client needs to display
//! and act on it: a line → node map, semantic tokens, a symbol tree, link
//! ranges and the node hierarchy itself. Structure comes from this tree, never
//! from parsing the rendered text.

use crate::protocol::Target;
use lsp_types::{FoldingRange, FoldingRangeKind};
use serde::{Deserialize, Serialize};

/// Stable identity of a node, derived from what the node is, never its position.
pub type NodeId = String;

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

    /// Navigation targets derived from the tree, sorted by line.
    pub fn targets(&self) -> Vec<Target> {
        let mut targets: Vec<Target> = self
            .nodes
            .iter()
            .filter_map(|node| match node.kind {
                NodeKind::Section => Some(Target {
                    kind: "section".to_string(),
                    label: node.label.clone(),
                    line: node.start_line as u32 + 1,
                    section: node.section.clone(),
                    path: None,
                }),
                NodeKind::File => Some(Target {
                    kind: "file".to_string(),
                    label: node.path.clone().unwrap_or_else(|| node.label.clone()),
                    line: node.start_line as u32 + 1,
                    section: node.section.clone(),
                    path: node.path.clone(),
                }),
                NodeKind::Hunk => Some(Target {
                    kind: "hunk".to_string(),
                    label: "@@".to_string(),
                    line: node.start_line as u32 + 1,
                    section: node.section.clone(),
                    path: node.path.clone(),
                }),
                _ => None,
            })
            .collect();
        targets.sort_by_key(|target| target.line);
        targets
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
