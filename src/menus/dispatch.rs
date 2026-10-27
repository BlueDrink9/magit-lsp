//! The dispatch menu (Magit `magit-dispatch`), generated from the registry and
//! the active profile's resolved keymaps.
//!
//! Only keys the server actually binds are listed, and every entry takes its
//! status from the command registry, so a Planned command is greyed here just
//! like it is inside its own menu.

use super::*;
use crate::commands::{registry, CommandStatus, Scope};
use crate::keymap::Keymap;
use crate::menu::{Args, Group, Item, MenuStatus};
use std::collections::BTreeMap;

/// Commands that open another menu or view.
const OPENERS: &[(&str, &str)] = &[
    ("open.cherry-pick", "Apply"),
    ("open.branch", "Branch"),
    ("open.bisect", "Bisect"),
    ("open.commit", "Commit"),
    ("open.diff", "Diff"),
    ("open.fetch", "Fetch"),
    ("open.pull", "Pull"),
    ("open.ignore", "Ignore"),
    ("open.log", "Log"),
    ("open.merge", "Merge"),
    ("open.remote", "Remote"),
    ("open.submodule", "Submodule"),
    ("open.push", "Push"),
    ("open.rebase", "Rebase"),
    ("open.tag", "Tag"),
    ("open.revert", "Revert"),
    ("open.patch-apply", "Apply patches"),
    ("open.patch-create", "Format patches"),
    ("open.reset", "Reset"),
    ("open.refs", "Show Refs"),
    ("open.cherries", "Cherries"),
    ("open.stash", "Stash"),
    ("open.worktree", "Worktree"),
    ("open.run", "Run"),
    ("open.process-log", "Process log"),
];

/// Commands that change the index or worktree.
const APPLYING: &[(&str, &str)] = &[
    ("status.apply", "Apply"),
    ("status.reverse", "Reverse"),
    ("status.discard", "Discard"),
    ("status.stage", "Stage"),
    ("status.unstage", "Unstage"),
    ("status.stage-all", "Stage all"),
    ("status.unstage-all", "Unstage all"),
];

/// Commands available in every view.
const ESSENTIAL: &[(&str, &str)] = &[
    ("common.refresh", "Refresh current buffer"),
    ("common.toggle-section", "Toggle section at point"),
    ("common.visit", "Visit thing at point"),
    ("common.copy-value", "Copy thing at point"),
    ("common.toggle-text-mode", "Toggle text mode"),
];

/// Build the dispatch menu for the current keymaps.
pub fn menu(keymaps: &BTreeMap<String, Keymap>) -> Menu {
    Menu {
        id: "dispatch".to_string(),
        title: "Dispatch".to_string(),
        saved_args: Args::default(),
        groups: vec![
            dispatch_group("Transient and dwim commands", OPENERS, keymaps),
            dispatch_group("Applying changes", APPLYING, keymaps),
            dispatch_group("Essential commands", ESSENTIAL, keymaps),
        ]
        .into_iter()
        .flatten()
        .collect(),
    }
}

fn dispatch_group(
    title: &str,
    entries: &[(&str, &str)],
    keymaps: &BTreeMap<String, Keymap>,
) -> Option<Group> {
    let items: Vec<Item> = entries
        .iter()
        .filter_map(|(id, label)| item(id, label, keymaps))
        .collect();
    if items.is_empty() {
        None
    } else {
        Some(Group {
            title: title.to_string(),
            items,
        })
    }
}

fn item(id: &str, label: &str, keymaps: &BTreeMap<String, Keymap>) -> Option<Item> {
    let command = registry().get(id)?;
    let key = key_for(id, command.scope, keymaps)?;
    let status = match command.status {
        CommandStatus::Implemented => MenuStatus::Implemented,
        CommandStatus::Planned { roadmap_id } => MenuStatus::Planned {
            roadmap_id: roadmap_id.to_string(),
        },
    };
    Some(Item::Action {
        key,
        command: id.to_string(),
        label: label.to_string(),
        status,
    })
}

fn key_for(id: &str, scope: Scope, keymaps: &BTreeMap<String, Keymap>) -> Option<String> {
    let scope = match scope {
        Scope::Common => "common",
        Scope::Status => "status",
        _ => return None,
    };
    let keymap = keymaps.get(scope)?;
    let binding = keymap
        .bindings()
        .iter()
        .find(|binding| binding.command == id)?;
    binding.keys.first().cloned()
}
