//! Standalone structured tree and identity-based editor for a paused rebase todo.

use crate::git::GitRepo;
use crate::rebase_todo::{RebaseTodo, TodoAction};
use crate::tree::{LineNode, Node, NodeKind, Tree};
use crate::view::{Ctx, Scope, View, ViewKind};
use crate::{Error, Result};

/// Editable paused-rebase todo with its owning repository.
pub struct RebaseTodoView {
    repo: GitRepo,
    todo: RebaseTodo,
}

impl RebaseTodoView {
    /// Load the current paused todo from `repo`.
    pub fn load(repo: GitRepo) -> Result<Self> {
        let todo = RebaseTodo::load(&repo)?;
        Ok(Self { repo, todo })
    }

    /// Render todo entries as structured commit rows.
    pub fn render(&self) -> Tree {
        let mut tree = Tree { version: 0, lines: Vec::new(), nodes: Vec::new(), line_nodes: Vec::new(), tokens: Vec::new(), symbols: Vec::new(), links: Vec::new() };
        for entry in &self.todo.entries {
            let line = tree.lines.len();
            let id = format!("rebase-todo:{}", entry.oid);
            let action = action_text(entry.action);
            tree.lines.push(format!("{action} {} {}", &entry.oid[..12.min(entry.oid.len())], entry.subject));
            tree.line_nodes.push(LineNode { node: id.clone(), kind: NodeKind::Commit, section: None, path: None, hunk_id: None, status: Some(action.to_owned()), marker: None, line_index: None, new_line: None, text: None });
            tree.nodes.push(Node { id, kind: NodeKind::Commit, label: entry.subject.clone(), start_line: line, end_line: line, section: None, path: None, hunk_id: None, status: Some(action.to_owned()), marker: None, line_index: None, new_line: None, copy: None, mutation: None, commit: Some(entry.oid.clone()), children: Vec::new() });
        }
        tree
    }

    /// Set action for the captured full commit identity, rejecting unknown identities.
    pub fn set_action(&mut self, oid: &str, action: TodoAction) -> Result<()> {
        let index = self.todo.entries.iter().position(|entry| entry.oid == oid).ok_or_else(|| Error::new("todo commit identity not found"))?;
        let previous = self.todo.entries[index].action;
        self.todo.entries[index].action = action;
        if matches!(self.todo.entries.first().map(|entry| entry.action), Some(TodoAction::Squash | TodoAction::Fixup)) {
            self.todo.entries[index].action = previous;
            return Err(Error::new("first todo action cannot squash or fixup"));
        }
        Ok(())
    }

    /// Move captured commit identity before another identity, or to the end when `before` is absent.
    pub fn move_before(&mut self, oid: &str, before: Option<&str>) -> Result<()> {
        if oid == before.unwrap_or("") { return Err(Error::new("invalid todo move")); }
        let from = self.todo.entries.iter().position(|entry| entry.oid == oid).ok_or_else(|| Error::new("todo commit identity not found"))?;
        let target = before.map(|id| self.todo.entries.iter().position(|entry| entry.oid == id).ok_or_else(|| Error::new("todo commit identity not found"))).transpose()?;
        let previous = self.todo.entries.clone();
        let entry = self.todo.entries.remove(from);
        let to = target.map(|index| if index > from { index - 1 } else { index }).unwrap_or(self.todo.entries.len());
        self.todo.entries.insert(to, entry);
        if matches!(self.todo.entries.first().map(|entry| entry.action), Some(TodoAction::Squash | TodoAction::Fixup)) {
            self.todo.entries = previous;
            return Err(Error::new("first todo action cannot squash or fixup"));
        }
        Ok(())
    }

    /// Save through guarded paused-todo persistence.
    pub fn save(&self) -> Result<()> { self.todo.save(&self.repo) }
}

impl View for RebaseTodoView {
    fn kind(&self) -> ViewKind { ViewKind::RebaseTodo }
    fn title(&self) -> String { "Rebase Todo".to_string() }
    fn scope(&self) -> Scope { Scope::new("rebase-todo") }
    fn capture(&self) -> bool { true }
    fn render(&mut self, _ctx: &Ctx<'_>) -> Tree { RebaseTodoView::render(self) }
    fn as_any(&self) -> Option<&dyn std::any::Any> { Some(self) }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> { Some(self) }
}

fn action_text(action: TodoAction) -> &'static str {
    match action { TodoAction::Pick => "pick", TodoAction::Reword => "reword", TodoAction::Edit => "edit", TodoAction::Squash => "squash", TodoAction::Fixup => "fixup", TodoAction::Drop => "drop" }
}
