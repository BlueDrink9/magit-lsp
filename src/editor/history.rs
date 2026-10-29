//! Server-lifetime, bounded commit-message retention.

use crate::{Error, Result};
use std::collections::VecDeque;

/// Maximum retained submissions per canonical repository.
pub const MAX_HISTORY_ENTRIES: usize = 50;
/// Maximum aggregate UTF-8 payload per canonical repository.
pub const MAX_HISTORY_BYTES: usize = 1024 * 1024;

/// Exact terminal saved messages, oldest first; never populated from Git logs.
#[derive(Default)]
pub struct History {
    entries: VecDeque<String>,
    bytes: usize,
}

impl History {
    #[cfg(test)]
    /// Inspect retention in private unit tests without a production recall API.
    pub(super) fn entries(&self) -> &VecDeque<String> {
        &self.entries
    }

    /// Retain a nonempty, changed submission even if Git subsequently fails.
    /// Oversized retention fails visibly to the caller without truncation/eviction.
    pub fn retain(&mut self, text: &str, initial: &str) -> Result<()> {
        if text.is_empty() || text == initial {
            return Ok(());
        }
        if text.len() > MAX_HISTORY_BYTES {
            return Err(Error::new(
                "Commit message was submitted but exceeds the 1 MiB history limit; not retained",
            ));
        }
        if self.entries.back().is_some_and(|last| last == text) {
            return Ok(());
        }
        while self.entries.len() >= MAX_HISTORY_ENTRIES
            || self.bytes + text.len() > MAX_HISTORY_BYTES
        {
            self.bytes -= self.entries.pop_front().unwrap().len();
        }
        self.bytes += text.len();
        self.entries.push_back(text.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eligibility_and_exact_consecutive_dedup() {
        let mut history = History::default();
        for text in ["", "initial", "# comment\r\n", "# comment\r\n", " ", "# comment\r\n"] {
            history.retain(text, "initial").unwrap();
        }
        assert_eq!(history.entries, ["# comment\r\n", " ", "# comment\r\n"]);
        assert_eq!(history.bytes, 23);
    }

    #[test]
    fn count_cap_evicts_oldest_without_reordering() {
        let mut history = History::default();
        for i in 0..MAX_HISTORY_ENTRIES + 2 {
            history.retain(&format!("message {i}"), "").unwrap();
        }
        assert_eq!(history.entries.len(), MAX_HISTORY_ENTRIES);
        assert_eq!(history.entries.front().unwrap(), "message 2");
        assert_eq!(history.entries.back().unwrap(), "message 51");
        assert_eq!(history.bytes, history.entries.iter().map(String::len).sum::<usize>());
    }

    #[test]
    fn utf8_byte_cap_and_oversize_rejection_are_atomic() {
        let mut history = History::default();
        let first = "🦀".repeat(MAX_HISTORY_BYTES / 8);
        let second = "é".repeat(MAX_HISTORY_BYTES / 4);
        history.retain(&first, "").unwrap();
        history.retain(&second, "").unwrap();
        assert_eq!(history.bytes, MAX_HISTORY_BYTES);
        assert!(history.retain(&"🦀".repeat(MAX_HISTORY_BYTES / 4 + 1), "").is_err());
        assert_eq!(history.entries, [first, second.clone()]);
        assert_eq!(history.bytes, MAX_HISTORY_BYTES);
        history.retain("x", "").unwrap();
        assert_eq!(history.entries, [second, "x".into()]);
        assert_eq!(history.bytes, MAX_HISTORY_BYTES / 2 + 1);
        let limit = "é".repeat(MAX_HISTORY_BYTES / 2);
        history.retain(&limit, "").unwrap();
        assert_eq!(history.entries, [limit]);
        assert_eq!(history.bytes, MAX_HISTORY_BYTES);
    }
}
