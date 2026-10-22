//! The revert menu (Magit `magit-revert`).

use super::*;
use crate::menu::Reader;

/// The revert transient with Magit's complete action and argument list.
pub fn definition() -> MenuDef {
    MenuDef {
        id: "revert",
        title: "Revert",
        base_command: "git revert",
        groups: vec![
            group(
                "Arguments",
                vec![
                    option(
                        "=m",
                        "--mainline=",
                        "Replay merge relative to parent",
                        Reader::Number,
                    ),
                    switch("-e", "--edit", "Edit commit message"),
                    switch("-E", "--no-edit", "Don't edit commit message"),
                    option("=s", "--strategy=", "Strategy", Reader::Choice),
                    switch("-g", "--gpg-sign", "Sign using gpg"),
                    switch("-s", "--signoff", "Add Signed-off-by trailer"),
                ],
            ),
            group(
                "Actions",
                vec![
                    action(
                        "V",
                        "revert.commit",
                        "Revert commit(s)",
                        "magit-revert-and-commit",
                        "R-7.2",
                    ),
                    action(
                        "v",
                        "revert.changes",
                        "Revert changes",
                        "magit-revert-no-commit",
                        "R-7.2",
                    ),
                ],
            ),
        ],
    }
}
