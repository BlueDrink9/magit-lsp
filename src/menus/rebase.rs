//! The rebase menu (Magit `magit-rebase`).

use super::*;
use crate::menu::Reader;

/// The rebase transient with Magit's complete action and argument list.
pub fn definition() -> MenuDef {
    MenuDef {
        id: "rebase",
        title: "Rebase",
        base_command: "git rebase",
        groups: vec![
            group(
                "Arguments",
                vec![
                    switch("-k", "--keep-empty", "Keep empty commits"),
                    switch("-p", "--preserve-merges", "Preserve merges"),
                    switch("-r", "--rebase-merges", "Rebase merges"),
                    switch("-u", "--update-refs", "Update branches"),
                    option("=s", "--strategy=", "Strategy", Reader::Choice),
                    option(
                        "=X",
                        "--strategy-option=",
                        "Strategy option",
                        Reader::Choice,
                    ),
                    option("=a", "--diff-algorithm=", "Diff algorithm", Reader::Choice),
                    switch("-f", "--force-rebase", "Force rebase"),
                    switch(
                        "-d",
                        "--committer-date-is-author-date",
                        "Use author date as committer date",
                    ),
                    switch("-t", "--ignore-date", "Use current time as author date"),
                    switch("-A", "--autosquash", "Autosquash"),
                    switch("-S", "--autostash", "Autostash"),
                    switch("-i", "--interactive", "Interactive"),
                    switch("-h", "--no-verify", "Disable hooks"),
                    option("=x", "--exec=", "Run command after commits", Reader::Text),
                    switch("-g", "--gpg-sign", "Sign using gpg"),
                    switch("-s", "--signoff", "Add Signed-off-by trailer"),
                ],
            ),
            group(
                "Rebase onto",
                vec![
                    action(
                        "p",
                        "rebase.onto-pushremote",
                        "push-remote",
                        "magit-rebase-onto-pushremote",
                        "R-7.3",
                    ),
                    action(
                        "u",
                        "rebase.onto-upstream",
                        "upstream",
                        "magit-rebase-onto-upstream",
                        "R-7.3",
                    ),
                    action(
                        "e",
                        "rebase.elsewhere",
                        "elsewhere",
                        "magit-rebase-branch",
                        "R-7.3",
                    ),
                ],
            ),
            group(
                "Rebase",
                vec![
                    action(
                        "i",
                        "rebase.interactive",
                        "interactively",
                        "magit-rebase-interactive",
                        "R-7.3",
                    ),
                    action(
                        "s",
                        "rebase.subset",
                        "a subset",
                        "magit-rebase-subset",
                        "R-7.3",
                    ),
                    action(
                        "m",
                        "rebase.edit-commit",
                        "to modify a commit",
                        "magit-rebase-edit-commit",
                        "R-7.3",
                    ),
                    action(
                        "w",
                        "rebase.reword-commit",
                        "to reword a commit",
                        "magit-rebase-reword-commit",
                        "R-7.3",
                    ),
                    action(
                        "k",
                        "rebase.remove-commit",
                        "to remove a commit",
                        "magit-rebase-remove-commit",
                        "R-7.3",
                    ),
                    action(
                        "f",
                        "rebase.autosquash",
                        "to autosquash",
                        "magit-rebase-autosquash",
                        "R-7.3",
                    ),
                    action(
                        "t",
                        "rebase.change-dates",
                        "to change dates",
                        "magit-reshelve-since",
                        "R-7.3",
                    ),
                ],
            ),
            group(
                "Actions",
                vec![
                    action(
                        "r",
                        "rebase.continue",
                        "Continue",
                        "magit-rebase-continue",
                        "R-7.3",
                    ),
                    action("a", "rebase.abort", "Abort", "magit-rebase-abort", "R-7.3"),
                ],
            ),
        ],
    }
}
