//! Static keymap manifest: every profile × scope × binding, plus emitters.
//!
//! The manifest is the offline view of the server's key space (§3.9). It is
//! used by `magit-lsp keymap`, by CI snapshots, and by static clients such as
//! VSCode that cannot receive `view/keymap` at runtime.

use crate::commands;
use crate::config::{self, Config, ConfigError, Profile};
use crate::keymap::{command_for_key, Binding, EditorMode, Keymap};
use crate::keys::KeyChord;
use serde::Serialize;
use std::collections::BTreeSet;
use std::io::Write;

/// A manifest output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Vscode,
    Neovim,
    Markdown,
}

impl Format {
    /// Parse a `--format` value.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "json" => Ok(Format::Json),
            "vscode" => Ok(Format::Vscode),
            "neovim" => Ok(Format::Neovim),
            "markdown" => Ok(Format::Markdown),
            other => Err(format!(
                "unknown format `{other}`; expected json, vscode, neovim or markdown"
            )),
        }
    }

    /// The name used on the command line.
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::Vscode => "vscode",
            Format::Neovim => "neovim",
            Format::Markdown => "markdown",
        }
    }
}

/// The bindings of one scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeManifest {
    pub scope: String,
    pub bindings: Vec<Binding>,
}

/// One profile's scopes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileManifest {
    pub profile: String,
    pub scopes: Vec<ScopeManifest>,
}

/// The full static manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub profiles: Vec<ProfileManifest>,
}

/// Build the manifest for `profiles`, overlaying `config` on each profile.
pub fn build(profiles: &[Profile], config: &Config) -> Result<Manifest, ConfigError> {
    let mut out = Vec::new();
    for profile in profiles {
        let mut merged = config.clone();
        merged.profile = *profile;
        let mut scopes = Vec::new();
        for scope in config::VIEW_SCOPES {
            let keymap = Keymap::resolve(scope, *profile, &merged)?;
            let bindings = keymap.bindings().to_vec();
            scopes.push(ScopeManifest {
                scope: scope.to_string(),
                bindings,
            });
        }
        out.push(ProfileManifest {
            profile: profile.as_str().to_string(),
            scopes,
        });
    }
    Ok(Manifest { profiles: out })
}

/// Validate the merged keymap and profile coverage for each profile.
pub fn check(profiles: &[Profile], config: &Config) -> Result<(), ConfigError> {
    for profile in profiles {
        check_coverage(*profile)?;
        let mut merged = config.clone();
        merged.profile = *profile;
        crate::keymap::validate(&merged)?;
    }
    Ok(())
}

/// Check that a shipped profile binds or explicitly unbinds every command (R5).
fn check_coverage(profile: Profile) -> Result<(), ConfigError> {
    let keys = config::profile_keys(profile)?;
    let mut declared = BTreeSet::new();
    for (scope, scope_keys) in &keys.scopes {
        for name in scope_keys.commands.keys() {
            let id = command_for_key(scope, name).ok_or_else(|| {
                ConfigError::new(format!("unknown command `{name}` in scope `{scope}`"))
                    .at_file(profile.file_label())
            })?;
            declared.insert(id);
        }
    }
    for command in commands::registry().iter() {
        if command.doc.trim().is_empty() {
            return Err(
                ConfigError::new(format!("command `{}` has no doc", command.id))
                    .at_file(profile.file_label()),
            );
        }
        // Menu actions are bound by their menu item, not by a profile scope.
        if command.scope == commands::Scope::MenuAction {
            continue;
        }
        if !declared.contains(command.id) {
            return Err(ConfigError::new(format!(
                "{} profile does not bind or explicitly unbind `{}`",
                profile.as_str(),
                command.id
            ))
            .at_file(profile.file_label()));
        }
    }
    Ok(())
}

/// A VSCode `contributes.keybindings` entry plus the data the deny-list needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VscodeEntry {
    pub profile: String,
    pub scope: String,
    pub key: String,
    /// The canonical server command passed to `magit.invoke` as `args.command`.
    pub binding: String,
    /// The canonical step or chord passed as `args.key`.
    pub when: String,
}

