//! Key notation: parse and format canonical key strings (§3.8).
//!
//! Canonical form is `[modifier+]*key` with modifiers lowercase in the order
//! `ctrl`, `alt`, `shift`, `meta`. Printable characters are written as the
//! character they produce (so `S` means shift+s and `shift+S` is rejected);
//! the two awkward characters use the spelled-out names `Space` and `Plus`.
//! Chords (key sequences) are space-separated: `g r`, `ctrl+k ctrl+s`.

use std::fmt;
use std::str::FromStr;

/// A key independent of its modifiers.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    /// A single printable character (including `Space` and `Plus`).
    Char(char),
    /// A named W3C key value such as `Enter` or `ArrowUp`.
    Named(String),
}

/// One key press: the key plus which modifiers are held.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyStep {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
    pub key: Key,
}

impl KeyStep {
    /// A bare key with no modifiers.
    pub fn plain(key: Key) -> Self {
        Self {
            ctrl: false,
            alt: false,
            shift: false,
            meta: false,
            key,
        }
    }

    /// The VSCode `keybindings.json` spelling of this step (§3.8).
    ///
    /// Shifted printable characters become `shift+<base>` because VSCode
    /// resolves bindings by the physical key plus modifiers.
    pub fn to_vscode(&self) -> String {
        let (base, shifted) = match &self.key {
            Key::Char(c) => vscode_char(*c),
            Key::Named(name) => (vscode_named(name), false),
        };
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("ctrl");
        }
        if self.alt {
            parts.push("alt");
        }
        if self.shift || shifted {
            parts.push("shift");
        }
        if self.meta {
            parts.push("meta");
        }
        parts.push(&base);
        parts.join("+")
    }

    /// The Neovim `lhs` spelling of this step (§3.8).
    pub fn to_neovim(&self) -> String {
        let base = match &self.key {
            Key::Char(' ') => "<Space>".to_string(),
            Key::Char('<') => "<lt>".to_string(),
            Key::Char(c) => c.to_string(),
            Key::Named(name) => format!("<{}>", neovim_named(name)),
        };
        let mut modifiers = String::new();
        if self.ctrl {
            modifiers.push_str("C-");
        }
        if self.alt {
            modifiers.push_str("A-");
        }
        if self.shift {
            modifiers.push_str("S-");
        }
        if self.meta {
            modifiers.push_str("M-");
        }
        if modifiers.is_empty() {
            return base;
        }
        // Named keys already carry angle brackets; insert the modifiers inside.
        if base.starts_with('<') && base.ends_with('>') {
            format!("<{modifiers}{}>", &base[1..base.len() - 1])
        } else {
            format!("<{modifiers}{base}>")
        }
    }
}

/// The VSCode base key and whether it implies shift for a printable character.
fn vscode_char(c: char) -> (String, bool) {
    let shifted = match c {
        'A'..='Z' => Some(c.to_ascii_lowercase()),
        '!' => Some('1'),
        '@' => Some('2'),
        '#' => Some('3'),
        '$' => Some('4'),
        '%' => Some('5'),
        '^' => Some('6'),
        '&' => Some('7'),
        '*' => Some('8'),
        '(' => Some('9'),
        ')' => Some('0'),
        '_' => Some('-'),
        '+' => Some('='),
        '{' => Some('['),
        '}' => Some(']'),
        '|' => Some('\\'),
        ':' => Some(';'),
        '"' => Some('\''),
        '<' => Some(','),
        '>' => Some('.'),
        '?' => Some('/'),
        '~' => Some('`'),
        _ => None,
    };
    match shifted {
        Some(base) => (base.to_string(), true),
        None if c == ' ' => ("space".to_string(), false),
        None => (c.to_string(), false),
    }
}

fn vscode_named(name: &str) -> String {
    let lower = match name {
        "Enter" => "enter",
        "Escape" => "escape",
        "Tab" => "tab",
        "Backspace" => "backspace",
        "Delete" => "delete",
        "Insert" => "insert",
        "Home" => "home",
        "End" => "end",
        "PageUp" => "pageup",
        "PageDown" => "pagedown",
        "ArrowUp" => "up",
        "ArrowDown" => "down",
        "ArrowLeft" => "left",
        "ArrowRight" => "right",
        other => other,
    };
    lower.to_ascii_lowercase()
}

fn neovim_named(name: &str) -> &str {
    match name {
        "Enter" => "CR",
        "Escape" => "Esc",
        "Tab" => "Tab",
        "Backspace" => "BS",
        "Delete" => "Del",
        "Insert" => "Insert",
        "Home" => "Home",
        "End" => "End",
        "PageUp" => "PageUp",
        "PageDown" => "PageDown",
        "ArrowUp" => "Up",
        "ArrowDown" => "Down",
        "ArrowLeft" => "Left",
        "ArrowRight" => "Right",
        other => other,
    }
}

/// A sequence of one or more keys, e.g. `g r`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyChord(pub Vec<KeyStep>);

/// Why a key string could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyError(String);

impl KeyError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    /// The human-readable reason.
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for KeyError {}

/// W3C `KeyboardEvent.key` names this notation knows about.
const NAMED_KEYS: &[&str] = &[
    "Enter",
    "Escape",
    "Tab",
    "Backspace",
    "Delete",
    "Insert",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "ArrowUp",
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
];

