//! The command registry (R-2.1): ids, docs, scopes and Planned semantics.

mod common;

use common::*;
use magit_lsp::commands::{registry, Command, CommandStatus, EffectKind, Scope};
use magit_lsp::config::Profile;
use magit_lsp::protocol::{Cursor, EditDocumentMode, Effect, MessageLevel};

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
    "open.notes",
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
fn remote_session_flows_preserve_configuration_and_require_confirmation() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ui()
        .reply_text(Some("origin".into()))
        .reply_text(Some("https://fetch".into()));
    test.invoke_ui("remote.add");
    assert_eq!(
        tmp.git(&["config", "--get", "remote.origin.url"]),
        "https://fetch\n"
    );
    tmp.git(&["config", "remote.origin.tagOpt", "--no-tags"]);
    test.ui()
        .reply_text(Some("origin".into()))
        .reply_text(Some("upstream".into()));
    test.invoke_ui("remote.rename");
    assert_eq!(tmp.git(&["remote"]), "upstream\n");
    test.ui()
        .reply_text(Some("upstream".into()))
        .reply_text(Some("https://new-fetch".into()))
        .reply_text(Some("https://new-push".into()));
    test.invoke_ui("remote.configure");
    assert_eq!(
        tmp.git(&["config", "--get", "remote.upstream.url"]),
        "https://new-fetch\n"
    );
    assert_eq!(
        tmp.git(&["config", "--get", "remote.upstream.pushurl"]),
        "https://new-push\n"
    );
    assert_eq!(
        tmp.git(&["config", "--get", "remote.upstream.tagOpt"]),
        "--no-tags\n"
    );
    assert_eq!(
        tmp.git(&["config", "--get", "remote.upstream.fetch"]),
        "+refs/heads/*:refs/remotes/upstream/*\n"
    );
    let before = tmp.git(&["config", "--local", "--list"]);
    test.ui()
        .reply_text(Some("upstream".into()))
        .reply_confirm(false);
    test.invoke_ui("remote.remove");
    let request = test.ui().requests().last().unwrap().clone();
    assert_eq!(request.destructive, Some(true));
    assert_eq!(request.default, Some(serde_json::json!(false)));
    assert_eq!(tmp.git(&["config", "--local", "--list"]), before);
    let Execution::Prompt(name) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "remote.remove".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected name prompt");
    };
    let Execution::Prompt(confirmation) = test
        .session
        .resume_prompt(&name.prompt_id, Some(PromptValue::Text("upstream".into())))
        .unwrap()
    else {
        panic!("expected confirmation");
    };
    assert!(matches!(
        test.session
            .resume_prompt(&confirmation.prompt_id, None)
            .unwrap(),
        Execution::Done(_)
    ));
    assert_eq!(tmp.git(&["config", "--local", "--list"]), before);
    test.ui()
        .reply_text(Some("upstream".into()))
        .reply_confirm(true);
    test.invoke_ui("remote.remove");
    assert!(tmp.git(&["remote"]).is_empty());
}

#[test]
fn remote_session_cancellation_invalid_and_conflicting_inputs_do_not_mutate() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    tmp.git(&["remote", "add", "origin", "https://fetch"]);
    tmp.git(&["remote", "add", "other", "https://other"]);
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    let before = tmp.git(&["config", "--local", "--list"]);
    for (command, answers) in [
        ("remote.add", vec![Some("new"), None]),
        ("remote.rename", vec![Some("origin"), None]),
        (
            "remote.configure",
            vec![Some("origin"), Some("https://changed"), None],
        ),
        ("remote.remove", vec![None]),
    ] {
        for answer in answers {
            test.ui().reply_text(answer.map(Into::into));
        }
        test.invoke_ui(command);
        assert_eq!(
            tmp.git(&["config", "--local", "--list"]),
            before,
            "{command}"
        );
    }
    for (command, answers) in [
        ("remote.add", vec!["origin", "https://conflict"]),
        ("remote.add", vec!["bad name", "https://invalid"]),
        ("remote.rename", vec!["origin", "other"]),
        (
            "remote.configure",
            vec!["origin", "https://changed", "--unsafe"],
        ),
        (
            "remote.configure",
            vec!["missing", "https://fetch", "https://push"],
        ),
    ] {
        for answer in answers {
            test.ui().reply_text(Some(answer.into()));
        }
        let result = test.invoke_ui(command);
        assert!(result.effects.iter().any(|effect| matches!(effect, Effect::Message { level: MessageLevel::Warn, text, .. } if !text.is_empty() && !text.starts_with('{'))), "{command}: {:?}", result.effects);
        assert_eq!(
            tmp.git(&["config", "--local", "--list"]),
            before,
            "{command}"
        );
    }
}

