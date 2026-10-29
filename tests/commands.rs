//! The command registry (R-2.1): ids, docs, scopes and Planned semantics.

mod common;

use common::*;
use magit_lsp::commands::{registry, Command, CommandStatus, Scope};
use magit_lsp::config::Profile;
use magit_lsp::protocol::{Cursor, Effect, MessageLevel};

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
    "common.jump",
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

#[test]
fn commit_arguments_are_typed_and_invalid_input_never_stages() {
    use serde_json::json;
    use magit_lsp::git::CommitOperation;
    let (tmp, _) = RepoFixture::new().commit("init", [("f.txt", TEN_LINES)]).build();
    std::fs::write(tmp.path().join("f.txt"), "changed\n").unwrap();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    let before = tmp.git(&["status", "--porcelain"]);
    for args in [
        json!({"switches": [], "options": {}, "raw": "--amend"}),
        json!({"switches": ["--amend"], "options": {}}),
        json!({"switches": ["-a", "-a"], "options": {}}),
        json!({"switches": ["-a"]}),
        json!({"switches": [], "options": {"=A": ""}}),
        json!({"switches": [], "options": {"=D": "not a date"}}),
        json!({"switches": [], "options": {"=A": "bad author"}}),
        json!({"switches": [], "options": {"=S": null}}),
        json!({"switches": [], "options": {"=X": "unsupported"}}),
        json!({"switches": [], "options": {"=C": "HEAD", "=c": "HEAD"}}),
        json!({"switches": ["-R"], "options": {}}),
        json!({"switches": ["-R"], "options": {"=A": "A <a@b>"}}),
        json!({"switches": ["-R"], "options": {"=C": "HEAD", "=A": "A <a@b>"}}),
        json!({"switches": [], "options": {"=C": "--all"}}),
    ] {
        let result = test.invoke_args("commit.create", args.clone());
        assert!(result.commit.is_none(), "{args}");
        assert!(message_texts(&result.effects).iter().any(|text| text.contains("Invalid commit arguments")), "{args}: {:?}", result.effects);
        assert_eq!(tmp.git(&["status", "--porcelain"]), before);
    }
    let result = test.invoke_args("commit.create", json!({
        "switches": ["-a", "-e", "-v", "-n", "-s", "-R"],
        "options": {"=D": "2001-02-03T04:05:06+00:00", "=S": "", "=c": "HEAD"}
    }));
    let CommitOperation::Create(options) = result.commit.expect("typed operation") else { panic!("expected create operation") };
    assert!(options.all && options.allow_empty && options.verbose && options.no_verify && options.signoff && options.reset_author);
    assert_eq!(options.signing_key.as_deref(), Some(""));
    assert_eq!(options.reedit_message.as_deref(), Some(tmp.git(&["rev-parse", "HEAD"]).trim()));
    assert_eq!(options.argv(), vec![
        "--all".to_string(), "--allow-empty".into(), "--verbose".into(), "--no-verify".into(),
        "--reset-author".into(), "--signoff".into(), "--date=2001-02-03T04:05:06+00:00".into(),
        "--gpg-sign=".into(), format!("--reedit-message={}", tmp.git(&["rev-parse", "HEAD"]).trim()),
    ]);
    assert_eq!(tmp.git(&["status", "--porcelain"]), before, "dispatch itself does not stage");
}

#[test]
fn captured_reuse_and_reedit_options_have_real_git_effects() {
    use serde_json::json;
    for (key, reset) in [("=C", false), ("=c", false), ("=C", true), ("=c", true)] {
        let (tmp, _) = RepoFixture::new().commit("original message", [("f.txt", TEN_LINES)]).build();
        let original_tree = tmp.git(&["rev-parse", "HEAD^{tree}"]);
        let original_author = tmp.git(&["log", "-1", "--format=%an <%ae>"]);
        let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
        let operation = test.invoke_args("commit.create", json!({
            "switches": if reset { vec!["-e", "-R"] } else { vec!["-e"] }, "options": {(key): "HEAD"}
        })).commit.expect("allow-empty bypasses clean guard");
        let argv = operation.argv();
        let repo = magit_lsp::git::GitRepo::open(tmp.path()).unwrap();
        repo.commit(&argv.iter().map(String::as_str).collect::<Vec<_>>(), &[
            ("GIT_EDITOR", "true"), ("GIT_AUTHOR_NAME", "New Committer"),
            ("GIT_AUTHOR_EMAIL", "new@example.com"),
        ]).unwrap();
        assert_eq!(tmp.git(&["rev-list", "--count", "HEAD"]).trim(), "2");
        assert_eq!(tmp.git(&["log", "-1", "--format=%s"]).trim(), "original message");
        assert_eq!(tmp.git(&["rev-parse", "HEAD^{tree}"]), original_tree);
        let expected = if reset { "New Committer <new@example.com>" } else { original_author.trim() };
        assert_eq!(tmp.git(&["log", "-1", "--format=%an <%ae>"]).trim(), expected);
    }
}

