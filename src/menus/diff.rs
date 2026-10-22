//! The diff menu (Magit `magit-diff`).

use super::*;
use crate::menu::Reader;

/// The diff transient with Magit's complete action and argument list.
pub fn definition() -> MenuDef {
    MenuDef {
        id: "diff",
        title: "Diff",
        base_command: "git diff",
        groups: vec![
            group(
                "Limit arguments",
                vec![
                    switch("-b", "--ignore-space-change", "Ignore whitespace changes"),
                    switch("-w", "--ignore-all-space", "Ignore all whitespace"),
                    switch("-D", "--irreversible-delete", "Omit preimage for deletes"),
                ],
            ),
            group(
                "Context arguments",
                vec![switch("-W", "--function-context", "Show surrounding functions")],
            ),
            group(
                "Tune arguments",
                vec![
                    option("=a", "--diff-algorithm=", "Diff algorithm", Reader::Choice),
                    option("=m", "--diff-merges=", "Diff merges", Reader::Choice),
                    switch("-M", "--find-renames", "Detect renames"),
                    switch("-C", "--find-copies", "Detect copies"),
                    switch("-H", "--find-copies-harder", "Detect copies if source unmodified"),
                    switch("-R", "--relative", "Relative paths"),
                    option("=c", "--color-moved=", "Highlight moved lines", Reader::Choice),
                    switch("-S", "--show-signature", "Show signatures"),
                    switch("-s", "--stat", "Show diffstat"),
                ],
            ),
            group(
                "Actions",
                vec![
                    action("d", "diff.dwim", "Dwim", "magit-diff-dwim", "R-5.2"),
                    action("r", "diff.range", "Diff range", "magit-diff-range", "R-5.2"),
                    action("p", "diff.paths", "Diff paths", "magit-diff-paths", "R-5.2"),
                    action(
                        "u",
                        "diff.unstaged",
                        "Diff unstaged",
                        "magit-diff-unstaged",
                        "R-5.2",
                    ),
                    action(
                        "s",
                        "diff.staged",
                        "Diff staged",
                        "magit-diff-staged",
                        "R-5.2",
                    ),
                    action(
                        "w",
                        "diff.worktree",
                        "Diff worktree",
                        "magit-diff-working-tree",
                        "R-5.2",
                    ),
                    action(
                        "c",
                        "diff.show-commit",
                        "Show commit",
                        "magit-show-commit",
                        "R-5.2",
                    ),
                    action(
                        "t",
                        "diff.show-stash",
                        "Show stash",
                        "magit-stash-show",
                        "R-5.2",
                    ),
                ],
            ),
        ],
    }
}
