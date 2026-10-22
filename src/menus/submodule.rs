//! The submodule menu (Magit `magit-submodule`).

use super::*;

/// The submodule transient with Magit's complete action and argument list.
pub fn definition() -> MenuDef {
    MenuDef {
        id: "submodule",
        title: "Submodule",
        base_command: "git submodule",
        groups: vec![
            group(
                "Arguments",
                vec![
                    switch("-f", "--force", "Force"),
                    switch("-r", "--recursive", "Recursive"),
                    switch("-N", "--no-fetch", "Do not fetch"),
                    switch("-C", "--checkout", "Checkout tip"),
                    switch("-R", "--rebase", "Rebase onto tip"),
                    switch("-M", "--merge", "Merge tip"),
                    switch("-U", "--remote", "Use upstream tip"),
                ],
            ),
            group(
                "One module actions",
                vec![
                    action("a", "submodule.add", "Add", "magit-submodule-add", "R-8.2"),
                    action(
                        "r",
                        "submodule.register",
                        "Register",
                        "magit-submodule-register",
                        "R-8.2",
                    ),
                    action(
                        "p",
                        "submodule.populate",
                        "Populate",
                        "magit-submodule-populate",
                        "R-8.2",
                    ),
                    action(
                        "u",
                        "submodule.update",
                        "Update",
                        "magit-submodule-update",
                        "R-8.2",
                    ),
                    action(
                        "s",
                        "submodule.synchronize",
                        "Synchronize",
                        "magit-submodule-synchronize",
                        "R-8.2",
                    ),
                    action(
                        "d",
                        "submodule.unpopulate",
                        "Unpopulate",
                        "magit-submodule-unpopulate",
                        "R-8.2",
                    ),
                    action(
                        "k",
                        "submodule.remove",
                        "Remove",
                        "magit-submodule-remove",
                        "R-8.2",
                    ),
                ],
            ),
            group(
                "Populated modules actions",
                vec![
                    action(
                        "l",
                        "submodule.list",
                        "List modules",
                        "magit-list-submodules",
                        "R-8.2",
                    ),
                    action(
                        "f",
                        "submodule.fetch",
                        "Fetch modules",
                        "magit-fetch-modules",
                        "R-8.2",
                    ),
                ],
            ),
        ],
    }
}