#[test]
fn tag_session_creates_lightweight_tag_at_selected_revision() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .commit("second", [("f.txt", "second\n")])
        .build();
    let chosen = tmp.git(&["rev-parse", "HEAD^"]);
    tmp.git(&["config", "tag.gpgSign", "true"]);
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ui()
        .reply_text(Some("release".into()))
        .reply_text(Some("HEAD^".into()));
    test.invoke_ui("tag.create");
    assert_eq!(tmp.git(&["rev-parse", "release"]), chosen);
    assert_eq!(tmp.git(&["cat-file", "-t", "release"]), "commit\n");
}

#[test]
fn tag_session_cancellation_and_refusals_preserve_refs() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    tmp.git(&["tag", "existing"]);
    let before = tmp.git(&["show-ref"]);
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    for answers in [vec![None], vec![Some("cancelled"), None]] {
        for answer in answers {
            test.ui().reply_text(answer.map(Into::into));
        }
        test.invoke_ui("tag.create");
        assert_eq!(tmp.git(&["show-ref"]), before);
    }
    for (name, revision) in [
        ("bad name", "HEAD"),
        ("existing", "HEAD"),
        ("new", "missing"),
        ("--unsafe", "HEAD"),
    ] {
        test.ui()
            .reply_text(Some(name.into()))
            .reply_text(Some(revision.into()));
        let result = test.invoke_ui("tag.create");
        assert!(result.effects.iter().any(|effect| matches!(effect, Effect::Message { level: MessageLevel::Warn, text, .. } if !text.is_empty() && !text.starts_with('{'))), "{:?}", result.effects);
        assert_eq!(tmp.git(&["show-ref"]), before);
    }
}

#[test]
fn commit_arguments_are_typed_and_invalid_input_never_stages() {
    use magit_lsp::git::CommitOperation;
    use serde_json::json;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
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
        assert!(
            message_texts(&result.effects)
                .iter()
                .any(|text| text.contains("Invalid commit arguments")),
            "{args}: {:?}",
            result.effects
        );
        assert_eq!(tmp.git(&["status", "--porcelain"]), before);
    }
    let result = test.invoke_args(
        "commit.create",
        json!({
            "switches": ["-a", "-e", "-v", "-n", "-s", "-R"],
            "options": {"=D": "2001-02-03T04:05:06+00:00", "=S": "", "=c": "HEAD"}
        }),
    );
    let magit_lsp::git::RepositoryOperation::Commit(CommitOperation::Create(options)) =
        result.commit.expect("typed operation")
    else {
        panic!("expected create operation")
    };
    assert!(
        options.all
            && options.allow_empty
            && options.verbose
            && options.no_verify
            && options.signoff
            && options.reset_author
    );
    assert_eq!(options.signing_key.as_deref(), Some(""));
    assert_eq!(
        options.reedit_message.as_deref(),
        Some(tmp.git(&["rev-parse", "HEAD"]).trim())
    );
    assert_eq!(
        options.argv(),
        vec![
            "--all".to_string(),
            "--allow-empty".into(),
            "--verbose".into(),
            "--no-verify".into(),
            "--reset-author".into(),
            "--signoff".into(),
            "--date=2001-02-03T04:05:06+00:00".into(),
            "--gpg-sign=".into(),
            format!(
                "--reedit-message={}",
                tmp.git(&["rev-parse", "HEAD"]).trim()
            ),
        ]
    );
    assert_eq!(
        tmp.git(&["status", "--porcelain"]),
        before,
        "dispatch itself does not stage"
    );
}

