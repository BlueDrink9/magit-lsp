//! The cherry-pick menu (Magit `magit-cherry-pick`).

use super::*;
use crate::menu::Reader;

/// The cherry-pick transient with Magit's complete action and argument list.
pub fn definition() -> MenuDef {
    MenuDef {
        id: "cherry-pick",
        title: "Cherry-Pick",
        base_command: "git cherry-pick",
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
                    option("=s", "--strategy=", "Strategy", Reader::Choice),
                    switch("-F", "--ff", "Attempt fast-forward"),
                    switch("-x", "-x", "Reference cherry in commit message"),
                    switch("-e", "--edit", "Edit commit messages"),
                    switch("-g", "--gpg-sign", "Sign using gpg"),
                    switch("-s", "--signoff", "Add Signed-off-by trailer"),
                ],
            ),
            group(
                "Apply here",
                vec![
                    action(
                        "A",
                        "cherry-pick.pick",
                        "Pick",
                        "magit-cherry-copy",
                        "R-7.2",
                    ),
                    action(
                        "a",
                        "cherry-pick.apply",
                        "Apply",
                        "magit-cherry-apply",
                        "R-7.2",
                    ),
                    action(
                        "h",
                        "cherry-pick.harvest",
                        "Harvest",
                        "magit-cherry-harvest",
                        "R-7.2",
                    ),
                    action(
                        "m",
                        "cherry-pick.squash",
                        "Squash",
                        "magit-merge-squash",
                        "R-7.2",
                    ),
                ],
            ),
            group(
                "Apply elsewhere",
                vec![
                    action(
                        "d",
                        "cherry-pick.donate",
                        "Donate",
                        "magit-cherry-donate",
                        "R-7.2",
                    ),
                    action(
                        "n",
                        "cherry-pick.spinout",
                        "Spinout",
                        "magit-cherry-spinout",
                        "R-7.2",
                    ),
                    action(
                        "s",
                        "cherry-pick.spinoff",
                        "Spinoff",
                        "magit-cherry-spinoff",
                        "R-7.2",
                    ),
                ],
            ),
        ],
    }
}
