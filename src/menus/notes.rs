//! Basic Git notes actions for explicit commit targets.

use super::*;

/// Define the server-owned notes actions.
pub fn definition() -> MenuDef {
    MenuDef {
        id: "notes",
        title: "Notes",
        base_command: "git notes",
        groups: vec![group(
            "Notes",
            vec![
                action("r", "notes.read", "read", "-", ""),
                action("a", "notes.add", "add", "-", ""),
                action("e", "notes.edit", "edit", "magit-notes-edit", ""),
                action("k", "notes.remove", "remove", "magit-notes-remove", ""),
            ],
        )],
    }
}