#[test]
fn failed_and_noop_staging_do_not_advance_to_a_sibling() {
    for failure in [true, false] {
        let (tmp, _) = RepoFixture::new()
            .commit("init", [("f.txt", lines(40).to_string())])
            .modify("f.txt", two_hunk_changes()).build();
        let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
        test.ack();
        let line = if failure {
            write(&tmp.path().join(".git/index.lock"), "locked\n");
            test.goto_marker("f.txt", '+')
        } else {
            test.goto_section("staged")
        };
        let result = test.invoke_here("status.stage");
        assert!(!message_texts(&result.effects).is_empty());
        assert_eq!(result.render.unwrap().cursor.unwrap().line, line);
        if failure { std::fs::remove_file(tmp.path().join(".git/index.lock")).unwrap(); }
        assert!(tmp.git(&["diff", "--cached"]).is_empty());
    }
}

#[test]
fn staging_a_staged_hunk_is_a_quiet_exact_noop() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", lines(40).to_string())])
        .modify("f.txt", two_hunk_changes()).build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    test.goto_marker("f.txt", '+');
    test.invoke_here("status.stage");
    test.ack();
    let staged_line = test.tree().line_nodes.iter().position(|node| node.kind == magit_lsp::tree::NodeKind::Line && node.section.as_deref() == Some("staged") && node.marker == Some('+')).unwrap();
    let head_before = tmp.git(&["rev-parse", "HEAD"]);
    let index_before = tmp.git(&["hash-object", ".git/index"]);
    let worktree_before = std::fs::read(tmp.path().join("f.txt")).unwrap();
    for line in [staged_line, staged_line + 1] {
        test.goto(line);
        let result = test.invoke_here("status.stage");
        assert!(message_texts(&result.effects).is_empty());
        assert_eq!(result.render.unwrap().cursor.unwrap().line, line);
        assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head_before);
        assert_eq!(tmp.git(&["hash-object", ".git/index"]), index_before);
        assert_eq!(std::fs::read(tmp.path().join("f.txt")).unwrap(), worktree_before);
    }
}

#[test]
fn common_hierarchy_commands_are_implemented() {
    for id in ["common.cycle-visibility", "common.next-sibling", "common.prev-sibling", "common.parent-section"] {
        assert_eq!(registry().get(id).unwrap().status, CommandStatus::Implemented);
    }
}

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
        Scope::Log,
        Scope::LogSelect,
        Scope::Menu,
        Scope::CommitMessage,
        Scope::ProcessLog,
        Scope::MenuAction,
    ]
    .iter()
    .map(|&scope| registry().by_scope(scope).count())
    .sum();
    assert_eq!(by_scope, seen.len());

    for id in ["log.load-more"] {
        let command = registry().get(id).expect("registered");
        assert_eq!(command.scope, Scope::Log, "{id}");
        assert_eq!(command.status, CommandStatus::Implemented, "{id}");
    }
    for id in ["log.current", "log.select-and-copy"] {
        let command = registry().get(id).expect("registered");
        assert_eq!(command.scope, Scope::MenuAction, "{id}");
        assert_eq!(command.status, CommandStatus::Implemented, "{id}");
    }
    for id in ["commit-message.history-prev", "commit-message.history-next"] {
        let command = registry().get(id).expect("registered");
        assert_eq!(command.scope, Scope::CommitMessage);
        assert_eq!(command.status, CommandStatus::Planned { roadmap_id: "R-EDIT-HISTORY" });
    }

    // The status scope (§5.2) consists of the `status.*` actions and every
    // `open.*` opener except the dispatch menu (§5.1). Assert the exact
    // partition instead of a hardcoded count.
    let status_ids: Vec<&str> = EXPECTED
        .iter()
        .copied()
        .filter(|id| {
            id.starts_with("status.") || (id.starts_with("open.") && *id != "open.dispatch")
        })
        .collect();
    for id in &status_ids {
        assert_eq!(
            registry().get(id).expect("registered").scope,
            Scope::Status,
            "{id} is in the status scope"
        );
    }
    assert_eq!(
        registry().by_scope(Scope::Status).count(),
        status_ids.len(),
        "the status scope contains exactly the §5.2 set"
    );
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

/// `common.visit` resolves through the server tree: the acked line names both
/// the worktree file and its new-side line, and the client only receives the
/// resulting location.
#[test]
fn visit_reports_the_server_resolved_worktree_location() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();

    let line = test.goto_text("f.txt", "line3 CHANGED");
    let outcome = test.invoke_at("common.visit", Cursor { line, column: 0 });

    let uri = format!("file://{}/f.txt", tmp.path().display());
    assert_eq!(
        outcome.effects,
        vec![Effect::VisitFile {
            uri,
            line: 2,
            column: 0,
        }]
    );
}

/// A section header has no file location, so visiting it must explain instead
/// of opening an arbitrary file.
#[test]
fn visiting_a_line_without_a_location_reports_nothing_to_visit() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();

    let line = test.goto_section("unstaged");
    let outcome = test.invoke_at("common.visit", Cursor { line, column: 0 });

    assert_eq!(message_texts(&outcome.effects), vec!["Nothing to visit here"]);
}

