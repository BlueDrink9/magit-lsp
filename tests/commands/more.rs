use super::additional::{change_identity_file, file_identity_matrix};
use super::*;

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
    for kind in [
        "binary",
        "mode",
        "added-mode",
        "type",
        "collapsed",
        "untracked",
        "untracked-link",
        "directory",
    ] {
        for accept in [false, true] {
            let (tmp, _) = RepoFixture::new()
                .commit("init", [("base.txt", "base\n")])
                .build();
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
            let line = test.goto_file(
                if untracked { "untracked" } else { "unstaged" },
                if kind == "directory" {
                    "f.txt/"
                } else {
                    "f.txt"
                },
            );
            let Execution::Prompt(prompt) = test
                .session
                .start_invoke(&ViewInvokeParams {
                    view_id: test.view_id().to_string(),
                    command: "status.discard".into(),
                    version: Some(test.version()),
                    cursor: Some(Cursor { line, column: 0 }),
                    selection: None,
                    args: None,
                })
                .unwrap()
            else {
                panic!("{kind}: expected confirmation");
            };
            if accept {
                change_identity_file(tmp.path(), kind, true);
            }
            let head = tmp.git(&["rev-parse", "HEAD"]);
            let index = std::fs::read(tmp.path().join(".git/index")).unwrap();
            let worktree = identity_worktree(tmp.path());
            let Execution::Done(result) = test
                .session
                .resume_prompt(&prompt.prompt_id, Some(PromptValue::Bool(accept)))
                .unwrap()
            else {
                panic!("{kind}: completion");
            };
            if accept {
                assert!(
                    message_texts(&result.effects)
                        .iter()
                        .any(|text| text.contains("changed")),
                    "{kind}"
                );
            }
            assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head, "{kind}");
            assert_eq!(
                std::fs::read(tmp.path().join(".git/index")).unwrap(),
                index,
                "{kind}"
            );
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
        let (tmp, _) = RepoFixture::new()
            .commit("init", [("a.bin", "base\0a"), ("b.bin", "base\0b")])
            .build();
        write(&tmp.path().join("a.bin"), "shown\0a");
        write(&tmp.path().join("b.bin"), "shown\0b");
        if action == "unstage" {
            tmp.git(&["add", "."]);
        }
        let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
        test.ack();
        let section = if action == "unstage" {
            "staged"
        } else {
            "unstaged"
        };
        let start_line = test.goto_file(section, "a.bin");
        let end_line = test.goto_file(section, "b.bin");
        let version = test.version();
        write(&tmp.path().join("b.bin"), "later\0b");
        if action == "unstage" {
            tmp.git(&["add", "b.bin"]);
        }
        let head = tmp.git(&["rev-parse", "HEAD"]);
        let index = std::fs::read(tmp.path().join(".git/index")).unwrap();
        let worktree = identity_worktree(tmp.path());
        let step = test
            .session
            .start_invoke(&ViewInvokeParams {
                view_id: test.view_id().to_string(),
                command: format!("status.{action}"),
                version: Some(version),
                cursor: None,
                selection: Some(Selection {
                    start_line,
                    end_line,
                }),
                args: None,
            })
            .unwrap();
        let result = match step {
            Execution::Done(result) => result,
            Execution::Prompt(prompt) => {
                let Execution::Done(result) = test
                    .session
                    .resume_prompt(&prompt.prompt_id, Some(PromptValue::Bool(true)))
                    .unwrap()
                else {
                    panic!("expected completion");
                };
                result
            }
            Execution::Pick(pick) => panic!("unexpected pick for {action}: {pick:?}"),
        };
        assert!(
            message_texts(&result.effects)
                .iter()
                .any(|text| text.contains("changed")),
            "{action}"
        );
        assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head, "{action}");
        assert_eq!(
            std::fs::read(tmp.path().join(".git/index")).unwrap(),
            index,
            "{action}"
        );
        assert_eq!(identity_worktree(tmp.path()), worktree, "{action}");
    }
}

#[test]
fn discarding_an_untracked_directory_symlink_never_deletes_its_target() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    use std::os::unix::fs::symlink;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("target/kept", "keep\n")])
        .build();
    symlink("target", tmp.path().join("link")).unwrap();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let line = test.goto_file("untracked", "link");
    let Execution::Prompt(prompt) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "status.discard".into(),
            version: Some(test.version()),
            cursor: Some(Cursor { line, column: 0 }),
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected confirmation");
    };
    test.session
        .resume_prompt(&prompt.prompt_id, Some(PromptValue::Bool(true)))
        .unwrap();
    assert!(std::fs::symlink_metadata(tmp.path().join("link")).is_err());
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("target/kept")).unwrap(),
        "keep\n"
    );
    assert_eq!(tmp.git(&["status", "--porcelain"]), "");
}

#[test]
fn renamed_file_unstage_validates_the_source_index_but_ignores_source_worktree_edits() {
    for stale in [false, true] {
        let (tmp, _) = RepoFixture::new()
            .commit("init", [("old.txt", "rename bytes\n")])
            .rename("old.txt", "new.txt")
            .build();
        let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
        test.ack();
        let line = test.goto_file("staged", "new.txt");
        let version = test.version();
        write(&tmp.path().join("old.txt"), "external source\n");
        if stale {
            tmp.git(&["add", "old.txt"]);
        }
        let head = tmp.git(&["rev-parse", "HEAD"]);
        let index = std::fs::read(tmp.path().join(".git/index")).unwrap();
        let worktree = identity_worktree(tmp.path());
        let result = test.invoke_at_version(
            "status.unstage",
            version,
            Some(Cursor { line, column: 0 }),
            None,
            None,
        );
        assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
        assert_eq!(identity_worktree(tmp.path()), worktree);
        if stale {
            assert!(message_texts(&result.effects)
                .iter()
                .any(|text| text.contains("changed")));
            assert_eq!(std::fs::read(tmp.path().join(".git/index")).unwrap(), index);
        } else {
            assert!(message_texts(&result.effects).is_empty());
            assert_eq!(tmp.git(&["ls-files"]), "old.txt\n");
            assert_eq!(tmp.git(&["show", ":old.txt"]), "rename bytes\n");
            assert_eq!(
                tmp.git(&["write-tree"]),
                tmp.git(&["rev-parse", "HEAD^{tree}"])
            );
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

#[test]
fn rich_dispatch_opens_commit_menu_from_log_without_widening_status_scope() {
    let (tmp, _) = RepoFixture::new()
        .commit("first log entry", [("f.txt", TEN_LINES)])
        .commit("second log entry", [("f.txt", "changed\n")])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    test.press("g l");
    test.invoke("log.current");
    let log = test.text_of(test.view_id()).join("\n");
    assert!(
        log.contains("first log entry"),
        "expected concrete log before dispatch: {log}"
    );
    let log_id = test.view_id().to_owned();
    let refused = test.invoke("status.stage");
    assert!(message_texts(&refused.effects)
        .iter()
        .any(|message| message.contains("Not available")));
    assert_eq!(test.view_id(), log_id);

    let (commit, _) = test.open_via_command("open.commit");
    assert!(test.text_of(&commit.id).join("\n").contains("git commit"));
}

/// Confirm on a select-and-copy picker delivers the full structured commit id
/// and restores the caller that started the selection.
#[test]
fn select_mode_confirm_copies_the_full_oid_and_restores_the_caller() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("old commit", [("f.txt", "old\n")])
        .commit("new commit", [("f.txt", "new\n")])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Standard, WIDE_VIEWPORT);
    test.ack();
    let caller = test.view_id().to_string();
    test.invoke("log.select-and-copy");
    let picker = test.view_id().to_string();
    test.ack();
    let (oid, line) = {
        let node = test
            .tree()
            .nodes
            .iter()
            .find(|node| node.kind == magit_lsp::tree::NodeKind::Commit)
            .expect("a commit row");
        (
            node.commit.clone().expect("the full commit id"),
            node.start_line,
        )
    };
    let result = test.invoke_at("common.confirm", Cursor { line, column: 0 });
    assert!(
        result
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Clipboard { text } if text == &oid)),
        "expected the full {} oid in {:?}",
        oid,
        result.effects
    );
    assert!(result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::PopView { view_id } if view_id == &picker)));
    assert!(result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::FocusView { view_id } if view_id == &caller)));
    assert_eq!(test.view_id(), caller);
    assert!(!test.view_ids().contains(&picker));
}

/// Cancel on a select-and-copy picker restores the caller and changes nothing.
#[test]
fn select_mode_cancel_restores_the_caller_without_mutation() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("old commit", [("f.txt", "old\n")])
        .commit("new commit", [("f.txt", "new\n")])
        .build();
    let before_head = tmp.git(&["rev-parse", "HEAD"]);
    let before_status = tmp.git(&["status", "--porcelain"]);
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Standard, WIDE_VIEWPORT);
    test.ack();
    let caller = test.view_id().to_string();
    test.invoke("log.select-and-copy");
    let picker = test.view_id().to_string();
    test.ack();
    let result = test.invoke("common.cancel");
    assert!(result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::PopView { view_id } if view_id == &picker)));
    assert!(result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::FocusView { view_id } if view_id == &caller)));
    assert_eq!(test.view_id(), caller);
    assert!(!test.view_ids().contains(&picker));
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), before_head);
    assert_eq!(tmp.git(&["status", "--porcelain"]), before_status);
}

/// A commit that vanished from the repository is refused readably: the picker
/// stays open with the caller intact and nothing is copied.
#[test]
fn select_mode_stale_selection_keeps_the_picker_and_refuses() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("old commit", [("f.txt", "old\n")])
        .commit("new commit", [("f.txt", "new\n")])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Standard, WIDE_VIEWPORT);
    test.ack();
    let caller = test.view_id().to_string();
    test.invoke("log.select-and-copy");
    let picker = test.view_id().to_string();
    test.ack();
    // The oldest row: deleting the newest commit shortens the refreshed tree,
    // so the stale cursor no longer resolves to any loaded commit.
    let line = test
        .tree()
        .nodes
        .iter()
        .rev()
        .find(|node| node.kind == magit_lsp::tree::NodeKind::Commit)
        .expect("a commit row")
        .start_line;
    tmp.git(&["reset", "--hard", "HEAD~1"]);
    tmp.git(&["reflog", "expire", "--expire=now", "--all"]);
    tmp.git(&["gc", "--prune=now"]);
    test.refresh();
    test.ack();
    let result = test.invoke_at("common.confirm", Cursor { line, column: 0 });
    let messages: Vec<_> = result
        .effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Message { level, text, .. } => Some((*level, text.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(messages.len(), 1, "{:?}", result.effects);
    assert_eq!(messages[0].0, MessageLevel::Info, "{:?}", result.effects);
    assert_eq!(
        messages[0].1, "Place the cursor on a commit to confirm the selection",
        "{:?}",
        result.effects
    );
    assert!(!result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::PopView { .. })));
    assert!(!result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::FocusView { .. })));
    assert!(!result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::Clipboard { .. })));
    assert_eq!(test.view_id(), picker);
    assert!(test.view_ids().contains(&picker));
    assert!(test.view_ids().contains(&caller));
}

