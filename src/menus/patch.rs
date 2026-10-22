//! The patch menus: applying maildir/plain patches and creating them.

use super::*;
use crate::menu::Reader;

/// The apply-patches menu (Magit `magit-am`, `w`).
pub fn apply_definition() -> MenuDef {
    MenuDef {
        id: "patch-apply",
        title: "Apply Patches",
        base_command: "git am",
        groups: vec![
            group(
                "Arguments",
                vec![
                    switch("-3", "--3way", "Fall back on 3way merge"),
                    switch("-R", "--reject", "Reject only failed hunks"),
                    option("=p", "-p", "Remove leading slashes from paths", Reader::Number),
                    switch("-c", "--scissors", "Remove text before scissors line"),
                    switch("-k", "--keep", "Inhibit removal of email cruft"),
                    switch("-b", "--keep-non-patch", "Limit removal of email cruft"),
                    switch(
                        "-d",
                        "--committer-date-is-author-date",
                        "Use author date as committer date",
                    ),
                    switch("-t", "--ignore-date", "Use current time as author date"),
                    switch("-g", "--gpg-sign", "Sign using gpg"),
                    switch("-s", "--signoff", "Add Signed-off-by trailer"),
                ],
            ),
            group(
                "Apply",
                vec![
                    action(
                        "m",
                        "patch.apply-maildir",
                        "maildir",
                        "magit-am-apply-maildir",
                        "R-8.2",
                    ),
                    action(
                        "w",
                        "patch.apply-patches",
                        "patches",
                        "magit-am-apply-patches",
                        "R-8.2",
                    ),
                    action(
                        "a",
                        "patch.apply-plain",
                        "plain patch",
                        "magit-patch-apply",
                        "R-8.2",
                    ),
                ],
            ),
        ],
    }
}

/// The format-patches menu (Magit `magit-patch`, `W`).
pub fn create_definition() -> MenuDef {
    MenuDef {
        id: "patch-create",
        title: "Format Patches",
        base_command: "git format-patch",
        groups: vec![group(
            "Actions",
            vec![
                action(
                    "c",
                    "patch.create",
                    "Create patches",
                    "magit-patch-create",
                    "R-8.2",
                ),
                action("w", "open.patch-apply", "Apply patches", "magit-am", ""),
                action(
                    "a",
                    "patch.apply-plain",
                    "Apply plain patch",
                    "magit-patch-apply",
                    "R-8.2",
                ),
                action(
                    "s",
                    "patch.save",
                    "Save diff as patch",
                    "magit-patch-save",
                    "R-8.2",
                ),
                action(
                    "r",
                    "patch.request-pull",
                    "Request pull",
                    "magit-request-pull",
                    "R-8.2",
                ),
            ],
        )],
    }
}