/// Transferred from the deleted Lua `unstaging an untracked file reports
/// nothing to unstage` case: an untracked file has no index entry to remove,
/// so the invoke must notify instead of resolving an `UnstageFile` action.
#[test]
fn unstaging_an_untracked_file_reports_nothing_to_unstage() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    write(&tmp.path().join("new.txt"), "new\n");
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();

    let line = test.goto_file("untracked", "new.txt");
    let outcome = test.invoke_at("status.unstage", Cursor { line, column: 0 });

    assert_eq!(
        message_texts(&outcome.effects),
        vec![magit_lsp::dispatch::NOTHING_TO_UNSTAGE]
    );
    assert_eq!(tmp.git(&["status", "--porcelain"]), "?? new.txt\n");
}

#[test]
fn stale_displayed_selection_does_not_stage_changed_hunk() {
    use magit_lsp::protocol::Selection;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", lines(40).to_string())])
        .modify("f.txt", two_hunk_changes())
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let version = test.version();
    let selected = test.tree().nodes.iter().find(|node| node.kind == magit_lsp::tree::NodeKind::Line
        && node.path.as_deref() == Some("f.txt") && node.marker == Some('+')).unwrap().start_line;
    let before_head = tmp.git(&["rev-parse", "HEAD"]);
    let before_index = tmp.git(&["hash-object", ".git/index"]);
    let before_worktree = std::fs::read(tmp.path().join("f.txt")).unwrap();
    std::fs::write(tmp.path().join("f.txt"), lines(40).with(5, "new concurrent change").to_string()).unwrap();
    let after_worktree = std::fs::read(tmp.path().join("f.txt")).unwrap();

    let result = test.invoke_at_version("status.stage", version, None,
        Some(Selection { start_line: selected, end_line: selected }), None);
    assert!(message_texts(&result.effects).iter().any(|text| text.contains("changed")));
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), before_head);
    assert_eq!(tmp.git(&["hash-object", ".git/index"]), before_index);
    assert_eq!(std::fs::read(tmp.path().join("f.txt")).unwrap(), after_worktree);
    assert_ne!(before_worktree, after_worktree);
    assert!(tmp.git(&["diff", "--cached"]).is_empty());
}

#[test]
fn file_row_does_not_stage_changes_added_after_its_render() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", lines(40).to_string())])
        .modify("f.txt", two_hunk_changes())
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let version = test.version();
    let file_line = test.tree().nodes.iter().find(|node| node.kind == magit_lsp::tree::NodeKind::File
        && node.path.as_deref() == Some("f.txt") && node.section.as_deref() == Some("unstaged")).unwrap().start_line;
    let before_head = tmp.git(&["rev-parse", "HEAD"]);
    let before_index = tmp.git(&["hash-object", ".git/index"]);
    std::fs::write(tmp.path().join("f.txt"), lines(40).with(5, "line5 CHANGED").with(20, "new hidden change").to_string()).unwrap();

    let result = test.invoke_at_version("status.stage", version, Some(Cursor { line: file_line, column: 0 }), None, None);
    assert!(message_texts(&result.effects).iter().any(|text| text.contains("changed")));
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), before_head);
    assert_eq!(tmp.git(&["hash-object", ".git/index"]), before_index);
    assert!(tmp.git(&["diff", "--cached"]).is_empty());
    assert!(tmp.git(&["diff"]).contains("new hidden change"));
}

#[test]
fn mode_only_change_after_render_is_rejected_without_staging() {
    use std::os::unix::fs::PermissionsExt;
    let (tmp, _) = RepoFixture::new().commit("init", [("f.txt", "bytes\n")]).build();
    let path = tmp.path().join("f.txt");
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).unwrap();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let version = test.version();
    let line = test.goto_file("unstaged", "f.txt");
    let before = tmp.git(&["hash-object", ".git/index"]);
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o644);
    std::fs::set_permissions(&path, permissions).unwrap();
    let outcome = test.invoke_at_version("status.stage", version, Some(Cursor { line, column: 0 }), None, None);
    assert!(message_texts(&outcome.effects).iter().any(|text| text.contains("changed")));
    assert_eq!(tmp.git(&["hash-object", ".git/index"]), before);
    assert!(tmp.git(&["diff", "--cached"]).is_empty());
}

#[test]
fn binary_content_change_after_render_is_rejected_without_staging() {
    let (tmp, _) = RepoFixture::new().commit("init", [("f.bin", "old\0binary")]).build();
    write(&tmp.path().join("f.bin"), "shown\0binary");
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let version = test.version();
    let line = test.goto_file("unstaged", "f.bin");
    let before = tmp.git(&["hash-object", ".git/index"]);
    write(&tmp.path().join("f.bin"), "new\0binary");
    let outcome = test.invoke_at_version("status.stage", version, Some(Cursor { line, column: 0 }), None, None);
    assert!(message_texts(&outcome.effects).iter().any(|text| text.contains("changed")));
    assert_eq!(tmp.git(&["hash-object", ".git/index"]), before);
    assert!(tmp.git(&["diff", "--cached"]).is_empty());
}