#[test]
fn branch_create_tracks_local_branch_startpoint_without_switching() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .commit("second", [("second.txt", "second\n")])
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Prompt(start) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "branch.create".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected starting point prompt");
    };
    let Execution::Prompt(name) = test
        .session
        .resume_prompt(&start.prompt_id, Some(PromptValue::Text("main".into())))
        .unwrap()
    else {
        panic!("expected branch name prompt after the starting point");
    };
    let Execution::Done(result) = test
        .session
        .resume_prompt(&name.prompt_id, Some(PromptValue::Text("feature".into())))
        .unwrap()
    else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&result.effects)
            .iter()
            .any(|text| *text == "Created branch feature"),
        "{:?}",
        result.effects
    );
    let main_oid = tmp.git(&["rev-parse", "main"]).trim().to_owned();
    assert_eq!(
        tmp.git(&["rev-parse", "feature"]).trim(),
        main_oid,
        "the branch must start at the startpoint's commit"
    );
    assert_eq!(
        tmp.git(&["config", "--get", "branch.feature.merge"]).trim(),
        "refs/heads/main",
        "a local-branch startpoint must become the new branch's upstream"
    );
    assert_eq!(
        tmp.git(&["config", "--get", "branch.feature.remote"])
            .trim(),
        ".",
        "a local-branch startpoint tracks a local branch"
    );
    assert!(
        tmp.git(&["symbolic-ref", "HEAD"]).trim().ends_with("main"),
        "create must not switch HEAD"
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn branch_create_from_revision_startpoint_does_not_track() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .commit("second", [("second.txt", "second\n")])
        .build();
    let earlier = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let head = tmp.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Prompt(start) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "branch.create".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected starting point prompt");
    };
    let Execution::Prompt(name) = test
        .session
        .resume_prompt(&start.prompt_id, Some(PromptValue::Text("HEAD~1".into())))
        .unwrap()
    else {
        panic!("expected branch name prompt after the starting point");
    };
    let Execution::Done(result) = test
        .session
        .resume_prompt(&name.prompt_id, Some(PromptValue::Text("feature".into())))
        .unwrap()
    else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&result.effects)
            .iter()
            .any(|text| *text == "Created branch feature"),
        "{:?}",
        result.effects
    );
    assert_eq!(
        tmp.git(&["rev-parse", "feature"]).trim(),
        earlier,
        "a revision startpoint must be the new branch's commit"
    );
    assert_ne!(earlier, head, "fixture must have two distinct commits");
    assert_eq!(
        tmp.git(&[
            "for-each-ref",
            "--format=%(upstream:short)",
            "refs/heads/feature",
        ])
        .trim(),
        "",
        "a revision startpoint must not set an upstream"
    );
    assert!(
        tmp.git(&["symbolic-ref", "HEAD"]).trim().ends_with("main"),
        "create must not switch HEAD"
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn branch_create_refuses_option_like_answers() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let before = tmp.git(&["for-each-ref", "--format=%(refname:short)", "refs/heads/"]);
    let head = tmp.git(&["symbolic-ref", "HEAD"]);

    let Execution::Prompt(start) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "branch.create".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected starting point prompt");
    };
    let Execution::Done(refused_start) = test
        .session
        .resume_prompt(&start.prompt_id, Some(PromptValue::Text("-D".into())))
        .unwrap()
    else {
        panic!("an option-like starting point must be refused");
    };
    assert!(
        message_texts(&refused_start.effects)
            .iter()
            .any(|text| text.contains("cannot begin with '-'")),
        "{:?}",
        refused_start.effects
    );
    assert!(!test.session.has_pending_prompt());

    // The option-like name is refused at the Git boundary after the startpoint.
    let Execution::Prompt(start) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "branch.create".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected starting point prompt");
    };
    let Execution::Prompt(name) = test
        .session
        .resume_prompt(&start.prompt_id, Some(PromptValue::Text("HEAD".into())))
        .unwrap()
    else {
        panic!("expected branch name prompt after the starting point");
    };
    let Execution::Done(refused_name) = test
        .session
        .resume_prompt(&name.prompt_id, Some(PromptValue::Text("-D".into())))
        .unwrap()
    else {
        panic!("an option-like branch name must be refused");
    };
    assert!(
        refused_name.effects.iter().any(|effect| matches!(
            effect,
            Effect::Message {
                level: MessageLevel::Warn,
                ..
            }
        )),
        "{:?}",
        refused_name.effects
    );
    assert_eq!(
        tmp.git(&["for-each-ref", "--format=%(refname:short)", "refs/heads/"]),
        before,
        "refused answers must not create a branch"
    );
    assert_eq!(
        tmp.git(&["symbolic-ref", "HEAD"]),
        head,
        "refused answers must not move HEAD"
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn tag_create_prompts_and_creates_tag_at_head() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    tmp.git(&["config", "tag.gpgSign", "true"]);
    tmp.git(&["config", "core.editor", "false"]);
    tmp.git(&["config", "gpg.program", "false"]);
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Prompt(prompt) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "tag.create".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected tag name prompt");
    };
    let Execution::Prompt(revision) = test
        .session
        .resume_prompt(&prompt.prompt_id, Some(PromptValue::Text("v1".into())))
        .unwrap()
    else {
        panic!("expected revision prompt");
    };
    assert_eq!(revision.default, Some(serde_json::json!("HEAD")));
    let Execution::Done(result) = test
        .session
        .resume_prompt(&revision.prompt_id, Some(PromptValue::Text("HEAD".into())))
        .unwrap()
    else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&result.effects)
            .iter()
            .any(|text| *text == "Created tag v1"),
        "{:?}",
        result.effects
    );
    let tags = tmp.git(&["tag", "--list"]);
    assert!(tags.lines().any(|line| line == "v1"), "{tags}");
    assert_eq!(
        tmp.git(&["cat-file", "-t", "v1"]).trim(),
        "commit",
        "a signing default must not turn the tag into an annotated object"
    );
    assert_eq!(
        tmp.git(&["rev-parse", "v1"]).trim(),
        head.trim(),
        "the lightweight tag must point at the captured HEAD"
    );
    assert_eq!(tmp.git(&["config", "tag.gpgSign"]).trim(), "true");
    assert_eq!(tmp.git(&["config", "gpg.program"]).trim(), "false");
    assert!(
        tmp.git(&["symbolic-ref", "HEAD"]).trim().ends_with("main"),
        "creating a tag must not switch HEAD"
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn tag_create_refuses_selected_menu_arguments() {
    use magit_lsp::protocol::ViewInvokeParams;
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let refs = tmp.git(&["for-each-ref", "--format=%(refname)"]);
    let config = tmp.git(&["config", "--list", "--local"]);
    let Execution::Done(result) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "tag.create".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: Some(serde_json::json!({"switches": ["-s"], "options": {"=u": "Alice"}})),
        })
        .unwrap()
    else {
        panic!("selected unsupported arguments must refuse, not prompt");
    };
    let messages = message_texts(&result.effects);
    assert!(
        messages
            .iter()
            .any(|text| text.contains("tag.create") && text.contains("-s") && text.contains("=u")),
        "{messages:?}"
    );
    assert!(!test.session.has_pending_prompt());
    assert_eq!(
        tmp.git(&["for-each-ref", "--format=%(refname)"]),
        refs,
        "a refused tag create must not add a ref"
    );
    assert_eq!(
        tmp.git(&["config", "--list", "--local"]),
        config,
        "a refused tag create must not change local configuration"
    );
}

#[test]
fn tag_delete_removes_an_existing_tag() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    tmp.git(&["tag", "v1"]);
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Prompt(prompt) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "tag.delete".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected tag name prompt");
    };
    let Execution::Done(result) = test
        .session
        .resume_prompt(&prompt.prompt_id, Some(PromptValue::Text("v1".into())))
        .unwrap()
    else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&result.effects)
            .iter()
            .any(|text| *text == "Deleted tag v1"),
        "{:?}",
        result.effects
    );
    let tags = tmp.git(&["tag", "--list"]);
    assert!(!tags.lines().any(|line| line == "v1"), "{tags}");
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn tag_delete_of_missing_tag_surfaces_gits_refusal() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Prompt(prompt) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "tag.delete".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected tag name prompt");
    };
    let Execution::Done(result) = test
        .session
        .resume_prompt(&prompt.prompt_id, Some(PromptValue::Text("v9".into())))
        .unwrap()
    else {
        panic!("expected completion");
    };
    let texts = message_texts(&result.effects);
    assert!(
        texts
            .iter()
            .any(|text| text.to_lowercase().contains("not found")),
        "{:?}",
        result.effects
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn branch_rename_prompts_twice_and_moves_the_ref() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Prompt(first) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "branch.rename".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected branch name prompt");
    };
    let Execution::Prompt(second) = test
        .session
        .resume_prompt(&first.prompt_id, Some(PromptValue::Text("main".into())))
        .unwrap()
    else {
        panic!("expected rename target prompt");
    };
    let Execution::Done(result) = test
        .session
        .resume_prompt(&second.prompt_id, Some(PromptValue::Text("renamed".into())))
        .unwrap()
    else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&result.effects)
            .iter()
            .any(|text| *text == "Renamed branch main to renamed"),
        "{:?}",
        result.effects
    );
    let branches = tmp.git(&["for-each-ref", "--format=%(refname:short)", "refs/heads/"]);
    assert!(!branches.lines().any(|line| line == "main"), "{branches}");
    assert!(branches.lines().any(|line| line == "renamed"), "{branches}");
    assert!(
        tmp.git(&["symbolic-ref", "HEAD"])
            .trim()
            .ends_with("renamed"),
        "rename must move the checked-out ref"
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn branch_delete_removes_a_side_branch_without_switching() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    tmp.git(&["branch", "feature"]);
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Prompt(prompt) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "branch.delete".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected branch name prompt");
    };
    let Execution::Done(result) = test
        .session
        .resume_prompt(&prompt.prompt_id, Some(PromptValue::Text("feature".into())))
        .unwrap()
    else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&result.effects)
            .iter()
            .any(|text| *text == "Deleted branch feature"),
        "{:?}",
        result.effects
    );
    let branches = tmp.git(&["for-each-ref", "--format=%(refname:short)", "refs/heads/"]);
    assert!(
        !branches.lines().any(|line| line == "feature"),
        "{branches}"
    );
    assert!(
        tmp.git(&["symbolic-ref", "HEAD"]).trim().ends_with("main"),
        "delete must leave main checked out"
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn branch_delete_of_current_branch_surfaces_gits_refusal() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Prompt(prompt) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "branch.delete".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected branch name prompt");
    };
    let Execution::Done(result) = test
        .session
        .resume_prompt(&prompt.prompt_id, Some(PromptValue::Text("main".into())))
        .unwrap()
    else {
        panic!("expected completion");
    };
    let texts = message_texts(&result.effects);
    assert!(
        texts.iter().any(|text| {
            let text = text.to_lowercase();
            text.contains("cannot delete branch") && text.contains("main")
        }),
        "{:?}",
        result.effects
    );
    let branches = tmp.git(&["for-each-ref", "--format=%(refname:short)", "refs/heads/"]);
    assert!(
        branches.lines().any(|line| line == "main"),
        "Git's refusal must keep the checked-out branch: {branches}"
    );
}