/// The generated VSCode keybindings, in manifest order.
pub fn vscode_entries(manifest: &Manifest) -> Vec<VscodeEntry> {
    let mut entries = Vec::new();
    for profile in &manifest.profiles {
        for scope in &profile.scopes {
            for binding in &scope.bindings {
                for key in &binding.keys {
                    let chord = KeyChord::parse(key).expect("manifest keys are canonical");
                    entries.push(VscodeEntry {
                        profile: profile.profile.clone(),
                        scope: scope.scope.clone(),
                        key: chord.to_vscode(),
                        binding: binding.command.clone(),
                        when: when_clause(&scope.scope, false),
                    });
                    // Text mode keeps exactly the toggle command bound. Capture
                    // mode is dynamic (`type`), so it has no static entries.
                    if binding.command == "common.toggle-text-mode" {
                        entries.push(VscodeEntry {
                            profile: profile.profile.clone(),
                            scope: scope.scope.clone(),
                            key: chord.to_vscode(),
                            binding: binding.command.clone(),
                            when: when_clause(&scope.scope, true),
                        });
                    }
                }
            }
        }
    }
    entries
}

fn when_clause(scope: &str, text_mode: bool) -> String {
    if text_mode {
        format!("magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == '{scope}' && magit.textMode")
    } else {
        format!("magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == '{scope}' && !magit.textMode")
    }
}

/// One Neovim `lhs` binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeovimEntry {
    pub profile: String,
    pub lhs: String,
    pub command: String,
    pub modes: Vec<EditorMode>,
    pub scope: String,
}

/// The generated Neovim bindings, in manifest order.
pub fn neovim_entries(manifest: &Manifest) -> Vec<NeovimEntry> {
    let mut entries = Vec::new();
    for profile in &manifest.profiles {
        for scope in &profile.scopes {
            for binding in &scope.bindings {
                for key in &binding.keys {
                    let chord = KeyChord::parse(key).expect("manifest keys are canonical");
                    entries.push(NeovimEntry {
                        profile: profile.profile.clone(),
                        lhs: chord.to_neovim(),
                        command: binding.command.clone(),
                        modes: binding.modes.clone(),
                        scope: scope.scope.clone(),
                    });
                }
            }
        }
    }
    entries
}

/// VSCode workbench shortcuts the generated keybindings must not shadow.
pub const WORKBENCH_DENY_LIST: &[&str] = &[
    "ctrl+p",
    "ctrl+shift+p",
    "ctrl+w",
    "ctrl+tab",
    "ctrl+n",
    "ctrl+t",
    "f1",
    "alt+tab",
    "ctrl+shift+n",
    "ctrl+shift+w",
    "ctrl+q",
    "ctrl+s",
    "ctrl+r",
    "ctrl+z",
    "ctrl+c",
    "ctrl+v",
    "ctrl+x",
];

/// A deliberate override of a denied workbench shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllowEntry {
    pub key: &'static str,
    /// `None` allows the key in every scope; `Some(scope)` only in that scope.
    pub scope: Option<&'static str>,
    pub reason: &'static str,
}

/// Denied keys we bind on purpose, with the reason.
pub const WORKBENCH_ALLOW: &[AllowEntry] = &[
    AllowEntry {
        key: "ctrl+t",
        scope: None,
        reason: "common.toggle-text-mode is the only key that leaves text mode",
    },
    AllowEntry {
        key: "ctrl+c",
        scope: None,
        reason: "common.cancel passes through to copy when there is nothing to cancel",
    },
    AllowEntry {
        key: "ctrl+s",
        scope: Some("menu"),
        reason: "menu.save-args is restricted to menu views",
    },
    AllowEntry {
        key: "ctrl+r",
        scope: Some("menu"),
        reason: "menu.reset-args is restricted to menu views",
    },
];