#[test]
fn unchanged_binary_file_row_stages_successfully() {
    let (tmp, _) = RepoFixture::new().commit("init", [("f.bin", "old\0binary")]).build();
    write(&tmp.path().join("f.bin"), "new\0binary");
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let line = test.goto_file("unstaged", "f.bin");
    let outcome = test.invoke_at("status.stage", Cursor { line, column: 0 });
    assert!(message_texts(&outcome.effects).is_empty());
    assert_eq!(tmp.git(&["show", ":f.bin"]), "new\0binary");
    assert!(tmp.git(&["diff"]).is_empty());
}

#[test]
fn unchanged_type_change_file_row_stages_successfully() {
    use std::os::unix::fs::symlink;
    let (tmp, _) = RepoFixture::new().commit("init", [("f.txt", "regular\n")]).build();
    std::fs::remove_file(tmp.path().join("f.txt")).unwrap();
    symlink("missing-target", tmp.path().join("f.txt")).unwrap();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let line = test.goto_file("unstaged", "f.txt");
    let outcome = test.invoke_at("status.stage", Cursor { line, column: 0 });
    assert!(message_texts(&outcome.effects).is_empty());
    assert_eq!(tmp.git(&["ls-files", "--stage", "f.txt"]).split_whitespace().next(), Some("120000"));
    assert!(tmp.git(&["diff"]).is_empty());
}

#[test]
fn collapsed_file_row_stages_the_complete_unchanged_file() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", lines(40).to_string())])
        .modify("f.txt", two_hunk_changes())
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let file_line = test.tree().nodes.iter().find(|node| node.kind == magit_lsp::tree::NodeKind::File
        && node.path.as_deref() == Some("f.txt") && node.section.as_deref() == Some("unstaged")).unwrap().start_line;
    test.invoke_at("common.hide-section", Cursor { line: file_line, column: 0 });
    let version = test.version();
    let file = test.tree().nodes.iter().find(|node| node.kind == magit_lsp::tree::NodeKind::File
        && node.path.as_deref() == Some("f.txt") && node.section.as_deref() == Some("unstaged")).unwrap();
    assert!(file.children.is_empty());
    let file_line = file.start_line;
    let before_head = tmp.git(&["rev-parse", "HEAD"]);
    let before_index = tmp.git(&["hash-object", ".git/index"]);

    let result = test.invoke_at_version("status.stage", version, Some(Cursor { line: file_line, column: 0 }), None, None);
    assert!(message_texts(&result.effects).is_empty());
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), before_head);
    assert_ne!(tmp.git(&["hash-object", ".git/index"]), before_index);
    assert!(tmp.git(&["diff", "--cached"]).contains("line5 CHANGED"));
    assert!(tmp.git(&["diff"]).is_empty());
}

#[test]
fn untracked_file_row_stages_fresh_content_but_rejects_stale_content() {
    let (tmp, _) = RepoFixture::new().commit("init", [("base.txt", "base\n")]).build();
    write(&tmp.path().join("new.txt"), "initial\n");
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let version = test.version();
    let line = test.goto_file("untracked", "new.txt");
    let before = tmp.git(&["hash-object", ".git/index"]);
    write(&tmp.path().join("new.txt"), "later\n");
    let result = test.invoke_at_version("status.stage", version, Some(Cursor { line, column: 0 }), None, None);
    assert!(message_texts(&result.effects).iter().any(|text| text.contains("changed")));
    assert_eq!(tmp.git(&["hash-object", ".git/index"]), before);
    assert_eq!(tmp.git(&["status", "--porcelain"]), "?? new.txt\n");
}

#[test]
fn untracked_file_row_stages_fresh_content() {
    let (tmp, _) = RepoFixture::new().commit("init", [("base.txt", "base\n")]).build();
    write(&tmp.path().join("new.txt"), "content\n");
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let line = test.goto_file("untracked", "new.txt");
    let outcome = test.invoke_at("status.stage", Cursor { line, column: 0 });
    assert!(message_texts(&outcome.effects).is_empty());
    assert_eq!(tmp.git(&["show", ":new.txt"]), "content\n");
    assert_eq!(tmp.git(&["status", "--porcelain"]), "A  new.txt\n");
}

