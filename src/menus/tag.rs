//! The tag menu (Magit `magit-tag`).

use super::*;
use crate::menu::Reader;

/// The tag transient with Magit's complete action and argument list.
pub fn definition() -> MenuDef {
    MenuDef {
        id: "tag",
        title: "Tag",
        base_command: "git tag",
        groups: vec![
            group(
                "Arguments",
                vec![
                    switch("-f", "--force", "Force"),
                    switch("-e", "--edit", "Edit message"),
                    switch("-a", "--annotate", "Annotate"),
                    switch("-s", "--sign", "Sign"),
                    option("=u", "--local-user=", "Sign as", Reader::Text),
                ],
            ),
            group(
                "Create",
                vec![
                    action("t", "tag.create", "tag", "magit-tag-create", "R-6.3"),
                    action(
                        "r",
                        "tag.release",
                        "release",
                        "magit-tag-release",
                        "R-6.3",
                    ),
                ],
            ),
            group(
                "Do",
                vec![
                    action("k", "tag.delete", "delete", "magit-tag-delete", "R-6.3"),
                    action("p", "tag.prune", "prune", "magit-tag-prune", "R-6.3"),
                ],
            ),
        ],
    }
}
