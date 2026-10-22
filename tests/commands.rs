//! The command registry (R-2.1): ids, docs, scopes and Planned semantics.

mod common;

use common::*;
use magit_lsp::commands::{registry, Command, CommandStatus, Scope};
use magit_lsp::protocol::{Effect, MessageLevel, ViewInvokeParams};
use magit_lsp::session::Session;
use magit_lsp::view::{Placement, ViewKind, Viewport};

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
];

const NAMESPACES: &[&str] = &["common", "status", "open", "select", "menu", "commit-message"];

fn open_status(session: &mut Session) {
    session
        .open(
            "v1",
            ViewKind::Status,
            Placement::Main,
            Viewport { cols: 80, rows: 24 },
        )
        .expect("open status view");
}

#[test]
fn the_registry_lists_every_named_command_once() {
    let mut seen = std::collections::HashSet::new();
    for command in registry().iter() {
        assert!(seen.insert(command.id), "duplicate command id {}", command.id);
        let namespace = command
            .id
            .split('.')
            .next()
            .expect("an id has a namespace");
        assert!(
            NAMESPACES.contains(&namespace),
            "{} has no known namespace",
            command.id
        );
        assert!(!command.doc.trim().is_empty(), "{} has no doc", command.id);
    }

    for id in EXPECTED {
        assert!(registry().get(id).is_some(), "missing command {id}");
    }
    assert_eq!(seen.len(), EXPECTED.len(), "unexpected extra commands");

    // `by_scope` partitions the registry.
    let by_scope: usize = [
        Scope::Common,
        Scope::Status,
        Scope::Select,
        Scope::Menu,
        Scope::CommitMessage,
    ]
    .iter()
    .map(|&scope| registry().by_scope(scope).count())
    .sum();
    assert_eq!(by_scope, EXPECTED.len());
    assert_eq!(registry().by_scope(Scope::Status).count(), 32);
}

#[test]
fn planned_commands_report_their_roadmap_and_change_nothing() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = Session::new(tmp.path().to_string_lossy());
    open_status(&mut session);

    let status_before = tmp.git(&["status", "--porcelain"]);
    let cached_before = cached_diff(tmp.path());
    assert!(cached_before.is_empty(), "nothing is staged to begin with");

    let planned: Vec<&Command> = registry()
        .iter()
        .filter(|command| matches!(command.status, CommandStatus::Planned { .. }))
        .collect();
    assert!(!planned.is_empty(), "there are planned commands to check");

    for command in planned {
        let before = session
            .views()
            .get("v1")
            .expect("view")
            .tree()
            .lines
            .clone();
        let outcome = session
            .invoke(&ViewInvokeParams {
                view_id: "v1".to_string(),
                command: command.id.to_string(),
                args: None,
                cursor: None,
                selection: None,
            })
            .expect("invoke a planned command");

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
        assert!(outcome.render.is_none(), "{} must not re-render", command.id);
        assert_eq!(
            session.views().get("v1").expect("view").tree().lines,
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
    let mut session = Session::new(tmp.path().to_string_lossy());
    open_status(&mut session);

    let error = session
        .invoke(&ViewInvokeParams {
            view_id: "v1".to_string(),
            command: "no.such-command".to_string(),
            args: None,
            cursor: None,
            selection: None,
        })
        .expect_err("unknown command is a JSON-RPC error");
    assert!(error.message.contains("unknown command"), "{error}");
}