#[test]
fn one_stale_file_prevents_a_displayed_multifile_selection_from_mutating_any_file() {
    use magit_lsp::protocol::Selection;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("a.txt", lines(40).to_string()), ("b.txt", lines(40).to_string())])
        .modify("a.txt", lines(40).with(5, "a changed").to_string())
        .modify("b.txt", lines(40).with(5, "b changed").to_string())
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let version = test.version();
    let changed_lines: Vec<_> = test.tree().nodes.iter().filter(|node| node.kind == magit_lsp::tree::NodeKind::Line
        && node.section.as_deref() == Some("unstaged") && node.marker == Some('+'))
        .map(|node| node.start_line).collect();
    assert_eq!(changed_lines.len(), 2);
    let before_head = tmp.git(&["rev-parse", "HEAD"]);
    let before_index = tmp.git(&["hash-object", ".git/index"]);
    let a_before = std::fs::read(tmp.path().join("a.txt")).unwrap();
    std::fs::write(tmp.path().join("b.txt"), lines(40).with(5, "b concurrent edit").to_string()).unwrap();
    let b_after = std::fs::read(tmp.path().join("b.txt")).unwrap();

    let result = test.invoke_at_version("status.stage", version, None, Some(Selection {
        start_line: changed_lines[0], end_line: changed_lines[1],
    }), None);
    assert!(message_texts(&result.effects).iter().any(|text| text.contains("changed")));
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), before_head);
    assert_eq!(tmp.git(&["hash-object", ".git/index"]), before_index);
    assert_eq!(std::fs::read(tmp.path().join("a.txt")).unwrap(), a_before);
    assert_eq!(std::fs::read(tmp.path().join("b.txt")).unwrap(), b_after);
    assert!(tmp.git(&["diff", "--cached"]).is_empty());
}

#[test]
fn unchanged_hunk_can_be_staged_after_an_unrelated_hunk_shifts_it() {
    use magit_lsp::protocol::Selection;
    let original = lines(100).to_string();
    let initial = lines(100).with(80, "line80 changed").to_string();
    let (tmp, _) = RepoFixture::new().commit("init", [("f.txt", original)])
        .modify("f.txt", initial).build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let version = test.version();
    let changed_line = test.tree().nodes.iter().find(|node| node.kind == magit_lsp::tree::NodeKind::Line
        && node.marker == Some('+')).unwrap().start_line;
    let mut shifted: Vec<_> = (1..=100).map(|n| format!("line{n}")).collect();
    shifted.insert(10, "new unrelated line".to_string());
    shifted[80] = "line80 changed".to_string();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    std::fs::write(tmp.path().join("f.txt"), shifted.join("\n") + "\n").unwrap();

    let result = test.invoke_at_version("status.stage", version, None, Some(Selection {
        start_line: changed_line, end_line: changed_line,
    }), None);
    assert!(!message_texts(&result.effects).iter().any(|text| text.contains("changed")));
    assert!(tmp.git(&["diff", "--cached"]).contains("line80 changed"));
    assert!(tmp.git(&["diff"]).contains("new unrelated line"));
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
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

// Independent byte/mode snapshots, including symlink targets and directory
// membership. Do not use the production identity routine to prove no mutation.
fn identity_worktree(path: &std::path::Path) -> Vec<(std::path::PathBuf, u32, Vec<u8>)> {
    use std::os::unix::fs::PermissionsExt;
    fn visit(root: &std::path::Path, path: &std::path::Path, out: &mut Vec<(std::path::PathBuf, u32, Vec<u8>)>) {
        let metadata = std::fs::symlink_metadata(path).unwrap();
        let bytes = if metadata.file_type().is_symlink() {
            std::fs::read_link(path).unwrap().as_os_str().as_encoded_bytes().to_vec()
        } else if metadata.is_file() { std::fs::read(path).unwrap() } else { Vec::new() };
        out.push((path.strip_prefix(root).unwrap().to_path_buf(), metadata.permissions().mode(), bytes));
        if metadata.is_dir() {
            let mut children: Vec<_> = std::fs::read_dir(path).unwrap().map(|entry| entry.unwrap().path()).collect();
            children.sort();
            for child in children {
                if child.file_name().unwrap() != ".git" { visit(root, &child, out); }
            }
        }
    }
    let mut out = Vec::new();
    visit(path, path, &mut out);
    out
}

fn change_identity_file(root: &std::path::Path, kind: &str, later: bool) {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let path = root.join("f.txt");
    match kind {
        "binary" => std::fs::write(path, if later { b"later\0bytes" } else { b"shown\0bytes" }).unwrap(),
        "mode" | "text-mode" | "added-mode" => {
            if kind != "mode" { std::fs::write(&path, "shown text\n").unwrap(); }
            let executable = if kind == "added-mode" { later } else { !later };
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 })).unwrap();
        }
        "type" => {
            std::fs::remove_file(&path).unwrap();
            symlink(if later { "later-target" } else { "shown-target" }, path).unwrap();
        }
        "untracked-link" => {
            if later { std::fs::remove_file(&path).unwrap(); }
            symlink(if later { "later-target" } else { "missing-target" }, path).unwrap();
        }
        "directory" => {
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join(if later { "new-member" } else { "shown-member" }), "member\n").unwrap();
        }
        _ => std::fs::write(path, if later { "later text\n" } else { "shown text\n" }).unwrap(),
    }
}

