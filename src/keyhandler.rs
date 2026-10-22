//! Per-view key handling: pending chords and key → command resolution (R-2.3).
//!
//! A client sends one canonical key step at a time via `view/key`. The server
//! accumulates steps until they match a binding, then runs the command through
//! the same path as `view/invoke` so the two entry points can never diverge.

use crate::keymap::{EditorMode, Keymap, Match};
use crate::keys::{KeyChord, KeyStep};
use crate::Result;

/// What one key press resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// The steps are a chord prefix; keep collecting and return no effects.
    Pending,
    /// A complete chord selected this command.
    Run { command: String, chord: String },
    /// Unbound in a non-capture view; the client should replay the whole chord.
    PassThrough { chord: String },
    /// Unbound in a capture view; swallow it and show the chord as a hint.
    Swallow { chord: String },
}

/// The key steps collected so far for one view.
#[derive(Debug, Default, Clone)]
pub struct PendingChord {
    steps: Vec<KeyStep>,
}

impl PendingChord {
    /// Whether no steps are waiting.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Forget the pending chord.
    pub fn clear(&mut self) {
        self.steps.clear();
    }
}

/// Resolve one key step against `keymap`, appending it to `pending`.
///
/// A lone cancel key (`common.cancel`, e.g. `Escape`) discards a pending chord
/// instead of extending it, so a stuck prefix is always escapable.
///
/// When the chord is unbound, `capture` decides between passing the whole
/// chord back to the editor and swallowing it with a hint.
pub fn resolve(
    keymap: Option<&Keymap>,
    pending: &mut PendingChord,
    key: &str,
    mode: EditorMode,
    capture: bool,
) -> Result<Resolution> {
    let chord = KeyChord::parse(key).map_err(|error| crate::Error::new(error.to_string()))?;
    let steps = chord.steps();

    if !pending.is_empty() {
        if let Some(map) = keymap {
            if let Match::Exact(command) = map.lookup(steps, mode) {
                if command == "common.cancel" {
                    pending.clear();
                }
            }
        }
    }

    pending.steps.extend_from_slice(steps);
    let matched = keymap
        .map(|map| map.lookup(&pending.steps, mode))
        .unwrap_or(Match::None);
    let chord_text = KeyChord(pending.steps.clone()).to_canonical();

    Ok(match matched {
        Match::Prefix => Resolution::Pending,
        Match::Exact(command) => {
            pending.clear();
            Resolution::Run {
                command,
                chord: chord_text,
            }
        }
        Match::None => {
            pending.clear();
            if capture {
                Resolution::Swallow { chord: chord_text }
            } else {
                Resolution::PassThrough { chord: chord_text }
            }
        }
    })
}
