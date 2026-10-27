//! Server-lifetime, bounded commit-message retention and per-draft navigation.

use crate::protocol::DraftHistoryCommand;
use crate::{Error, Result};
use std::collections::VecDeque;

/// Maximum retained submissions per canonical repository.
pub const MAX_HISTORY_ENTRIES: usize = 50;
/// Maximum aggregate UTF-8 payload per canonical repository.
pub const MAX_HISTORY_BYTES: usize = 1024 * 1024;

/// Exact submitted messages, oldest first; never populated from logs or saves.
#[derive(Default)]
pub struct History {
    entries: VecDeque<String>,
    bytes: usize,
}

impl History {
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

/// A browsing snapshot survives ring eviction without changing navigation order.
/// Returning to newest ends browsing; the next previous preserves the new draft.
#[derive(Default)]
pub(super) struct Navigation {
    browsing: Option<Browse>,
}

struct Browse {
    entries: Vec<String>,
    position: usize,
    unsent: String,
}

impl Navigation {
    pub(super) fn recall(
        &mut self,
        history: &History,
        command: DraftHistoryCommand,
        current: &str,
    ) -> String {
        if self.browsing.is_none() && command == DraftHistoryCommand::Previous {
            if history.entries.is_empty() {
                return current.to_string();
            }
            self.browsing = Some(Browse {
                entries: history.entries.iter().cloned().collect(),
                position: history.entries.len(),
                unsent: current.to_string(),
            });
        }
        let Some(browse) = &mut self.browsing else {
            return current.to_string();
        };
        match command {
            DraftHistoryCommand::Previous => {
                browse.position = browse.position.saturating_sub(1);
            }
            DraftHistoryCommand::Next => browse.position += 1,
        }
        if browse.position == browse.entries.len() {
            let text = browse.unsent.clone();
            self.browsing = None;
            text
        } else {
            browse.entries[browse.position].clone()
        }
    }
}
