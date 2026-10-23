//! Menu data (R-2.8): every Magit menu is complete, Planned actions change
//! nothing, the dispatch menu uses the active profile's keys and the large
//! menus reflow with greyed Planned labels.

mod common;

use common::*;
use expect_test::expect_file;
use magit_lsp::commands::{registry, CommandStatus};
use magit_lsp::config::{Config, Profile};
use magit_lsp::keymap::{self, Keymap};
use magit_lsp::menu::{Item, Menu, MenuStatus};
use magit_lsp::protocol::{Cursor, Effect, ViewInvokeParams, ViewKeyParams};
use magit_lsp::session::Session;
use magit_lsp::view::{Placement, ViewKind, Viewport};
use std::collections::BTreeMap;
use std::sync::Arc;

fn keymaps(profile: Profile) -> Arc<BTreeMap<String, Keymap>> {
    let config = Config {
        profile,
        ..Default::default()
    };
    Arc::new(keymap::validate(&config).expect("resolve keymaps"))
}

fn session(repo: &std::path::Path, profile: Profile) -> Session {
    let mut session = Session::new(repo.to_string_lossy());
    session.set_keymaps(keymaps(profile));
    session
}

fn open_status(session: &mut Session) {
    let (_descriptor, render) = session
        .open(
            "v1",
            ViewKind::Status,
            Placement::Main,
            Viewport { cols: 100, rows: 30 },
        )
        .expect("open status view");
    session.ack("v1", render.version).expect("ack");
}

fn press(session: &mut Session, view: &str, key: &str) -> magit_lsp::session::InvokeResult {
    let mut result = None;
    for step in key.split(' ') {
        let outcome = session
            .key(&ViewKeyParams {
                view_id: view.to_string(),
                version: 1,
                key: step.to_string(),
                editor_mode: magit_lsp::keymap::EditorMode::Normal,
                cursor: Cursor { line: 0, column: 0 },
                selection: None,
            })
            .expect("view/key");
        result = Some(outcome);
    }
    result.expect("at least one step")
}

fn messages(effects: &[Effect]) -> Vec<&str> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Message { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn actions(menu: &Menu) -> Vec<&Item> {
    menu.groups
        .iter()
        .flat_map(|group| &group.items)
        .filter(|item| item.is_action())
        .collect()
}

fn planned_actions(menu: &Menu) -> Vec<&Item> {
    actions(menu)
        .into_iter()
        .filter(|item| matches!(item.status(), MenuStatus::Planned { .. }))
        .collect()
}

fn git_state(repo: &std::path::Path) -> (String, String, String) {
    (
        git(repo, &["status", "--porcelain"]),
        git(repo, &["diff", "--cached"]),
        git(repo, &["show-ref"]),
    )
}

/// Every menu, including the generated dispatch menu.
fn menus() -> Vec<(String, Menu)> {
    let mut menus: Vec<(String, Menu)> = magit_lsp::menus::all()
        .iter()
        .map(|def| (def.id.to_string(), magit_lsp::menus::build(def)))
        .collect();
    menus.push((
        "dispatch".to_string(),
        magit_lsp::menus::by_name("dispatch", &keymaps(Profile::Vim)).expect("dispatch"),
    ));
    menus
}

#[test]
fn every_planned_menu_action_reports_its_roadmap_and_changes_nothing() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let before = git_state(tmp.path());
    let mut session = session(tmp.path(), Profile::Vim);
    open_status(&mut session);

    let mut checked = 0;
    for (name, menu) in menus() {
        let planned = planned_actions(&menu);
        if planned.is_empty() {
            continue;
        }
        let (descriptor, render) = session
            .open(
                "m1",
                ViewKind::Menu(format!("menu.{name}")),
                Placement::Bottom,
                Viewport { cols: 100, rows: 30 },
            )
            .expect("open menu");
        session.ack(&descriptor.id, render.version).expect("ack");

        for item in planned {
            let Item::Action {
                key,
                label,
                status: MenuStatus::Planned { roadmap_id },
                ..
            } = item
            else {
                unreachable!("filtered to planned actions");
            };
            let outcome = press(&mut session, &descriptor.id, key);
            let expected = format!("Not implemented yet: {label} (roadmap {roadmap_id})");
            assert_eq!(
                messages(&outcome.effects),
                vec![expected.as_str()],
                "{name} {key}"
            );
            assert!(outcome.render.is_none(), "{name} {key} re-rendered");
            assert!(
                session.views().get(&descriptor.id).is_some(),
                "{name} {key} closed the menu"
            );
            checked += 1;
        }
        session.close(&descriptor.id).expect("close menu");
    }

    assert!(checked > 100, "checked {checked} planned menu actions");
    assert_eq!(git_state(tmp.path()), before, "a planned action touched git");
}

