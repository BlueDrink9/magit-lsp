//! The remote menu (Magit `magit-remote`).

use super::*;
use crate::menu::Reader;

/// The remote transient with Magit's complete action and argument list.
pub fn definition() -> MenuDef {
    MenuDef {
        id: "remote",
        title: "Remote",
        base_command: "git remote",
        groups: vec![
            group(
                "Arguments for add",
                vec![switch("-f", "-f", "Fetch after add")],
            ),
            group(
                "Variables",
                vec![
                    option("=u", "--url=", "Set the remote URL", Reader::Text),
                    option("=U", "--fetch=", "Set the fetch refspec", Reader::Text),
                    option("=s", "--pushurl=", "Set the push URL", Reader::Text),
                    option("=S", "--push=", "Set the push refspec", Reader::Text),
                    option("=O", "--tagopt=", "Set tagopt", Reader::Choice),
                    option(
                        "=h",
                        "--follow-remote-head=",
                        "Set followRemoteHead",
                        Reader::Choice,
                    ),
                ],
            ),
            group(
                "Actions",
                vec![
                    action("a", "remote.add", "Add", "magit-remote-add", "R-6.3"),
                    action(
                        "r",
                        "remote.rename",
                        "Rename",
                        "magit-remote-rename",
                        "R-6.3",
                    ),
                    action(
                        "k",
                        "remote.remove",
                        "Remove",
                        "magit-remote-remove",
                        "R-6.3",
                    ),
                    action(
                        "C",
                        "remote.configure",
                        "Configure...",
                        "magit-remote-configure",
                        "R-6.3",
                    ),
                    action(
                        "p",
                        "remote.prune",
                        "Prune stale branches",
                        "magit-remote-prune",
                        "R-6.3",
                    ),
                    action(
                        "P",
                        "remote.prune-refspecs",
                        "Prune stale refspecs",
                        "magit-remote-prune-refspecs",
                        "R-6.3",
                    ),
                    action(
                        "z",
                        "remote.unshallow",
                        "Unshallow remote",
                        "magit-remote-unshallow",
                        "R-6.3",
                    ),
                    action(
                        "d u",
                        "remote.update-default",
                        "Update default branch",
                        "magit-update-default-branch",
                        "R-6.3",
                    ),
                ],
            ),
        ],
    }
}