/// A denied key that has no allow entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeniedEntry {
    pub profile: String,
    pub scope: String,
    pub key: String,
    pub command: String,
}

/// Every generated VSCode key that shadows a workbench shortcut without an
/// explicit, matching allow entry.
pub fn denied_entries(manifest: &Manifest) -> Vec<DeniedEntry> {
    let mut denied = Vec::new();
    for entry in vscode_entries(manifest) {
        if !WORKBENCH_DENY_LIST.contains(&entry.key.as_str()) {
            continue;
        }
        let allowed = WORKBENCH_ALLOW.iter().any(|allow| {
            allow.key == entry.key
                && allow
                    .scope
                    .map_or(true, |scope| scope == entry.scope.as_str())
        });
        if !allowed {
            denied.push(DeniedEntry {
                profile: entry.profile,
                scope: entry.scope,
                key: entry.key,
                command: entry.binding,
            });
        }
    }
    denied
}

/// The JSON manifest, pretty-printed with a trailing newline.
pub fn json(manifest: &Manifest) -> String {
    let mut text = serde_json::to_string_pretty(manifest).expect("manifest serialises");
    text.push('\n');
    text
}

/// The VSCode `contributes.keybindings` array.
pub fn vscode(manifest: &Manifest) -> String {
    #[derive(Serialize)]
    struct Args {
        command: String,
    }

    #[derive(Serialize)]
    struct Entry {
        key: String,
        command: &'static str,
        args: Args,
        when: String,
    }

    let entries: Vec<Entry> = vscode_entries(manifest)
        .into_iter()
        .map(|entry| Entry {
            key: entry.key,
            command: "magit.invoke",
            args: Args { command: entry.binding },
            when: entry.when,
        })
        .collect();
    let mut text = serde_json::to_string_pretty(&entries).expect("keybindings serialise");
    text.push('\n');
    text
}

/// The Neovim binding list.
pub fn neovim(manifest: &Manifest) -> String {
    #[derive(Serialize)]
    struct Entry {
        lhs: String,
        command: String,
        modes: Vec<EditorMode>,
        scope: String,
    }

    let entries: Vec<Entry> = neovim_entries(manifest)
        .into_iter()
        .map(|entry| Entry {
            lhs: entry.lhs,
            command: entry.command,
            modes: entry.modes,
            scope: entry.scope,
        })
        .collect();
    let mut text = serde_json::to_string_pretty(&entries).expect("bindings serialise");
    text.push('\n');
    text
}

/// Human-readable tables per profile and scope.
pub fn markdown(manifest: &Manifest) -> String {
    let mut out = String::new();
    for profile in &manifest.profiles {
        out.push_str(&format!("# magit-lsp keymap: {}\n\n", profile.profile));
        for scope in &profile.scopes {
            out.push_str(&format!("## {}\n\n", scope.scope));
            out.push_str("| Command | Keys | Status | Description |\n");
            out.push_str("| --- | --- | --- | --- |\n");
            for binding in &scope.bindings {
                let keys = if binding.keys.is_empty() {
                    "(unbound)".to_string()
                } else {
                    binding
                        .keys
                        .iter()
                        .map(|key| format!("`{key}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let doc = binding.label.replace('|', "\\|");
                out.push_str(&format!(
                    "| `{}` | {} | {} | {} |\n",
                    binding.command, keys, binding.status, doc
                ));
            }
            out.push('\n');
        }
    }
    out
}

/// Render `format` for `profiles` to `out`.
pub fn render(
    format: Format,
    profiles: &[Profile],
    config: &Config,
    out: &mut impl Write,
) -> crate::Result<()> {
    let manifest = build(profiles, config).map_err(|error| crate::Error::new(error.to_string()))?;
    let text = match format {
        Format::Json => json(&manifest),
        Format::Vscode => vscode(&manifest),
        Format::Neovim => neovim(&manifest),
        Format::Markdown => markdown(&manifest),
    };
    out.write_all(text.as_bytes())?;
    Ok(())
}