#[test]
fn captured_reuse_and_reedit_options_have_real_git_effects() {
    use serde_json::json;
    for (key, reset) in [("=C", false), ("=c", false), ("=C", true), ("=c", true)] {
        let (tmp, _) = RepoFixture::new()
            .commit("original message", [("f.txt", TEN_LINES)])
            .build();
        let original_tree = tmp.git(&["rev-parse", "HEAD^{tree}"]);
        let original_author = tmp.git(&["log", "-1", "--format=%an <%ae>"]);
        let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
        let operation = test.invoke_args("commit.create", json!({
            "switches": if reset { vec!["-e", "-R"] } else { vec!["-e"] }, "options": {(key): "HEAD"}
        })).commit.expect("allow-empty bypasses clean guard");
        let magit_lsp::git::RepositoryOperation::Commit(operation) = operation else {
            panic!("expected commit operation")
        };
        let argv = operation.argv();
        let repo = magit_lsp::git::GitRepo::open(tmp.path()).unwrap();
        repo.commit(
            &argv.iter().map(String::as_str).collect::<Vec<_>>(),
            &[
                ("GIT_EDITOR", "true"),
                ("GIT_AUTHOR_NAME", "New Committer"),
                ("GIT_AUTHOR_EMAIL", "new@example.com"),
            ],
        )
        .unwrap();
        assert_eq!(tmp.git(&["rev-list", "--count", "HEAD"]).trim(), "2");
        assert_eq!(
            tmp.git(&["log", "-1", "--format=%s"]).trim(),
            "original message"
        );
        assert_eq!(tmp.git(&["rev-parse", "HEAD^{tree}"]), original_tree);
        let expected = if reset {
            "New Committer <new@example.com>"
        } else {
            original_author.trim()
        };
        assert_eq!(
            tmp.git(&["log", "-1", "--format=%an <%ae>"]).trim(),
            expected
        );
    }
}

#[test]
fn failed_and_noop_staging_do_not_advance_to_a_sibling() {
    for failure in [true, false] {
        let (tmp, _) = RepoFixture::new()
            .commit("init", [("f.txt", lines(40).to_string())])
            .modify("f.txt", two_hunk_changes())
            .build();
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
        if failure {
            std::fs::remove_file(tmp.path().join(".git/index.lock")).unwrap();
        }
        assert!(tmp.git(&["diff", "--cached"]).is_empty());
    }
}

#[test]
fn staging_a_staged_hunk_is_a_quiet_exact_noop() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", lines(40).to_string())])
        .modify("f.txt", two_hunk_changes())
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    test.goto_marker("f.txt", '+');
    test.invoke_here("status.stage");
    test.ack();
    let staged_line = test
        .tree()
        .line_nodes
        .iter()
        .position(|node| {
            node.kind == magit_lsp::tree::NodeKind::Line
                && node.section.as_deref() == Some("staged")
                && node.marker == Some('+')
        })
        .unwrap();
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
        assert_eq!(
            std::fs::read(tmp.path().join("f.txt")).unwrap(),
            worktree_before
        );
    }
}

