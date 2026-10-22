//! The fetch menu (Magit `magit-fetch`).

use super::*;

/// The fetch transient with Magit's complete action and argument list.
pub fn definition() -> MenuDef {
    MenuDef {
        id: "fetch",
        title: "Fetch",
        base_command: "git fetch",
        groups: vec![
            group(
                "Arguments",
                vec![
                    switch("-p", "--prune", "Prune deleted branches"),
                    switch("-t", "--tags", "Fetch all tags"),
                    switch("-u", "--unshallow", "Fetch full history"),
                    switch("-F", "--force", "Force"),
                ],
            ),
            group(
                "Fetch from",
                vec![
                    action(
                        "p",
                        "fetch.from-pushremote",
                        "push-remote",
                        "magit-fetch-from-pushremote",
                        "R-6.2",
                    ),
                    action(
                        "u",
                        "fetch.from-upstream",
                        "upstream",
                        "magit-fetch-from-upstream",
                        "R-6.2",
                    ),
                    action(
                        "e",
                        "fetch.other",
                        "elsewhere",
                        "magit-fetch-other",
                        "R-6.2",
                    ),
                    action(
                        "a",
                        "fetch.all",
                        "all remotes",
                        "magit-fetch-all",
                        "R-6.2",
                    ),
                ],
            ),
            group(
                "Fetch",
                vec![
                    action(
                        "o",
                        "fetch.branch",
                        "another branch",
                        "magit-fetch-branch",
                        "R-6.2",
                    ),
                    action(
                        "r",
                        "fetch.refspec",
                        "explicit refspec",
                        "magit-fetch-refspec",
                        "R-6.2",
                    ),
                    action(
                        "m",
                        "fetch.modules",
                        "submodules",
                        "magit-fetch-modules",
                        "R-6.2",
                    ),
                ],
            ),
            group(
                "Configure",
                vec![action(
                    "C",
                    "fetch.configure",
                    "variables...",
                    "magit-branch-configure",
                    "R-6.2",
                )],
            ),
        ],
    }
}
