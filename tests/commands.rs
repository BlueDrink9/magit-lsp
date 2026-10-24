//! The command registry (R-2.1): ids, docs, scopes and Planned semantics.

mod common;

use common::*;
use magit_lsp::commands::{registry, Command, CommandStatus, Scope};
use magit_lsp::config::Profile;
use magit_lsp::protocol::{Effect, MessageLevel};

/// Every command id named in TASKS.md §5.1–§5.5.
const EXPECTED: &[&str] = &[
    // §5.1 common scope
    "common.confirm",
    "common.cancel",
    "common.quit",
    "common.refresh",
    "common.toggle-section",
    "common.show-section",
    "common.hide-section",
    "common.cycle-visibility",
    "common.show-all",
    "common.hide-all",
    "common.more-detail",
    "common.less-detail",
    "common.detail-level-1",
    "common.detail-level-2",
    "common.detail-level-3",
    "common.detail-level-4",
    "common.next-section",
    "common.prev-section",
    "common.next-sibling",
    "common.prev-sibling",
    "common.parent-section",
    "common.toggle-text-mode",
    "common.copy-value",
    "common.visit",
    "open.dispatch",
    // §5.2 status scope
    "status.stage",
    "status.unstage",
    "status.stage-all",
    "status.unstage-all",
    "status.discard",
    "status.reverse",
    "status.apply",
    "open.commit",
    "open.branch",
    "open.log",
    "open.diff",
    "open.fetch",
    "open.pull",
    "open.push",
    "open.merge",
    "open.rebase",
    "open.stash",
    "open.tag",
    "open.reset",
    "open.revert",
    "open.cherry-pick",
    "open.bisect",
    "open.remote",
    "open.submodule",
    "open.worktree",
    "open.ignore",
    "open.refs",
    "open.cherries",
    "open.patch-apply",
    "open.patch-create",
    "open.run",
    "open.process-log",
    // §5.3 select mode
    "select.toggle-mark",
    "select.enter-rev",
    // §5.4 menu scopes
    "menu.save-args",
    "menu.reset-args",
    // §5.5 editable scopes
    "commit-message.history-prev",
    "commit-message.history-next",
    "commit-message.add-signoff",
    // §5.6 process log
    "process-log.clear",
];

const NAMESPACES: &[&str] = &[
    "common",
    "status",
    "open",
    "select",
    "menu",
    "commit-message",
    "process-log",
];

/// Namespaces used only by commands that a menu item binds (R-2.8).
const MENU_NAMESPACES: &[&str] = &[
    "commit",
    "branch",
    "fetch",
    "pull",
    "push",
    "merge",
    "rebase",
    "stash",
    "tag",
    "reset",
    "cherry-pick",
    "revert",
    "bisect",
    "remote",
    "submodule",
    "worktree",
    "ignore",
    "log",
    "diff",
    "patch",
    "blame",
];

#[test]
fn the_registry_lists_every_named_command_once() {
    let mut seen = std::collections::HashSet::new();
    let mut profile_commands = 0;
    for command in registry().iter() {
        assert!(
            seen.insert(command.id),
            "duplicate command id {}",
            command.id
        );
        let namespace = command.id.split('.').next().expect("an id has a namespace");
        assert!(
            NAMESPACES.contains(&namespace) || MENU_NAMESPACES.contains(&namespace),
            "{} has no known namespace",
            command.id
        );
        assert!(!command.doc.trim().is_empty(), "{} has no doc", command.id);
        if !MENU_NAMESPACES.contains(&namespace) {
            profile_commands += 1;
        }
    }

    for id in EXPECTED {
        assert!(registry().get(id).is_some(), "missing command {id}");
    }
    assert_eq!(
        profile_commands,
        EXPECTED.len(),
        "unexpected extra profile command"
    );

    // Menu actions are registered too, with the status their menu item shows.
    for def in magit_lsp::menus::all() {
        for group in &def.groups {
            for item in &group.items {
                let magit_lsp::menus::ItemDef::Action {
                    command, roadmap, ..
                } = item
                else {
                    continue;
                };
                if roadmap.is_empty() {
                    continue;
                }
                let registered = registry()
                    .get(command)
                    .unwrap_or_else(|| panic!("{command} is registered"));
                assert_eq!(registered.scope, Scope::MenuAction, "{command}");
                assert_eq!(
                    registered.status,
                    CommandStatus::Planned {
                        roadmap_id: roadmap
                    },
                    "{command}"
                );
            }
        }
    }

    // `by_scope` partitions the registry.
    let by_scope: usize = [
        Scope::Common,
        Scope::Status,
        Scope::Select,
        Scope::Menu,
        Scope::CommitMessage,
        Scope::ProcessLog,
        Scope::MenuAction,
    ]
    .iter()
    .map(|&scope| registry().by_scope(scope).count())
    .sum();
    assert_eq!(by_scope, seen.len());
    assert_eq!(registry().by_scope(Scope::Status).count(), 32);
}

#[test]
fn planned_commands_report_their_roadmap_and_change_nothing() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();

    let status_before = tmp.git(&["status", "--porcelain"]);
    let cached_before = cached_diff(tmp.path());
    assert!(cached_before.is_empty(), "nothing is staged to begin with");

    let planned: Vec<&Command> = registry()
        .iter()
        .filter(|command| matches!(command.status, CommandStatus::Planned { .. }))
        .collect();
    assert!(!planned.is_empty(), "there are planned commands to check");

    for command in planned {
        let before = test.text();
        let outcome = test.invoke(command.id);

        let CommandStatus::Planned { roadmap_id } = command.status else {
            unreachable!("filtered to planned");
        };
        let expected = format!(
            "Not implemented yet: {} (roadmap {})",
            command.doc, roadmap_id
        );
        assert_eq!(
            outcome.effects,
            vec![Effect::message(MessageLevel::Info, expected)],
            "{} produced the wrong effects",
            command.id
        );
        assert!(
            outcome.render.is_none(),
            "{} must not re-render",
            command.id
        );
        assert_eq!(
            test.text(),
            before,
            "{} changed the rendered text",
            command.id
        );
    }

    assert_eq!(tmp.git(&["status", "--porcelain"]), status_before);
    assert_eq!(cached_diff(tmp.path()), cached_before);
}

#[test]
fn an_unknown_command_is_a_protocol_error() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();

    let error = test
        .try_invoke("no.such-command")
        .expect_err("unknown command is a JSON-RPC error");
    assert!(error.message.contains("unknown command"), "{error}");
}