/// A hand-maintained list of Magit's well-known key → menu pairs. The key is
/// the part after the menu's prefix (`c c` is listed as `("commit", "c")`).
/// Keys that contradict the Magit manual are corrected here: reset is `X`
/// (not `x`), rebase's in-progress skip/edit reuse `s`/`e` and are therefore
/// not separate items, and bisect's in-progress Bad reuses `B`.
const PARITY_KEYS: &[(&str, &str)] = &[
    ("commit", "c"),
    ("commit", "a"),
    ("commit", "e"),
    ("commit", "w"),
    ("commit", "f"),
    ("commit", "s"),
    ("branch", "b"),
    ("branch", "c"),
    ("branch", "n"),
    ("branch", "s"),
    ("branch", "m"),
    ("branch", "x"),
    ("branch", "k"),
    ("branch", "d"),
    ("fetch", "p"),
    ("fetch", "u"),
    ("fetch", "e"),
    ("fetch", "a"),
    ("pull", "p"),
    ("pull", "u"),
    ("pull", "e"),
    ("push", "p"),
    ("push", "u"),
    ("push", "e"),
    ("push", "t"),
    ("push", "T"),
    ("merge", "m"),
    ("merge", "e"),
    ("merge", "n"),
    ("merge", "a"),
    ("merge", "p"),
    ("merge", "s"),
    ("merge", "d"),
    ("rebase", "p"),
    ("rebase", "u"),
    ("rebase", "e"),
    ("rebase", "i"),
    ("rebase", "s"),
    ("rebase", "m"),
    ("rebase", "f"),
    ("rebase", "w"),
    ("rebase", "k"),
    ("rebase", "r"),
    ("rebase", "a"),
    ("stash", "z"),
    ("stash", "i"),
    ("stash", "w"),
    ("stash", "x"),
    ("stash", "a"),
    ("stash", "p"),
    ("stash", "k"),
    ("stash", "b"),
    ("tag", "t"),
    ("tag", "r"),
    ("tag", "k"),
    ("tag", "p"),
    ("reset", "m"),
    ("reset", "s"),
    ("reset", "h"),
    ("reset", "k"),
    ("reset", "i"),
    ("reset", "w"),
    ("reset", "f"),
    ("cherry-pick", "A"),
    ("cherry-pick", "a"),
    ("cherry-pick", "h"),
    ("cherry-pick", "s"),
    ("cherry-pick", "d"),
    ("revert", "V"),
    ("revert", "v"),
    ("bisect", "B"),
    ("bisect", "s"),
    ("bisect", "g"),
    ("bisect", "r"),
    ("bisect", "k"),
    ("bisect", "m"),
    ("log", "l"),
    ("log", "o"),
    ("log", "h"),
    ("log", "u"),
    ("log", "a"),
    ("log", "L"),
    ("log", "b"),
    ("diff", "d"),
    ("diff", "r"),
    ("diff", "p"),
    ("diff", "u"),
    ("diff", "s"),
    ("diff", "c"),
    ("diff", "t"),
    ("diff", "w"),
    ("ignore", "t"),
    ("ignore", "g"),
    ("ignore", "p"),
    ("remote", "a"),
    ("remote", "r"),
    ("remote", "k"),
    ("remote", "p"),
    ("remote", "P"),
    ("worktree", "b"),
    ("worktree", "c"),
    ("worktree", "m"),
    ("worktree", "k"),
    ("worktree", "g"),
    ("submodule", "a"),
    ("submodule", "r"),
    ("submodule", "p"),
    ("submodule", "u"),
    ("submodule", "f"),
    ("submodule", "d"),
    ("submodule", "s"),
    ("submodule", "l"),
    ("patch-apply", "a"),
    ("patch-apply", "m"),
    ("patch-create", "c"),
    ("patch-create", "s"),
    ("patch-create", "r"),
];

