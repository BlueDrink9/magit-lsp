//! Cached, server-rendered Git history and commit selection metadata.

use crate::git::{GitCommit, GitLogOptions, GitLogPage, GitLogRow, GitRepo};
use crate::tree::{LineNode, Node, NodeKind, Token, Tree};
use crate::view::{Ctx, Scope, View, ViewKind};
use crate::Result;
use std::collections::HashSet;
use std::path::PathBuf;

/// Selection settings rendered with a log while a caller awaits a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitSelection {
    /// Human-readable purpose, such as `rebase onto`.
    pub purpose: String,
    /// Whether multiple commits may be selected.
    pub multi: bool,
    /// Visible instruction for the user.
    pub prompt: String,
    /// Whether a caller may separately accept free-form revision text.
    pub allow_free_text: bool,
}

/// A cached Git log view; all repository access occurs in refresh/load-more.
pub struct LogView {
    repo: GitRepo,
    options: GitLogOptions,
    page: GitLogPage,
    selection: Option<CommitSelection>,
    marked: HashSet<String>,
    revision: String,
}

impl LogView {
    /// Create a view and populate its initial page.
    pub fn new(
        path: PathBuf,
        options: GitLogOptions,
        selection: Option<CommitSelection>,
    ) -> Result<Self> {
        let repo = GitRepo::open(path)?;
        let page = repo.log(&options)?;
        Ok(Self {
            repo,
            options,
            page,
            selection,
            marked: HashSet::new(),
            revision: String::new(),
        })
    }

    /// Full commit identities in the cached page, in Git's traversal order.
    pub fn commits(&self) -> impl Iterator<Item = &str> {
        self.page.commits.iter().map(|commit| commit.oid.as_str())
    }

    /// Selection settings attached to this view, if it is awaiting a commit.
    pub fn selection(&self) -> Option<&CommitSelection> {
        self.selection.as_ref()
    }

    /// The free-text revision entered for this selection, if it accepts one.
    pub fn revision(&self) -> Option<&str> {
        self.selection
            .as_ref()
            .is_some_and(|selection| selection.allow_free_text)
            .then_some(self.revision.as_str())
    }

    /// Ingest one client revision of the rendered log-selection document.
    ///
    /// Field positions come from the authoritative render; the revision text
    /// is adopted only when every non-field line and the line count are
    /// unchanged. Otherwise the stored revision is kept and the result reports
    /// a repair, so the caller re-renders instead of trusting the client.
    pub fn reconcile(
        &mut self,
        document: &str,
    ) -> std::result::Result<(crate::presentation::prompt::Reconcile, Tree), String> {
        let tree = self.selection_tree();
        let fields = [("log:field:rev", "Revision")];
        let Some(values) = crate::presentation::prompt::reconcile_fields(&tree, document, &fields)?
        else {
            return Ok((
                crate::presentation::prompt::Reconcile { repaired: true },
                tree,
            ));
        };
        if let Some(value) = values.into_iter().next() {
            self.revision = value;
        }
        Ok((
            crate::presentation::prompt::Reconcile { repaired: false },
            self.selection_tree(),
        ))
    }

    /// The authoritative tree for this view's current state.
    pub fn selection_tree(&self) -> Tree {
        render(
            &self.page,
            self.selection.as_ref(),
            &self.marked,
            &self.revision,
        )
    }

    /// Selected full object IDs in log traversal order.
    ///
    /// A marked multi-selection wins, then the cursor commit. Otherwise a
    /// non-empty free-text revision is returned as entered, so the session can
    /// resolve it through the Git boundary; it is never parsed from rendered
    /// text.
    pub fn selected_commits(&self, cursor_oid: Option<&str>) -> Vec<String> {
        if self.selection.as_ref().is_some_and(|s| s.multi) && !self.marked.is_empty() {
            return self
                .page
                .commits
                .iter()
                .filter(|c| self.marked.contains(&c.oid))
                .map(|c| c.oid.clone())
                .collect();
        }
        if let Some(oid) = cursor_oid.filter(|oid| self.page.commits.iter().any(|c| c.oid == *oid))
        {
            return vec![oid.to_string()];
        }
        if let Some(revision) = self.revision() {
            let revision = revision.trim();
            if !revision.is_empty() {
                return vec![revision.to_string()];
            }
        }
        Vec::new()
    }