#[test]
fn branch_description_prompts_twice_and_writes_config() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Prompt(first) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "branch.configure-description".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected branch name prompt");
    };
    let Execution::Prompt(second) = test
        .session
        .resume_prompt(&first.prompt_id, Some(PromptValue::Text("main".into())))
        .unwrap()
    else {
        panic!("expected description prompt");
    };
    let Execution::Done(result) = test
        .session
        .resume_prompt(
            &second.prompt_id,
            Some(PromptValue::Text("tidy work".into())),
        )
        .unwrap()
    else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&result.effects)
            .iter()
            .any(|text| *text == "Set description for main"),
        "{:?}",
        result.effects
    );
    assert_eq!(
        tmp.git(&["config", "branch.main.description"]).trim(),
        "tidy work"
    );
    // An option-like answer must be stored literally. Seed a description first
    // so a misparsed `git config ... --unset` would visibly delete it.
    tmp.git(&["config", "branch.main.description", "seeded description"]);
    tmp.git(&["config", "--local", "magit.test.marker", "keep"]);
    let refs_before = tmp.git(&["for-each-ref"]);
    let mut describe = |answer: &str| {
        let Execution::Prompt(first) = test
            .session
            .start_invoke(&ViewInvokeParams {
                view_id: test.view_id().to_string(),
                command: "branch.configure-description".into(),
                version: Some(test.version()),
                cursor: None,
                selection: None,
                args: None,
            })
            .unwrap()
        else {
            panic!("expected branch name prompt");
        };
        let Execution::Prompt(second) = test
            .session
            .resume_prompt(&first.prompt_id, Some(PromptValue::Text("main".into())))
            .unwrap()
        else {
            panic!("expected description prompt");
        };
        let Execution::Done(result) = test
            .session
            .resume_prompt(&second.prompt_id, Some(PromptValue::Text(answer.into())))
            .unwrap()
        else {
            panic!("expected completion");
        };
        message_texts(&result.effects)
            .iter()
            .any(|text| *text == "Set description for main")
    };
    for answer in ["--unset", "--get"] {
        assert!(describe(answer), "{answer} must report success");
        assert_eq!(
            tmp.git(&["config", "branch.main.description"]).trim(),
            answer,
            "an option-like description must be stored literally"
        );
    }
    assert_eq!(
        tmp.git(&["for-each-ref"]),
        refs_before,
        "descriptions must not change refs"
    );
    assert_eq!(
        tmp.git(&["config", "--local", "magit.test.marker"]).trim(),
        "keep",
        "unrelated local configuration must be unchanged"
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn branch_configuration_workflow_targets_selected_branch_and_repository() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    tmp.git(&["branch", "feature"]);
    tmp.git(&["remote", "add", "origin", "https://example.invalid/repo"]);
    let refs_before = tmp.git(&["for-each-ref"]);
    let local_list = || tmp.git(&["config", "--local", "--list"]);
    let has_key = |list: &str, key: &str| list.lines().any(|line| line.starts_with(key));
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let status = test.view_id().to_string();
    let start = || ViewInvokeParams {
        view_id: String::new(),
        command: "branch.configure".into(),
        version: None,
        cursor: None,
        selection: None,
        args: None,
    };
    let invoke = |test: &mut TestSession| {
        let mut request = start();
        request.view_id = test.view_id().to_string();
        request.version = Some(test.version());
        test.session.start_invoke(&request).unwrap()
    };
    let configure_branch =
        |test: &mut TestSession, setting: &str, branch: &str, value: &str, current: &str| {
            let Execution::Prompt(setting_prompt) = invoke(test) else {
                panic!("expected setting prompt");
            };
            let Execution::Prompt(branch_prompt) = test
                .session
                .resume_prompt(
                    &setting_prompt.prompt_id,
                    Some(PromptValue::Text(setting.into())),
                )
                .unwrap()
            else {
                panic!("expected branch prompt");
            };
            let Execution::Prompt(value_prompt) = test
                .session
                .resume_prompt(
                    &branch_prompt.prompt_id,
                    Some(PromptValue::Text(branch.into())),
                )
                .unwrap()
            else {
                panic!("expected value prompt");
            };
            assert!(
                value_prompt.title.contains(&format!("current: {current}")),
                "{}",
                value_prompt.title
            );
            test.session
                .resume_prompt(
                    &value_prompt.prompt_id,
                    Some(PromptValue::Text(value.into())),
                )
                .unwrap()
        };
    let configure_repository =
        |test: &mut TestSession, setting: &str, value: &str, current: &str| {
            let Execution::Prompt(first) = invoke(test) else {
                panic!("expected setting prompt");
            };
            let Execution::Prompt(second) = test
                .session
                .resume_prompt(&first.prompt_id, Some(PromptValue::Text(setting.into())))
                .unwrap()
            else {
                panic!("expected repository value prompt");
            };
            assert!(
                second.title.contains(&format!("current: {current}")),
                "{}",
                second.title
            );
            test.session
                .resume_prompt(&second.prompt_id, Some(PromptValue::Text(value.into())))
                .unwrap()
        };

    let unchanged = local_list();
    let Execution::Prompt(initial) = invoke(&mut test) else {
        panic!("expected configuration prompt");
    };
    let detail = initial.detail.as_deref().unwrap_or_default();
    assert!(
        detail.contains("main: upstream=unset, rebase=unset, pushRemote=unset"),
        "{detail}"
    );
    assert!(detail.contains("pull.rebase=unset"), "{detail}");
    assert!(detail.contains("push.default=unset"), "{detail}");
    let Execution::Done(cancelled) = test
        .session
        .resume_prompt(&initial.prompt_id, None)
        .unwrap()
    else {
        panic!("cancellation must end the prompt");
    };
    assert!(
        message_texts(&cancelled.effects).is_empty(),
        "{:?}",
        cancelled.effects
    );
    assert_eq!(local_list(), unchanged);

    test.ui()
        .reply_text(Some("rebase".into()))
        .reply_text(Some("feature".into()))
        .reply_text(Some("true".into()));
    test.press_ui("b");
    test.press_ui("C");
    let requests = test.ui().requests();
    assert!(
        requests
            .iter()
            .any(|request| request.title.starts_with("Configure setting")),
        "{requests:?}"
    );
    let key_value_prompt = requests
        .iter()
        .find(|request| request.title.contains("rebase for feature"))
        .expect("the key-driven value prompt");
    assert!(
        key_value_prompt.title.contains("current: unset"),
        "{}",
        key_value_prompt.title
    );
    assert_eq!(tmp.git(&["config", "branch.feature.rebase"]).trim(), "true");
    assert_eq!(tmp.git(&["symbolic-ref", "--short", "HEAD"]).trim(), "main");
    assert!(!has_key(&local_list(), "branch.main."));
    test.focus(&status);

    let _ = configure_branch(&mut test, "upstream", "feature", "main", "unset");
    let _ = configure_branch(&mut test, "pushRemote", "feature", "origin", "unset");
    assert_eq!(tmp.git(&["config", "branch.feature.remote"]).trim(), ".");
    assert_eq!(
        tmp.git(&["config", "branch.feature.merge"]).trim(),
        "refs/heads/main"
    );
    assert_eq!(tmp.git(&["config", "branch.feature.rebase"]).trim(), "true");
    assert_eq!(
        tmp.git(&["config", "branch.feature.pushRemote"]).trim(),
        "origin"
    );
    assert_eq!(tmp.git(&["symbolic-ref", "--short", "HEAD"]).trim(), "main");
    let _ = configure_branch(&mut test, "upstream", "feature", "unset", "main");
    let _ = configure_branch(&mut test, "rebase", "feature", "unset", "true");
    let _ = configure_branch(&mut test, "pushRemote", "feature", "unset", "origin");
    let cleared = local_list();
    for key in [
        "branch.feature.remote=",
        "branch.feature.merge=",
        "branch.feature.rebase=",
        "branch.feature.pushRemote=",
    ] {
        assert!(!has_key(&cleared, key), "{key} must be cleared");
    }
    assert!(!has_key(&cleared, "branch.main."));
    assert_eq!(tmp.git(&["symbolic-ref", "--short", "HEAD"]).trim(), "main");
    assert_eq!(tmp.git(&["for-each-ref"]), refs_before);

    let _ = configure_repository(&mut test, "pull.rebase", "true", "unset");
    let _ = configure_repository(&mut test, "push.default", "current", "unset");
    assert_eq!(
        tmp.git(&["config", "--local", "pull.rebase"]).trim(),
        "true"
    );
    assert_eq!(
        tmp.git(&["config", "--local", "push.default"]).trim(),
        "current"
    );
    let Execution::Prompt(current_values) = invoke(&mut test) else {
        panic!("expected configuration prompt");
    };
    let detail = current_values.detail.as_deref().unwrap_or_default();
    assert!(detail.contains("pull.rebase=true"), "{detail}");
    assert!(detail.contains("push.default=current"), "{detail}");
    let _ = test
        .session
        .resume_prompt(&current_values.prompt_id, None)
        .unwrap();
    let _ = configure_repository(&mut test, "pull.rebase", "unset", "true");
    let _ = configure_repository(&mut test, "push.default", "unset", "current");
    let final_list = local_list();
    assert!(!has_key(&final_list, "pull.rebase="));
    assert!(!has_key(&final_list, "push.default="));

    let before_invalid = local_list();
    let Execution::Prompt(first) = invoke(&mut test) else {
        panic!("expected setting prompt");
    };
    let Execution::Done(invalid_setting) = test
        .session
        .resume_prompt(&first.prompt_id, Some(PromptValue::Text("bogus".into())))
        .unwrap()
    else {
        panic!("an invalid setting must end the prompt");
    };
    assert!(
        message_texts(&invalid_setting.effects)
            .iter()
            .any(|text| text.contains("Choose upstream")),
        "{:?}",
        invalid_setting.effects
    );
    assert_eq!(local_list(), before_invalid);

    let Execution::Prompt(first) = invoke(&mut test) else {
        panic!("expected setting prompt");
    };
    let Execution::Prompt(second) = test
        .session
        .resume_prompt(&first.prompt_id, Some(PromptValue::Text("rebase".into())))
        .unwrap()
    else {
        panic!("expected branch prompt");
    };
    let Execution::Done(missing_branch) = test
        .session
        .resume_prompt(&second.prompt_id, Some(PromptValue::Text("missing".into())))
        .unwrap()
    else {
        panic!("a missing branch must end the prompt");
    };
    assert!(
        message_texts(&missing_branch.effects)
            .iter()
            .any(|text| text.contains("Local branch does not exist: missing")),
        "{:?}",
        missing_branch.effects
    );
    assert_eq!(local_list(), before_invalid);

    let Execution::Prompt(first) = invoke(&mut test) else {
        panic!("expected setting prompt");
    };
    let Execution::Prompt(second) = test
        .session
        .resume_prompt(&first.prompt_id, Some(PromptValue::Text("rebase".into())))
        .unwrap()
    else {
        panic!("expected branch prompt");
    };
    let Execution::Prompt(third) = test
        .session
        .resume_prompt(&second.prompt_id, Some(PromptValue::Text("feature".into())))
        .unwrap()
    else {
        panic!("expected value prompt");
    };
    let Execution::Done(invalid_value) = test
        .session
        .resume_prompt(&third.prompt_id, Some(PromptValue::Text("maybe".into())))
        .unwrap()
    else {
        panic!("an invalid value must end the prompt");
    };
    assert!(
        message_texts(&invalid_value.effects)
            .iter()
            .any(|text| text.contains("Expected true, false, or unset")),
        "{:?}",
        invalid_value.effects
    );
    assert_eq!(local_list(), before_invalid);

    let Execution::Prompt(first) = invoke(&mut test) else {
        panic!("expected setting prompt");
    };
    let Execution::Prompt(second) = test
        .session
        .resume_prompt(&first.prompt_id, Some(PromptValue::Text("upstream".into())))
        .unwrap()
    else {
        panic!("expected branch prompt");
    };
    let Execution::Done(cancelled_branch) =
        test.session.resume_prompt(&second.prompt_id, None).unwrap()
    else {
        panic!("cancellation must end the branch prompt");
    };
    assert!(message_texts(&cancelled_branch.effects).is_empty());
    assert_eq!(local_list(), before_invalid);

    let Execution::Prompt(first) = invoke(&mut test) else {
        panic!("expected setting prompt");
    };
    let Execution::Prompt(second) = test
        .session
        .resume_prompt(
            &first.prompt_id,
            Some(PromptValue::Text("pull.rebase".into())),
        )
        .unwrap()
    else {
        panic!("expected repository value prompt");
    };
    let Execution::Done(cancelled_value) =
        test.session.resume_prompt(&second.prompt_id, None).unwrap()
    else {
        panic!("cancellation must end the value prompt");
    };
    assert!(message_texts(&cancelled_value.effects).is_empty());
    assert_eq!(local_list(), before_invalid);
    assert_eq!(tmp.git(&["for-each-ref"]), refs_before);
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn branch_configuration_clear_lock_failure_is_reported_without_changes() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    tmp.git(&["branch", "feature"]);
    tmp.git(&["config", "branch.feature.rebase", "true"]);
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let start = || ViewInvokeParams {
        view_id: String::new(),
        command: "branch.configure".into(),
        version: None,
        cursor: None,
        selection: None,
        args: None,
    };
    let invoke = |test: &mut TestSession| {
        let mut request = start();
        request.view_id = test.view_id().to_string();
        request.version = Some(test.version());
        test.session.start_invoke(&request).unwrap()
    };
    let Execution::Prompt(setting) = invoke(&mut test) else {
        panic!("expected setting prompt")
    };
    let Execution::Prompt(branch) = test
        .session
        .resume_prompt(&setting.prompt_id, Some(PromptValue::Text("rebase".into())))
        .unwrap()
    else {
        panic!("expected branch prompt")
    };
    let Execution::Prompt(value) = test
        .session
        .resume_prompt(&branch.prompt_id, Some(PromptValue::Text("feature".into())))
        .unwrap()
    else {
        panic!("expected value prompt")
    };
    assert!(value.title.contains("current: true"), "{}", value.title);
    std::fs::write(tmp.path().join(".git/config.lock"), "locked").unwrap();
    let Execution::Done(result) = test
        .session
        .resume_prompt(&value.prompt_id, Some(PromptValue::Text("unset".into())))
        .unwrap()
    else {
        panic!("clear must finish")
    };
    assert!(
        format!("{:?}", result.effects)
            .to_ascii_lowercase()
            .contains("lock"),
        "{:?}",
        result.effects
    );
    assert!(
        !message_texts(&result.effects)
            .iter()
            .any(|text| text.contains("Configured")),
        "{:?}",
        result.effects
    );
    std::fs::remove_file(tmp.path().join(".git/config.lock")).unwrap();
    assert_eq!(tmp.git(&["config", "branch.feature.rebase"]).trim(), "true");
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
}

