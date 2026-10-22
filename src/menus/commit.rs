//! The commit menu (Magit `magit-commit`).

use super::*;
use crate::menu::Reader;

/// The commit transient with Magit's complete action and argument list.
pub fn definition() -> MenuDef {
    MenuDef {
        id: "commit",
        title: "Commit",
        base_command: "git commit",
        groups: vec![
            group(
                "Arguments",
                vec![
                    switch("-a", "--all", "Stage all modified and deleted files"),
                    switch("-e", "--allow-empty", "Allow empty commit"),
                    switch("-v", "--verbose", "Show diff of changes to be committed"),
                    switch("-n", "--no-verify", "Disable hooks"),
                    switch("-R", "--reset-author", "Claim authorship and reset author date"),
                    switch("-s", "--signoff", "Add Signed-off-by trailer"),
                    option("=A", "--author=", "Override the author", Reader::Text),
                    option("=D", "--date=", "Override the author date", Reader::Date),
                    option("=S", "--gpg-sign=", "Sign using gpg", Reader::Text),
                    option(
                        "=C",
                        "--reuse-message=",
                        "Reuse commit message",
                        Reader::Commit,
                    ),
                    option(
                        "=c",
                        "--reedit-message=",
                        "Reedit commit message",
                        Reader::Commit,
                    ),
                ],
            ),
            group(
                "Create",
                vec![action(
                    "c",
                    "commit.create",
                    "Commit",
                    "magit-commit-create",
                    "R-4.1",
                )],
            ),
            group(
                "Edit HEAD",
                vec![
                    action("e", "commit.extend", "Extend", "magit-commit-extend", "R-4.1"),
                    action("a", "commit.amend", "Amend", "magit-commit-amend", "R-4.1"),
                    action("w", "commit.reword", "Reword", "magit-commit-reword", "R-4.1"),
                    action(
                        "d",
                        "commit.reshelve",
                        "Reshelve",
                        "magit-commit-reshelve",
                        "R-4.1",
                    ),
                ],
            ),
            group(
                "Edit",
                vec![
                    action("f", "commit.fixup", "Fixup", "magit-commit-fixup", "R-4.1"),
                    action("s", "commit.squash", "Squash", "magit-commit-squash", "R-4.1"),
                    action("A", "commit.alter", "Alter", "magit-commit-alter", "R-4.1"),
                    action(
                        "n",
                        "commit.augment",
                        "Augment",
                        "magit-commit-augment",
                        "R-4.1",
                    ),
                    action("W", "commit.revise", "Revise", "magit-commit-revise", "R-4.1"),
                ],
            ),
            group(
                "Edit and rebase",
                vec![
                    action(
                        "F",
                        "commit.instant-fixup",
                        "Instant fixup",
                        "magit-commit-instant-fixup",
                        "R-4.1",
                    ),
                    action(
                        "S",
                        "commit.instant-squash",
                        "Instant squash",
                        "magit-commit-instant-squash",
                        "R-4.1",
                    ),
                    action(
                        "R",
                        "commit.reword-past",
                        "Reword past",
                        "magit-rebase-reword-commit",
                        "R-4.1",
                    ),
                ],
            ),
            group(
                "Spread across commits",
                vec![
                    action(
                        "x",
                        "commit.autofixup",
                        "Modified files",
                        "magit-commit-autofixup",
                        "R-4.1",
                    ),
                    action(
                        "X",
                        "commit.absorb-modules",
                        "Updated modules",
                        "magit-commit-absorb-modules",
                        "R-4.1",
                    ),
                ],
            ),
        ],
    }
}