    /// Toggle a mark by full object ID; returns false for an unloaded commit.
    pub fn toggle_mark(&mut self, oid: &str) -> bool {
        if !self
            .selection
            .as_ref()
            .is_some_and(|selection| selection.multi)
        {
            return false;
        }
        if !self.page.commits.iter().any(|commit| commit.oid == oid) {
            return false;
        }
        if !self.marked.insert(oid.to_string()) {
            self.marked.remove(oid);
        }
        true
    }
}

impl View for LogView {
    fn kind(&self) -> ViewKind {
        ViewKind::Log
    }
    fn title(&self) -> String {
        "Log".to_string()
    }
    fn scope(&self) -> Scope {
        Scope::new(if self.selection.is_some() {
            "log.select"
        } else {
            "log"
        })
    }
    fn commit_selection(&self) -> Option<&CommitSelection> {
        self.selection()
    }
    fn selected_commits(&self, cursor_oid: Option<&str>) -> Vec<String> {
        LogView::selected_commits(self, cursor_oid)
    }
    fn toggle_commit_mark(&mut self, oid: &str) -> bool {
        self.toggle_mark(oid)
    }
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
    fn render(&mut self, _ctx: &Ctx<'_>) -> Tree {
        self.selection_tree()
    }
    fn on_refresh(&mut self) -> Result<()> {
        self.page = self.repo.log(&self.options)?;
        self.marked
            .retain(|oid| self.page.commits.iter().any(|commit| &commit.oid == oid));
        Ok(())
    }
    fn load_more(&mut self) -> Result<()> {
        if !self.page.has_more {
            return Ok(());
        }
        self.options.page_size = if self.options.page_size == 0 {
            100
        } else {
            self.options.page_size
        };
        self.options.page_size = self.options.page_size.saturating_add(100);
        self.page = self.repo.log(&self.options)?;
        self.marked
            .retain(|oid| self.page.commits.iter().any(|commit| &commit.oid == oid));
        Ok(())
    }
}

fn render(
    page: &GitLogPage,
    selection: Option<&CommitSelection>,
    marked: &HashSet<String>,
    revision: &str,
) -> Tree {
    let mut tree = Tree {
        version: 0,
        lines: Vec::new(),
        nodes: Vec::new(),
        line_nodes: Vec::new(),
        tokens: Vec::new(),
        symbols: Vec::new(),
        links: Vec::new(),
    };
    let heading = selection.map_or_else(|| "Log".to_string(), |s| format!("Log — {}", s.purpose));
    tree.lines.push(heading.clone());
    tree.line_nodes
        .push(LineNode::new("log:header", NodeKind::Header));
    tree.nodes.push(node(
        "log:header",
        NodeKind::Header,
        heading.clone(),
        None,
        0,
    ));
    if let Some(selection) = selection {
        let text = format!(
            "{}{}",
            selection.prompt,
            if selection.allow_free_text {
                " (or enter a revision)"
            } else {
                ""
            }
        );
        let index = tree.lines.len();
        tree.lines.push(text.clone());
        tree.line_nodes
            .push(LineNode::new("log:prompt", NodeKind::Message));
        tree.nodes
            .push(node("log:prompt", NodeKind::Message, text, None, index));
        if selection.allow_free_text {
            let index = tree.lines.len();
            tree.lines.push(revision.to_string());
            tree.line_nodes
                .push(LineNode::new("log:field:rev", NodeKind::Field));
            tree.nodes.push(node(
                "log:field:rev",
                NodeKind::Field,
                "Revision",
                None,
                index,
            ));
        }
    }
    let lookup: std::collections::HashMap<_, _> =
        page.commits.iter().map(|c| (c.oid.as_str(), c)).collect();
    for row in &page.rows {
        match row {
            GitLogRow::Connector(text) => {
                tree.lines.push(text.clone());
                let index = tree.lines.len() - 1;
                let id = format!("log:graph:{index}");
                tree.line_nodes.push(LineNode::new(&id, NodeKind::Blank));
                tree.nodes
                    .push(node(&id, NodeKind::Blank, text.clone(), None, index));
            }
            GitLogRow::Commit(oid) => {
                if let Some(commit) = lookup.get(oid.as_str()) {
                    commit_node(
                        &mut tree,
                        commit,
                        selection.is_some_and(|s| s.multi),
                        marked.contains(oid),
                    );
                }
            }
        }
    }
    if page.commits.is_empty() {
        tree.lines.push("(no commits)".into());
        let n = tree.lines.len() - 1;
        tree.line_nodes
            .push(LineNode::new("log:empty", NodeKind::Message));
        tree.nodes.push(node(
            "log:empty",
            NodeKind::Message,
            "(no commits)",
            None,
            n,
        ));
    }
    if page.has_more {
        let n = tree.lines.len();
        tree.lines.push("  ... load more ...".into());
        tree.line_nodes
            .push(LineNode::new("log:more", NodeKind::Message));
        tree.nodes
            .push(node("log:more", NodeKind::Message, "load more", None, n));
    }
    tree
}