#[test]
fn common_hierarchy_commands_are_implemented() {
    for id in [
        "common.cycle-visibility",
        "common.next-sibling",
        "common.prev-sibling",
        "common.parent-section",
    ] {
        assert_eq!(
            registry().get(id).unwrap().status,
            CommandStatus::Implemented
        );
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
    "notes",
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
                if matches!(
                    *command,
                    "commit.fixup"
                        | "commit.instant-fixup"
                        | "commit.instant-squash"
                        | "rebase.continue"
                        | "rebase.skip"
                        | "rebase.abort"
                        | "rebase.interactive"
                ) {
                    assert_eq!(registered.status, CommandStatus::Implemented);
                    continue;
                }
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
        Scope::RebaseTodo,
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
    for id in [
        "rebase.todo-set-action",
        "rebase.todo-move-before",
        "rebase.todo-submit",
        "rebase.todo-pick",
        "rebase.todo-reword",
        "rebase.todo-edit",
        "rebase.todo-squash",
        "rebase.todo-fixup",
        "rebase.todo-drop",
        "rebase.todo-up",
        "rebase.todo-down",
    ] {
        let command = registry().get(id).expect("registered");
        assert_eq!(command.scope, Scope::RebaseTodo, "{id}");
        assert!(command.applies_to("rebase-todo"), "{id}");
        assert!(!command.applies_to("status"), "{id}");
        assert_eq!(
            command.requires_profile_binding(),
            !matches!(id, "rebase.todo-set-action" | "rebase.todo-move-before"),
            "{id}"
        );
    }
    assert_eq!(
        registry().get("rebase.edit-todo").unwrap().scope,
        Scope::MenuAction
    );
    for id in ["commit-message.history-prev", "commit-message.history-next"] {
        let command = registry().get(id).expect("registered");
        assert_eq!(command.scope, Scope::CommitMessage);
        assert_eq!(
            command.status,
            CommandStatus::Planned {
                roadmap_id: "R-EDIT-HISTORY"
            }
        );
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
    assert_eq!(
        magit_lsp::commands::required_effects("common.visit"),
        [EffectKind::VisitFile].as_slice()
    );
}

#[test]
fn copy_value_emits_registered_clipboard_effect() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let line = test.goto_copy("line3 CHANGED");
    let outcome = test.invoke_at("common.copy-value", Cursor { line, column: 0 });
    assert!(outcome
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::Clipboard { .. })));
    assert_eq!(
        magit_lsp::commands::required_effects("common.copy-value"),
        [EffectKind::Clipboard].as_slice()
    );
}

#[test]
fn effect_kinds_cover_optional_effects() {
    use magit_lsp::commands::effect_kind;

    assert_eq!(
        effect_kind(&Effect::VisitFile {
            uri: "file:///x".into(),
            line: 0,
            column: 0,
        }),
        Some(EffectKind::VisitFile)
    );
    assert_eq!(
        effect_kind(&Effect::EditDocument {
            run_id: "r".into(),
            uri: "file:///x".into(),
            text: String::new(),
            version: 1,
            kind: "commit".into(),
            title: "t".into(),
            mode: EditDocumentMode::RealFile,
            scope: String::new(),
        }),
        Some(EffectKind::EditDocument)
    );
    assert_eq!(
        effect_kind(&Effect::Clipboard { text: "x".into() }),
        Some(EffectKind::Clipboard)
    );
    assert_eq!(effect_kind(&Effect::message(MessageLevel::Warn, "w")), None);
    assert_eq!(
        effect_kind(&Effect::PopView {
            view_id: "v".into()
        }),
        None
    );
}

