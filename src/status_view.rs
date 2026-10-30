//! Render the status view: `Status` → node tree → text and structure.
//!
//! The text format is the one the Neovim client has always shown, so clients
//! only replace buffer lines and never compose the view themselves.

use crate::git::{FileEntry, GitRepo, Status};
use crate::patch::Hunk;
use crate::tree::{
    last_nonblank, LineNode, Link, Node, NodeKind, Symbol, Token, Tree, Visibility, VisibilityState,
};
use crate::view::{Ctx, Scope, View, ViewKind, VisibilityCommand};
use crate::Result;
use std::collections::HashMap;

/// The shallowest visibility level: section headers only.
const LEVEL_MIN: u8 = 1;
/// The deepest visibility level: sections, files, hunks and hunk bodies.
const LEVEL_MAX: u8 = 4;
/// The level the status view opens at, which matches the old full render.
const LEVEL_DEFAULT: u8 = LEVEL_MAX;

/// A live status view: the repository root plus the last read status.
pub struct StatusView {
    repo: GitRepo,
    /// The last listing: branch and file entries, without any hunks.
    status: Status,
    /// Hunks loaded for visible files this refresh, keyed by `(path, staged)`.
    hunks: HashMap<(String, bool), Vec<Hunk>>,
    /// Per-view visibility, keyed by node id so it survives a refresh.
    visibility: Visibility,
    /// The last visibility level set by `more-detail`/`less-detail`.
    level: u8,
    /// Optional maximum rendered line count for a section.
    section_line_limit: Option<usize>,
}

impl StatusView {
    /// Read the repository status and prepare a view for it.
    pub fn open(repo_path: &str) -> Result<Self> {
        let repo = GitRepo::open(repo_path)?;
        let status = repo.status_listing()?;
        Ok(Self {
            repo,
            status,
            hunks: HashMap::new(),
            visibility: Visibility::new(),
            level: LEVEL_DEFAULT,
            section_line_limit: None,
        })
    }

    /// Render the visible tree, loading hunks only for expanded files.
    fn render_tree(&mut self) -> Tree {
        for (section, path) in self.file_keys() {
            if section == "untracked"
                || !self.visibility.is_expanded(&format!("section:{section}"))
                || !self.visibility.is_expanded(&format!("{section}/{path}"))
            {
                continue;
            }
            let key = (path.clone(), section == "staged");
            if !self.hunks.contains_key(&key) {
                let loaded = match self.repo.file_hunks(&path, key.1) {
                    Ok(hunks) => hunks,
                    Err(error) => {
                        tracing::warn!(path = %path, %error, "failed to load file body");
                        Vec::new()
                    }
                };
                self.hunks.insert(key.clone(), loaded);
            }
            if self.level < 4 {
                for hunk in self.hunks.get(&key).into_iter().flatten() {
                    self.visibility.set(
                        format!("{section}/{path}/hunk:{}", hunk.id),
                        VisibilityState::Collapsed,
                    );
                }
            }
        }
        let status = &self.status;
        let visibility = &self.visibility;
        let hunks = &mut self.hunks;
        build_with_limit(status, visibility, self.section_line_limit, |section, file| {
            if section == "untracked" {
                return Vec::new();
            }
            let staged = section == "staged";
            let key = (file.path.clone(), staged);
            hunks.get(&key).cloned().unwrap_or_default()
        })
    }

    /// All `(section, path)` pairs in the listing, cloned so callers may mutate.
    fn file_keys(&self) -> Vec<(String, String)> {
        ["staged", "unstaged", "untracked"]
            .iter()
            .flat_map(|section| {
                self.entries(section)
                    .iter()
                    .map(move |file| (section.to_string(), file.path.clone()))
            })
            .collect()
    }

    fn entries(&self, section: &str) -> &[FileEntry] {
        match section {
            "staged" => &self.status.staged,
            "unstaged" => &self.status.unstaged,
            _ => &self.status.untracked,
        }
    }