fn file_identity_matrix(action: &str, stale: bool, selection: bool) {
    use magit_lsp::protocol::{PromptValue, Selection, ViewInvokeParams};
    use magit_lsp::session::Execution;
    for kind in ["binary", "mode", "text-mode", "added-mode", "type", "collapsed", "untracked", "untracked-link", "directory"] {
        let untracked = matches!(kind, "untracked" | "untracked-link" | "directory");
        if untracked && action == "unstage" { continue; }
        if untracked && selection { continue; }
        let initial = if kind == "binary" { "base\0bytes" } else { "base text\n" };
        let (tmp, _) = RepoFixture::new().commit("init", [("base.txt", "base\n")]).build();
        tmp.git(&["config", "core.fileMode", "true"]);
        if !untracked {
            std::fs::write(tmp.path().join("f.txt"), initial).unwrap();
            tmp.git(&["add", "f.txt"]);
            tmp.git(&["commit", "-m", "tracked base"]);
        }
        change_identity_file(tmp.path(), kind, false);
        if action == "unstage" { tmp.git(&["add", "f.txt"]); }
        let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
        test.ack();
        let section = if action == "unstage" { "staged" } else if untracked { "untracked" } else { "unstaged" };
        let path = if kind == "directory" { "f.txt/" } else { "f.txt" };
        let mut line = test.goto_file(section, path);
        if kind == "collapsed" {
            test.invoke_at("common.hide-section", Cursor { line, column: 0 });
            line = test.goto_file(section, path);
            assert!(test.tree().nodes.iter().find(|node| node.start_line == line
                && node.kind == magit_lsp::tree::NodeKind::File).unwrap().children.is_empty());
        }
        let version = test.version();
        if stale {
            change_identity_file(tmp.path(), kind, true);
            if action == "unstage" { tmp.git(&["add", "f.txt"]); }
        } else {
            // Single-file authorization must not fingerprint the repository;
            // unstage must also ignore edits to this file's worktree side.
            std::fs::write(tmp.path().join("base.txt"), "unrelated edit\n").unwrap();
            if action == "unstage" { change_identity_file(tmp.path(), kind, true); }
        }
        let head = tmp.git(&["rev-parse", "HEAD"]);
        let index = std::fs::read(tmp.path().join(".git/index")).unwrap();
        let index_entries = tmp.git(&["ls-files", "--stage", "-z"]);
        let worktree = identity_worktree(tmp.path());
        let step = test.session.start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(), command: format!("status.{action}"), version: Some(version),
            cursor: if selection { None } else { Some(Cursor { line, column: 0 }) },
            selection: selection.then_some(Selection { start_line: line, end_line: line }), args: None,
        }).unwrap();
        let result = match step {
            Execution::Prompt(prompt) => {
                assert!(action == "discard" && (!stale || selection), "{kind}/{action}: stale file action prompted");
                let Execution::Done(result) = test.session.resume_prompt(&prompt.prompt_id, Some(PromptValue::Bool(true))).unwrap()
                    else { panic!("discard completed"); };
                result
            }
            Execution::Done(result) => {
                assert!(stale || action != "discard", "{kind}: fresh discard did not prompt");
                result
            }
        };
        let messages = message_texts(&result.effects);
        assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head, "{kind}/{action}: HEAD");
        if stale {
            assert!(messages.iter().any(|text| text.contains("changed")), "{kind}/{action}: {messages:?}");
            assert!(result.render.is_some(), "{kind}/{action}: stale rejection must refresh");
            assert_eq!(std::fs::read(tmp.path().join(".git/index")).unwrap(), index, "{kind}/{action}: index");
            assert_eq!(identity_worktree(tmp.path()), worktree, "{kind}/{action}: worktree");
        } else {
            assert!(!messages.iter().any(|text| text.contains("changed") || text.contains("failed")), "{kind}/{action}: {messages:?}");
            assert_eq!(std::fs::read_to_string(tmp.path().join("base.txt")).unwrap(), "unrelated edit\n");
            match action {
                "stage" => {
                    assert_eq!(identity_worktree(tmp.path()), worktree, "{kind}: stage worktree");
                    if kind == "directory" {
                        assert_eq!(tmp.git(&["show", ":f.txt/shown-member"]), "member\n");
                        assert_eq!(tmp.git(&["ls-files", "f.txt/"]), "f.txt/shown-member\n");
                    } else {
                        let expected = match kind {
                            "binary" => "shown\0bytes", "mode" => initial,
                            "type" => "shown-target", "untracked-link" => "missing-target", _ => "shown text\n",
                        };
                        assert_eq!(tmp.git(&["show", ":f.txt"]), expected, "{kind}: stage bytes");
                        let mode = if matches!(kind, "type" | "untracked-link") { "120000" }
                            else if matches!(kind, "mode" | "text-mode") { "100755" } else { "100644" };
                        assert_eq!(tmp.git(&["ls-files", "--stage", "f.txt"]).split_whitespace().next(), Some(mode));
                    }
                }
                "unstage" => {
                    assert_eq!(identity_worktree(tmp.path()), worktree, "{kind}: unstage worktree");
                    assert_eq!(tmp.git(&["show", ":f.txt"]), initial, "{kind}: unstage bytes");
                    assert_eq!(tmp.git(&["ls-files", "--stage", "f.txt"]).split_whitespace().next(), Some("100644"));
                    assert_eq!(tmp.git(&["write-tree"]), tmp.git(&["rev-parse", "HEAD^{tree}"]));
                }
                "discard" => {
                    // Git restore legitimately updates the index stat cache,
                    // but must preserve every staged path/blob/mode.
                    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), index_entries, "{kind}: discard index");
                    if untracked { assert!(std::fs::symlink_metadata(tmp.path().join("f.txt")).is_err()); }
                    else {
                        use std::os::unix::fs::PermissionsExt;
                        let metadata = std::fs::symlink_metadata(tmp.path().join("f.txt")).unwrap();
                        assert!(metadata.is_file());
                        assert_eq!(metadata.permissions().mode() & 0o777, 0o644);
                        assert_eq!(std::fs::read(tmp.path().join("f.txt")).unwrap(), initial.as_bytes());
                    }
                }
                _ => unreachable!(),
            }
        }
    }
}

