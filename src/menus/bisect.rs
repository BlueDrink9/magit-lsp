//! The bisect menu (Magit `magit-bisect`).

use super::*;
use crate::menu::Reader;

/// The bisect transient with Magit's complete action and argument list.
pub fn definition() -> MenuDef {
    MenuDef {
        id: "bisect",
        title: "Bisect",
        base_command: "git bisect",
        groups: vec![
            group(
                "Arguments",
                vec![
                    switch("-n", "--no-checkout", "Don't checkout commits"),
                    switch(
                        "-p",
                        "--first-parent",
                        "Follow only first parent of a merge",
                    ),
                    option("=o", "--term-old=", "Old/good term", Reader::Text),
                    option("=n", "--term-new=", "New/bad term", Reader::Text),
                ],
            ),
            group(
                "Start",
                vec![
                    action("B", "bisect.start", "Start", "magit-bisect-start", "R-8.2"),
                    action(
                        "s",
                        "bisect.run",
                        "Start script",
                        "magit-bisect-run",
                        "R-8.2",
                    ),
                ],
            ),
            group(
                "Mark",
                vec![
                    action("g", "bisect.good", "Good", "magit-bisect-good", "R-8.2"),
                    action("m", "bisect.mark", "Mark", "magit-bisect-mark", "R-8.2"),
                    action("k", "bisect.skip", "Skip", "magit-bisect-skip", "R-8.2"),
                    action(
                        "r",
                        "bisect.reset",
                        "Reset",
                        "magit-bisect-reset",
                        "R-8.2",
                    ),
                ],
            ),
        ],
    }
}