#[test]
fn branch_configuration_read_failure_is_not_shown_as_unset() {
    use magit_lsp::protocol::ViewInvokeParams;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let config = tmp.path().join(".git/config");
    std::fs::write(&config, "[broken\n").unwrap();
    let request = ViewInvokeParams {
        view_id: test.view_id().to_string(),
        command: "branch.configure".into(),
        version: Some(test.version()),
        cursor: None,
        selection: None,
        args: None,
    };
    let error = test.session.start_invoke(&request).unwrap_err();
    assert!(error.to_string().contains("bad config line"), "{error}");
    assert_eq!(std::fs::read_to_string(&config).unwrap(), "[broken\n");
    assert!(!config.with_extension("lock").exists());
}

#[test]
fn branch_configuration_handles_detached_and_unborn_contexts() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (detached_tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .detached()
        .build();
    let head = detached_tmp.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let (mut test, _) = TestSession::open_status(&detached_tmp, Profile::Vim);
    test.ack();
    let start = || ViewInvokeParams {
        view_id: String::new(),
        command: "branch.configure".into(),
        version: None,
        cursor: None,
        selection: None,
        args: None,
    };
    let invoke = |test: &mut TestSession| {
        let mut request = start();
        request.view_id = test.view_id().to_string();
        request.version = Some(test.version());
        test.session.start_invoke(&request).unwrap()
    };
    let Execution::Prompt(initial) = invoke(&mut test) else {
        panic!("expected configuration prompt on detached HEAD");
    };
    let detail = initial.detail.as_deref().unwrap_or_default();
    assert!(
        detail.contains("No current local branch (detached or unborn)"),
        "{detail}"
    );
    assert!(detail.contains("Repository: pull.rebase=unset"), "{detail}");
    let _ = test
        .session
        .resume_prompt(&initial.prompt_id, None)
        .unwrap();

    let Execution::Prompt(first) = invoke(&mut test) else {
        panic!("expected setting prompt");
    };
    let Execution::Prompt(second) = test
        .session
        .resume_prompt(
            &first.prompt_id,
            Some(PromptValue::Text("pull.rebase".into())),
        )
        .unwrap()
    else {
        panic!("expected repository value prompt");
    };
    let _ = test
        .session
        .resume_prompt(&second.prompt_id, Some(PromptValue::Text("true".into())))
        .unwrap();
    assert_eq!(
        detached_tmp
            .git(&["config", "--local", "pull.rebase"])
            .trim(),
        "true"
    );

    let Execution::Prompt(first) = invoke(&mut test) else {
        panic!("expected setting prompt");
    };
    let Execution::Prompt(second) = test
        .session
        .resume_prompt(&first.prompt_id, Some(PromptValue::Text("rebase".into())))
        .unwrap()
    else {
        panic!("expected branch prompt");
    };
    let Execution::Prompt(third) = test
        .session
        .resume_prompt(&second.prompt_id, Some(PromptValue::Text("main".into())))
        .unwrap()
    else {
        panic!("expected value prompt");
    };
    let _ = test
        .session
        .resume_prompt(&third.prompt_id, Some(PromptValue::Text("false".into())))
        .unwrap();
    assert_eq!(
        detached_tmp.git(&["config", "branch.main.rebase"]).trim(),
        "false"
    );
    assert_eq!(detached_tmp.git(&["rev-parse", "HEAD"]).trim(), head);
    assert_eq!(
        detached_tmp
            .git(&["rev-parse", "--abbrev-ref", "HEAD"])
            .trim(),
        "HEAD"
    );

    let (unborn_tmp, _) = RepoFixture::new().unborn().build();
    let refs_before = unborn_tmp.git(&["for-each-ref"]);
    let (mut test, _) = TestSession::open_status(&unborn_tmp, Profile::Vim);
    test.ack();
    let invoke = |test: &mut TestSession| {
        let mut request = start();
        request.view_id = test.view_id().to_string();
        request.version = Some(test.version());
        test.session.start_invoke(&request).unwrap()
    };
    let Execution::Prompt(initial) = invoke(&mut test) else {
        panic!("expected configuration prompt on an unborn HEAD");
    };
    let detail = initial.detail.as_deref().unwrap_or_default();
    assert!(
        detail.contains("No current local branch (detached or unborn)"),
        "{detail}"
    );
    let _ = test
        .session
        .resume_prompt(&initial.prompt_id, None)
        .unwrap();

    let Execution::Prompt(first) = invoke(&mut test) else {
        panic!("expected setting prompt");
    };
    let Execution::Prompt(second) = test
        .session
        .resume_prompt(&first.prompt_id, Some(PromptValue::Text("rebase".into())))
        .unwrap()
    else {
        panic!("expected branch prompt");
    };
    let Execution::Done(missing_branch) = test
        .session
        .resume_prompt(&second.prompt_id, Some(PromptValue::Text("main".into())))
        .unwrap()
    else {
        panic!("an unborn branch target must end the prompt");
    };
    assert!(
        message_texts(&missing_branch.effects)
            .iter()
            .any(|text| text.contains("Local branch does not exist: main")),
        "{:?}",
        missing_branch.effects
    );
    assert!(!unborn_tmp
        .git(&["config", "--local", "--list"])
        .lines()
        .any(|line| line.starts_with("branch.main.")));

    let Execution::Prompt(first) = invoke(&mut test) else {
        panic!("expected setting prompt");
    };
    let Execution::Prompt(second) = test
        .session
        .resume_prompt(
            &first.prompt_id,
            Some(PromptValue::Text("pull.rebase".into())),
        )
        .unwrap()
    else {
        panic!("expected repository value prompt");
    };
    let _ = test
        .session
        .resume_prompt(&second.prompt_id, Some(PromptValue::Text("true".into())))
        .unwrap();
    assert_eq!(
        unborn_tmp.git(&["config", "--local", "pull.rebase"]).trim(),
        "true"
    );
    assert_eq!(unborn_tmp.git(&["for-each-ref"]), refs_before);
    let Execution::Prompt(final_prompt) = invoke(&mut test) else {
        panic!("expected configuration prompt");
    };
    assert!(final_prompt
        .detail
        .as_deref()
        .unwrap_or_default()
        .contains("pull.rebase=true"));
    let _ = test
        .session
        .resume_prompt(&final_prompt.prompt_id, None)
        .unwrap();
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn branch_description_targets_a_side_branch() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    tmp.git(&["branch", "feature"]);
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Prompt(first) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "branch.configure-description".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected branch name prompt");
    };
    let Execution::Prompt(second) = test
        .session
        .resume_prompt(&first.prompt_id, Some(PromptValue::Text("feature".into())))
        .unwrap()
    else {
        panic!("expected description prompt");
    };
    let Execution::Done(result) = test
        .session
        .resume_prompt(
            &second.prompt_id,
            Some(PromptValue::Text("side note".into())),
        )
        .unwrap()
    else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&result.effects)
            .iter()
            .any(|text| *text == "Set description for feature"),
        "{:?}",
        result.effects
    );
    assert_eq!(
        tmp.git(&["config", "branch.feature.description"]).trim(),
        "side note"
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn remote_add_prompts_for_name_and_url() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Prompt(prompt) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "remote.add".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected remote name prompt");
    };
    let Execution::Prompt(prompt) = test
        .session
        .resume_prompt(&prompt.prompt_id, Some(PromptValue::Text("origin".into())))
        .unwrap()
    else {
        panic!("expected remote URL prompt");
    };
    let Execution::Done(result) = test
        .session
        .resume_prompt(
            &prompt.prompt_id,
            Some(PromptValue::Text("/tmp/magit-remote-target".into())),
        )
        .unwrap()
    else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&result.effects)
            .iter()
            .any(|text| *text == "Added remote origin"),
        "{:?}",
        result.effects
    );
    let remotes = tmp.git(&["remote"]);
    assert!(remotes.lines().any(|line| line == "origin"), "{remotes}");
    assert_eq!(
        tmp.git(&["remote", "get-url", "origin"]).trim(),
        "/tmp/magit-remote-target"
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn remote_add_refuses_selected_menu_arguments() {
    use magit_lsp::protocol::ViewInvokeParams;
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let remotes = tmp.git(&["remote", "-v"]);
    let config = tmp.git(&["config", "--list", "--local"]);
    let Execution::Done(result) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "remote.add".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: Some(serde_json::json!({"switches": ["-f"], "options": {"=u": "file:///tmp/x"}})),
        })
        .unwrap()
    else {
        panic!("selected unsupported arguments must refuse, not prompt");
    };
    let messages = message_texts(&result.effects);
    assert!(
        messages
            .iter()
            .any(|text| text.contains("remote.add") && text.contains("-f") && text.contains("=u")),
        "{messages:?}"
    );
    assert!(!test.session.has_pending_prompt());
    assert_eq!(
        tmp.git(&["remote", "-v"]),
        remotes,
        "a refused remote add must not add or change a remote"
    );
    assert_eq!(
        tmp.git(&["config", "--list", "--local"]),
        config,
        "a refused remote add must not change local configuration"
    );
}

