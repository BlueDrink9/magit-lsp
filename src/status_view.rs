//! Render the status view: `Status` → node tree → text and structure.
//!
//! The text format is the one the Neovim client has always shown, so clients
//! only replace buffer lines and never compose the view themselves.

use crate::git::{FileEntry, Status};
use crate::patch::Hunk;
use crate::tree::{last_nonblank, LineNode, Link, Node, NodeKind, Symbol, Token, Tree};

/// Build the status view tree from a Git status.
pub fn build(status: &Status) -> Tree {
    let mut builder = Builder::new();
    builder.header(status);
    builder.section("staged", "Staged changes", &status.staged);
    builder.section("unstaged", "Unstaged changes", &status.unstaged);
    builder.section("untracked", "Untracked files", &status.untracked);
    if status.staged.is_empty() && status.unstaged.is_empty() && status.untracked.is_empty() {
        builder.message();
    }
    builder.finish()
}

struct Builder {
    tree: Tree,
}

impl Builder {
    fn new() -> Self {
        Self {
            tree: Tree {
                version: 0,
                lines: Vec::new(),
                nodes: Vec::new(),
                line_nodes: Vec::new(),
                tokens: Vec::new(),
                symbols: Vec::new(),
                links: Vec::new(),
            },
        }
    }

    fn header(&mut self, status: &Status) {
        let line = self.tree.lines.len();
        let branch = if status.branch.is_empty() {
            "HEAD"
        } else {
            status.branch.as_str()
        };
        self.tree.lines.push(format!("Head: {branch}"));
        self.tree
            .line_nodes
            .push(line_node("header", NodeKind::Header));
        self.tree
            .nodes
            .push(node("header", NodeKind::Header, "Head", line));
    }

    fn section(&mut self, key: &str, title: &str, files: &[FileEntry]) {
        let section_id = format!("section:{key}");

        self.tree.lines.push(String::new());
        self.tree
            .line_nodes
            .push(line_node(&section_id, NodeKind::Blank));

        let line = self.tree.lines.len();
        let text = format!("{title} ({})", files.len());
        self.tree.lines.push(text.clone());

        let mut entry = line_node(&section_id, NodeKind::Section);
        entry.section = Some(key.to_string());
        self.tree.line_nodes.push(entry);

        let index = self.tree.nodes.len();
        let mut section = node(&section_id, NodeKind::Section, title, line);
        section.section = Some(key.to_string());
        self.tree.nodes.push(section);

        self.tree.tokens.push(Token {
            line,
            start: 0,
            length: title.len(),
            token_type: "magit.section".to_string(),
            modifiers: Vec::new(),
        });
        self.tree.tokens.push(Token {
            line,
            start: title.len() + 1,
            length: text.len() - title.len() - 1,
            token_type: "magit.count".to_string(),
            modifiers: Vec::new(),
        });

        let children = files
            .iter()
            .map(|file| self.file(key, file))
            .collect::<Vec<_>>();
        self.tree.nodes[index].children = children;
    }

    fn file(&mut self, section: &str, file: &FileEntry) -> String {
        let file_id = format!("{section}/{}", file.path);
        let line = self.tree.lines.len();
        self.tree
            .lines
            .push(format!("  {} {}", file.status, file.path));

        let mut entry = line_node(&file_id, NodeKind::File);
        entry.section = Some(section.to_string());
        entry.path = Some(file.path.clone());
        entry.status = Some(file.status.clone());
        self.tree.line_nodes.push(entry);

        let index = self.tree.nodes.len();
        let mut file_node = node(&file_id, NodeKind::File, file.path.clone(), line);
        file_node.section = Some(section.to_string());
        file_node.path = Some(file.path.clone());
        file_node.status = Some(file.status.clone());
        self.tree.nodes.push(file_node);

        let staged = section == "staged";
        let modifiers = if staged {
            vec!["staged".to_string()]
        } else {
            Vec::new()
        };
        let path_start = 2 + file.status.len() + 1;
        self.tree.tokens.push(Token {
            line,
            start: 2,
            length: file.status.len(),
            token_type: status_token(&file.status).to_string(),
            modifiers: modifiers.clone(),
        });
        self.tree.tokens.push(Token {
            line,
            start: path_start,
            length: file.path.len(),
            token_type: "magit.file".to_string(),
            modifiers,
        });
        self.tree.links.push(Link {
            line,
            start: path_start,
            length: file.path.len(),
            path: file.path.clone(),
        });

        let children = file
            .hunks
            .iter()
            .map(|hunk| self.hunk(section, file, &file_id, hunk))
            .collect::<Vec<_>>();
        self.tree.nodes[index].children = children;
        file_id
    }