impl KeyChord {
    /// Parse a canonical key string, e.g. `ctrl+Enter`, `g r`.
    pub fn parse(input: &str) -> Result<Self, KeyError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(KeyError::new("empty key"));
        }
        let mut steps = Vec::new();
        for step in trimmed.split_whitespace() {
            steps.push(parse_step(step)?);
        }
        Ok(KeyChord(steps))
    }

    /// The canonical string; `Self::parse(self.to_string()) == *self`.
    pub fn to_canonical(&self) -> String {
        self.0
            .iter()
            .map(format_step)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The VSCode `keybindings.json` key string; chord steps are space-separated.
    pub fn to_vscode(&self) -> String {
        self.0
            .iter()
            .map(KeyStep::to_vscode)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The Neovim `lhs`; chord steps are concatenated into one string.
    pub fn to_neovim(&self) -> String {
        self.0.iter().map(KeyStep::to_neovim).collect()
    }

    /// The individual key presses of the chord.
    pub fn steps(&self) -> &[KeyStep] {
        &self.0
    }

    /// Whether `self` starts with the steps in `prefix`.
    pub fn starts_with(&self, prefix: &[KeyStep]) -> bool {
        self.0.len() >= prefix.len() && self.0[..prefix.len()] == *prefix
    }
}

impl FromStr for KeyChord {
    type Err = KeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        KeyChord::parse(s)
    }
}

impl fmt::Display for KeyChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_canonical())
    }
}

fn parse_step(step: &str) -> Result<KeyStep, KeyError> {
    if step.is_empty() {
        return Err(KeyError::new("empty key"));
    }
    // A step of exactly `Plus` is the plus character; splitting on `+` first
    // would produce two empty parts.
    if step == "Plus" {
        return Ok(KeyStep::plain(Key::Char('+')));
    }

    let parts: Vec<&str> = step.split('+').collect();
    let (modifiers, key_part) = parts.split_at(parts.len() - 1);
    let key_part = key_part[0];

    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut meta = false;
    for modifier in modifiers {
        let slot = match *modifier {
            "ctrl" => &mut ctrl,
            "alt" => &mut alt,
            "shift" => &mut shift,
            "meta" => &mut meta,
            "" => return Err(KeyError::new("empty modifier in key")),
            other => return Err(KeyError::new(format!("unknown modifier: {other}"))),
        };
        if *slot {
            return Err(KeyError::new(format!("duplicate modifier: {modifier}")));
        }
        *slot = true;
    }

    let key = if key_part == "Space" {
        Key::Char(' ')
    } else if key_part == "Plus" {
        Key::Char('+')
    } else if NAMED_KEYS.contains(&key_part) {
        Key::Named(key_part.to_string())
    } else if key_part == "F1"
        || key_part == "F2"
        || key_part == "F3"
        || key_part == "F4"
        || key_part == "F5"
        || key_part == "F6"
        || key_part == "F7"
        || key_part == "F8"
        || key_part == "F9"
        || key_part == "F10"
        || key_part == "F11"
        || key_part == "F12"
    {
        Key::Named(key_part.to_string())
    } else if key_part.chars().count() == 1 {
        let c = key_part.chars().next().expect("one char");
        if c == '+' {
            return Err(KeyError::new("write the plus key as `Plus`"));
        }
        if c.is_whitespace() {
            return Err(KeyError::new("write the space key as `Space`"));
        }
        if shift && !(ctrl || alt || meta) {
            return Err(KeyError::new(format!(
                "`shift+{c}` is invalid; write the shifted character `{c}` instead"
            )));
        }
        Key::Char(c)
    } else {
        return Err(KeyError::new(format!("unknown key: {key_part}")));
    };

    Ok(KeyStep {
        ctrl,
        alt,
        shift,
        meta,
        key,
    })
}

fn format_step(step: &KeyStep) -> String {
    let mut out = String::new();
    if step.ctrl {
        out.push_str("ctrl+");
    }
    if step.alt {
        out.push_str("alt+");
    }
    if step.shift {
        out.push_str("shift+");
    }
    if step.meta {
        out.push_str("meta+");
    }
    match &step.key {
        Key::Char(' ') => out.push_str("Space"),
        Key::Char('+') => out.push_str("Plus"),
        Key::Char(c) => out.push(*c),
        Key::Named(name) => out.push_str(name),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shared §3.8 fixture at `tests/data/keys.json`; `tests/test_keymap.lua`
    /// pins the Lua translator to the same cases.
    #[derive(serde::Deserialize)]
    struct Fixture {
        #[serde(rename = "roundTrip")]
        round_trip: Vec<String>,
        #[serde(rename = "toNeovim")]
        to_neovim: Vec<(String, String)>,
        #[serde(rename = "toVscode")]
        to_vscode: Vec<(String, String)>,
        #[serde(rename = "invalidCanonical")]
        invalid_canonical: Vec<String>,
    }

    fn fixture() -> Fixture {
        serde_json::from_str(include_str!("../tests/data/keys.json")).expect("keys fixture")
    }

    #[test]
    fn round_trips_canonical_keys() {
        for case in fixture().round_trip {
            let parsed = KeyChord::parse(&case).unwrap_or_else(|e| panic!("{case}: {e}"));
            assert_eq!(parsed.to_canonical(), case, "canonical form of {case}");
            assert_eq!(KeyChord::parse(&parsed.to_canonical()).unwrap(), parsed);
        }
    }

    #[test]
    fn rejects_invalid_keys() {
        for case in fixture().invalid_canonical {
            assert!(KeyChord::parse(&case).is_err(), "expected rejection: {case:?}");
        }
    }

    #[test]
    fn translates_to_vscode_keybindings() {
        for (canonical, expected) in fixture().to_vscode {
            assert_eq!(
                KeyChord::parse(&canonical).unwrap().to_vscode(),
                expected,
                "{canonical}"
            );
        }
    }

    #[test]
    fn translates_to_neovim_lhs() {
        for (canonical, expected) in fixture().to_neovim {
            assert_eq!(
                KeyChord::parse(&canonical).unwrap().to_neovim(),
                expected,
                "{canonical}"
            );
        }
    }
}
