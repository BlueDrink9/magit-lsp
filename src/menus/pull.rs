//! The pull menu (Magit `magit-pull`).

use super::*;
use crate::menu::Reader;

/// The pull transient with Magit's complete action and argument list.
pub fn definition() -> MenuDef {
    MenuDef {
        id: "pull",
        title: "Pull",
        base_command: "git pull",
        groups: vec![
            group(
                "Arguments",
                vec![
                    switch("-f", "--ff-only", "Fast-forward only"),
                    option("=r", "--rebase=", "Rebase instead of merge", Reader::Choice),
                    switch("-A", "--autostash", "Autostash"),
                    switch("-F", "--force", "Force"),
                ],
            ),
            group(
                "Pull from",
                vec![
                    action(
                        "p",
                        "pull.from-pushremote",
                        "push-remote",
                        "magit-pull-from-pushremote",
                        "R-6.2",
                    ),
                    action(
                        "u",
                        "pull.from-upstream",
                        "upstream",
                        "magit-pull-from-upstream",
                        "R-6.2",
                    ),
                    action(
                        "e",
                        "pull.other",
                        "elsewhere",
                        "magit-pull-branch",
                        "R-6.2",
                    ),
                ],
            ),
            group(
                "Pull into upstream",
                vec![action(
                    "U",
                    "pull.into-upstream",
                    "from upstream",
                    "magit-pull-into-upstream",
                    "R-6.2",
                )],
            ),
            group(
                "Fetch from",
                vec![
                    action(
                        "f",
                        "pull.fetch-remotes",
                        "remotes",
                        "magit-fetch-all-no-prune",
                        "R-6.2",
                    ),
                    action(
                        "F",
                        "pull.fetch-remotes-prune",
                        "remotes and prune",
                        "magit-fetch-all-prune",
                        "R-6.2",
                    ),
                ],
            ),
        ],
    }
}