#[test]
fn remote_remove_drops_an_existing_remote() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    tmp.git(&["remote", "add", "origin", "/tmp/magit-remote-target"]);
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Prompt(prompt) = test
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
        panic!("expected remote name prompt");
    };
    let Execution::Prompt(confirmation) = test
        .session
        .resume_prompt(&prompt.prompt_id, Some(PromptValue::Text("origin".into())))
        .unwrap()
    else {
        panic!("expected removal confirmation");
    };
    let Execution::Done(result) = test
        .session
        .resume_prompt(&confirmation.prompt_id, Some(PromptValue::Bool(true)))
        .unwrap()
    else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&result.effects)
            .iter()
            .any(|text| *text == "Removed remote origin"),
        "{:?}",
        result.effects
    );
    let remotes = tmp.git(&["remote"]);
    assert!(!remotes.lines().any(|line| line == "origin"), "{remotes}");
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn remote_remove_of_missing_remote_surfaces_gits_refusal() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Prompt(prompt) = test
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
        panic!("expected remote name prompt");
    };
    let Execution::Prompt(confirmation) = test
        .session
        .resume_prompt(&prompt.prompt_id, Some(PromptValue::Text("v9".into())))
        .unwrap()
    else {
        panic!("expected removal confirmation");
    };
    let Execution::Done(result) = test
        .session
        .resume_prompt(&confirmation.prompt_id, Some(PromptValue::Bool(true)))
        .unwrap()
    else {
        panic!("expected completion");
    };
    let texts = message_texts(&result.effects);
    assert!(
        texts
            .iter()
            .any(|text| text.to_lowercase().contains("no such remote")),
        "{:?}",
        result.effects
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn remote_rename_prompts_for_old_and_new_names() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    tmp.git(&["remote", "add", "origin", "/tmp/magit-remote-target"]);
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Prompt(prompt) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "remote.rename".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected remote name prompt");
    };
    let Execution::Prompt(prompt) = test
        .session
        .resume_prompt(&prompt.prompt_id, Some(PromptValue::Text("origin".into())))
        .unwrap()
    else {
        panic!("expected new remote name prompt");
    };
    let Execution::Done(result) = test
        .session
        .resume_prompt(
            &prompt.prompt_id,
            Some(PromptValue::Text("upstream".into())),
        )
        .unwrap()
    else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&result.effects)
            .iter()
            .any(|text| *text == "Renamed remote origin to upstream"),
        "{:?}",
        result.effects
    );
    let remotes = tmp.git(&["remote"]);
    assert!(remotes.lines().any(|line| line == "upstream"), "{remotes}");
    assert!(!remotes.lines().any(|line| line == "origin"), "{remotes}");
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn remote_prune_drops_stale_tracking_refs_but_keeps_live_ones() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    use tempfile::TempDir;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    let bare = TempDir::new().expect("bare remote");
    git(bare.path(), &["init", "--bare", "-q"]);
    let remote_url = bare.path().to_str().expect("utf8 remote");
    tmp.git(&["branch", "feature"]);
    tmp.git(&["remote", "add", "origin", remote_url]);
    tmp.git(&["push", "-q", "origin", "main", "feature"]);
    tmp.git(&["fetch", "-q", "origin"]);
    // A fresh bare repository's HEAD names a nonexistent `master`; point it at
    // the pushed `main` so the side branch is an ordinary merged deletion.
    git(bare.path(), &["symbolic-ref", "HEAD", "refs/heads/main"]);
    // Delete the side branch on the remote, leaving a stale tracking ref.
    git(bare.path(), &["branch", "-d", "feature"]);
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Prompt(prompt) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "remote.prune".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected remote name prompt");
    };
    let Execution::Done(result) = test
        .session
        .resume_prompt(&prompt.prompt_id, Some(PromptValue::Text("origin".into())))
        .unwrap()
    else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&result.effects)
            .iter()
            .any(|text| *text == "Pruned remote origin"),
        "{:?}",
        result.effects
    );
    let refs = tmp.git(&["for-each-ref", "refs/remotes/"]);
    assert!(
        !refs.contains("refs/remotes/origin/feature"),
        "stale tracking ref survived: {refs}"
    );
    assert!(
        refs.contains("refs/remotes/origin/main"),
        "live tracking ref was pruned: {refs}"
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn remote_prune_of_missing_remote_surfaces_gits_refusal() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Prompt(prompt) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "remote.prune".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected remote name prompt");
    };
    let Execution::Done(result) = test
        .session
        .resume_prompt(&prompt.prompt_id, Some(PromptValue::Text("nosuch".into())))
        .unwrap()
    else {
        panic!("expected completion");
    };
    let refusal = result.effects.iter().find_map(|effect| match effect {
        Effect::Message {
            level: MessageLevel::Warn,
            text,
            ..
        } => Some(text.as_str()),
        _ => None,
    });
    assert!(
        refusal.is_some_and(|text| text.contains("git remote prune -- nosuch failed")),
        "{:?}",
        result.effects
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn branch_reset_moves_a_side_branch_to_a_revision_without_switching() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .commit("second", [("second.txt", "second\n")])
        .build();
    tmp.git(&["branch", "feature", "HEAD~1"]);
    let feature_before = tmp.git(&["rev-parse", "feature"]).trim().to_owned();
    let head_before = tmp.git(&["rev-parse", "HEAD"]).trim().to_owned();
    assert_ne!(
        feature_before, head_before,
        "the fixture must start the branch and target at distinct commits"
    );
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Prompt(first) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "branch.reset".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected branch name prompt");
    };
    let Execution::Prompt(second) = test
        .session
        .resume_prompt(&first.prompt_id, Some(PromptValue::Text("feature".into())))
        .unwrap()
    else {
        panic!("expected reset target prompt");
    };
    let Execution::Done(result) = test
        .session
        .resume_prompt(&second.prompt_id, Some(PromptValue::Text("HEAD".into())))
        .unwrap()
    else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&result.effects)
            .iter()
            .any(|text| *text == "Reset branch feature to HEAD"),
        "{:?}",
        result.effects
    );
    let feature = tmp.git(&["rev-parse", "feature"]).trim().to_owned();
    let head = tmp.git(&["rev-parse", "HEAD"]).trim().to_owned();
    assert_eq!(
        feature, head_before,
        "reset must move feature to the captured target OID"
    );
    assert_eq!(
        head, head_before,
        "force-moving a side branch must leave the current branch in place"
    );
    assert!(
        tmp.git(&["symbolic-ref", "HEAD"]).trim().ends_with("main"),
        "reset must not check the branch out"
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn branch_reset_of_current_branch_hard_resets_to_a_distinct_commit() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .commit("second", [("second.txt", "second\n")])
        .build();
    let parent = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let second = tmp.git(&["rev-parse", "HEAD"]).trim().to_owned();
    assert_ne!(parent, second, "the fixture must have two distinct commits");
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Prompt(first) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "branch.reset".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected branch name prompt");
    };
    let Execution::Prompt(target) = test
        .session
        .resume_prompt(&first.prompt_id, Some(PromptValue::Text("main".into())))
        .unwrap()
    else {
        panic!("expected reset target prompt");
    };
    let Execution::Done(result) = test
        .session
        .resume_prompt(&target.prompt_id, Some(PromptValue::Text("HEAD~1".into())))
        .unwrap()
    else {
        panic!("a clean tree must not ask for confirmation");
    };
    assert!(
        message_texts(&result.effects).contains(&"Reset to HEAD~1 (hard)"),
        "{:?}",
        result.effects
    );
    assert_eq!(
        tmp.git(&["rev-parse", "HEAD"]).trim(),
        parent,
        "resetting the current branch must hard-reset HEAD to the captured parent OID"
    );
    assert_eq!(
        tmp.git(&["rev-parse", "main"]).trim(),
        parent,
        "the current branch ref must move with HEAD"
    );
    assert!(
        !tmp.path().join("second.txt").exists(),
        "the current-branch reset must remove the child commit's file from the worktree"
    );
    assert_eq!(tmp.git(&["status", "--porcelain"]).trim(), "");
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn branch_reset_of_current_branch_declines_a_dirty_tree_then_discards_when_confirmed() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .commit("second", [("second.txt", "second\n")])
        .build();
    std::fs::write(tmp.path().join("second.txt"), "dirty\n").unwrap();
    let parent = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let second = tmp.git(&["rev-parse", "HEAD"]).trim().to_owned();
    assert_ne!(parent, second, "the fixture must have two distinct commits");
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let invoke = |test: &mut TestSession| {
        let Execution::Prompt(first) = test
            .session
            .start_invoke(&ViewInvokeParams {
                view_id: test.view_id().to_string(),
                command: "branch.reset".into(),
                version: Some(test.version()),
                cursor: None,
                selection: None,
                args: None,
            })
            .unwrap()
        else {
            panic!("expected branch name prompt");
        };
        let Execution::Prompt(target) = test
            .session
            .resume_prompt(&first.prompt_id, Some(PromptValue::Text("main".into())))
            .unwrap()
        else {
            panic!("expected reset target prompt");
        };
        let Execution::Prompt(confirm) = test
            .session
            .resume_prompt(&target.prompt_id, Some(PromptValue::Text("HEAD~1".into())))
            .unwrap()
        else {
            panic!("a dirty tree must ask for confirmation");
        };
        confirm.prompt_id
    };
    let confirm_id = invoke(&mut test);
    let Execution::Done(declined) = test
        .session
        .resume_prompt(&confirm_id, Some(PromptValue::Text("no".into())))
        .unwrap()
    else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&declined.effects).contains(&"Hard reset to HEAD~1 cancelled"),
        "{:?}",
        declined.effects
    );
    assert_eq!(
        tmp.git(&["rev-parse", "HEAD"]).trim(),
        second,
        "declining must leave the current branch at its captured OID"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("second.txt")).unwrap(),
        "dirty\n",
        "declining must leave the worktree in place"
    );
    let confirm_id = invoke(&mut test);
    let Execution::Done(confirmed) = test
        .session
        .resume_prompt(&confirm_id, Some(PromptValue::Text("yes".into())))
        .unwrap()
    else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&confirmed.effects).contains(&"Reset to HEAD~1 (hard)"),
        "{:?}",
        confirmed.effects
    );
    assert_eq!(
        tmp.git(&["rev-parse", "HEAD"]).trim(),
        parent,
        "confirming must hard-reset the current branch to the captured parent OID"
    );
    assert!(
        !tmp.path().join("second.txt").exists(),
        "confirming must discard the child commit's file"
    );
    assert_eq!(
        tmp.git(&["status", "--porcelain"]).trim(),
        "",
        "confirming must leave a clean tree behind"
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn reset_branch_menu_id_drives_the_same_hard_reset_flow() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .commit("second", [("second.txt", "second\n")])
        .build();
    let parent = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let second = tmp.git(&["rev-parse", "HEAD"]).trim().to_owned();
    assert_ne!(parent, second, "the fixture must have two distinct commits");
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Prompt(first) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "reset.branch".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected branch name prompt");
    };
    let Execution::Prompt(target) = test
        .session
        .resume_prompt(&first.prompt_id, Some(PromptValue::Text("main".into())))
        .unwrap()
    else {
        panic!("expected reset target prompt");
    };
    let Execution::Done(result) = test
        .session
        .resume_prompt(&target.prompt_id, Some(PromptValue::Text("HEAD~1".into())))
        .unwrap()
    else {
        panic!("the X b id must drive the same clean current-branch reset");
    };
    assert!(
        message_texts(&result.effects).contains(&"Reset to HEAD~1 (hard)"),
        "{:?}",
        result.effects
    );
    assert_eq!(
        tmp.git(&["rev-parse", "HEAD"]).trim(),
        parent,
        "the registered reset.branch id must move HEAD to the captured parent OID"
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn branch_reset_answers_cannot_select_a_delete_operation() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .commit("second", [("second.txt", "second\n")])
        .build();
    tmp.git(&["branch", "feature", "HEAD~1"]);
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let snapshot = |tmp: &RepoFixture| {
        (
            tmp.git(&[
                "for-each-ref",
                "--format=%(refname) %(objectname)",
                "refs/heads/",
                "refs/tags/",
            ]),
            tmp.git(&["symbolic-ref", "HEAD"]) + &tmp.git(&["rev-parse", "HEAD"]),
            tmp.git(&["ls-files", "--stage", "-z"]),
            std::fs::read_to_string(tmp.path().join("base.txt")).unwrap()
                + &std::fs::read_to_string(tmp.path().join("second.txt")).unwrap(),
        )
    };
    let before = snapshot(&tmp);
    let feature_before = tmp.git(&["rev-parse", "feature"]);
    for answer in ["--delete", "-D"] {
        let Execution::Prompt(first) = test
            .session
            .start_invoke(&ViewInvokeParams {
                view_id: test.view_id().to_string(),
                command: "branch.reset".into(),
                version: Some(test.version()),
                cursor: None,
                selection: None,
                args: None,
            })
            .unwrap()
        else {
            panic!("expected branch name prompt");
        };
        let Execution::Prompt(second) = test
            .session
            .resume_prompt(&first.prompt_id, Some(PromptValue::Text("feature".into())))
            .unwrap()
        else {
            panic!("expected reset target prompt");
        };
        let Execution::Done(result) = test
            .session
            .resume_prompt(&second.prompt_id, Some(PromptValue::Text(answer.into())))
            .unwrap()
        else {
            panic!("expected completion");
        };
        let texts = message_texts(&result.effects);
        assert!(
            texts
                .iter()
                .any(|text| text.contains("revision cannot begin with '-'")),
            "answer {answer}: expected readable refusal, got {:?}",
            result.effects
        );
        assert!(
            !texts.iter().any(|text| text.starts_with("Reset branch")),
            "answer {answer}: must not report a reset: {:?}",
            result.effects
        );
        assert_eq!(
            snapshot(&tmp),
            before,
            "answer {answer} must change nothing"
        );
        assert_eq!(
            tmp.git(&["rev-parse", "feature"]),
            feature_before,
            "answer {answer} must leave the branch ref in place"
        );
        assert!(!test.session.has_pending_prompt());
    }
    for answer in ["--delete", "-D"] {
        let Execution::Prompt(first) = test
            .session
            .start_invoke(&ViewInvokeParams {
                view_id: test.view_id().to_string(),
                command: "branch.reset".into(),
                version: Some(test.version()),
                cursor: None,
                selection: None,
                args: None,
            })
            .unwrap()
        else {
            panic!("expected branch name prompt");
        };
        let Execution::Prompt(second) = test
            .session
            .resume_prompt(&first.prompt_id, Some(PromptValue::Text(answer.into())))
            .unwrap()
        else {
            panic!("expected reset target prompt");
        };
        let Execution::Done(result) = test
            .session
            .resume_prompt(&second.prompt_id, Some(PromptValue::Text("HEAD".into())))
            .unwrap()
        else {
            panic!("expected completion");
        };
        let texts = message_texts(&result.effects);
        assert!(
            texts
                .iter()
                .any(|text| text.contains("not a valid branch name")),
            "name answer {answer}: expected readable refusal, got {:?}",
            result.effects
        );
        assert!(
            !texts.iter().any(|text| text.starts_with("Reset branch")),
            "name answer {answer}: must not report a reset: {:?}",
            result.effects
        );
        assert_eq!(
            snapshot(&tmp),
            before,
            "name answer {answer} must change nothing"
        );
        assert!(!test.session.has_pending_prompt());
    }
    let Execution::Prompt(first) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "branch.reset".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected branch name prompt");
    };
    let Execution::Prompt(second) = test
        .session
        .resume_prompt(&first.prompt_id, Some(PromptValue::Text("feature".into())))
        .unwrap()
    else {
        panic!("expected reset target prompt");
    };
    let Execution::Done(result) = test
        .session
        .resume_prompt(&second.prompt_id, Some(PromptValue::Text("HEAD".into())))
        .unwrap()
    else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&result.effects)
            .iter()
            .any(|text| *text == "Reset branch feature to HEAD"),
        "{:?}",
        result.effects
    );
    assert_eq!(
        tmp.git(&["rev-parse", "feature"]),
        tmp.git(&["rev-parse", "HEAD"]),
        "an ordinary reset must still move the branch"
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn option_like_tag_and_remote_answers_cannot_select_other_operations() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    tmp.git(&["tag", "keep"]);
    let mirror = tmp.path().join("mirror").display().to_string();
    tmp.git(&["remote", "add", "upstream", mirror.as_str()]);
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let state = |tmp: &RepoFixture| {
        (
            tmp.git(&["for-each-ref", "--format=%(refname) %(objectname)"]),
            tmp.git(&["config", "--local", "--list"]),
            tmp.git(&["status", "--porcelain"]),
        )
    };
    let before = state(&tmp);
    let run = |test: &mut TestSession, command: &str, answers: &[&str]| {
        let Execution::Prompt(mut prompt) = test
            .session
            .start_invoke(&ViewInvokeParams {
                view_id: test.view_id().to_string(),
                command: command.into(),
                version: Some(test.version()),
                cursor: None,
                selection: None,
                args: None,
            })
            .unwrap()
        else {
            panic!("{command}: expected prompt");
        };
        for (index, answer) in answers.iter().enumerate() {
            let last = index + 1 == answers.len();
            let execution = test
                .session
                .resume_prompt(&prompt.prompt_id, Some(PromptValue::Text((*answer).into())))
                .unwrap();
            let execution = if command == "remote.remove" && last {
                let Execution::Prompt(confirmation) = execution else {
                    panic!("expected removal confirmation");
                };
                test.session
                    .resume_prompt(&confirmation.prompt_id, Some(PromptValue::Bool(true)))
                    .unwrap()
            } else {
                execution
            };
            if last {
                let Execution::Done(result) = execution else {
                    panic!("{command}: expected completion for {answer}");
                };
                return result;
            }
            let Execution::Prompt(next) = execution else {
                panic!("{command}: expected follow-up prompt");
            };
            prompt = next;
        }
        unreachable!("answers are never empty")
    };
    let cases: &[(&str, &[&str])] = &[
        ("tag.create", &["-l", "HEAD"]),
        ("tag.create", &["--delete", "HEAD"]),
        ("tag.create", &["-d", "HEAD"]),
        ("tag.delete", &["-d"]),
        ("tag.delete", &["--delete"]),
        ("remote.add", &["--mirror", mirror.as_str()]),
        ("remote.add", &["origin2", "--upload-pack=/bin/sh"]),
        ("remote.remove", &["--all"]),
        ("remote.remove", &["-n"]),
        ("remote.rename", &["--dry-run", "renamed"]),
        ("remote.rename", &["upstream", "-f"]),
        ("remote.prune", &["-n"]),
        ("remote.prune", &["--dry-run"]),
    ];
    for (command, answers) in cases {
        let result = run(&mut test, command, answers);
        let texts = message_texts(&result.effects);
        assert!(
            texts
                .iter()
                .any(|text| text.contains("cannot begin with '-'")),
            "{command} {answers:?}: expected readable refusal, got {:?}",
            result.effects
        );
        for success in [
            "Created tag",
            "Deleted tag",
            "Added remote",
            "Removed remote",
            "Renamed remote",
            "Pruned remote",
        ] {
            assert!(
                !texts.iter().any(|text| text.starts_with(success)),
                "{command} {answers:?}: must not report {success}: {:?}",
                result.effects
            );
        }
        assert_eq!(
            state(&tmp),
            before,
            "{command} {answers:?} must change nothing"
        );
        assert!(!test.session.has_pending_prompt());
    }
    let result = run(&mut test, "tag.create", &["v1", "HEAD"]);
    assert!(
        message_texts(&result.effects).contains(&"Created tag v1"),
        "{:?}",
        result.effects
    );
    let result = run(&mut test, "remote.add", &["origin2", mirror.as_str()]);
    assert!(
        message_texts(&result.effects).contains(&"Added remote origin2"),
        "{:?}",
        result.effects
    );
    let result = run(&mut test, "remote.rename", &["origin2", "renamed"]);
    assert!(
        message_texts(&result.effects).contains(&"Renamed remote origin2 to renamed"),
        "{:?}",
        result.effects
    );
    let result = run(&mut test, "remote.remove", &["renamed"]);
    assert!(
        message_texts(&result.effects).contains(&"Removed remote renamed"),
        "{:?}",
        result.effects
    );
    let tags = tmp.git(&["tag", "--list"]);
    assert!(tags.lines().any(|line| line == "v1"), "{tags}");
    assert!(tags.lines().any(|line| line == "keep"), "{tags}");
    let remotes = tmp.git(&["remote"]);
    assert!(remotes.lines().any(|line| line == "upstream"), "{remotes}");
    assert!(!remotes.lines().any(|line| line == "renamed"), "{remotes}");
    let config = tmp.git(&["config", "--local", "--list"]);
    assert!(
        !config.contains("remote.renamed."),
        "a removed remote must leave no config: {config}"
    );
}

