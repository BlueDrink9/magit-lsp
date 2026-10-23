//! Per-view key handling: pending chords and key → command resolution (R-2.3).
//!
//! A client sends one canonical key step at a time via `view/key`. The server
//! accumulates steps until they match a binding, then runs the command through
//! the same path as `view/invoke` so the two entry points can never diverge.

use crate::keymap::{EditorMode, Keymap, Match};
use crate::keys::{KeyChord, KeyStep};
use crate::Result;

/// What one key press means inside a menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuResolution {
    /// The pending steps changed; re-render and push the live key set.
    Pending,
    /// Nothing happened, e.g. `Backspace` with no pending step.
    Ignored,
    /// A complete item key was typed; run that item.
    Complete(String),
    /// Close the menu and return to the previous view.
    Pop,
    /// Save the current arguments as the menu's defaults.
    SaveArgs,
    /// Restore the menu's saved arguments.
    ResetArgs,
    /// Unbound in a capture view; swallow it with a one-line hint.
    Unbound { chord: String },
}

/// Resolve one key step against the menu's own item keys.
///
/// `pending` holds the canonical key steps typed so far (`-`, `=` or a chord
/// prefix such as `g`). `keys` are the menu's item keys, so an item may be a
/// single key (`c`), a two-step flag (`-a`) or a chord (`g l` in the `vim`
/// profile's dispatch menu). Menus capture every key, so an unbound one is
/// swallowed rather than passed back to the editor. A non-matching key keeps
/// `pending`, so the user can correct it with `Backspace` instead of starting
/// over. `Escape`, `ctrl+c` and `q` always close.
pub fn resolve_menu(pending: &mut Vec<String>, key: &str, keys: &[String]) -> MenuResolution {
    if matches!(key, "Escape" | "ctrl+c" | "q") {
        pending.clear();
        return MenuResolution::Pop;
    }
    if key == "ctrl+s" {
        pending.clear();
        return MenuResolution::SaveArgs;
    }
    if key == "ctrl+r" {
        pending.clear();
        return MenuResolution::ResetArgs;
    }
    if key == "Backspace" {
        return if pending.pop().is_some() {
            MenuResolution::Pending
        } else {
            MenuResolution::Ignored
        };
    }

    if pending.is_empty() {
        return match key {
            "-" | "=" => {
                pending.push(key.to_string());
                MenuResolution::Pending
            }
            _ => action_or_pending(pending, key, keys),
        };
    }

    // `-` and `=` are literal key characters, so their step concatenates
    // (`-` + `a` = `-a`); any other step starts a space-separated chord.
    let combined = if pending.len() == 1 && matches!(pending[0].as_str(), "-" | "=") {
        format!("{}{key}", pending[0])
    } else {
        format!("{} {key}", pending.join(" "))
    };

    if keys.iter().any(|candidate| candidate == &combined) {
        pending.clear();
        return MenuResolution::Complete(combined);
    }
    if keys
        .iter()
        .any(|candidate| candidate.starts_with(&format!("{combined} ")))
    {
        pending.push(key.to_string());
        return MenuResolution::Pending;
    }
    MenuResolution::Unbound { chord: combined }
}

/// Match one key step against the menu's item keys.
fn action_or_pending(pending: &mut Vec<String>, key: &str, keys: &[String]) -> MenuResolution {
    if keys.iter().any(|candidate| candidate == key) {
        return MenuResolution::Complete(key.to_string());
    }
    if keys
        .iter()
        .any(|candidate| candidate.starts_with(&format!("{key} ")))
    {
        pending.push(key.to_string());
        return MenuResolution::Pending;
    }
    MenuResolution::Unbound {
        chord: key.to_string(),
    }
}

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
