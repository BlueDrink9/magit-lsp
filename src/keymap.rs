//! Keymap resolution and validation (§3.9).
//!
//! Bindings come from a shipped profile and from user configuration. For a view
//! scope the effective binding of a command is the first declaration found in
//! this order: user config for the scope, profile for the scope, user config for
//! `common`, profile for `common`. A more specific declaration wins a key it
//! shares with a broader one; two commands claiming the same key at the same
//! level are a conflict.

use crate::commands::{self, Command, CommandStatus, Scope};
use crate::config::{BindingSpec, Config, ConfigError, Origin, Profile, ScopeKeys};
use crate::keys::{KeyChord, KeyStep};
use serde::Serialize;
use std::collections::BTreeMap;

/// The editor mode a binding is active in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorMode {
    Normal,
    Visual,
}

impl EditorMode {
    /// The wire name used in `view/keymap`.
    pub fn as_str(self) -> &'static str {
        match self {
            EditorMode::Normal => "normal",
            EditorMode::Visual => "visual",
        }
    }

    /// Parse a mode name from configuration.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "normal" => Ok(EditorMode::Normal),
            "visual" => Ok(EditorMode::Visual),
            other => Err(format!("unknown editor mode: {other}")),
        }
    }
}

/// One resolved binding: a command, its keys and where it applies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    /// Registry command id, e.g. `status.stage`.
    pub command: String,
    /// Alternative key chords, canonical strings.
    pub keys: Vec<String>,
    /// Editor modes the binding is active in.
    pub modes: Vec<EditorMode>,
    /// Short label for menus and help.
    pub label: String,
    /// Longer description.
    pub doc: String,
    /// The command family, e.g. `status` or `open`.
    pub group: String,
    /// `implemented`, `planned` or `unavailable`.
    pub status: String,
}

/// The effective keymap for one view scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Keymap {
    pub scope: String,
    pub bindings: Vec<Binding>,
}

/// The result of matching a key prefix against a keymap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Match {
    /// No binding starts with this prefix.
    None,
    /// A binding needs more keys; keep collecting.
    Prefix,
    /// A complete chord maps to this command.
    Exact(String),
}

impl Keymap {
    /// Resolve the keymap for `scope` from the profile and the user config.
    pub fn resolve(scope: &str, profile: Profile, config: &Config) -> Result<Self, ConfigError> {
        let registry = commands::registry();
        let profile_keys = crate::config::profile_keys(profile)?;
        let empty = ScopeKeys::default();

        let user_scope = config.keys.scopes.get(scope).unwrap_or(&empty);
        let profile_scope = profile_keys.scopes.get(scope).unwrap_or(&empty);
        let user_common = config.keys.scopes.get("common").unwrap_or(&empty);
        let profile_common = profile_keys.scopes.get("common").unwrap_or(&empty);

        // The scope's own `unbind` list removes profile bindings for this scope.
        let unbind = &user_scope.unbind;

        let levels = [
            declarations(scope, user_scope, 0, &[])?,
            declarations(scope, profile_scope, 1, unbind)?,
            declarations("common", user_common, 2, &[])?,
            declarations("common", profile_common, 3, unbind)?,
        ];

        let candidates: Vec<&Command> = registry
            .iter()
            .filter(|command| {
                command.scope == Scope::Common || command.scope.as_str() == scope
            })
            .collect();

        // Choose the highest-precedence declaration per command.
        let mut chosen: BTreeMap<&'static str, Decl> = BTreeMap::new();
        for command in &candidates {
            for level in &levels {
                if let Some(decl) = level.get(command.id) {
                    chosen.insert(command.id, decl.clone());
                    break;
                }
            }
        }

        // Resolve which command owns each key chord.
        let mut owners: BTreeMap<KeyChord, KeyOwner> = BTreeMap::new();
        for (command, decl) in &chosen {
            for key in &decl.spec.keys {
                match owners.get(key) {
                    None => {
                        owners.insert(
                            key.clone(),
                            KeyOwner {
                                command: *command,
                                level: decl.level,
                                origin: decl.origin.clone(),
                            },
                        );
                    }
                    Some(existing) if existing.command == *command => {}
                    Some(existing) if existing.level == decl.level => {
                        let mut error = ConfigError::new(format!(
                            "key `{key}` is bound to both `{}` and `{}` in scope `{scope}`",
                            existing.command, command
                        ));
                        if let Some(origin) = &decl.origin {
                            error = error.at(origin);
                        } else if let Some(origin) = &existing.origin {
                            error = error.at(origin);
                        }
                        return Err(error);
                    }
                    Some(existing) if decl.level < existing.level => {
                        owners.insert(
                            key.clone(),
                            KeyOwner {
                                command: *command,
                                level: decl.level,
                                origin: decl.origin.clone(),
                            },
                        );
                    }
                    Some(_) => {}
                }
            }
        }

        let mut keys_by_command: BTreeMap<&'static str, Vec<KeyChord>> = BTreeMap::new();
        for (key, owner) in &owners {
            keys_by_command
                .entry(owner.command)
                .or_default()
                .push(key.clone());
        }

        let mut bindings = Vec::new();
        for command in &candidates {
            let Some(decl) = chosen.get(command.id) else {
                continue;
            };
            let mut keys = keys_by_command.remove(command.id).unwrap_or_default();
            keys.sort_by_key(KeyChord::to_canonical);
            bindings.push(Binding {
                command: command.id.to_string(),
                keys: keys.iter().map(KeyChord::to_canonical).collect(),
                modes: decl.spec.modes.clone(),
                label: command.doc.to_string(),
                doc: command.doc.to_string(),
                group: command
                    .id
                    .split('.')
                    .next()
                    .unwrap_or(command.id)
                    .to_string(),
                status: status_name(command.status).to_string(),
            });
        }

        Ok(Keymap {
            scope: scope.to_string(),
            bindings,
        })
    }

