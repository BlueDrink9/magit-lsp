//! The stash menu (Magit `magit-stash`).

use super::*;

/// The stash transient with Magit's complete action and argument list.
pub fn definition() -> MenuDef {
    MenuDef {
        id: "stash",
        title: "Stash",
        base_command: "git stash",
        groups: vec![
            group(
                "Arguments",
                vec![
                    switch("-u", "--include-untracked", "Also save untracked files"),
                    switch("-a", "--all", "Also save untracked and ignored files"),
                ],
            ),
            group(
                "Stash",
                vec![
                    action("z", "stash.both", "both", "magit-stash-both", "R-6.5"),
                    action("i", "stash.index", "index", "magit-stash-index", "R-6.5"),
                    action(
                        "w",
                        "stash.worktree",
                        "worktree",
                        "magit-stash-worktree",
                        "R-6.5",
                    ),
                    action(
                        "x",
                        "stash.keep-index",
                        "keeping index",
                        "magit-stash-keep-index",
                        "R-6.5",
                    ),
                    action("P", "stash.push", "push", "magit-stash-push", "R-6.5"),
                ],
            ),
            group(
                "Snapshot",
                vec![
                    action(
                        "Z",
                        "stash.snapshot-both",
                        "both",
                        "magit-snapshot-both",
                        "R-6.5",
                    ),
                    action(
                        "I",
                        "stash.snapshot-index",
                        "index",
                        "magit-snapshot-index",
                        "R-6.5",
                    ),
                    action(
                        "W",
                        "stash.snapshot-worktree",
                        "worktree",
                        "magit-snapshot-worktree",
                        "R-6.5",
                    ),
                    action(
                        "r",
                        "stash.wip-commit",
                        "to wip ref",
                        "magit-wip-commit",
                        "R-6.5",
                    ),
                ],
            ),
            group(
                "Use",
                vec![
                    action("a", "stash.apply", "Apply", "magit-stash-apply", "R-6.5"),
                    action("p", "stash.pop", "Pop", "magit-stash-pop", "R-6.5"),
                    action("k", "stash.drop", "Drop", "magit-stash-drop", "R-6.5"),
                ],
            ),
            group(
                "Inspect",
                vec![
                    action("l", "stash.list", "List", "magit-stash-list", "R-6.5"),
                    action("v", "stash.show", "Show", "magit-stash-show", "R-6.5"),
                ],
            ),
            group(
                "Transform",
                vec![
                    action(
                        "b",
                        "stash.branch",
                        "Branch",
                        "magit-stash-branch",
                        "R-6.5",
                    ),
                    action(
                        "B",
                        "stash.branch-here",
                        "Branch here",
                        "magit-stash-branch-here",
                        "R-6.5",
                    ),
                    action(
                        "f",
                        "stash.format-patch",
                        "Format patch",
                        "magit-stash-format-patch",
                        "R-6.5",
                    ),
                ],
            ),
        ],
    }
}