#[test]
fn complete_file_stage_identity_fresh_and_stale_matrix() {
    for selection in [false, true] {
        file_identity_matrix("stage", false, selection);
        file_identity_matrix("stage", true, selection);
    }
}

#[test]
fn complete_file_unstage_identity_fresh_and_stale_matrix() {
    for selection in [false, true] {
        file_identity_matrix("unstage", false, selection);
        file_identity_matrix("unstage", true, selection);
    }
}

#[test]
fn complete_file_discard_identity_fresh_and_stale_matrix() {
    for selection in [false, true] {
        file_identity_matrix("discard", false, selection);
        file_identity_matrix("discard", true, selection);
    }
}

#[test]
fn file_discard_decline_and_open_prompt_changes_preserve_exact_state() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    for kind in ["binary", "mode", "added-mode", "type", "collapsed", "untracked", "untracked-link", "directory"] {
        for accept in [false, true] {
            let (tmp, _) = RepoFixture::new().commit("init", [("base.txt", "base\n")]).build();
            tmp.git(&["config", "core.fileMode", "true"]);
            let untracked = matches!(kind, "untracked" | "untracked-link" | "directory");
            if !untracked {
                std::fs::write(tmp.path().join("f.txt"), "base\n").unwrap();
                tmp.git(&["add", "f.txt"]);
                tmp.git(&["commit", "-m", "base"]);
            }
            change_identity_file(tmp.path(), kind, false);
            let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
            test.ack();
            let line = test.goto_file(if untracked { "untracked" } else { "unstaged" },
                if kind == "directory" { "f.txt/" } else { "f.txt" });
            let Execution::Prompt(prompt) = test.session.start_invoke(&ViewInvokeParams {
                view_id: test.view_id().to_string(), command: "status.discard".into(), version: Some(test.version()),
                cursor: Some(Cursor { line, column: 0 }), selection: None, args: None,
            }).unwrap() else { panic!("{kind}: expected confirmation"); };
            if accept { change_identity_file(tmp.path(), kind, true); }
            let head = tmp.git(&["rev-parse", "HEAD"]);
            let index = std::fs::read(tmp.path().join(".git/index")).unwrap();
            let worktree = identity_worktree(tmp.path());
            let Execution::Done(result) = test.session.resume_prompt(&prompt.prompt_id, Some(PromptValue::Bool(accept))).unwrap()
                else { panic!("{kind}: completion"); };
            if accept { assert!(message_texts(&result.effects).iter().any(|text| text.contains("changed")), "{kind}"); }
            assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head, "{kind}");
            assert_eq!(std::fs::read(tmp.path().join(".git/index")).unwrap(), index, "{kind}");
            assert_eq!(identity_worktree(tmp.path()), worktree, "{kind}");
            assert!(!test.session.has_pending_prompt());
        }
    }
}

#[test]
fn stale_binary_whole_file_selection_validates_all_members_before_mutation() {
    use magit_lsp::protocol::{PromptValue, Selection, ViewInvokeParams};
    use magit_lsp::session::Execution;
    for action in ["stage", "unstage", "discard"] {
        let (tmp, _) = RepoFixture::new().commit("init", [("a.bin", "base\0a"), ("b.bin", "base\0b")]).build();
        write(&tmp.path().join("a.bin"), "shown\0a");
        write(&tmp.path().join("b.bin"), "shown\0b");
        if action == "unstage" { tmp.git(&["add", "."]); }
        let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
        test.ack();
        let section = if action == "unstage" { "staged" } else { "unstaged" };
        let start_line = test.goto_file(section, "a.bin");
        let end_line = test.goto_file(section, "b.bin");
        let version = test.version();
        write(&tmp.path().join("b.bin"), "later\0b");
        if action == "unstage" { tmp.git(&["add", "b.bin"]); }
        let head = tmp.git(&["rev-parse", "HEAD"]);
        let index = std::fs::read(tmp.path().join(".git/index")).unwrap();
        let worktree = identity_worktree(tmp.path());
        let step = test.session.start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(), command: format!("status.{action}"), version: Some(version),
            cursor: None, selection: Some(Selection { start_line, end_line }), args: None,
        }).unwrap();
        let result = match step {
            Execution::Done(result) => result,
            Execution::Prompt(prompt) => {
                let Execution::Done(result) = test.session.resume_prompt(&prompt.prompt_id, Some(PromptValue::Bool(true))).unwrap()
                    else { panic!("expected completion"); };
                result
            }
        };
        assert!(message_texts(&result.effects).iter().any(|text| text.contains("changed")), "{action}");
        assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head, "{action}");
        assert_eq!(std::fs::read(tmp.path().join(".git/index")).unwrap(), index, "{action}");
        assert_eq!(identity_worktree(tmp.path()), worktree, "{action}");
    }
}

