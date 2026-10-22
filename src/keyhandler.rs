//! Per-view key handling: pending chords and key → command resolution (R-2.3).
//!
//! A client sends one canonical key step at a time via `view/key`. The server
//! accumulates steps until they match a binding, then runs the command through
//! the same path as `view/invoke` so the two entry points can never diverge.

use crate::keymap::{EditorMode, Keymap, Match};
use crate::keys::{KeyChord, KeyStep};
use crate::Result;

/// Which prefix a menu is waiting for.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MenuPrefix {
    #[default]
    None,
    Switch,
    Option,
    /// Keys collected for a multi-step binding such as `g l`.
    Chord(String),
}

/// What one key press means inside a menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuResolution {
    /// A prefix (`-` or `=`) was consumed; keep collecting.
    Pending,
    /// Toggle the switch with this letter.
    ToggleSwitch(char),
    /// Set or clear the option with this letter.
    SetOption(char),
    /// Run the action with this key (a letter or a chord such as `g l`).
    Action(String),
    /// Close the menu and return to the previous view.
    Pop,
    /// Save the current arguments as the menu's defaults.
    SaveArgs,
    /// Restore the menu's saved arguments.
    ResetArgs,
    /// Unbound in a capture view; swallow it with a one-line hint.
    Unbound { chord: String },
}

/// Resolve one key step with the menu's own two-step matcher.
///
/// `keys` are the menu's item keys, so an action may be a single key or a
/// multi-step chord (`g l` in the `vim` profile's dispatch menu). Menus capture
/// every key, so an unbound one is swallowed rather than passed back to the
/// editor. `Escape`, `ctrl+c` and `q` always close.
pub fn resolve_menu(prefix: &mut MenuPrefix, key: &str, keys: &[String]) -> MenuResolution {
    if matches!(key, "Escape" | "ctrl+c" | "q") {
        *prefix = MenuPrefix::None;
        return MenuResolution::Pop;
    }
    if key == "ctrl+s" {
        *prefix = MenuPrefix::None;
        return MenuResolution::SaveArgs;
    }
    if key == "ctrl+r" {
        *prefix = MenuPrefix::None;
        return MenuResolution::ResetArgs;
    }

    let mut chars = key.chars();
    let single = match (chars.next(), chars.next()) {
        (Some(c), None) => Some(c),
        _ => None,
    };

    let previous = std::mem::take(prefix);
    match previous {
        MenuPrefix::None => match key {
            "-" => {
                *prefix = MenuPrefix::Switch;
                MenuResolution::Pending
            }
            "=" => {
                *prefix = MenuPrefix::Option;
                MenuResolution::Pending
            }
            _ => action_or_pending(prefix, key, keys, single),
        },
        MenuPrefix::Switch => match single {
            Some(c) => MenuResolution::ToggleSwitch(c),
            None => MenuResolution::Unbound {
                chord: format!("-{key}"),
            },
        },
        MenuPrefix::Option => match single {
            Some(c) => MenuResolution::SetOption(c),
            None => MenuResolution::Unbound {
                chord: format!("={key}"),
            },
        },
        MenuPrefix::Chord(prefix_keys) => {
            let chord = format!("{prefix_keys} {key}");
            if keys.iter().any(|candidate| candidate == &chord) {
                MenuResolution::Action(chord)
            } else if keys.iter().any(|candidate| candidate.starts_with(&format!("{chord} "))) {
                *prefix = MenuPrefix::Chord(chord);
                MenuResolution::Pending
            } else {
                MenuResolution::Unbound { chord }
            }
        }
    }
}

/// Match a single key step against the menu's action keys.
fn action_or_pending(
    prefix: &mut MenuPrefix,
    key: &str,
    keys: &[String],
    single: Option<char>,
) -> MenuResolution {
    if keys.iter().any(|candidate| candidate == key) {
        return MenuResolution::Action(key.to_string());
    }
    if keys
        .iter()
        .any(|candidate| candidate.starts_with(&format!("{key} ")))
    {
        *prefix = MenuPrefix::Chord(key.to_string());
        return MenuResolution::Pending;
    }
    match single {
        Some(_) => MenuResolution::Action(key.to_string()),
        None => MenuResolution::Unbound {
            chord: key.to_string(),
        },
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
