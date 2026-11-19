//! The reset menu (Magit `magit-reset`).

use super::*;

/// The reset transient with Magit's complete action list.
pub fn definition() -> MenuDef {
    MenuDef {
        id: "reset",
        title: "Reset",
        base_command: "git reset",
        groups: vec![
            group(
                "Reset",
                vec![
                    action("b", "reset.branch", "branch", "magit-branch-reset", "R-6.3"),
                    action("f", "reset.file", "file", "magit-file-checkout", "R-6.3"),
                ],
            ),
            group(
                "Reset this",
                vec![
                    action(
                        "m",
                        "reset.mixed",
                        "mixed    (HEAD and index)",
                        "magit-reset-mixed",
                        "",
                    ),
                    action(
                        "s",
                        "reset.soft",
                        "soft     (HEAD only)",
                        "magit-reset-soft",
                        "",
                    ),
                    action(
                        "h",
                        "reset.hard",
                        "hard     (HEAD, index and worktree)",
                        "magit-reset-hard",
                        "",
                    ),
                    action(
                        "k",
                        "reset.keep",
                        "keep     (HEAD and index, keeping uncommitted)",
                        "magit-reset-keep",
                        "",
                    ),
                    action(
                        "i",
                        "reset.index",
                        "index    (only)",
                        "magit-reset-index",
                        "R-6.3",
                    ),
                    action(
                        "w",
                        "reset.worktree",
                        "worktree (only)",
                        "magit-reset-worktree",
                        "R-6.3",
                    ),
                ],
            ),
        ],
    }
}