    /// The hunk node ids loaded for one file, if its body was ever visible.
    fn hunk_ids(&self, section: &str, path: &str) -> Vec<String> {
        let key = (path.to_string(), section == "staged");
        self.hunks
            .get(&key)
            .map(|hunks| {
                hunks
                    .iter()
                    .map(|hunk| format!("{section}/{path}/hunk:{}", hunk.id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Apply a `more-detail`/`less-detail` level to every known node.
    fn apply_level(&mut self, level: u8) {
        self.level = level.clamp(LEVEL_MIN, LEVEL_MAX);
        let level = self.level;
        let section_state = if level >= 2 {
            VisibilityState::Expanded
        } else {
            VisibilityState::Collapsed
        };
        let hunk_state = if level >= 3 {
            VisibilityState::Expanded
        } else {
            VisibilityState::Collapsed
        };
        let body_state = if level >= 4 {
            VisibilityState::Expanded
        } else {
            VisibilityState::Collapsed
        };
        for section in ["staged", "unstaged", "untracked"] {
            self.visibility
                .set(format!("section:{section}"), section_state);
        }
        for (section, path) in self.file_keys() {
            self.visibility.set(format!("{section}/{path}"), hunk_state);
            for hunk_id in self.hunk_ids(&section, &path) {
                self.visibility.set(hunk_id, body_state);
            }
        }
    }

    /// Toggle/show/hide the node at `line`, optionally with its descendants.
    fn apply_at(
        &mut self,
        tree: &Tree,
        line: Option<usize>,
        state: Option<VisibilityState>,
        recursive: bool,
    ) -> bool {
        let Some(node) = line.and_then(|line| {
            tree.line_nodes
                .get(line)
                .and_then(|entry| tree.node(&entry.node))
        }) else {
            return false;
        };
        let Some(target) = tree.collapsible_ancestor(&node.id) else {
            return false;
        };
        let state = state.unwrap_or_else(|| {
            if self.visibility.is_expanded(&target.id) {
                VisibilityState::Collapsed
            } else {
                VisibilityState::Expanded
            }
        });
        self.visibility.set(target.id.clone(), state);
        if recursive {
            for id in self.descendants(target) {
                self.visibility.set(id, state);
            }
        }
        true
    }

    /// The collapsible descendants of `node`: files below a section, hunks
    /// below a file. Lines follow their hunk, so they carry no separate state.
    fn descendants(&self, node: &Node) -> Vec<String> {
        match node.kind {
            NodeKind::Section => {
                let section = node.section.as_deref().unwrap_or_default();
                self.file_keys()
                    .into_iter()
                    .filter(|(key, _)| key == section)
                    .flat_map(|(key, path)| {
                        let file_id = format!("{key}/{path}");
                        let mut ids = vec![file_id];
                        ids.extend(self.hunk_ids(&key, &path));
                        ids
                    })
                    .collect()
            }
            NodeKind::File => {
                let section = node.section.as_deref().unwrap_or_default();
                let path = node.path.as_deref().unwrap_or_default();
                self.hunk_ids(section, path)
            }
            _ => Vec::new(),
        }
    }
}

impl View for StatusView {
    fn kind(&self) -> ViewKind {
        ViewKind::Status
    }

    fn title(&self) -> String {
        "Status".to_string()
    }

    fn scope(&self) -> Scope {
        Scope::new("status")
    }

    fn render(&mut self, _ctx: &Ctx<'_>) -> Tree {
        self.render_tree()
    }

    fn set_section_line_limit(&mut self, limit: Option<usize>) -> bool {
        self.section_line_limit = limit.filter(|limit| *limit > 0);
        true
    }

    fn on_refresh(&mut self) -> Result<()> {
        self.status = self.repo.status_listing()?;
        // Hunk bodies are reloaded lazily from the fresh listing.
        self.hunks.clear();
        Ok(())
    }

    fn set_visibility(
        &mut self,
        tree: &Tree,
        line: Option<usize>,
        command: VisibilityCommand,
    ) -> bool {
        match command {
            VisibilityCommand::ShowAll => {
                self.visibility.show_all();
                self.level = LEVEL_MAX;
                true
            }
            VisibilityCommand::HideAll => {
                self.apply_level(LEVEL_MIN);
                true
            }
            VisibilityCommand::MoreDetail => {
                self.apply_level(self.level.saturating_add(1));
                true
            }
            VisibilityCommand::LessDetail => {
                self.apply_level(self.level.saturating_sub(1));
                true
            }
            VisibilityCommand::SetDetail(level) => {
                self.apply_level(level);
                true
            }
            VisibilityCommand::Show { recursive } => {
                self.apply_at(tree, line, Some(VisibilityState::Expanded), recursive)
            }
            VisibilityCommand::Hide { recursive } => {
                self.apply_at(tree, line, Some(VisibilityState::Collapsed), recursive)
            }
            VisibilityCommand::Toggle { recursive } => self.apply_at(tree, line, None, recursive),
        }
    }
}

/// Build the status view tree from a Git status.
pub fn build(status: &Status) -> Tree {
    // The tree-only builder keeps the old fully-expanded rendering.
    build_with(status, &Visibility::new(), |section, file| {
        if section == "untracked" {
            Vec::new()
        } else {
            file.hunks.clone()
        }
    })
}

/// Build the status view tree, using `visibility` and loading hunks only for
/// files whose body is visible.
pub(crate) fn build_with<F>(status: &Status, visibility: &Visibility, mut load: F) -> Tree
where
    F: FnMut(&str, &FileEntry) -> Vec<Hunk>,
{
    build_with_limit(status, visibility, None, &mut load)
}

/// Build with an optional rendered-line bound per section.
pub fn build_with_limit<F>(status: &Status, visibility: &Visibility, limit: Option<usize>, mut load: F) -> Tree
where
    F: FnMut(&str, &FileEntry) -> Vec<Hunk>,
{
    let full = Builder::new(visibility, &mut load, &HashMap::new()).build(status);
    let Some(limit) = limit else {
        return full;
    };
    let summaries: HashMap<String, usize> = full
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Section)
        .filter_map(|section| {
            if visibility.explicit_state(&section.id) == Some(VisibilityState::Expanded) {
                return None;
            }
            let body_lines = section.end_line.saturating_sub(section.start_line);
            (body_lines > limit)
                .then(|| (section.section.clone().unwrap_or_default(), body_lines))
        })
        .collect();
    if summaries.is_empty() {
        return full;
    }
    let mut builder = Builder::new(visibility, &mut load, &summaries);
    builder.populate(status);
    builder.finish()
}

struct Builder<'a, F> {
    tree: Tree,
    visibility: &'a Visibility,
    load: &'a mut F,
    summaries: &'a HashMap<String, usize>,
}

impl<'a, F> Builder<'a, F>
where
    F: FnMut(&str, &FileEntry) -> Vec<Hunk>,
{
    fn new(visibility: &'a Visibility, load: &'a mut F, summaries: &'a HashMap<String, usize>) -> Self {
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
            visibility,
            load,
            summaries,
        }
    }

    fn build(mut self, status: &Status) -> Tree {
        self.populate(status);
        self.finish()
    }

    fn populate(&mut self, status: &Status) {
        self.header(status);
        self.section("staged", "Staged changes", &status.staged);
        self.section("unstaged", "Unstaged changes", &status.unstaged);
        self.section("untracked", "Untracked files", &status.untracked);
        if status.staged.is_empty() && status.unstaged.is_empty() && status.untracked.is_empty() {
            self.message();
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

        if let Some(hidden) = self.summaries.get(key).copied() {
            let id = format!("{section_id}:overflow");
            let text = format!("{hidden} more lines: expand");
            let row = self.tree.lines.len();
            self.tree.lines.push(text.clone());
            self.tree.line_nodes.push(line_node(&id, NodeKind::Message));
            self.tree.nodes.push(node(&id, NodeKind::Message, text, row));
            self.tree.nodes[index].children = vec![id];
        } else if self.visibility.is_expanded(&section_id) {
            let children = files
                .iter()
                .map(|file| self.file(key, file))
                .collect::<Vec<_>>();
            self.tree.nodes[index].children = children;
        }
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
        file_node.mutation = file.mutation_identity.clone().map(crate::tree::MutationContent::File);
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

        if self.visibility.is_expanded(&file_id) {
            let hunks = (self.load)(section, file);
            let children = hunks
                .iter()
                .map(|hunk| {
                    let identical = hunks.iter().filter(|other| other.same_content(hunk)).count();
                    self.hunk(section, file, &file_id, hunk, identical)
                })
                .collect::<Vec<_>>();
            self.tree.nodes[index].children = children;
        }
        file_id
    }

    fn hunk(&mut self, section: &str, file: &FileEntry, file_id: &str, hunk: &Hunk, identical: usize) -> String {
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
        entry.new_line = Some(hunk.new_start);
        self.tree.line_nodes.push(entry);

        let index = self.tree.nodes.len();
        let mut hunk_node = node(&hunk_id, NodeKind::Hunk, "@@", line);
        hunk_node.section = Some(section.to_string());
        hunk_node.path = Some(file.path.clone());
        hunk_node.hunk_id = Some(hunk.id.clone());
        hunk_node.new_line = Some(hunk.new_start);
        hunk_node.copy = Some(hunk_source_text(hunk));
        hunk_node.mutation = Some(crate::tree::MutationContent::Hunk { hunk: hunk.clone(), identical });
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

        // A removed line has no new-side line; it visits the new line before it.
        let mut last_new = hunk.new_start;
        let lines: &[_] = if self.visibility.is_expanded(&hunk_id) {
            &hunk.lines
        } else {
            &[]
        };
        let children = lines
            .iter()
            .enumerate()
            .map(|(offset, diff_line)| {
                let line = self.tree.lines.len();
                let text = format!("    {}{}", diff_line.marker, diff_line.text);
                self.tree.lines.push(text.clone());
                let id = format!("{hunk_id}/line:{offset}");

                let new_line = match diff_line.marker {
                    '+' | ' ' => diff_line.new_no.unwrap_or(last_new),
                    _ => last_new,
                };
                if matches!(diff_line.marker, '+' | ' ') {
                    last_new = new_line;
                }

                let mut entry = line_node(&id, NodeKind::Line);
                entry.section = Some(section.to_string());
                entry.path = Some(file.path.clone());
                entry.hunk_id = Some(hunk.id.clone());
                entry.marker = Some(diff_line.marker);
                entry.line_index = Some(offset);
                entry.new_line = Some(new_line);
                entry.text = Some(diff_line.text.clone());
                self.tree.line_nodes.push(entry);

                let mut line_node = node(&id, NodeKind::Line, diff_line.text.clone(), line);
                if matches!(diff_line.marker, '+' | '-' | ' ') {
                    line_node.copy = Some(diff_line.text.clone());
                }
                line_node.section = Some(section.to_string());
                line_node.path = Some(file.path.clone());
                line_node.hunk_id = Some(hunk.id.clone());
                line_node.marker = Some(diff_line.marker);
                line_node.line_index = Some(offset);
                line_node.new_line = Some(new_line);
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
            detail: None,
            kind: "hunk".to_string(),
            node: hunk.id.clone(),
            start_line: hunk.start_line,
            end_line: hunk.end_line,
            children: Vec::new(),
        };
        let file_symbol = |file: &Node| Symbol {
            name: file.label.clone(),
            detail: file.status.clone(),
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
                detail: Some(section.children.len().to_string()),
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
        new_line: None,
        text: None,
    }
}

/// Source content from each valid hunk line, without markers or metadata.
fn hunk_source_text(hunk: &Hunk) -> String {
    let mut text = String::new();
    let mut first = true;
    for line in &hunk.lines {
        if !matches!(line.marker, '+' | '-' | ' ') {
            continue;
        }
        if !first {
            text.push('\n');
        }
        text.push_str(&line.text);
        first = false;
    }
    text
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
        new_line: None,
        copy: None,
        mutation: None,
        commit: None,
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
