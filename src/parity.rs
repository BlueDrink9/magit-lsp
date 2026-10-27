//! The Magit parity table (§3.12).
//!
//! Rows come from the menu definitions, so every switch, option and action is
//! listed exactly once per menu. Action keys are shown with their Magit prefix
//! (`c c`, `X m`); switches and options use magit-lsp's canonical menu notation
//! (`-a` for a switch, `=A` for an option).

use crate::menus::{ItemDef, MenuDef};
use serde::Serialize;

/// One row of the parity table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    /// The Magit command this item mirrors (`-` for a switch or option).
    pub magit: String,
    /// The Magit default key (or the canonical argument key).
    pub magit_key: String,
    /// Our command id (`-` for a switch or option).
    pub command: String,
    /// `Implemented` or `Planned R-x.y`.
    pub status: String,
    /// The menu that contains the item.
    pub menu: String,
}

/// The Magit prefix key of a menu, used to spell action keys.
fn prefix(id: &str) -> &'static str {
    match id {
        "commit" => "c",
        "branch" => "b",
        "fetch" => "f",
        "pull" => "F",
        "push" => "P",
        "merge" => "m",
        "rebase" => "r",
        "stash" => "z",
        "tag" => "t",
        "reset" => "X",
        "cherry-pick" => "A",
        "revert" => "V",
        "bisect" => "B",
        "remote" => "M",
        "submodule" => "o",
        "worktree" => "Z",
        "ignore" => "i",
        "log" => "l",
        "diff" => "d",
        "patch-apply" => "w",
        "patch-create" => "W",
        _ => "",
    }
}

/// Every row, in menu definition order.
pub fn rows() -> Vec<Row> {
    let mut rows = Vec::new();
    for def in crate::menus::all() {
        rows.extend(menu_rows(&def));
    }
    rows
}

fn menu_rows(def: &MenuDef) -> Vec<Row> {
    let mut rows = Vec::new();
    for group in &def.groups {
        for item in &group.items {
            rows.push(match item {
                ItemDef::Switch { key, .. } | ItemDef::Option { key, .. } => Row {
                    magit: "-".to_string(),
                    magit_key: key.to_string(),
                    command: "-".to_string(),
                    status: "Implemented".to_string(),
                    menu: def.id.to_string(),
                },
                ItemDef::Action {
                    key,
                    command,
                    magit,
                    roadmap,
                    ..
                } => Row {
                    magit: magit.to_string(),
                    magit_key: match prefix(def.id) {
                        "" => key.to_string(),
                        prefix => format!("{prefix} {key}"),
                    },
                    command: command.to_string(),
                    status: if roadmap.is_empty() {
                        "Implemented".to_string()
                    } else {
                        format!("Planned {roadmap}")
                    },
                    menu: def.id.to_string(),
                },
            });
        }
    }
    // `build` appends the dispatch toggle to every menu; list it too.
    if def.id != "dispatch" {
        rows.push(Row {
            magit: "-".to_string(),
            magit_key: "?".to_string(),
            command: "open.dispatch".to_string(),
            status: "Implemented".to_string(),
            menu: def.id.to_string(),
        });
    }
    rows
}

/// The markdown table, including its header row.
pub fn markdown() -> String {
    let mut out = String::from(
        "| Magit command | Magit key | magit-lsp command | Status | Menu |\n\
         | --- | --- | --- | --- | --- |\n",
    );
    for row in rows() {
        out.push_str(&format!(
            "| `{}` | `{}` | `{}` | {} | {} |\n",
            row.magit, row.magit_key, row.command, row.status, row.menu
        ));
    }
    out
}

/// The JSON form, pretty-printed with a trailing newline.
pub fn json() -> String {
    let mut text = serde_json::to_string_pretty(&rows()).expect("parity rows serialise");
    text.push('\n');
    text
}

/// The README markers that delimit the generated table.
pub const README_START: &str = "<!-- parity:start -->";
/// The closing marker.
pub const README_END: &str = "<!-- parity:end -->";

/// Wrap `markdown` in the README markers.
pub fn readme_block() -> String {
    format!("{README_START}\n{}{README_END}\n", markdown())
}