fn commit_node(tree: &mut Tree, c: &GitCommit, show_marks: bool, marked: bool) {
    let id = format!("log:commit:{}", c.oid);
    let n = tree.lines.len();
    let decorations = if c.decorations.is_empty() {
        String::new()
    } else {
        format!(" ({})", c.decorations.join(", "))
    };
    let mark = if show_marks {
        if marked {
            "[x] "
        } else {
            "[ ] "
        }
    } else {
        ""
    };
    let prefix = format!(
        "{}{} {}{} ",
        c.graph_prefix,
        mark,
        &c.oid[..c.oid.len().min(8)],
        decorations
    );
    let text = format!("{prefix}{}  {}  {}", c.subject, c.author, c.date);
    tree.lines.push(text.clone());
    tree.line_nodes.push(LineNode::new(&id, NodeKind::Commit));
    let mut commit_node = node(
        &id,
        NodeKind::Commit,
        c.subject.clone(),
        Some(c.oid.clone()),
        n,
    );
    commit_node.copy = Some(c.oid.clone());
    tree.nodes.push(commit_node);
    // The rendered prefix has a separator between the mark and the hash.
    let hash_start = c.graph_prefix.len() + mark.len() + 1;
    tree.tokens.push(Token {
        line: n,
        start: hash_start,
        length: 8.min(c.oid.len()),
        token_type: "identifier".into(),
        modifiers: Vec::new(),
    });
    if !decorations.is_empty() {
        let start = hash_start + 8.min(c.oid.len());
        tree.tokens.push(Token {
            line: n,
            start,
            length: decorations.len(),
            token_type: "ref".into(),
            modifiers: Vec::new(),
        });
    }
    let author_start = prefix.len() + c.subject.len() + 2;
    let date_start = author_start + c.author.len() + 2;
    tree.tokens.push(Token {
        line: n,
        start: author_start,
        length: c.author.len(),
        token_type: "muted".into(),
        modifiers: Vec::new(),
    });
    tree.tokens.push(Token {
        line: n,
        start: date_start,
        length: c.date.len(),
        token_type: "muted".into(),
        modifiers: Vec::new(),
    });
}
/// A commit node: [`Node::new`] plus the full commit identity for selection.
fn node(
    id: &str,
    kind: NodeKind,
    label: impl Into<String>,
    commit: Option<String>,
    line: usize,
) -> Node {
    let mut node = Node::new(id, kind, label, line);
    node.commit = commit;
    node
}
