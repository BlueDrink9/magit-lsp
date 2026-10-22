//! The branch menu (Magit `magit-branch`).

use super::*;
use crate::menu::Reader;
/// The branch transient with Magit's complete action and argument list.
pub fn definition() -> MenuDef {
    MenuDef {
        id: "branch",
        title: "Branch",
        base_command: "git branch",
        groups: vec![
            group(
                "Arguments",
                vec![
                    switch(
                        "-m",
                        "--merge",
                        "Automatically merge conflicting local modifications",
                    ),
                    switch(
                        "-r",
                        "--recurse-submodules",
                        "Recurse submodules when checking out an existing branch",
                    ),
                ],
            ),
            group(
                "Configure",
                vec![
                    action(
                        "d",
                        "branch.configure-description",
                        "Set the branch description",
                        "magit-branch.<branch>.description",
                        "R-6.1",
                    ),
                    option(
                        "=u",
                        "--set-upstream-to=",
                        "Set the upstream branch",
                        Reader::Ref,
                    ),
                    option(
                        "=r",
                        "--rebase=",
                        "Set whether to rebase onto upstream",
                        Reader::Choice,
                    ),
                    option(
                        "=p",
                        "--set-push-remote=",
                        "Set the push-remote",
                        Reader::Ref,
                    ),
                    option("=R", "--pull-rebase=", "Set pull.rebase", Reader::Choice),
                    option(
                        "=P",
                        "--push-default=",
                        "Set remote.pushDefault",
                        Reader::Ref,
                    ),
                    action(
                        "B",
                        "branch.update-default",
                        "Update default branch",
                        "magit-update-default-branch",
                        "R-6.1",
                    ),
                ],
            ),
            group(
                "Checkout",
                vec![
                    action(
                        "b",
                        "branch.checkout",
                        "branch/revision",
                        "magit-checkout",
                        "R-6.1",
                    ),
                    action(
                        "l",
                        "branch.checkout-local",
                        "local branch",
                        "magit-branch-checkout",
                        "R-6.1",
                    ),
                    action(
                        "o",
                        "branch.orphan",
                        "new orphan",
                        "magit-branch-orphan",
                        "R-6.1",
                    ),
                    action(
                        "r",
                        "branch.checkout-remote",
                        "remote ref",
                        "magit-checkout-remote-ref",
                        "R-6.1",
                    ),
                ],
            ),
            group(
                "",
                vec![
                    action(
                        "c",
                        "branch.create-and-checkout",
                        "new branch",
                        "magit-branch-and-checkout",
                        "R-6.1",
                    ),
                    action(
                        "s",
                        "branch.spinoff",
                        "new spin-off",
                        "magit-branch-spinoff",
                        "R-6.1",
                    ),
                    action(
                        "w",
                        "branch.worktree-checkout",
                        "new worktree",
                        "magit-worktree-checkout",
                        "R-6.1",
                    ),
                ],
            ),
            group(
                "Create",
                vec![
                    action(
                        "n",
                        "branch.create",
                        "new branch",
                        "magit-branch-create",
                        "R-6.1",
                    ),
                    action(
                        "S",
                        "branch.spinout",
                        "new spin-out",
                        "magit-branch-spinout",
                        "R-6.1",
                    ),
                    action(
                        "W",
                        "branch.worktree-create",
                        "new worktree",
                        "magit-worktree-branch",
                        "R-6.1",
                    ),
                ],
            ),
            group(
                "Do",
                vec![
                    action(
                        "C",
                        "branch.configure",
                        "configure...",
                        "magit-branch-configure",
                        "R-6.1",
                    ),
                    action("m", "branch.rename", "rename", "magit-branch-rename", "R-6.1"),
                    action("x", "branch.reset", "reset", "magit-branch-reset", "R-6.1"),
                    action("k", "branch.delete", "delete", "magit-branch-delete", "R-6.1"),
                    action(
                        "h",
                        "branch.shelve",
                        "shelve",
                        "magit-branch-shelve",
                        "R-6.1",
                    ),
                    action(
                        "H",
                        "branch.unshelve",
                        "unshelve",
                        "magit-branch-unshelve",
                        "R-6.1",
                    ),
                ],
            ),
        ],
    }
}
