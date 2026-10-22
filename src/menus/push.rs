//! The push menu (Magit `magit-push`).

use super::*;
use crate::menu::Reader;

/// The push transient with Magit's complete action and argument list.
pub fn definition() -> MenuDef {
    MenuDef {
        id: "push",
        title: "Push",
        base_command: "git push",
        groups: vec![
            group(
                "Arguments",
                vec![
                    switch("-f", "--force-with-lease", "Force with lease"),
                    switch("-F", "--force", "Force"),
                    switch("-h", "--no-verify", "Disable hooks"),
                    switch("-n", "--dry-run", "Dry run"),
                    switch("-u", "--set-upstream", "Set upstream"),
                    switch("-T", "--tags", "Include all tags"),
                    switch("-t", "--follow-tags", "Include related annotated tags"),
                    option(
                        "=o",
                        "--push-option=",
                        "Set push-options",
                        Reader::Text,
                    ),
                ],
            ),
            group(
                "Push to",
                vec![
                    action(
                        "p",
                        "push.to-pushremote",
                        "push-remote",
                        "magit-push-current-to-pushremote",
                        "R-6.2",
                    ),
                    action(
                        "u",
                        "push.to-upstream",
                        "upstream",
                        "magit-push-current-to-upstream",
                        "R-6.2",
                    ),
                    action(
                        "e",
                        "push.current",
                        "elsewhere",
                        "magit-push-current",
                        "R-6.2",
                    ),
                ],
            ),
            group(
                "Push",
                vec![
                    action(
                        "o",
                        "push.other",
                        "another branch",
                        "magit-push-other",
                        "R-6.2",
                    ),
                    action(
                        "r",
                        "push.refspecs",
                        "explicit refspecs",
                        "magit-push-refspecs",
                        "R-6.2",
                    ),
                    action(
                        "m",
                        "push.matching",
                        "matching branches",
                        "magit-push-matching",
                        "R-6.2",
                    ),
                    action("T", "push.tag", "a tag", "magit-push-tag", "R-6.2"),
                    action("t", "push.tags", "all tags", "magit-push-tags", "R-6.2"),
                    action(
                        "n",
                        "push.notes-ref",
                        "a note ref",
                        "magit-push-notes-ref",
                        "R-6.2",
                    ),
                ],
            ),
            group(
                "Configure",
                vec![action(
                    "C",
                    "push.configure",
                    "Set variables...",
                    "magit-branch-configure",
                    "R-6.2",
                )],
            ),
        ],
    }
}
