//! Prepared, in-memory authority for an initial linear interactive rebase.

use crate::git::GitRepo;
use crate::rebase_todo::{RebaseTodo, TodoAction};
use crate::{Error, Result};

/// An initial rebase plan, bound to the exact clean repository state it captured.
#[derive(Debug, Clone)]
pub struct PreparedRebase {
    base: String,
    head: String,
    head_reference: Option<String>,
    entries: Vec<crate::rebase_todo::TodoEntry>,
    membership: Vec<String>,
    index: String,
    worktree: String,
    repository: String,
}

/// Result after launch validation, distinguishing refusal from an attempted Git operation.
#[derive(Debug)]
pub enum LaunchOutcome { Started(Result<()>) }

impl PreparedRebase {
    /// Prepare commits after exclusive `base` through current `HEAD` for replay.
    pub fn prepare(repo: &GitRepo, base: &str) -> Result<Self> {
        let repository = repo.repository_identity()?;
        let (head, head_reference) = repo.head_identity()?;
        let base = repo.resolve_commit(base)?;
        let (index, worktree) = repo.rebase_launch_preconditions()?;
        let oids = repo.linear_replay_membership(&base, &head)?;
        if oids.is_empty() { return Err(Error::new("selected base has no commits to replay")); }
        let source = oids.iter().map(|oid| format!("pick {oid} {}\n", repo.commit_subject(oid).unwrap_or_else(|_| "(no subject)".into()))).collect::<String>();
        let todo = RebaseTodo::parse(repo, source.as_bytes())?;
        Ok(Self { base, head, head_reference, entries: todo.entries, membership: oids, index, worktree, repository })
    }

    /// Current editable ordered instructions; edits remain in memory only.
    pub fn entries(&self) -> &[crate::rebase_todo::TodoEntry] { &self.entries }

    /// Replace the plan entries after validating the captured membership and todo constraints.
    pub(crate) fn replace_entries(&mut self, entries: Vec<crate::rebase_todo::TodoEntry>) -> Result<()> {
        if entries.len() != self.membership.len() || entries.iter().any(|entry| !self.membership.contains(&entry.oid)) || entries.iter().map(|entry| &entry.oid).collect::<std::collections::HashSet<_>>().len() != entries.len() {
            return Err(Error::new("edited rebase plan membership changed"));
        }
        if matches!(entries.first().map(|entry| entry.action), Some(TodoAction::Squash | TodoAction::Fixup)) { return Err(Error::new("first todo action cannot squash or fixup")); }
        self.entries = entries;
        Ok(())
    }

    /// Change an entry action by full commit object ID.
    pub fn set_action(&mut self, oid: &str, action: TodoAction) -> Result<()> {
        let mut entries = self.entries.clone();
        let index = entries.iter().position(|entry| entry.oid == oid).ok_or_else(|| Error::new("commit is not in the prepared rebase plan"))?;
        entries[index].action = action;
        self.replace_entries(entries)
    }

    /// Move an existing commit before another, or to the end when `before` is absent.
    pub fn move_before(&mut self, oid: &str, before: Option<&str>) -> Result<()> {
        let mut entries = self.entries.clone();
        let from = entries.iter().position(|entry| entry.oid == oid).ok_or_else(|| Error::new("commit is not in the prepared rebase plan"))?;
        let to = match before { Some(id) => entries.iter().position(|candidate| candidate.oid == id).ok_or_else(|| Error::new("target is not in the prepared rebase plan"))?, None => entries.len() };
        if oid == before.unwrap_or("") { return Err(Error::new("invalid todo move")); }
        let entry = entries.remove(from);
        let to = if to > from { to - 1 } else { to };
        entries.insert(to, entry);
        self.replace_entries(entries)
    }

    /// Recheck launch inputs without changing the edited plan or repository.
    pub fn validate_launch(&self, repo: &GitRepo) -> Result<()> {
        if matches!(self.entries.first().map(|entry| entry.action), Some(TodoAction::Squash | TodoAction::Fixup)) { return Err(Error::new("first todo action cannot squash or fixup")); }
        if repo.repository_identity()? != self.repository { return Err(Error::new("repository identity changed since rebase plan preparation")); }
        let (head, reference) = repo.head_identity()?;
        if head != self.head { return Err(Error::new("HEAD changed since rebase plan preparation")); }
        if reference != self.head_reference { return Err(Error::new("HEAD attachment changed since rebase plan preparation")); }
        let (index, worktree) = repo.rebase_launch_preconditions()?;
        if index != self.index { return Err(Error::new("index changed since rebase plan preparation")); }
        if worktree != self.worktree { return Err(Error::new("worktree changed since rebase plan preparation")); }
        let current = repo.linear_replay_membership(&self.base, &self.head)?;
        let mut planned = self.entries.iter().map(|entry| entry.oid.clone()).collect::<Vec<_>>();
        planned.sort();
        let mut captured = self.membership.clone();
        captured.sort();
        if current != self.membership || planned != captured {
            return Err(Error::new("edited rebase plan membership changed"));
        }
        Ok(())
    }

    /// Revalidate captured repository state, then launch this exact edited plan.
    pub fn launch(&self, repo: &GitRepo, executable: &std::path::Path) -> Result<()> {
        match self.launch_outcome(repo, executable)? { LaunchOutcome::Started(result) => result }
    }

    /// Revalidate, then distinguish a preflight refusal from a started Git operation.
    pub fn launch_outcome(&self, repo: &GitRepo, executable: &std::path::Path) -> Result<LaunchOutcome> {
        self.validate_launch(repo)?;
        let mut todo = String::new();
        for entry in &self.entries {
            let action = match entry.action { TodoAction::Pick => "pick", TodoAction::Reword => "reword", TodoAction::Edit => "edit", TodoAction::Squash => "squash", TodoAction::Fixup => "fixup", TodoAction::Drop => "drop" };
            todo.push_str(&format!("{action} {} {}\n", entry.oid, entry.subject));
        }
        Ok(LaunchOutcome::Started(repo.launch_interactive_rebase(&self.base, todo.as_bytes(), executable)))
    }
}