#[test]
fn the_hand_maintained_parity_keys_exist_in_their_menus() {
    for (menu_name, key) in PARITY_KEYS {
        let menu = magit_lsp::menus::named(menu_name)
            .unwrap_or_else(|| panic!("menu {menu_name} exists"));
        assert!(
            menu.item(key).is_some(),
            "{menu_name} has no item for key `{key}`"
        );
    }
}

#[test]
fn the_dispatch_menu_lists_exactly_the_keys_the_profile_binds() {
    for profile in [Profile::Standard, Profile::Vim] {
        let keymaps = keymaps(profile);
        let (tmp, _repo) = RepoFixture::new()
            .commit("init", [("f.txt", TEN_LINES)])
            .build();
        let mut session = Session::new(tmp.path().to_string_lossy());
        session.set_keymaps(Arc::clone(&keymaps));
        open_status(&mut session);

        let outcome = session
            .invoke(&ViewInvokeParams {
                view_id: "v1".to_string(),
                command: "open.dispatch".to_string(),
                args: None,
                cursor: None,
                selection: None,
            })
            .expect("open.dispatch");
        let descriptor = outcome
            .effects
            .iter()
            .find_map(|effect| match effect {
                Effect::PushView { descriptor } => Some(descriptor.clone()),
                _ => None,
            })
            .expect("a pushed dispatch menu");
        let menu = session
            .views()
            .get(&descriptor.id)
            .and_then(|view| view.menu())
            .expect("the dispatch model");

        let status = keymaps.get("status").expect("status keymap");
        let mut expected = 0;
        for binding in status.bindings() {
            if !binding.command.starts_with("open.") || binding.keys.is_empty() {
                continue;
            }
            if binding.command == "open.dispatch" {
                continue;
            }
            let key = &binding.keys[0];
            let item = menu
                .item(key)
                .unwrap_or_else(|| panic!("{}: no dispatch item for {key}", profile.as_str()));
            match item {
                Item::Action { command, .. } => assert_eq!(
                    command,
                    &binding.command,
                    "{}: {key} points at the wrong command",
                    profile.as_str()
                ),
                other => panic!("{key} is not an action: {other:?}"),
            }
            expected += 1;
        }
        assert!(expected > 20, "{}: {expected} openers", profile.as_str());

        // The active profile's keys are used, not Magit's defaults.
        match profile {
            Profile::Standard => assert!(
                menu.item("z")
                    .is_some_and(|item| matches!(item, Item::Action { command, .. } if command == "open.stash"))
            ),
            Profile::Vim => assert!(
                menu.item("Z")
                    .is_some_and(|item| matches!(item, Item::Action { command, .. } if command == "open.stash"))
            ),
        }
        assert!(menu.item("g ?").is_none(), "dispatch does not list itself");

        // Every listed key is really bound by the profile in that scope.
        for item in actions(&menu) {
            let Item::Action { key, command, .. } = item else {
                continue;
            };
            let scope = if command.starts_with("common.") {
                "common"
            } else {
                "status"
            };
            let bound = keymaps
                .get(scope)
                .expect("scope keymap")
                .bindings()
                .iter()
                .any(|binding| binding.command == *command && binding.keys.contains(key));
            assert!(bound, "{}: {command} key {key} is not bound", profile.as_str());
        }
    }
}

