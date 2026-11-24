//! The process log view: `runner::global_log()` rendered as a headed list.
//!
//! The view is read-only and never runs Git. Each record's node id derives from
//! its monotonic id, so a refresh keeps the node stable; copy text is recorded
//! on the node rather than recovered from the rendered line.

use crate::runner::{self, ProcessRecord};
use crate::tree::{LineNode, Node, NodeKind, Token, Tree};
use crate::view::{Ctx, Scope, View, ViewKind};

/// A read-only view over every finished Git invocation.
pub struct ProcessLogView;

impl ProcessLogView {
    /// Create a view; the log itself is read on every render.
    pub fn new() -> Self {
        Self
    }
}

impl Default for ProcessLogView {
    fn default() -> Self {
        Self::new()
    }
}

impl View for ProcessLogView {
    fn kind(&self) -> ViewKind {
        ViewKind::ProcessLog
    }

    fn title(&self) -> String {
        "Process log".to_string()
    }

    fn scope(&self) -> Scope {
        Scope::new("process-log")
    }

    fn render(&mut self, _ctx: &Ctx<'_>) -> Tree {
        build(runner::global_log().iter())
    }
}

/// Build the process log tree from a snapshot of the ring.
fn build(records: Vec<ProcessRecord>) -> Tree {
    let mut tree = Tree {
        version: 0,
        lines: Vec::new(),
        nodes: Vec::new(),
        line_nodes: Vec::new(),
        tokens: Vec::new(),
        symbols: Vec::new(),
        links: Vec::new(),
    };

    let header = "Process log";
    let count = format!("({})", records.len());
    let text = format!("{header} {count}");
    tree.lines.push(text.clone());
    tree.line_nodes
        .push(LineNode::new("process-log:header", NodeKind::Header));
    tree.nodes
        .push(Node::new("process-log:header", NodeKind::Header, header, 0));
    tree.tokens.push(Token {
        line: 0,
        start: 0,
        length: header.len(),
        token_type: "heading".to_string(),
        modifiers: Vec::new(),
    });
    tree.tokens.push(Token {
        line: 0,
        start: header.len() + 1,
        length: count.len(),
        token_type: "muted".to_string(),
        modifiers: Vec::new(),
    });

    for record in records {
        record_node(&mut tree, &record);
    }
    tree
}

fn record_node(tree: &mut Tree, record: &ProcessRecord) {
    let id = format!("process-log:record:{}", record.id);
    let command = format!("git {}", record.args.join(" "));
    let line = tree.lines.len();
    let text = format!(
        "{command} (exit {}, {} ms)",
        record.exit_code, record.duration_ms
    );
    tree.lines.push(text.clone());

    tree.line_nodes.push(LineNode::new(&id, NodeKind::Hunk));

    let mut record_node = Node::new(&id, NodeKind::Hunk, command.clone(), line);
    record_node.copy = Some(command.clone());
    tree.nodes.push(record_node);

    tree.tokens.push(Token {
        line,
        start: 0,
        length: command.len(),
        token_type: "path".to_string(),
        modifiers: Vec::new(),
    });
    tree.tokens.push(Token {
        line,
        start: command.len() + 1,
        length: text.len() - command.len() - 1,
        token_type: "muted".to_string(),
        modifiers: Vec::new(),
    });

    let failed = record.exit_code != 0;
    output_lines(tree, &id, "stderr", &record.stderr, failed);
    output_lines(tree, &id, "stdout", &record.stdout, false);

    let end_line = tree.lines.len().saturating_sub(1);
    if let Some(node) = tree.nodes.iter_mut().find(|node| node.id == id) {
        node.end_line = end_line;
    }
}

/// Render one non-empty output field as indented `label: text` lines.
fn output_lines(tree: &mut Tree, record_id: &str, label: &str, output: &str, failed: bool) {
    if output.is_empty() {
        return;
    }
    for output_line in output.lines() {
        let line = tree.lines.len();
        let text = format!("  {label}: {output_line}");
        tree.lines.push(text.clone());
        tree.line_nodes
            .push(LineNode::new(record_id, NodeKind::Line));
        if label == "stderr" {
            tree.tokens.push(Token {
                line,
                start: 0,
                length: text.len(),
                token_type: if failed { "error" } else { "warning" }.to_string(),
                modifiers: Vec::new(),
            });
        }
    }
}
