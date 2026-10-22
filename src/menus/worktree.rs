//! The worktree menu (Magit `magit-worktree`).

use super::*;

/// The worktree transient with Magit's complete action list.
pub fn definition() -> MenuDef {
    MenuDef {
        id: "worktree",
        title: "Worktree",
        base_command: "git worktree",
        groups: vec![
            group(
                "Create new",
                vec![
                    action(
                        "b",
                        "worktree.checkout",
                        "worktree",
                        "magit-worktree-checkout",
                        "R-8.2",
                    ),
                    action(
                        "c",
                        "worktree.create",
                        "branch and worktree",
                        "magit-worktree-branch",
                        "R-8.2",
                    ),
                ],
            ),
            group(
                "Commands",
                vec![
                    action(
                        "m",
                        "worktree.move",
                        "Move worktree",
                        "magit-worktree-move",
                        "R-8.2",
                    ),
                    action(
                        "k",
                        "worktree.delete",
                        "Delete worktree",
                        "magit-worktree-delete",
                        "R-8.2",
                    ),
                    action(
                        "g",
                        "worktree.visit",
                        "Visit worktree",
                        "magit-worktree-status",
                        "R-8.2",
                    ),
                ],
            ),
        ],
    }
}