#[test]
#[should_panic(expected = "undeclared optional effect")]
fn undeclared_effect_is_rejected() {
    magit_lsp::commands::enforce_effects(
        "common.cancel",
        &mut [Effect::Clipboard { text: "x".into() }],
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

    assert_eq!(
        message_texts(&outcome.effects),
        vec!["Nothing to visit here"]
    );
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
    let selected = test
        .tree()
        .nodes
        .iter()
        .find(|node| {
            node.kind == magit_lsp::tree::NodeKind::Line
                && node.path.as_deref() == Some("f.txt")
                && node.marker == Some('+')
        })
        .unwrap()
        .start_line;
    let before_head = tmp.git(&["rev-parse", "HEAD"]);
    let before_index = tmp.git(&["hash-object", ".git/index"]);
    let before_worktree = std::fs::read(tmp.path().join("f.txt")).unwrap();
    std::fs::write(
        tmp.path().join("f.txt"),
        lines(40).with(5, "new concurrent change").to_string(),
    )
    .unwrap();
    let after_worktree = std::fs::read(tmp.path().join("f.txt")).unwrap();

    let result = test.invoke_at_version(
        "status.stage",
        version,
        None,
        Some(Selection {
            start_line: selected,
            end_line: selected,
        }),
        None,
    );
    assert!(message_texts(&result.effects)
        .iter()
        .any(|text| text.contains("changed")));
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), before_head);
    assert_eq!(tmp.git(&["hash-object", ".git/index"]), before_index);
    assert_eq!(
        std::fs::read(tmp.path().join("f.txt")).unwrap(),
        after_worktree
    );
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
    let file_line = test
        .tree()
        .nodes
        .iter()
        .find(|node| {
            node.kind == magit_lsp::tree::NodeKind::File
                && node.path.as_deref() == Some("f.txt")
                && node.section.as_deref() == Some("unstaged")
        })
        .unwrap()
        .start_line;
    let before_head = tmp.git(&["rev-parse", "HEAD"]);
    let before_index = tmp.git(&["hash-object", ".git/index"]);
    std::fs::write(
        tmp.path().join("f.txt"),
        lines(40)
            .with(5, "line5 CHANGED")
            .with(20, "new hidden change")
            .to_string(),
    )
    .unwrap();

    let result = test.invoke_at_version(
        "status.stage",
        version,
        Some(Cursor {
            line: file_line,
            column: 0,
        }),
        None,
        None,
    );
    assert!(message_texts(&result.effects)
        .iter()
        .any(|text| text.contains("changed")));
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), before_head);
    assert_eq!(tmp.git(&["hash-object", ".git/index"]), before_index);
    assert!(tmp.git(&["diff", "--cached"]).is_empty());
    assert!(tmp.git(&["diff"]).contains("new hidden change"));
}

#[test]
fn mode_only_change_after_render_is_rejected_without_staging() {
    use std::os::unix::fs::PermissionsExt;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", "bytes\n")])
        .build();
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
    let outcome = test.invoke_at_version(
        "status.stage",
        version,
        Some(Cursor { line, column: 0 }),
        None,
        None,
    );
    assert!(message_texts(&outcome.effects)
        .iter()
        .any(|text| text.contains("changed")));
    assert_eq!(tmp.git(&["hash-object", ".git/index"]), before);
    assert!(tmp.git(&["diff", "--cached"]).is_empty());
}

#[cfg(unix)]
#[test]
fn unique_hunk_target_rejects_executable_mode_added_after_render() {
    use std::os::unix::fs::PermissionsExt;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", lines(20).to_string())])
        .modify("f.txt", lines(20).with(5, "line5 CHANGED").to_string())
        .build();
    let path = tmp.path().join("f.txt");
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o644);
    std::fs::set_permissions(&path, permissions).unwrap();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let version = test.version();
    let line = test
        .tree()
        .nodes
        .iter()
        .find(|node| {
            node.kind == magit_lsp::tree::NodeKind::Hunk
                && node.path.as_deref() == Some("f.txt")
                && node.section.as_deref() == Some("unstaged")
        })
        .unwrap()
        .start_line;
    let before_head = tmp.git(&["rev-parse", "HEAD"]);
    let before_index = tmp.git(&["hash-object", ".git/index"]);
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).unwrap();
    let after_worktree = std::fs::read(&path).unwrap();
    let outcome = test.invoke_at_version(
        "status.stage",
        version,
        Some(Cursor { line, column: 0 }),
        None,
        None,
    );
    assert!(message_texts(&outcome.effects)
        .iter()
        .any(|text| text.contains("changed")));
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), before_head);
    assert_eq!(tmp.git(&["hash-object", ".git/index"]), before_index);
    assert_eq!(std::fs::read(&path).unwrap(), after_worktree);
    assert_eq!(
        tmp.git(&["ls-files", "--stage", "f.txt"])
            .split_whitespace()
            .next(),
        Some("100644")
    );
}