#[test]
fn every_menu_opener_is_implemented_and_pushes_its_menu() {
    let openers = [
        ("open.commit", "commit"),
        ("open.branch", "branch"),
        ("open.fetch", "fetch"),
        ("open.pull", "pull"),
        ("open.push", "push"),
        ("open.merge", "merge"),
        ("open.rebase", "rebase"),
        ("open.stash", "stash"),
        ("open.tag", "tag"),
        ("open.reset", "reset"),
        ("open.revert", "revert"),
        ("open.cherry-pick", "cherry-pick"),
        ("open.bisect", "bisect"),
        ("open.remote", "remote"),
        ("open.submodule", "submodule"),
        ("open.worktree", "worktree"),
        ("open.ignore", "ignore"),
        ("open.log", "log"),
        ("open.diff", "diff"),
        ("open.patch-apply", "patch-apply"),
        ("open.patch-create", "patch-create"),
    ];

    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let mut session = session(tmp.path(), Profile::Vim);
    open_status(&mut session);

    for (command, menu) in openers {
        let registered = registry().get(command).unwrap_or_else(|| panic!("{command}"));
        assert_eq!(registered.status, CommandStatus::Implemented, "{command}");
        let outcome = session
            .invoke(&ViewInvokeParams {
                view_id: "v1".to_string(),
                command: command.to_string(),
                args: None,
                cursor: None,
                selection: None,
            })
            .expect("open menu");
        let descriptor = outcome
            .effects
            .iter()
            .find_map(|effect| match effect {
                Effect::PushView { descriptor } => Some(descriptor.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{command} pushed no view"));
        assert_eq!(
            descriptor.kind,
            ViewKind::Menu(format!("menu.{menu}")),
            "{command}"
        );
        let (render, _keymap) = outcome.opened.into_iter().next().expect("initial render");
        session.ack(&descriptor.id, render.version).expect("ack");
        session.close(&descriptor.id).expect("close");
    }
}

#[test]
fn menu_item_keys_are_unique_and_every_menu_reaches_dispatch() {
    for def in magit_lsp::menus::all() {
        let menu = magit_lsp::menus::build(&def);
        let mut seen = std::collections::HashSet::new();
        for group in &menu.groups {
            for item in &group.items {
                assert!(
                    seen.insert(item.key().to_string()),
                    "{} has duplicate key {}",
                    def.id,
                    item.key()
                );
            }
        }
        assert!(
            menu.item("?").is_some(),
            "{} has no dispatch toggle",
            def.id
        );
    }
}

#[test]
fn every_menu_renders_all_of_its_action_keys() {
    let mut all: Vec<(String, Menu)> = magit_lsp::menus::all()
        .iter()
        .map(|def| (def.id.to_string(), magit_lsp::menus::build(def)))
        .collect();
    for (name, profile) in [
        ("dispatch (vim)", Profile::Vim),
        ("dispatch (standard)", Profile::Standard),
    ] {
        all.push((
            name.to_string(),
            magit_lsp::menus::by_name("dispatch", &keymaps(profile)).expect("dispatch"),
        ));
    }

    let mut missing = Vec::new();
    for (name, menu) in all {
        for cols in [80u32, 120] {
            let tree = menu.render(Viewport { cols, rows: 30 });
            for group in &menu.groups {
                for item in &group.items {
                    if !item.is_action() {
                        continue;
                    }
                    let needle = format!("  {}  {}", item.key(), item.label());
                    if !tree.lines.iter().any(|line| line.contains(&needle)) {
                        missing.push(format!(
                            "{name} at {cols}: `{}` ({})",
                            item.key(),
                            item.label()
                        ));
                    }
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "action keys missing from the rendered menus:\n{}",
        missing.join("\n")
    );
}

#[test]
fn the_largest_menus_reflow_and_grey_every_planned_action() {
    for (name, cols) in [
        ("rebase", 60u32),
        ("rebase", 100),
        ("rebase", 140),
        ("push", 60),
        ("push", 100),
        ("push", 140),
        ("diff", 60),
        ("diff", 100),
        ("diff", 140),
    ] {
        let menu = magit_lsp::menus::named(name).expect("menu");
        let tree = menu.render(Viewport { cols, rows: 40 });
        let text = tree.lines.join("\n");
        expect_file![format!("snapshots/menus-{name}-{cols}.txt")].assert_eq(&text);

        let planned = planned_actions(&menu).len();
        let greyed = tree
            .tokens
            .iter()
            .filter(|token| token.token_type == "magit.menu.planned")
            .count();
        assert_eq!(
            greyed, planned,
            "{name} at {cols}: every planned action is greyed"
        );
    }
}
