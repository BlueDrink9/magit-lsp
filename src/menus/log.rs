//! The log menu (Magit `magit-log`).

use super::*;
use crate::menu::Reader;

/// The log transient with Magit's complete action and argument list.
pub fn definition() -> MenuDef {
    MenuDef {
        id: "log",
        title: "Log",
        base_command: "git log",
        groups: vec![
            group(
                "Arguments",
                vec![
                    option("=n", "-n", "Limit the number of commits", Reader::Number),
                    option("=a", "--author=", "Limit to an author", Reader::Text),
                    option("=s", "--since=", "Limit to commits since", Reader::Date),
                    option("=u", "--until=", "Limit to commits until", Reader::Date),
                    option("=g", "--grep=", "Search commit messages", Reader::Text),
                    switch("-i", "--regexp-ignore-case", "Search case-insensitive"),
                    switch("-I", "--invert-grep", "Invert search pattern"),
                    option("=M", "--merges", "Only merges", Reader::Choice),
                    option("=m", "--no-merges", "Omit merges", Reader::Choice),
                    option("=p", "--first-parent", "First parent", Reader::Choice),
                    switch("-D", "--simplify-by-decoration", "Simplify by decoration"),
                    switch("-f", "--follow", "Follow renames when showing single-file log"),
                    switch("-r", "--reverse", "Reverse order"),
                    switch("-g", "--graph", "Show graph"),
                    switch("-c", "--color", "Show graph in color"),
                    switch("-d", "--decorate", "Show refnames"),
                    switch("-S", "--show-signature", "Show signatures"),
                    switch("-h", "++header", "Show header"),
                    switch("-p", "--patch", "Show diffs"),
                    switch("-s", "--stat", "Show diffstats"),
                ],
            ),
            group(
                "Log",
                vec![
                    action(
                        "l",
                        "log.current",
                        "current",
                        "magit-log-current",
                        "R-5.1",
                    ),
                    action("o", "log.other", "other", "magit-log-other", "R-5.1"),
                    action("h", "log.head", "HEAD", "magit-log-head", "R-5.1"),
                    action("u", "log.related", "related", "magit-log-related", "R-5.1"),
                ],
            ),
            group(
                "",
                vec![
                    action(
                        "L",
                        "log.local-branches",
                        "local branches",
                        "magit-log-branches",
                        "R-5.1",
                    ),
                    action(
                        "b",
                        "log.all-branches",
                        "all branches",
                        "magit-log-all-branches",
                        "R-5.1",
                    ),
                    action(
                        "a",
                        "log.all-references",
                        "all references",
                        "magit-log-all",
                        "R-5.1",
                    ),
                    action(
                        "R",
                        "log.reflog-objects",
                        "reflog objects",
                        "magit-log-reflog",
                        "R-5.1",
                    ),
                    action(
                        "B",
                        "log.matching-branches",
                        "matching branches",
                        "magit-log-matching-branches",
                        "R-5.1",
                    ),
                    action(
                        "T",
                        "log.matching-tags",
                        "matching tags",
                        "magit-log-matching-tags",
                        "R-5.1",
                    ),
                    action("m", "log.merged", "merged", "magit-log-merged", "R-5.1"),
                ],
            ),
            group(
                "Reflog",
                vec![
                    action(
                        "r",
                        "log.reflog-current",
                        "current",
                        "magit-reflog-current",
                        "R-5.1",
                    ),
                    action(
                        "O",
                        "log.reflog-other",
                        "other",
                        "magit-reflog-other",
                        "R-5.1",
                    ),
                    action(
                        "H",
                        "log.reflog-head",
                        "HEAD",
                        "magit-reflog-head",
                        "R-5.1",
                    ),
                ],
            ),
            group(
                "Wiplog",
                vec![
                    action(
                        "i",
                        "log.wiplog-index",
                        "index",
                        "magit-wip-log-index",
                        "R-5.1",
                    ),
                    action(
                        "w",
                        "log.wiplog-worktree",
                        "worktree",
                        "magit-wip-log-worktree",
                        "R-5.1",
                    ),
                ],
            ),
            group(
                "Other",
                vec![action("s", "log.shortlog", "shortlog", "magit-shortlog", "R-5.1")],
            ),
        ],
    }
}
