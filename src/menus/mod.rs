//! Menu definitions, one module per Magit dispatch menu (§3.12).
//!
//! Each menu is plain data: switches, options and actions with the keys and
//! labels Magit uses. [`build`] turns a definition into the runtime [`Menu`];
//! the parity generator walks the same definitions, so a menu item can never
//! be missing from the parity table.

use crate::keymap::Keymap;
use crate::menu::{Group, Item, Menu, MenuStatus, Reader};
use std::collections::BTreeMap;

mod bisect;
mod blame;
mod branch;
mod cherry_pick;
mod commit;
mod diff;
mod dispatch;
mod fetch;
mod ignore;
mod log;
mod merge;
mod patch;
mod pull;
mod push;
mod rebase;
mod remote;
mod reset;
mod revert;
mod stash;
mod submodule;
mod tag;
mod worktree;

/// One menu's complete definition.
pub struct MenuDef {
    pub id: &'static str,
    pub title: &'static str,
    /// The command shown in the argument preview, e.g. `git commit`.
    pub base_command: &'static str,
    pub groups: Vec<GroupDef>,
}

/// A titled group of item definitions.
pub struct GroupDef {
    pub title: &'static str,
    pub items: Vec<ItemDef>,
}

/// A switch, option or action before it becomes a runtime [`Item`].
pub enum ItemDef {
    Switch {
        key: &'static str,
        flag: &'static str,
        label: &'static str,
    },
    Option {
        key: &'static str,
        flag: &'static str,
        label: &'static str,
        reader: Reader,
    },
    Action {
        key: &'static str,
        command: &'static str,
        label: &'static str,
        /// The Magit command this action mirrors, for the parity table.
        magit: &'static str,
        /// The roadmap item that implements it; empty means implemented.
        roadmap: &'static str,
    },
}

/// A boolean argument.
pub(crate) fn switch(
    key: &'static str,
    flag: &'static str,
    label: &'static str,
) -> ItemDef {
    ItemDef::Switch { key, flag, label }
}

/// An argument with a value.
pub(crate) fn option(
    key: &'static str,
    flag: &'static str,
    label: &'static str,
    reader: Reader,
) -> ItemDef {
    ItemDef::Option {
        key,
        flag,
        label,
        reader,
    }
}

/// A single-key action.
pub(crate) fn action(
    key: &'static str,
    command: &'static str,
    label: &'static str,
    magit: &'static str,
    roadmap: &'static str,
) -> ItemDef {
    ItemDef::Action {
        key,
        command,
        label,
        magit,
        roadmap,
    }
}

/// A titled group.
pub(crate) fn group(title: &'static str, items: Vec<ItemDef>) -> GroupDef {
    GroupDef { title, items }
}

/// Build a runtime menu from its definition.
pub fn build(def: &MenuDef) -> Menu {
    let mut groups: Vec<Group> = def
        .groups
        .iter()
        .map(|group| Group {
            title: group.title.to_string(),
            columns: None,
            items: group.items.iter().map(item).collect(),
        })
        .collect();

    // Every menu can reach the dispatch view; the dispatch menu itself must
    // not recurse.
    if def.id != "dispatch" {
        groups.push(Group {
            title: "Menu".to_string(),
            columns: None,
            items: vec![Item::Action {
                key: "?".to_string(),
                command: "open.dispatch".to_string(),
                label: "Show the dispatch menu".to_string(),
                status: MenuStatus::Implemented,
            }],
        });
    }

    let mut menu = Menu {
        id: def.id.to_string(),
        title: def.title.to_string(),
        groups,
        saved_args: crate::menu::Args::default(),
    };
    if def.id == "log" {
        for group in &mut menu.groups {
            for item in &mut group.items {
                match item {
                    Item::Switch { key, on, status, .. } if key == "-g" || key == "-d" => {
                        *on = true;
                        *status = MenuStatus::Implemented;
                    }
                    Item::Option { key, status, .. } if key != "=n" && key != "=a" && key != "=s" && key != "=g" && key != "=S" && key != "=G" && key != "=f" => {
                        *status = MenuStatus::Planned { roadmap_id: "R-5.3".to_string() };
                    }
                    Item::Switch { key, status, .. } if key != "-g" && key != "-d" => {
                        *status = MenuStatus::Planned { roadmap_id: "R-5.3".to_string() };
                    }
                    _ => {}
                }
            }
        }
        menu.save_args();
    }
    menu
}

fn item(def: &ItemDef) -> Item {
    match def {
        ItemDef::Switch { key, flag, label } => Item::Switch {
            key: key.to_string(),
            flag: flag.to_string(),
            label: label.to_string(),
            on: false,
            status: MenuStatus::Implemented,
        },
        ItemDef::Option {
            key,
            flag,
            label,
            reader,
        } => Item::Option {
            key: key.to_string(),
            flag: flag.to_string(),
            label: label.to_string(),
            value: None,
            reader: *reader,
            status: MenuStatus::Implemented,
        },
        ItemDef::Action {
            key,
            command,
            label,
            roadmap,
            ..
        } => Item::Action {
            key: key.to_string(),
            command: command.to_string(),
            label: label.to_string(),
            status: if roadmap.is_empty() {
                MenuStatus::Implemented
            } else {
                MenuStatus::Planned {
                    roadmap_id: roadmap.to_string(),
                }
            },
        },
    }
}

/// Every menu definition, in dispatch order.
pub fn all() -> Vec<MenuDef> {
    vec![
        commit::definition(),
        branch::definition(),
        fetch::definition(),
        pull::definition(),
        push::definition(),
        merge::definition(),
        rebase::definition(),
        stash::definition(),
        tag::definition(),
        reset::definition(),
        cherry_pick::definition(),
        revert::definition(),
        bisect::definition(),
        remote::definition(),
        submodule::definition(),
        worktree::definition(),
        ignore::definition(),
        log::definition(),
        diff::definition(),
        patch::apply_definition(),
        patch::create_definition(),
        blame::definition(),
    ]
}

/// Look a menu up by its short name, generating the dispatch menu when asked.
pub fn by_name(name: &str, keymaps: &BTreeMap<String, Keymap>) -> Option<Menu> {
    if name == "dispatch" {
        return Some(dispatch::menu(keymaps));
    }
    named(name)
}

/// Build a static menu by name, without profile keymaps.
pub fn named(name: &str) -> Option<Menu> {
    all().iter().find(|def| def.id == name).map(build)
}

/// The preview command of a menu.
pub fn base_command(id: &str) -> Option<&'static str> {
    all()
        .iter()
        .find(|def| def.id == id)
        .map(|def| def.base_command)
}
