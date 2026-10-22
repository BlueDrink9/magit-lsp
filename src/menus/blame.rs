//! The blame menu. Magit has no blame transient; these are the commands from
//! the Blaming section of the manual, with the keys they use in a blame
//! buffer (R-8.1).

use super::*;

/// The blame command list.
pub fn definition() -> MenuDef {
    MenuDef {
        id: "blame",
        title: "Blame",
        base_command: "git blame",
        groups: vec![
            group(
                "Blame",
                vec![
                    action("b", "blame.start", "Blame", "magit-blame", "R-8.1"),
                    action(
                        "r",
                        "blame.removal",
                        "Removal",
                        "magit-blame-removal",
                        "R-8.1",
                    ),
                    action(
                        "f",
                        "blame.reverse",
                        "Reverse",
                        "magit-blame-reverse",
                        "R-8.1",
                    ),
                    action("e", "blame.echo", "Echo", "magit-blame-echo", "R-8.1"),
                ],
            ),
            group(
                "Chunks",
                vec![
                    action(
                        "n",
                        "blame.next-chunk",
                        "Next chunk",
                        "magit-blame-next-chunk",
                        "R-8.1",
                    ),
                    action(
                        "N",
                        "blame.next-chunk-same-commit",
                        "Next chunk of same commit",
                        "magit-blame-next-chunk-same-commit",
                        "R-8.1",
                    ),
                    action(
                        "p",
                        "blame.previous-chunk",
                        "Previous chunk",
                        "magit-blame-previous-chunk",
                        "R-8.1",
                    ),
                    action(
                        "P",
                        "blame.previous-chunk-same-commit",
                        "Previous chunk of same commit",
                        "magit-blame-previous-chunk-same-commit",
                        "R-8.1",
                    ),
                ],
            ),
            group(
                "Style",
                vec![action(
                    "c",
                    "blame.cycle-style",
                    "Cycle style",
                    "magit-blame-cycle-style",
                    "R-8.1",
                )],
            ),
        ],
    }
}
