//! Typed parser for the supported linear interactive-rebase todo grammar.

use crate::git::GitRepo;
use crate::{Error, Result};

/// Supported operation for one rebase todo entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoAction { Pick, Reword, Edit, Squash, Fixup, Drop }

/// One action and its canonical commit identity and subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoEntry { pub action: TodoAction, pub oid: String, pub subject: String }

/// Parsed todo entries with original source bytes retained for stale checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebaseTodo { pub entries: Vec<TodoEntry>, pub source: Vec<u8> }

impl RebaseTodo {
    /// Parse todo bytes, resolving each reference to a unique commit object.
    pub fn parse(repo: &GitRepo, source: &[u8]) -> Result<Self> {
        let text = std::str::from_utf8(source).map_err(|_| Error::new("todo is not UTF-8"))?;
        let mut entries = Vec::new();
        for (line_no, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            let mut fields = line.splitn(3, char::is_whitespace).filter(|s| !s.is_empty());
            let command = fields.next().unwrap_or("");
            let action = match command { "p"|"pick"=>TodoAction::Pick, "r"|"reword"=>TodoAction::Reword, "e"|"edit"=>TodoAction::Edit, "s"|"squash"=>TodoAction::Squash, "f"|"fixup"=>TodoAction::Fixup, "d"|"drop"=>TodoAction::Drop, _=>return Err(Error::new(format!("unsupported todo command on line {}", line_no+1))) };
            let reference = fields.next().ok_or_else(|| Error::new("todo action missing commit"))?;
            let subject = fields.next().ok_or_else(|| Error::new("todo action missing subject"))?;
            if subject.starts_with('-') { return Err(Error::new("todo options are unsupported")); }
            let oid = repo.resolve_commit(reference)?;
            if entries.iter().any(|entry: &TodoEntry| entry.oid == oid) { return Err(Error::new("duplicate todo commit identity")); }
            entries.push(TodoEntry { action, oid, subject: subject.to_owned() });
        }
        if matches!(entries.first().map(|e| e.action), Some(TodoAction::Squash|TodoAction::Fixup)) { return Err(Error::new("first todo action cannot squash or fixup")); }
        Ok(Self { entries, source: source.to_vec() })
    }
}