/// Invoke a single-prompt reset command and answer its revision prompt.
fn reset_invoke(
    test: &mut TestSession,
    command: &str,
    answer: &str,
) -> magit_lsp::session::Execution {
    let execution = test
        .session
        .start_invoke(&magit_lsp::protocol::ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: command.into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap();
    let magit_lsp::session::Execution::Prompt(prompt) = execution else {
        panic!("{command}: expected a revision prompt");
    };
    test.session
        .resume_prompt(
            &prompt.prompt_id,
            Some(magit_lsp::protocol::PromptValue::Text(answer.into())),
        )
        .unwrap()
}

#[test]
fn reset_mixed_moves_head_and_leaves_the_old_tree_as_a_worktree_change() {
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .commit("second", [("second.txt", "second\n")])
        .build();
    let parent = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let second = tmp.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Done(result) = reset_invoke(&mut test, "reset.mixed", "HEAD~1") else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&result.effects).contains(&"Reset to HEAD~1 (mixed)"),
        "{:?}",
        result.effects
    );
    assert_eq!(
        tmp.git(&["rev-parse", "HEAD"]).trim(),
        parent,
        "mixed reset must move HEAD to the parent"
    );
    assert_ne!(parent, second, "the fixture must have two distinct commits");
    assert_eq!(
        tmp.git(&["diff", "--cached", "--name-only"]).trim(),
        "",
        "mixed reset must clear the index back to the parent"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("second.txt")).unwrap(),
        "second\n",
        "mixed reset must leave the worktree untouched"
    );
    assert!(
        tmp.git(&["status", "--porcelain"])
            .lines()
            .any(|line| line.contains("second.txt")),
        "the child commit's file must remain an unstaged worktree change: {}",
        tmp.git(&["status", "--porcelain"])
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn reset_soft_keeps_a_staged_change_staged() {
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .commit("second", [("second.txt", "second\n")])
        .build();
    std::fs::write(tmp.path().join("base.txt"), "staged\n").unwrap();
    tmp.git(&["add", "base.txt"]);
    let parent = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let second = tmp.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Done(result) = reset_invoke(&mut test, "reset.soft", "HEAD~1") else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&result.effects).contains(&"Reset to HEAD~1 (soft)"),
        "{:?}",
        result.effects
    );
    assert_ne!(parent, second, "the fixture must have two distinct commits");
    assert_eq!(
        tmp.git(&["rev-parse", "HEAD"]).trim(),
        parent,
        "soft reset must move HEAD to the parent"
    );
    assert!(
        tmp.git(&["diff", "--cached", "--name-only"])
            .lines()
            .any(|line| line == "base.txt"),
        "soft reset must leave the staged change staged: {}",
        tmp.git(&["diff", "--cached", "--name-only"])
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("base.txt")).unwrap(),
        "staged\n",
        "soft reset must leave the worktree untouched"
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn reset_hard_on_a_clean_tree_proceeds_without_confirmation() {
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .commit("second", [("second.txt", "second\n")])
        .build();
    let parent = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let second = tmp.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Done(result) = reset_invoke(&mut test, "reset.hard", "HEAD~1") else {
        panic!("a clean tree must not ask for confirmation");
    };
    assert!(
        message_texts(&result.effects).contains(&"Reset to HEAD~1 (hard)"),
        "{:?}",
        result.effects
    );
    assert_ne!(parent, second, "the fixture must have two distinct commits");
    assert_eq!(
        tmp.git(&["rev-parse", "HEAD"]).trim(),
        parent,
        "hard reset must move HEAD to the parent"
    );
    assert!(
        !tmp.path().join("second.txt").exists(),
        "hard reset must remove the child commit's file from the worktree"
    );
    assert_eq!(tmp.git(&["status", "--porcelain"]).trim(), "");
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn reset_hard_on_a_dirty_tree_declines_then_discards_when_confirmed() {
    use magit_lsp::protocol::PromptValue;
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .commit("second", [("second.txt", "second\n")])
        .build();
    std::fs::write(tmp.path().join("second.txt"), "dirty\n").unwrap();
    let parent = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let second = tmp.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Prompt(confirm) = reset_invoke(&mut test, "reset.hard", "HEAD~1") else {
        panic!("a dirty tree must ask for confirmation");
    };
    let Execution::Done(declined) = test
        .session
        .resume_prompt(&confirm.prompt_id, Some(PromptValue::Text("no".into())))
        .unwrap()
    else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&declined.effects).contains(&"Hard reset to HEAD~1 cancelled"),
        "{:?}",
        declined.effects
    );
    assert_ne!(parent, second, "the fixture must have two distinct commits");
    assert_eq!(
        tmp.git(&["rev-parse", "HEAD"]).trim(),
        second,
        "declining must leave HEAD in place"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("second.txt")).unwrap(),
        "dirty\n",
        "declining must leave the worktree in place"
    );
    let Execution::Prompt(confirm) = reset_invoke(&mut test, "reset.hard", "HEAD~1") else {
        panic!("the dirty tree must still ask for confirmation");
    };
    let Execution::Done(confirmed) = test
        .session
        .resume_prompt(&confirm.prompt_id, Some(PromptValue::Text("yes".into())))
        .unwrap()
    else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&confirmed.effects).contains(&"Reset to HEAD~1 (hard)"),
        "{:?}",
        confirmed.effects
    );
    assert_eq!(
        tmp.git(&["rev-parse", "HEAD"]).trim(),
        parent,
        "confirming must move HEAD to the parent"
    );
    assert!(
        !tmp.path().join("second.txt").exists(),
        "confirming must discard the child commit's file"
    );
    assert_eq!(
        tmp.git(&["status", "--porcelain"]).trim(),
        "",
        "confirming must leave a clean tree behind"
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn reset_hard_confirmation_refuses_edits_and_a_same_oid_branch_switch() {
    use magit_lsp::protocol::PromptValue;
    use magit_lsp::session::Execution;
    // Each case applies exactly one change while the confirmation is open.
    let cases: [(&str, fn(&RepoFixture)); 2] = [
        ("edit", |tmp| {
            std::fs::write(tmp.path().join("second.txt"), "edited while open\n").unwrap();
        }),
        ("same-OID branch switch", |tmp| {
            tmp.git(&["checkout", "other"]);
        }),
    ];
    for (case, change) in cases {
        let (tmp, _) = RepoFixture::new()
            .commit("init", [("base.txt", "base\n")])
            .commit("second", [("second.txt", "second\n")])
            .build();
        tmp.git(&["branch", "other"]);
        std::fs::write(tmp.path().join("second.txt"), "dirty\n").unwrap();
        let second = tmp.git(&["rev-parse", "HEAD"]).trim().to_owned();
        let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
        test.ack();
        let Execution::Prompt(confirm) = reset_invoke(&mut test, "reset.hard", "HEAD~1") else {
            panic!("a dirty tree must ask for confirmation ({case})");
        };
        let snapshot = |tmp: &RepoFixture| {
            (
                tmp.git(&[
                    "for-each-ref",
                    "--format=%(refname) %(objectname)",
                    "refs/heads/",
                    "refs/tags/",
                ]),
                tmp.git(&["symbolic-ref", "HEAD"]) + &tmp.git(&["rev-parse", "HEAD"]),
                tmp.git(&["ls-files", "--stage", "-z"]),
                std::fs::read_to_string(tmp.path().join("base.txt")).unwrap()
                    + &std::fs::read_to_string(tmp.path().join("second.txt")).unwrap(),
            )
        };
        let before_change = snapshot(&tmp);
        change(&tmp);
        assert_ne!(
            snapshot(&tmp),
            before_change,
            "{case}: the change under test must take effect"
        );
        assert_eq!(
            tmp.git(&["rev-parse", "HEAD"]).trim(),
            second,
            "{case}: the change under test must not move HEAD's commit"
        );
        if case == "same-OID branch switch" {
            assert_eq!(
                tmp.git(&["symbolic-ref", "HEAD"]).trim(),
                "refs/heads/other",
                "the fixture must switch branches without moving HEAD's commit"
            );
        } else {
            assert_eq!(
                tmp.git(&["symbolic-ref", "HEAD"]).trim(),
                "refs/heads/main",
                "{case}: an edit alone must leave the checked-out branch in place"
            );
        }
        let after_change = snapshot(&tmp);
        let Execution::Done(refused) = test
            .session
            .resume_prompt(&confirm.prompt_id, Some(PromptValue::Text("yes".into())))
            .unwrap()
        else {
            panic!("expected completion ({case})");
        };
        let texts = message_texts(&refused.effects);
        assert!(
            texts.contains(&"Repository changed while the prompt was open; try again"),
            "{case}: {:?}",
            refused.effects
        );
        assert!(
            !texts.contains(&"Reset to HEAD~1 (hard)"),
            "{case}: a refused confirmation must not report a reset: {:?}",
            refused.effects
        );
        assert_eq!(
            snapshot(&tmp),
            after_change,
            "{case}: refusal must preserve the exact post-change refs, index and worktree"
        );
        assert_eq!(
            tmp.git(&["rev-parse", "HEAD"]).trim(),
            second,
            "{case}: refusal must leave HEAD's commit in place"
        );
        assert!(!test.session.has_pending_prompt(), "{case}");
    }
}

#[test]
fn reset_hard_confirmation_resets_the_captured_commit_when_the_target_ref_moves() {
    use magit_lsp::protocol::PromptValue;
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .commit("second", [("second.txt", "second\n")])
        .build();
    tmp.git(&["branch", "target", "HEAD~1"]);
    let parent = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let second = tmp.git(&["rev-parse", "HEAD"]).trim().to_owned();
    std::fs::write(tmp.path().join("second.txt"), "dirty\n").unwrap();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Prompt(confirm) = reset_invoke(&mut test, "reset.hard", "target") else {
        panic!("a dirty tree must ask for confirmation");
    };
    // Move the entered ref to the tip while the confirmation is open.
    tmp.git(&["branch", "-f", "target", "HEAD"]);
    assert_eq!(tmp.git(&["rev-parse", "target"]).trim(), second);
    let Execution::Done(confirmed) = test
        .session
        .resume_prompt(&confirm.prompt_id, Some(PromptValue::Text("yes".into())))
        .unwrap()
    else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&confirmed.effects).contains(&"Reset to target (hard)"),
        "{:?}",
        confirmed.effects
    );
    assert_eq!(
        tmp.git(&["rev-parse", "HEAD"]).trim(),
        parent,
        "approval must reset the commit captured before the prompt, not the moved ref"
    );
    assert!(
        !tmp.path().join("second.txt").exists(),
        "the captured commit must discard the child commit's file"
    );
    assert_eq!(
        tmp.git(&["status", "--porcelain"]).trim(),
        "",
        "the approved reset must leave a clean tree behind"
    );
    assert_eq!(
        tmp.git(&["rev-parse", "target"]).trim(),
        second,
        "the moved ref itself must be left untouched"
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn reset_confirmation_is_shared_by_all_three_reset_commands() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    for command in ["reset.hard", "branch.reset", "reset.branch"] {
        let (tmp, _) = RepoFixture::new()
            .commit("init", [("base.txt", "base\n")])
            .commit("second", [("second.txt", "second\n")])
            .build();
        let parent = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
        std::fs::write(tmp.path().join("second.txt"), "dirty\n").unwrap();
        let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
        test.ack();
        let confirmation = if command == "reset.hard" {
            reset_invoke(&mut test, command, "HEAD~1")
        } else {
            let Execution::Prompt(branch) = test
                .session
                .start_invoke(&ViewInvokeParams {
                    view_id: test.view_id().to_string(),
                    command: command.into(),
                    version: Some(test.version()),
                    cursor: None,
                    selection: None,
                    args: None,
                })
                .unwrap()
            else {
                panic!("{command}: expected branch name prompt");
            };
            let Execution::Prompt(target) = test
                .session
                .resume_prompt(&branch.prompt_id, Some(PromptValue::Text("main".into())))
                .unwrap()
            else {
                panic!("{command}: expected reset target prompt");
            };
            test.session
                .resume_prompt(&target.prompt_id, Some(PromptValue::Text("HEAD~1".into())))
                .unwrap()
        };
        let Execution::Prompt(confirm) = confirmation else {
            panic!("{command}: a dirty tree must ask for confirmation");
        };
        let Execution::Done(confirmed) = test
            .session
            .resume_prompt(&confirm.prompt_id, Some(PromptValue::Text("yes".into())))
            .unwrap()
        else {
            panic!("{command}: expected completion");
        };
        assert!(
            message_texts(&confirmed.effects).contains(&"Reset to HEAD~1 (hard)"),
            "{command}: {:?}",
            confirmed.effects
        );
        assert_eq!(
            tmp.git(&["rev-parse", "HEAD"]).trim(),
            parent,
            "{command}: an unchanged approval must reset the captured commit"
        );
        assert!(
            !tmp.path().join("second.txt").exists(),
            "{command}: the approved reset must discard the child commit's file"
        );
        assert_eq!(
            tmp.git(&["status", "--porcelain"]).trim(),
            "",
            "{command}: the approved reset must leave a clean tree behind"
        );
        assert!(!test.session.has_pending_prompt());
    }
}

