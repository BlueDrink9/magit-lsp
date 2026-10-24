//! Menu data (R-2.8): every Magit menu is complete, Planned actions change
//! nothing, the dispatch menu uses the active profile's keys and the large
//! menus reflow with greyed Planned labels.

mod common;

use common::*;
use expect_test::expect_file;
use magit_lsp::commands::{registry, CommandStatus};
use magit_lsp::config::Profile;
use magit_lsp::menu::{Item, Menu, MenuStatus};
use magit_lsp::view::{Placement, ViewKind, Viewport};

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
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();

    let mut checked = 0;
    for (name, menu) in menus() {
        let planned = planned_actions(&menu);
        if planned.is_empty() {
            continue;
        }
        let (descriptor, _render) = test.open_view(
            "m1",
            ViewKind::Menu(format!("menu.{name}")),
            Placement::Bottom,
            WIDE_VIEWPORT,
        );
        test.ack_view(&descriptor.id);

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
            let outcome = test.press(key);
            let expected = format!("Not implemented yet: {label} (roadmap {roadmap_id})");
            assert_eq!(
                message_texts(&outcome.effects),
                vec![expected.as_str()],
                "{name} {key}"
            );
            assert!(outcome.render.is_none(), "{name} {key} re-rendered");
            assert!(
                test.session().views().get(&descriptor.id).is_some(),
                "{name} {key} closed the menu"
            );
            checked += 1;
        }
        test.session_mut()
            .close(&descriptor.id)
            .expect("close menu");
    }

    assert!(checked > 100, "checked {checked} planned menu actions");
    assert_eq!(
        git_state(tmp.path()),
        before,
        "a planned action touched git"
    );
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AuditAction {
    menu: String,
    key: String,
    magit_command: String,
    local_command: String,
}

fn parse_audit(text: &str) -> Result<Vec<AuditAction>, String> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|(line_no, line)| {
            let fields: Vec<_> = line.split('\t').collect();
            if fields.len() != 4 || fields.iter().any(|field| field.is_empty()) {
                return Err(format!("line {}: expected four nonempty TSV fields", line_no + 1));
            }
            Ok(AuditAction {
                menu: fields[0].to_string(),
                key: fields[1].to_string(),
                magit_command: fields[2].to_string(),
                local_command: fields[3].to_string(),
            })
        })
        .collect()
}

fn validate_audit(expected: &[AuditAction], reference: &[AuditAction]) -> Result<(), String> {
    let mut expected_set = std::collections::HashSet::new();
    for action in expected {
        if !expected_set.insert(action) {
            return Err(format!("duplicate static action: {}/{}", action.menu, action.key));
        }
    }
    let mut reference_set = std::collections::HashSet::new();
    for row in reference {
        if !reference_set.insert(row) {
            return Err(format!("duplicate reference row: {}/{}", row.menu, row.key));
        }
    }
    for action in expected {
        if !reference_set.contains(action) {
            return Err(format!("missing reference row: {}/{} ({})", action.menu, action.key, action.local_command));
        }
    }
    for row in reference {
        if !expected_set.contains(row) {
            return Err(format!("orphan or mismatched reference row: {}/{} ({})", row.menu, row.key, row.local_command));
        }
    }
    Ok(())
}

fn definition_actions() -> Vec<AuditAction> {
    use magit_lsp::menus::ItemDef;
    magit_lsp::menus::all()
        .into_iter()
        .flat_map(|def| {
            let menu = def.id.to_string();
            def.groups.into_iter().flat_map(move |group| {
                let menu = menu.clone();
                group.items.into_iter().filter_map(move |item| match item {
                    ItemDef::Action { key, command, magit, .. } => Some(AuditAction {
                        menu: menu.clone(), key: key.to_string(),
                        magit_command: magit.to_string(), local_command: command.to_string(),
                    }),
                    _ => None,
                })
            })
        })
        .collect()
}

#[test]
fn static_menu_actions_match_the_independent_magit_key_audit_both_ways() {
    let reference = parse_audit(include_str!("data/magit-keys.txt")).expect("valid key audit TSV");
    let expected = definition_actions();
    validate_audit(&expected, &reference).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(expected.len(), 192, "all static action occurrences covered");
    let aliases = [
        ("rebase", "s", "in-progress rebase: Skip", "magit-rebase-skip"),
        ("rebase", "e", "in-progress rebase: Edit", "magit-rebase-edit"),
        ("bisect", "B", "active bisect: mark Bad", "magit-bisect-mark"),
    ];
    for (menu, key, state, command) in aliases {
        let action = reference.iter().find(|row| row.menu == menu && row.key == key)
            .unwrap_or_else(|| panic!("state alias {menu}/{key} has no static reference action"));
        assert!(!state.is_empty(), "state alias requires a reason");
        assert!(!command.is_empty(), "state alias requires Magit command identity");
        assert!(!action.magit_command.is_empty());
        assert!(!action.local_command.is_empty());
    }
}

#[test]
fn key_audit_validator_rejects_missing_orphan_and_duplicate_rows() {
    let action = AuditAction { menu: "remote".into(), key: "d u".into(), magit_command: "magit-update-default-branch".into(), local_command: "remote.update-default".into() };
    assert!(validate_audit(&[action.clone()], &[]).unwrap_err().contains("missing"));
    let orphan = AuditAction { key: "x".into(), ..action.clone() };
    assert!(validate_audit(&[], &[orphan]).unwrap_err().contains("orphan"));
    assert!(validate_audit(&[action.clone()], &[action.clone(), action]).unwrap_err().contains("duplicate"));
}

#[test]
fn the_dispatch_menu_lists_exactly_the_keys_the_profile_binds() {
    for profile in [Profile::Standard, Profile::Vim] {
        let (tmp, _repo) = RepoFixture::new()
            .commit("init", [("f.txt", TEN_LINES)])
            .build();
        let (mut test, _initial) = TestSession::open_status(&tmp, profile);
        test.ack();

        let (descriptor, _render) = test.open_via_command("open.dispatch");
        let menu = test.menu(&descriptor.id).expect("the dispatch model");
        let keymaps = test.keymaps();

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
            Profile::Standard => assert!(menu.item("z").is_some_and(
                |item| matches!(item, Item::Action { command, .. } if command == "open.stash")
            )),
            Profile::Vim => assert!(menu.item("Z").is_some_and(
                |item| matches!(item, Item::Action { command, .. } if command == "open.stash")
            )),
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
            assert!(
                bound,
                "{}: {command} key {key} is not bound",
                profile.as_str()
            );
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
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();

    for (command, menu) in openers {
        let registered = registry()
            .get(command)
            .unwrap_or_else(|| panic!("{command}"));
        assert_eq!(registered.status, CommandStatus::Implemented, "{command}");
        test.focus("v1");
        let (descriptor, _render) = test.open_via_command(command);
        assert_eq!(
            descriptor.kind,
            ViewKind::Menu(format!("menu.{menu}")),
            "{command}"
        );
        test.ack_view(&descriptor.id);
        test.session_mut().close(&descriptor.id).expect("close");
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