#[test]
fn binary_content_change_after_render_is_rejected_without_staging() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.bin", "old\0binary")])
        .build();
    write(&tmp.path().join("f.bin"), "shown\0binary");
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let version = test.version();
    let line = test.goto_file("unstaged", "f.bin");
    let before = tmp.git(&["hash-object", ".git/index"]);
    write(&tmp.path().join("f.bin"), "new\0binary");
    let outcome = test.invoke_at_version(
        "status.stage",
        version,
        Some(Cursor { line, column: 0 }),
        None,
        None,
    );
    assert!(message_texts(&outcome.effects)
        .iter()
        .any(|text| text.contains("changed")));
    assert_eq!(tmp.git(&["hash-object", ".git/index"]), before);
    assert!(tmp.git(&["diff", "--cached"]).is_empty());
}

#[test]
fn unchanged_binary_file_row_stages_successfully() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.bin", "old\0binary")])
        .build();
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
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", "regular\n")])
        .build();
    std::fs::remove_file(tmp.path().join("f.txt")).unwrap();
    symlink("missing-target", tmp.path().join("f.txt")).unwrap();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let line = test.goto_file("unstaged", "f.txt");
    let outcome = test.invoke_at("status.stage", Cursor { line, column: 0 });
    assert!(message_texts(&outcome.effects).is_empty());
    assert_eq!(
        tmp.git(&["ls-files", "--stage", "f.txt"])
            .split_whitespace()
            .next(),
        Some("120000")
    );
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
    let file_line = test
        .tree()
        .nodes
        .iter()
        .find(|node| {
            node.kind == magit_lsp::tree::NodeKind::File
                && node.path.as_deref() == Some("f.txt")
                && node.section.as_deref() == Some("unstaged")
        })
        .unwrap()
        .start_line;
    test.invoke_at(
        "common.hide-section",
        Cursor {
            line: file_line,
            column: 0,
        },
    );
    let version = test.version();
    let file = test
        .tree()
        .nodes
        .iter()
        .find(|node| {
            node.kind == magit_lsp::tree::NodeKind::File
                && node.path.as_deref() == Some("f.txt")
                && node.section.as_deref() == Some("unstaged")
        })
        .unwrap();
    assert!(file.children.is_empty());
    let file_line = file.start_line;
    let before_head = tmp.git(&["rev-parse", "HEAD"]);
    let before_index = tmp.git(&["hash-object", ".git/index"]);

    let result = test.invoke_at_version(
        "status.stage",
        version,
        Some(Cursor {
            line: file_line,
            column: 0,
        }),
        None,
        None,
    );
    assert!(message_texts(&result.effects).is_empty());
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), before_head);
    assert_ne!(tmp.git(&["hash-object", ".git/index"]), before_index);
    assert!(tmp.git(&["diff", "--cached"]).contains("line5 CHANGED"));
    assert!(tmp.git(&["diff"]).is_empty());
}

#[test]
fn untracked_file_row_stages_fresh_content_but_rejects_stale_content() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    write(&tmp.path().join("new.txt"), "initial\n");
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let version = test.version();
    let line = test.goto_file("untracked", "new.txt");
    let before = tmp.git(&["hash-object", ".git/index"]);
    write(&tmp.path().join("new.txt"), "later\n");
    let result = test.invoke_at_version(
        "status.stage",
        version,
        Some(Cursor { line, column: 0 }),
        None,
        None,
    );
    assert!(message_texts(&result.effects)
        .iter()
        .any(|text| text.contains("changed")));
    assert_eq!(tmp.git(&["hash-object", ".git/index"]), before);
    assert_eq!(tmp.git(&["status", "--porcelain"]), "?? new.txt\n");
}

