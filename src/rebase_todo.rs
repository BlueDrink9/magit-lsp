//! Typed parser for the supported linear interactive-rebase todo grammar.

use crate::git::{GitRepo, RebaseTodoOwner};
use crate::{Error, Result};

/// Supported operation for one rebase todo entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoAction { Pick, Reword, Edit, Squash, Fixup, Drop }

/// One action and its canonical commit identity and subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoEntry { pub action: TodoAction, pub oid: String, pub subject: String }

/// Parsed todo entries with original source bytes retained for stale checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebaseTodo { pub entries: Vec<TodoEntry>, source: Vec<u8>, identities: Vec<String>, original: Vec<TodoEntry>, owner: Option<RebaseTodoOwner> }

impl RebaseTodo {
    /// Return the immutable todo bytes captured when this model was parsed.
    pub fn source(&self) -> &[u8] { &self.source }

    /// Parse todo bytes, resolving each reference to a unique commit object.
    pub fn parse(repo: &GitRepo, source: &[u8]) -> Result<Self> {
        let text = std::str::from_utf8(source).map_err(|_| Error::new("todo is not UTF-8"))?;
        let mut entries = Vec::new();
        for (line_no, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            let line = line.trim_start();
            let mut fields = line.split_whitespace();
            let command = fields.next().unwrap_or("");
            let action = match command { "p"|"pick"=>TodoAction::Pick, "r"|"reword"=>TodoAction::Reword, "e"|"edit"=>TodoAction::Edit, "s"|"squash"=>TodoAction::Squash, "f"|"fixup"=>TodoAction::Fixup, "d"|"drop"=>TodoAction::Drop, _=>return Err(Error::new(format!("unsupported todo command on line {}", line_no+1))) };
            let reference = fields.next().ok_or_else(|| Error::new("todo action missing commit"))?;
            let remainder = line.find(reference).map(|at| &line[at + reference.len()..]).unwrap_or("");
            let subject = remainder.trim_start();
            if subject.is_empty() { return Err(Error::new("todo action missing subject")); }
            let oid = repo.resolve_todo_commit(reference)?;
            if entries.iter().any(|entry: &TodoEntry| entry.oid == oid) { return Err(Error::new("duplicate todo commit identity")); }
            entries.push(TodoEntry { action, oid, subject: subject.to_owned() });
        }
        if matches!(entries.first().map(|e| e.action), Some(TodoAction::Squash|TodoAction::Fixup)) { return Err(Error::new("first todo action cannot squash or fixup")); }
        let identities = entries.iter().map(|entry| entry.oid.clone()).collect();
        Ok(Self { entries: entries.clone(), source: source.to_vec(), identities, original: entries, owner: None })
    }

    /// Load and parse the todo owned by this repository's paused merge-backend rebase.
    pub fn load(repo: &GitRepo) -> Result<Self> {
        let snapshot = repo.read_rebase_todo_snapshot()?;
        let mut todo = Self::parse(repo, &snapshot.source)?;
        todo.owner = Some(snapshot.owner);
        Ok(todo)
    }

    /// Persist edited actions/order only when the captured paused todo remains unchanged.
    pub fn save(&self, repo: &GitRepo) -> Result<()> {
        let owner = self.owner.as_ref().ok_or_else(|| Error::new("parse-only rebase todo cannot be saved"))?;
        if &repo.rebase_todo_identity()? != owner { return Err(Error::new("rebase todo operation changed since it was loaded")); }
        if self.entries.iter().any(|entry| self.original.iter().find(|original| original.oid == entry.oid).is_none_or(|original| original.subject != entry.subject)) {
            return Err(Error::new("edited rebase todo changes a captured subject"));
        }
        if self.entries.iter().any(|entry| !self.identities.contains(&entry.oid))
            || self.entries.iter().map(|entry| &entry.oid).collect::<std::collections::HashSet<_>>().len() != self.entries.len()
            || matches!(self.entries.first().map(|entry| entry.action), Some(TodoAction::Squash | TodoAction::Fixup)) {
            return Err(Error::new("invalid edited rebase todo"));
        }
        let mut entries = self.entries.clone();
        for oid in &self.identities {
            if !entries.iter().any(|entry| &entry.oid == oid) {
                let original = self.original.iter().find(|entry| &entry.oid == oid);
                if let Some(original) = original { entries.push(TodoEntry { action: TodoAction::Drop, ..original.clone() }); }
            }
        }
        if entries.iter().map(|entry| &entry.oid).collect::<std::collections::HashSet<_>>().len() != self.identities.len() {
            return Err(Error::new("edited rebase todo omits captured commit identities"));
        }
        let mut output = String::new();
        for entry in entries {
            let action = match entry.action { TodoAction::Pick=>"pick", TodoAction::Reword=>"reword", TodoAction::Edit=>"edit", TodoAction::Squash=>"squash", TodoAction::Fixup=>"fixup", TodoAction::Drop=>"drop" };
            output.push_str(&format!("{action} {} {}\n", entry.oid, entry.subject));
        }
        repo.write_rebase_todo(owner, &self.source, output.as_bytes())
    }
}