    fn hunk(&mut self, section: &str, file: &FileEntry, file_id: &str, hunk: &Hunk) -> String {
        let hunk_id = format!("{file_id}/hunk:{}", hunk.id);
        let line = self.tree.lines.len();
        let header = format!(
            "    @@ -{},{} +{},{} @@",
            hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
        );
        self.tree.lines.push(header.clone());

        let mut entry = line_node(&hunk_id, NodeKind::Hunk);
        entry.section = Some(section.to_string());
        entry.path = Some(file.path.clone());
        entry.hunk_id = Some(hunk.id.clone());
        self.tree.line_nodes.push(entry);

        let index = self.tree.nodes.len();
        let mut hunk_node = node(&hunk_id, NodeKind::Hunk, "@@", line);
        hunk_node.section = Some(section.to_string());
        hunk_node.path = Some(file.path.clone());
        hunk_node.hunk_id = Some(hunk.id.clone());
        self.tree.nodes.push(hunk_node);

        let staged = section == "staged";
        let modifiers = if staged {
            vec!["staged".to_string()]
        } else {
            Vec::new()
        };
        self.tree.tokens.push(Token {
            line,
            start: 0,
            length: header.len(),
            token_type: "magit.hunk.header".to_string(),
            modifiers: modifiers.clone(),
        });

        let children = hunk
            .lines
            .iter()
            .enumerate()
            .map(|(offset, diff_line)| {
                let line = self.tree.lines.len();
                let text = format!("    {}{}", diff_line.marker, diff_line.text);
                self.tree.lines.push(text.clone());
                let id = format!("{hunk_id}/line:{offset}");

                let mut entry = line_node(&id, NodeKind::Line);
                entry.section = Some(section.to_string());
                entry.path = Some(file.path.clone());
                entry.hunk_id = Some(hunk.id.clone());
                entry.marker = Some(diff_line.marker);
                entry.line_index = Some(offset);
                entry.text = Some(diff_line.text.clone());
                self.tree.line_nodes.push(entry);

                let mut line_node = node(&id, NodeKind::Line, diff_line.text.clone(), line);
                line_node.section = Some(section.to_string());
                line_node.path = Some(file.path.clone());
                line_node.hunk_id = Some(hunk.id.clone());
                line_node.marker = Some(diff_line.marker);
                line_node.line_index = Some(offset);
                self.tree.nodes.push(line_node);

                let token_type = match diff_line.marker {
                    '+' => Some("magit.diff.added"),
                    '-' => Some("magit.diff.removed"),
                    _ => None,
                };
                if let Some(token_type) = token_type {
                    self.tree.tokens.push(Token {
                        line,
                        start: 0,
                        length: text.len(),
                        token_type: token_type.to_string(),
                        modifiers: modifiers.clone(),
                    });
                }
                id
            })
            .collect::<Vec<_>>();

        let end_line = self.tree.lines.len().saturating_sub(1);
        self.tree.nodes[index].end_line = end_line;
        self.tree.nodes[index].children = children;
        hunk_id
    }

    fn message(&mut self) {
        self.tree.lines.push(String::new());
        self.tree
            .line_nodes
            .push(line_node("message:clean", NodeKind::Blank));

        let line = self.tree.lines.len();
        let text = "Working tree clean";
        self.tree.lines.push(text.to_string());
        self.tree
            .line_nodes
            .push(line_node("message:clean", NodeKind::Message));
        self.tree
            .nodes
            .push(node("message:clean", NodeKind::Message, text, line));
    }

    fn finish(mut self) -> Tree {
        self.close_ranges();
        self.tree.symbols = self.symbols();
        self.tree
    }

    /// Close section and file ranges at their last non-blank line.
    fn close_ranges(&mut self) {
        let section_starts: Vec<usize> = self
            .tree
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Section)
            .map(|node| node.start_line)
            .collect();
        let file_starts: Vec<usize> = self
            .tree
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::File)
            .map(|node| node.start_line)
            .collect();

        for index in 0..self.tree.nodes.len() {
            let kind = self.tree.nodes[index].kind;
            let start = self.tree.nodes[index].start_line;
            let next = match kind {
                NodeKind::Section => section_starts
                    .iter()
                    .find(|&&line| line > start)
                    .copied()
                    .unwrap_or(self.tree.lines.len()),
                NodeKind::File => {
                    let next_file = file_starts.iter().find(|&&line| line > start).copied();
                    let next_section = section_starts.iter().find(|&&line| line > start).copied();
                    match (next_file, next_section) {
                        (Some(a), Some(b)) => a.min(b),
                        (Some(a), None) => a,
                        (None, Some(b)) => b,
                        (None, None) => self.tree.lines.len(),
                    }
                }
                _ => continue,
            };
            if let Some(end) = last_nonblank(&self.tree.lines, start, next) {
                self.tree.nodes[index].end_line = end;
            }
        }
    }

    fn symbols(&self) -> Vec<Symbol> {
        let tree = &self.tree;
        let hunk_symbol = |hunk: &Node| Symbol {
            name: hunk.label.clone(),
            kind: "hunk".to_string(),
            node: hunk.id.clone(),
            start_line: hunk.start_line,
            end_line: hunk.end_line,
            children: Vec::new(),
        };
        let file_symbol = |file: &Node| Symbol {
            name: file.label.clone(),
            kind: "file".to_string(),
            node: file.id.clone(),
            start_line: file.start_line,
            end_line: file.end_line,
            children: file
                .children
                .iter()
                .filter_map(|id| tree.node(id))
                .filter(|node| node.kind == NodeKind::Hunk)
                .map(hunk_symbol)
                .collect(),
        };
        tree.nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Section)
            .map(|section| Symbol {
                name: section.label.clone(),
                kind: "section".to_string(),
                node: section.id.clone(),
                start_line: section.start_line,
                end_line: section.end_line,
                children: section
                    .children
                    .iter()
                    .filter_map(|id| tree.node(id))
                    .filter(|node| node.kind == NodeKind::File)
                    .map(file_symbol)
                    .collect(),
            })
            .collect()
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
        text: None,
    }
}

fn node(id: &str, kind: NodeKind, label: impl Into<String>, start_line: usize) -> Node {
    Node {
        id: id.to_string(),
        kind,
        label: label.into(),
        start_line,
        end_line: start_line,
        section: None,
        path: None,
        hunk_id: None,
        status: None,
        marker: None,
        line_index: None,
        children: Vec::new(),
    }
}

fn status_token(status: &str) -> &'static str {
    match status {
        "A" => "magit.file.added",
        "D" => "magit.file.deleted",
        "R" => "magit.file.renamed",
        "?" => "magit.file.untracked",
        _ => "magit.file.modified",
    }
}