#[test]
fn untracked_file_row_stages_fresh_content() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
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
        .commit(
            "init",
            [
                ("a.txt", lines(40).to_string()),
                ("b.txt", lines(40).to_string()),
            ],
        )
        .modify("a.txt", lines(40).with(5, "a changed").to_string())
        .modify("b.txt", lines(40).with(5, "b changed").to_string())
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let version = test.version();
    let changed_lines: Vec<_> = test
        .tree()
        .nodes
        .iter()
        .filter(|node| {
            node.kind == magit_lsp::tree::NodeKind::Line
                && node.section.as_deref() == Some("unstaged")
                && node.marker == Some('+')
        })
        .map(|node| node.start_line)
        .collect();
    assert_eq!(changed_lines.len(), 2);
    let before_head = tmp.git(&["rev-parse", "HEAD"]);
    let before_index = tmp.git(&["hash-object", ".git/index"]);
    let a_before = std::fs::read(tmp.path().join("a.txt")).unwrap();
    std::fs::write(
        tmp.path().join("b.txt"),
        lines(40).with(5, "b concurrent edit").to_string(),
    )
    .unwrap();
    let b_after = std::fs::read(tmp.path().join("b.txt")).unwrap();

    let result = test.invoke_at_version(
        "status.stage",
        version,
        None,
        Some(Selection {
            start_line: changed_lines[0],
            end_line: changed_lines[1],
        }),
        None,
    );
    assert!(message_texts(&result.effects)
        .iter()
        .any(|text| text.contains("changed")));
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
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", original)])
        .modify("f.txt", initial)
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let version = test.version();
    let changed_line = test
        .tree()
        .nodes
        .iter()
        .find(|node| node.kind == magit_lsp::tree::NodeKind::Line && node.marker == Some('+'))
        .unwrap()
        .start_line;
    let mut shifted: Vec<_> = (1..=100).map(|n| format!("line{n}")).collect();
    shifted.insert(10, "new unrelated line".to_string());
    shifted[80] = "line80 changed".to_string();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    std::fs::write(tmp.path().join("f.txt"), shifted.join("\n") + "\n").unwrap();

    let result = test.invoke_at_version(
        "status.stage",
        version,
        None,
        Some(Selection {
            start_line: changed_line,
            end_line: changed_line,
        }),
        None,
    );
    assert!(!message_texts(&result.effects)
        .iter()
        .any(|text| text.contains("changed")));
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
    fn visit(
        root: &std::path::Path,
        path: &std::path::Path,
        out: &mut Vec<(std::path::PathBuf, u32, Vec<u8>)>,
    ) {
        let metadata = std::fs::symlink_metadata(path).unwrap();
        let bytes = if metadata.file_type().is_symlink() {
            std::fs::read_link(path)
                .unwrap()
                .as_os_str()
                .as_encoded_bytes()
                .to_vec()
        } else if metadata.is_file() {
            std::fs::read(path).unwrap()
        } else {
            Vec::new()
        };
        out.push((
            path.strip_prefix(root).unwrap().to_path_buf(),
            metadata.permissions().mode(),
            bytes,
        ));
        if metadata.is_dir() {
            let mut children: Vec<_> = std::fs::read_dir(path)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect();
            children.sort();
            for child in children {
                if child.file_name().unwrap() != ".git" {
                    visit(root, &child, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    visit(path, path, &mut out);
    out
}

// Independent exact HEAD/index/worktree evidence, including an absent index
// and unborn HEAD. Never use production target hashes as the state oracle.
#[derive(Debug, PartialEq, Eq)]
struct TargetState {
    head: Option<Vec<u8>>,
    index: Option<Vec<u8>>,
    worktree: Vec<(std::path::PathBuf, u32, Vec<u8>)>,
}

fn target_state(tmp: &RepoFixture) -> TargetState {
    let head = std::process::Command::new("git")
        .current_dir(tmp.path())
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .unwrap();
    TargetState {
        head: head.status.success().then_some(head.stdout),
        index: std::fs::read(tmp.path().join(".git/index")).ok(),
        worktree: identity_worktree(tmp.path()),
    }
}

fn offered_request(
    test: &TestSession,
    command: &str,
    node_id: &str,
) -> magit_lsp::dispatch::DispatchRequest {
    use magit_lsp::dispatch::{offer_target, DispatchRequest, DispatchTarget};
    DispatchRequest {
        command: command.into(),
        target: DispatchTarget {
            view_id: test.view_id().into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            key: None,
        },
        action_target: Some(
            offer_target(test.tree(), command, node_id).expect("typed content target"),
        ),
        args: None,
    }
}

#[path = "commands/additional.rs"]
mod additional;
#[path = "commands/cases.rs"]
mod cases;
#[path = "commands/more.rs"]
mod more;