#[test]
fn discarding_an_untracked_directory_symlink_never_deletes_its_target() {
    use std::os::unix::fs::symlink;
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new().commit("init", [("target/kept", "keep\n")]).build();
    symlink("target", tmp.path().join("link")).unwrap();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let line = test.goto_file("untracked", "link");
    let Execution::Prompt(prompt) = test.session.start_invoke(&ViewInvokeParams {
        view_id: test.view_id().to_string(), command: "status.discard".into(), version: Some(test.version()),
        cursor: Some(Cursor { line, column: 0 }), selection: None, args: None,
    }).unwrap() else { panic!("expected confirmation"); };
    test.session.resume_prompt(&prompt.prompt_id, Some(PromptValue::Bool(true))).unwrap();
    assert!(std::fs::symlink_metadata(tmp.path().join("link")).is_err());
    assert_eq!(std::fs::read_to_string(tmp.path().join("target/kept")).unwrap(), "keep\n");
    assert_eq!(tmp.git(&["status", "--porcelain"]), "");
}

#[test]
fn renamed_file_unstage_validates_the_source_index_but_ignores_source_worktree_edits() {
    for stale in [false, true] {
        let (tmp, _) = RepoFixture::new().commit("init", [("old.txt", "rename bytes\n")])
            .rename("old.txt", "new.txt").build();
        let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
        test.ack();
        let line = test.goto_file("staged", "new.txt");
        let version = test.version();
        write(&tmp.path().join("old.txt"), "external source\n");
        if stale { tmp.git(&["add", "old.txt"]); }
        let head = tmp.git(&["rev-parse", "HEAD"]);
        let index = std::fs::read(tmp.path().join(".git/index")).unwrap();
        let worktree = identity_worktree(tmp.path());
        let result = test.invoke_at_version("status.unstage", version, Some(Cursor { line, column: 0 }), None, None);
        assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
        assert_eq!(identity_worktree(tmp.path()), worktree);
        if stale {
            assert!(message_texts(&result.effects).iter().any(|text| text.contains("changed")));
            assert_eq!(std::fs::read(tmp.path().join(".git/index")).unwrap(), index);
        } else {
            assert!(message_texts(&result.effects).is_empty());
            assert_eq!(tmp.git(&["ls-files"]), "old.txt\n");
            assert_eq!(tmp.git(&["show", ":old.txt"]), "rename bytes\n");
            assert_eq!(tmp.git(&["write-tree"]), tmp.git(&["rev-parse", "HEAD^{tree}"]));
        }
    }
}

#[test]
fn status_command_in_menu_reports_scope_mismatch_without_changing_anything() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();

    let head_before = tmp.git(&["rev-parse", "HEAD"]);
    let index_before = tmp.git(&["write-tree"]);
    let status_before = tmp.git(&["status", "--porcelain=v2", "-z"]);
    let worktree_before = std::fs::read(tmp.path().join("f.txt")).unwrap();

    let (menu_descriptor, menu_render) = test.open_via_command("open.commit");
    test.focus(menu_descriptor.id.clone());
    test.ack();
    let menu_before = test.text_of(&menu_descriptor.id);
    let menu_model_before = test.menu(&menu_descriptor.id).expect("commit menu");
    let views_before = test.view_ids();

    let result = test.invoke("status.stage");
    assert_eq!(
        result.effects,
        vec![Effect::message(
            MessageLevel::Info,
            "Not available in this view"
        )]
    );
    assert!(result.render.is_none());
    assert!(result.opened.is_empty());
    assert_eq!(test.view_id(), menu_descriptor.id);
    assert_eq!(test.view_ids(), views_before);
    assert_eq!(test.text_of(&menu_descriptor.id), menu_before);
    assert_eq!(
        test.menu(&menu_descriptor.id).as_ref(),
        Some(&menu_model_before)
    );
    assert_eq!(test.version_of(&menu_descriptor.id), menu_render.version);

    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head_before);
    assert_eq!(tmp.git(&["write-tree"]), index_before);
    assert_eq!(tmp.git(&["status", "--porcelain=v2", "-z"]), status_before);
    assert_eq!(
        std::fs::read(tmp.path().join("f.txt")).unwrap(),
        worktree_before
    );
}