    /// Match a key prefix, preferring longer chords.
    pub fn lookup(&self, prefix: &[KeyStep], mode: EditorMode) -> Match {
        let mut exact = None;
        for binding in &self.bindings {
            if !binding.modes.contains(&mode) {
                continue;
            }
            for key in &binding.keys {
                let Ok(chord) = KeyChord::parse(key) else {
                    continue;
                };
                if chord.starts_with(prefix) {
                    if chord.steps().len() > prefix.len() {
                        return Match::Prefix;
                    }
                    if chord.steps().len() == prefix.len() {
                        exact = Some(binding.command.clone());
                    }
                }
            }
        }
        match exact {
            Some(command) => Match::Exact(command),
            None => Match::None,
        }
    }

    /// The resolved bindings.
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// The binding for `command`, if the scope declares it.
    pub fn command(&self, command: &str) -> Option<&Binding> {
        self.bindings.iter().find(|binding| binding.command == command)
    }
}

#[derive(Clone)]
struct Decl {
    spec: BindingSpec,
    level: u8,
    origin: Option<Origin>,
}

struct KeyOwner {
    command: &'static str,
    level: u8,
    origin: Option<Origin>,
}

fn declarations(
    scope: &str,
    keys: &ScopeKeys,
    level: u8,
    unbind: &[KeyChord],
) -> Result<BTreeMap<&'static str, Decl>, ConfigError> {
    let mut out = BTreeMap::new();
    for (name, spec) in &keys.commands {
        let Some(command) = command_for_key(scope, name) else {
            let mut error =
                ConfigError::new(format!("unknown command `{name}` in scope `{scope}`"));
            if let Some(origin) = keys.origins.get(name) {
                error = error.at(origin);
            }
            return Err(error);
        };
        let mut spec = spec.clone();
        if !unbind.is_empty() {
            spec.keys.retain(|key| !unbind.contains(key));
        }
        out.insert(
            command,
            Decl {
                spec,
                level,
                origin: keys.origins.get(name).cloned(),
            },
        );
    }
    Ok(out)
}

/// Resolve the name written in a `[keys.<scope>]` table to a registry id.
///
/// A name is either the id without its scope prefix (`stage` for
/// `status.stage`), a full id (`open.commit`), or a `common.*` name used from a
/// view scope (`confirm`).
pub fn command_for_key(scope: &str, name: &str) -> Option<&'static str> {
    let registry = commands::registry();
    let candidates = [
        format!("{scope}.{name}"),
        name.to_string(),
        format!("common.{name}"),
    ];
    candidates
        .iter()
        .find_map(|candidate| registry.get(candidate).map(|command| command.id))
}

fn status_name(status: CommandStatus) -> &'static str {
    match status {
        CommandStatus::Implemented => "implemented",
        CommandStatus::Planned { .. } => "planned",
        CommandStatus::Unavailable { .. } => "unavailable",
    }
}

/// Resolve every scope that matters, reporting unknown commands and conflicts.
pub fn validate(config: &Config) -> Result<BTreeMap<String, Keymap>, ConfigError> {
    let mut scopes: Vec<String> = crate::config::VIEW_SCOPES
        .iter()
        .map(|scope| scope.to_string())
        .collect();
    for scope in config.keys.scopes.keys() {
        if !scopes.contains(scope) {
            scopes.push(scope.clone());
        }
    }

    let mut keymaps = BTreeMap::new();
    for scope in scopes {
        keymaps.insert(
            scope.clone(),
            Keymap::resolve(&scope, config.profile, config)?,
        );
    }
    Ok(keymaps)
}
