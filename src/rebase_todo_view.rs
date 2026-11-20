//! Standalone structured tree and identity-based editor for a paused rebase todo.

use crate::git::GitRepo;
use crate::rebase_plan::PreparedRebase;
use crate::rebase_todo::{RebaseTodo, TodoAction};
use crate::tree::{LineNode, Node, NodeKind, Tree};
use crate::view::{Ctx, Scope, View, ViewKind};
use crate::{Error, Result};

/// Editable paused-rebase todo with its owning repository.
pub struct RebaseTodoView {
    owner: TodoOwner,
}

enum TodoOwner {
    Paused(GitRepo, RebaseTodo),
    Prepared(GitRepo, PreparedRebase),
}

/// Outcome of submitting a paused todo or prepared initial rebase.
pub enum SubmitOutcome {
    Saved,
    Started(Result<()>),
}

impl RebaseTodoView {
    /// Load the current paused todo from `repo`.
    pub fn load(repo: GitRepo) -> Result<Self> {
        let todo = RebaseTodo::load(&repo)?;
        Ok(Self {
            owner: TodoOwner::Paused(repo, todo),
        })
    }

    /// Create an in-memory editor for a prepared initial rebase plan.
    pub fn prepared(repo: GitRepo, plan: PreparedRebase) -> Self {
        Self {
            owner: TodoOwner::Prepared(repo, plan),
        }
    }

    fn entries(&self) -> &[crate::rebase_todo::TodoEntry] {
        match &self.owner {
            TodoOwner::Paused(_, todo) => &todo.entries,
            TodoOwner::Prepared(_, plan) => plan.entries(),
        }
    }

    fn edit_entries(
        &mut self,
        edit: impl FnOnce(&mut Vec<crate::rebase_todo::TodoEntry>) -> Result<()>,
    ) -> Result<()> {
        match &mut self.owner {
            TodoOwner::Paused(_, todo) => {
                let previous = todo.entries.clone();
                let result = edit(&mut todo.entries);
                if result.is_err() {
                    todo.entries = previous;
                }
                result
            }
            TodoOwner::Prepared(_, plan) => {
                let mut entries = plan.entries().to_vec();
                edit(&mut entries)?;
                plan.replace_entries(entries)
            }
        }
    }

    /// Render todo entries as structured commit rows.
    pub fn render(&self) -> Tree {
        let mut tree = Tree {
            version: 0,
            lines: Vec::new(),
            nodes: Vec::new(),
            line_nodes: Vec::new(),
            tokens: Vec::new(),
            symbols: Vec::new(),
            links: Vec::new(),
        };
        for entry in self.entries() {
            let line = tree.lines.len();
            let id = format!("rebase-todo:{}", entry.oid);
            let action = action_text(entry.action);
            tree.lines.push(format!(
                "{action} {} {}",
                &entry.oid[..12.min(entry.oid.len())],
                entry.subject
            ));
            tree.line_nodes.push(LineNode {
                node: id.clone(),
                kind: NodeKind::Commit,
                section: None,
                path: None,
                hunk_id: None,
                status: Some(action.to_owned()),
                marker: None,
                line_index: None,
                new_line: None,
                text: None,
            });
            tree.nodes.push(Node {
                id,
                kind: NodeKind::Commit,
                label: entry.subject.clone(),
                start_line: line,
                end_line: line,
                section: None,
                path: None,
                hunk_id: None,
                status: Some(action.to_owned()),
                marker: None,
                line_index: None,
                new_line: None,
                copy: None,
                mutation: None,
                commit: Some(entry.oid.clone()),
                children: Vec::new(),
            });
        }
        tree
    }