#[test]
fn reset_keep_surfaces_gits_refusal_when_local_changes_would_be_lost() {
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .commit("second", [("base.txt", "second\n")])
        .build();
    std::fs::write(tmp.path().join("base.txt"), "local\n").unwrap();
    let second = tmp.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Done(result) = reset_invoke(&mut test, "reset.keep", "HEAD~1") else {
        panic!("expected completion");
    };
    let texts = message_texts(&result.effects);
    assert!(
        texts
            .iter()
            .any(|text| text.to_lowercase().contains("not uptodate")),
        "Git's refusal must surface as a readable message: {:?}",
        result.effects
    );
    assert!(
        !texts.iter().any(|text| text.starts_with("Reset to")),
        "a refused keep reset must not report success: {:?}",
        result.effects
    );
    assert_eq!(
        tmp.git(&["rev-parse", "HEAD"]).trim(),
        second,
        "Git's refusal must leave HEAD in place"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("base.txt")).unwrap(),
        "local\n",
        "Git's refusal must leave the local change in place"
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn reset_revision_answers_cannot_select_git_options() {
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .commit("second", [("second.txt", "second\n")])
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let state = |tmp: &RepoFixture| {
        (
            tmp.git(&["rev-parse", "HEAD"]),
            tmp.git(&["status", "--porcelain"]),
            tmp.git(&["ls-files", "--stage", "-z"]),
            std::fs::read_to_string(tmp.path().join("base.txt")).unwrap()
                + &std::fs::read_to_string(tmp.path().join("second.txt")).unwrap(),
        )
    };
    let before = state(&tmp);
    for answer in ["-D", "--hard"] {
        let Execution::Done(result) = reset_invoke(&mut test, "reset.mixed", answer) else {
            panic!("answer {answer}: expected completion");
        };
        let texts = message_texts(&result.effects);
        assert!(
            texts
                .iter()
                .any(|text| text.contains("revision cannot begin with '-'")),
            "answer {answer}: expected readable refusal, got {:?}",
            result.effects
        );
        assert!(
            !texts.iter().any(|text| text.starts_with("Reset to")),
            "answer {answer}: must not report a reset: {:?}",
            result.effects
        );
        assert_eq!(state(&tmp), before, "answer {answer} must change nothing");
        assert!(!test.session.has_pending_prompt());
    }
}

/// Invoke a single-prompt spin command and answer its branch-name prompt.
fn spin_invoke(
    test: &mut TestSession,
    command: &str,
    answer: &str,
) -> magit_lsp::session::Execution {
    let execution = test
        .session
        .start_invoke(&magit_lsp::protocol::ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: command.into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap();
    let magit_lsp::session::Execution::Prompt(prompt) = execution else {
        panic!("{command}: expected a branch name prompt");
    };
    test.session
        .resume_prompt(
            &prompt.prompt_id,
            Some(magit_lsp::protocol::PromptValue::Text(answer.into())),
        )
        .unwrap()
}

/// Fixture with `foobar` checked out, one commit above its upstream `main`.
fn spin_fixture() -> RepoFixture {
    let (tmp, _) = RepoFixture::new()
        .commit("base", [("base.txt", "base\n")])
        .build();
    tmp.git(&["branch", "foobar"]);
    tmp.git(&["checkout", "foobar"]);
    tmp.git(&["branch", "--set-upstream-to=main", "foobar"]);
    std::fs::write(tmp.path().join("base.txt"), "child\n").unwrap();
    tmp.git(&["commit", "-am", "child"]);
    tmp
}

#[test]
fn branch_spinoff_checks_out_a_tracking_branch_and_moves_the_source_to_the_merge_base() {
    use magit_lsp::session::Execution;
    let tmp = spin_fixture();
    let common_oid = tmp.git(&["rev-parse", "main"]).trim().to_owned();
    // Advance the upstream with a commit the source does not contain, so the
    // merge base and the upstream tip are distinct commits.
    tmp.git(&["checkout", "main"]);
    std::fs::write(tmp.path().join("upstream.txt"), "upstream\n").unwrap();
    tmp.git(&["add", "upstream.txt"]);
    tmp.git(&["commit", "-m", "upstream"]);
    tmp.git(&["checkout", "foobar"]);
    let upstream_oid = tmp.git(&["rev-parse", "main"]).trim().to_owned();
    let source_oid = tmp.git(&["rev-parse", "foobar"]).trim().to_owned();
    assert_ne!(
        common_oid, upstream_oid,
        "the upstream must have moved past the merge base"
    );
    assert_ne!(
        common_oid, source_oid,
        "the source must commit above the merge base"
    );
    assert_ne!(
        source_oid, upstream_oid,
        "the source and upstream must diverge"
    );
    assert_eq!(
        tmp.git(&["merge-base", "foobar", "main"]).trim(),
        common_oid,
        "the common ancestor must stay the original branch point"
    );
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Done(result) = spin_invoke(&mut test, "branch.spinoff", "spun") else {
        panic!("expected completion");
    };
    let expected = format!("Created branch spun tracking foobar; moved foobar to {common_oid}");
    assert!(
        message_texts(&result.effects).contains(&expected.as_str()),
        "{:?}",
        result.effects
    );
    assert_eq!(
        tmp.git(&["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
        "spun"
    );
    assert_eq!(
        tmp.git(&["rev-parse", "spun"]).trim(),
        source_oid,
        "the new branch must start at the source's captured OID"
    );
    assert_eq!(
        tmp.git(&["rev-parse", "foobar"]).trim(),
        common_oid,
        "the source must move to the merge base, not the upstream tip"
    );
    assert_eq!(
        tmp.git(&["config", "branch.spun.merge"]).trim(),
        "refs/heads/foobar",
        "the new branch must track the source"
    );
    assert_eq!(
        tmp.git(&["config", "branch.spun.remote"]).trim(),
        ".",
        "tracking the source uses the local dot remote"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("base.txt")).unwrap(),
        "child\n",
        "a spinoff switches at the same commit and leaves the tree unchanged"
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn branch_spinout_stays_on_the_source_and_leaves_the_spun_commit_in_the_worktree() {
    use magit_lsp::session::Execution;
    let tmp = spin_fixture();
    let main_oid = tmp.git(&["rev-parse", "main"]).trim().to_owned();
    let source_oid = tmp.git(&["rev-parse", "foobar"]).trim().to_owned();
    assert_ne!(
        main_oid, source_oid,
        "the fixture must commit above its upstream"
    );
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Done(result) = spin_invoke(&mut test, "branch.spinout", "spun") else {
        panic!("expected completion");
    };
    let expected = format!("Created branch spun tracking foobar; moved foobar to {main_oid}");
    assert!(
        message_texts(&result.effects).contains(&expected.as_str()),
        "{:?}",
        result.effects
    );
    assert_eq!(
        tmp.git(&["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
        "foobar",
        "a spinout stays on the source branch"
    );
    assert_eq!(tmp.git(&["rev-parse", "foobar"]).trim(), main_oid);
    assert_eq!(
        tmp.git(&["rev-parse", "spun"]).trim(),
        source_oid,
        "the new branch must keep the spun-off commit"
    );
    assert_eq!(
        tmp.git(&["diff", "--cached", "--name-only"]).trim(),
        "",
        "the mixed reset must clear the index back to the merge base"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("base.txt")).unwrap(),
        "child\n",
        "the spun-off change must stay in the worktree"
    );
    assert!(
        tmp.git(&["status", "--porcelain"])
            .lines()
            .any(|line| line.trim() == "M base.txt"),
        "the spun-off change must be present unstaged: {}",
        tmp.git(&["status", "--porcelain"])
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn branch_spinout_with_a_dirty_tree_falls_back_to_a_spinoff() {
    use magit_lsp::session::Execution;
    let tmp = spin_fixture();
    std::fs::write(tmp.path().join("base.txt"), "dirty\n").unwrap();
    let main_oid = tmp.git(&["rev-parse", "main"]).trim().to_owned();
    let source_oid = tmp.git(&["rev-parse", "foobar"]).trim().to_owned();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Done(result) = spin_invoke(&mut test, "branch.spinout", "spun") else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&result.effects)
            .iter()
            .any(|text| text.contains("working tree had changes")),
        "a dirty spinout must say it fell back to a spinoff: {:?}",
        result.effects
    );
    assert_eq!(
        tmp.git(&["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
        "spun",
        "the fallback must move HEAD to the new branch"
    );
    assert_eq!(
        tmp.git(&["rev-parse", "foobar"]).trim(),
        main_oid,
        "the fallback must still move the source to the merge base"
    );
    assert_eq!(tmp.git(&["rev-parse", "spun"]).trim(), source_oid);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("base.txt")).unwrap(),
        "dirty\n",
        "the fallback must keep the uncommitted change in the worktree"
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn branch_spinoff_without_an_upstream_creates_the_branch_and_leaves_the_source_untouched() {
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("base", [("base.txt", "base\n")])
        .commit("second", [("second.txt", "second\n")])
        .build();
    let head_oid = tmp.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Done(result) = spin_invoke(&mut test, "branch.spinoff", "spun") else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&result.effects)
            .iter()
            .any(|text| text.contains("main has no upstream, so it was left untouched")),
        "{:?}",
        result.effects
    );
    assert_eq!(tmp.git(&["rev-parse", "spun"]).trim(), head_oid);
    assert_eq!(
        tmp.git(&["rev-parse", "main"]).trim(),
        head_oid,
        "the source must stay at its commit"
    );
    assert_eq!(
        tmp.git(&["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
        "main",
        "the edge path must leave the source checked out"
    );
    assert_eq!(
        tmp.git(&["config", "branch.spun.merge"]).trim(),
        "refs/heads/main",
        "the new branch must still track the source"
    );
    assert!(!test.session.has_pending_prompt());
}

#[test]
fn branch_spinoff_without_unpushed_commits_creates_the_branch_and_leaves_the_source_untouched() {
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("base", [("base.txt", "base\n")])
        .build();
    tmp.git(&["branch", "foobar"]);
    tmp.git(&["checkout", "foobar"]);
    tmp.git(&["branch", "--set-upstream-to=main", "foobar"]);
    let head_oid = tmp.git(&["rev-parse", "HEAD"]).trim().to_owned();
    assert_eq!(
        tmp.git(&["rev-parse", "main"]).trim(),
        head_oid,
        "the fixture must have nothing unpushed"
    );
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let Execution::Done(result) = spin_invoke(&mut test, "branch.spinoff", "spun") else {
        panic!("expected completion");
    };
    assert!(
        message_texts(&result.effects)
            .iter()
            .any(|text| text.contains("has no unpushed commits, so it was left untouched")),
        "{:?}",
        result.effects
    );
    assert_eq!(tmp.git(&["rev-parse", "spun"]).trim(), head_oid);
    assert_eq!(
        tmp.git(&["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
        "foobar",
        "the edge path must leave the source checked out"
    );
    assert_eq!(
        tmp.git(&["config", "branch.spun.merge"]).trim(),
        "refs/heads/foobar",
        "the new branch must track the source"
    );
    assert!(!test.session.has_pending_prompt());
}
