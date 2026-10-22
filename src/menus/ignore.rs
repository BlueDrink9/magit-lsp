//! The ignore menu (Magit `magit-gitignore`).

use super::*;

/// The gitignore transient with Magit's complete action list.
pub fn definition() -> MenuDef {
    MenuDef {
        id: "ignore",
        title: "Ignore",
        base_command: "git check-ignore",
        groups: vec![
            group(
                "Gitignore",
                vec![
                    action(
                        "t",
                        "ignore.topdir",
                        "shared at toplevel (.gitignore)",
                        "magit-gitignore-in-topdir",
                        "R-8.2",
                    ),
                    action(
                        "s",
                        "ignore.subdir",
                        "shared in subdirectory (.gitignore)",
                        "magit-gitignore-in-subdir",
                        "R-8.2",
                    ),
                    action(
                        "p",
                        "ignore.gitdir",
                        "private to this repository (.git/info/exclude)",
                        "magit-gitignore-in-gitdir",
                        "R-8.2",
                    ),
                    action(
                        "g",
                        "ignore.system",
                        "private to this user (core.excludesFile)",
                        "magit-gitignore-on-system",
                        "R-8.2",
                    ),
                ],
            ),
            group(
                "Skip worktree",
                vec![
                    action(
                        "w",
                        "ignore.skip-worktree",
                        "do skip worktree",
                        "magit-skip-worktree",
                        "R-8.2",
                    ),
                    action(
                        "W",
                        "ignore.no-skip-worktree",
                        "do not skip worktree",
                        "magit-no-skip-worktree",
                        "R-8.2",
                    ),
                ],
            ),
            group(
                "Assume unchanged",
                vec![
                    action(
                        "u",
                        "ignore.assume-unchanged",
                        "do assume unchanged",
                        "magit-assume-unchanged",
                        "R-8.2",
                    ),
                    action(
                        "U",
                        "ignore.no-assume-unchanged",
                        "do not assume unchanged",
                        "magit-no-assume-unchanged",
                        "R-8.2",
                    ),
                ],
            ),
        ],
    }
}