    /// Set action for the captured full commit identity, rejecting unknown identities.
    pub fn set_action(&mut self, oid: &str, action: TodoAction) -> Result<()> {
        self.edit_entries(|entries| {
            let index = entries
                .iter()
                .position(|entry| entry.oid == oid)
                .ok_or_else(|| Error::new("todo commit identity not found"))?;
            entries[index].action = action;
            if matches!(
                entries.first().map(|entry| entry.action),
                Some(TodoAction::Squash | TodoAction::Fixup)
            ) {
                return Err(Error::new("first todo action cannot squash or fixup"));
            }
            Ok(())
        })
    }

    /// Move captured commit identity before another identity, or to the end when `before` is absent.
    pub fn move_before(&mut self, oid: &str, before: Option<&str>) -> Result<()> {
        if oid == before.unwrap_or("") {
            return Err(Error::new("invalid todo move"));
        }
        self.edit_entries(|entries| {
            let from = entries
                .iter()
                .position(|entry| entry.oid == oid)
                .ok_or_else(|| Error::new("todo commit identity not found"))?;
            let target = before
                .map(|id| {
                    entries
                        .iter()
                        .position(|entry| entry.oid == id)
                        .ok_or_else(|| Error::new("todo commit identity not found"))
                })
                .transpose()?;
            let previous = entries.clone();
            let entry = entries.remove(from);
            let to = target
                .map(|index| if index > from { index - 1 } else { index })
                .unwrap_or(entries.len());
            entries.insert(to, entry);
            if matches!(
                entries.first().map(|entry| entry.action),
                Some(TodoAction::Squash | TodoAction::Fixup)
            ) {
                *entries = previous;
                return Err(Error::new("first todo action cannot squash or fixup"));
            }
            Ok(())
        })
    }

    /// Move selected full commit identity one position up or down.
    pub fn move_relative(&mut self, oid: &str, up: bool) -> Result<()> {
        let entries = self.entries();
        let index = entries
            .iter()
            .position(|entry| entry.oid == oid)
            .ok_or_else(|| Error::new("todo commit identity not found"))?;
        let target = if up {
            index.checked_sub(1)
        } else if index + 1 < entries.len() {
            Some(index + 1)
        } else {
            None
        };
        let Some(target) = target else {
            return Ok(());
        };
        let before = if target < index {
            Some(entries[target].oid.clone())
        } else {
            entries.get(target + 1).map(|entry| entry.oid.clone())
        };
        self.move_before(oid, before.as_deref())
    }

    /// Save through guarded paused-todo persistence.
    pub fn save(&self) -> Result<()> {
        match &self.owner {
            TodoOwner::Paused(repo, todo) => todo.save(repo),
            TodoOwner::Prepared(_, _) => Err(Error::new(
                "prepared rebase todo cannot be saved as a paused todo",
            )),
        }
    }

    /// Submit a paused todo by saving it, or launch a prepared initial rebase.
    pub fn submit(&self) -> Result<SubmitOutcome> {
        match &self.owner {
            TodoOwner::Paused(repo, todo) => {
                todo.save(repo)?;
                Ok(SubmitOutcome::Saved)
            }
            TodoOwner::Prepared(repo, plan) => {
                match plan.launch_outcome(repo, &std::env::current_exe()?)? {
                    crate::rebase_plan::LaunchOutcome::Started(result) => {
                        Ok(SubmitOutcome::Started(result))
                    }
                }
            }
        }
    }
}

impl View for RebaseTodoView {
    fn kind(&self) -> ViewKind {
        ViewKind::RebaseTodo
    }
    fn title(&self) -> String {
        "Rebase Todo".to_string()
    }
    fn scope(&self) -> Scope {
        Scope::new("rebase-todo")
    }
    fn capture(&self) -> bool {
        true
    }
    fn render(&mut self, _ctx: &Ctx<'_>) -> Tree {
        RebaseTodoView::render(self)
    }
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

fn action_text(action: TodoAction) -> &'static str {
    match action {
        TodoAction::Pick => "pick",
        TodoAction::Reword => "reword",
        TodoAction::Edit => "edit",
        TodoAction::Squash => "squash",
        TodoAction::Fixup => "fixup",
        TodoAction::Drop => "drop",
    }
}
