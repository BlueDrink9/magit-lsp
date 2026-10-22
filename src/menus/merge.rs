//! The merge menu (Magit `magit-merge`).

use super::*;
use crate::menu::Reader;

/// The merge transient with Magit's complete action and argument list.
pub fn definition() -> MenuDef {
    MenuDef {
        id: "merge",
        title: "Merge",
        base_command: "git merge",
        groups: vec![
            group(
                "Arguments",
                vec![
                    switch("-f", "--ff-only", "Fast-forward only"),
                    switch("-n", "--no-ff", "No fast-forward"),
                    option("=s", "--strategy=", "Strategy", Reader::Choice),
                    option(
                        "=X",
                        "--strategy-option=",
                        "Strategy option",
                        Reader::Choice,
                    ),
                    switch(
                        "-b",
                        "-Xignore-space-change",
                        "Ignore changes in amount of whitespace",
                    ),
                    switch(
                        "-w",
                        "-Xignore-all-space",
                        "Ignore whitespace when comparing lines",
                    ),
                    option("=a", "--diff-algorithm=", "Diff algorithm", Reader::Choice),
                    switch("-S", "--gpg-sign", "Sign using gpg"),
                    switch("-s", "--signoff", "Add Signed-off-by trailer"),
                ],
            ),
            group(
                "Actions",
                vec![
                    action("m", "merge.plain", "Merge", "magit-merge-plain", "R-7.1"),
                    action(
                        "e",
                        "merge.editmsg",
                        "Merge and edit message",
                        "magit-merge-editmsg",
                        "R-7.1",
                    ),
                    action(
                        "n",
                        "merge.nocommit",
                        "Merge but don't commit",
                        "magit-merge-nocommit",
                        "R-7.1",
                    ),
                    action(
                        "a",
                        "merge.absorb",
                        "Absorb",
                        "magit-merge-absorb",
                        "R-7.1",
                    ),
                    action(
                        "p",
                        "merge.preview",
                        "Preview merge",
                        "magit-merge-preview",
                        "R-7.1",
                    ),
                    action(
                        "s",
                        "merge.squash",
                        "Squash merge",
                        "magit-merge-squash",
                        "R-7.1",
                    ),
                    action(
                        "d",
                        "merge.dissolve",
                        "Dissolve",
                        "magit-merge-dissolve",
                        "R-7.1",
                    ),
                ],
            ),
        ],
    }
}
