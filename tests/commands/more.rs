use super::*;
use super::additional::{change_identity_file, file_identity_matrix};

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
fn branch_create_prompts_and_creates_branch_without_switching() {
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
            command: "branch.create".into(),
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
        .resume_prompt(
            &prompt.prompt_id,
            Some(PromptValue::Text("feature".into())),
        )
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
    let branches = tmp.git(&["for-each-ref", "--format=%(refname:short)", "refs/heads/"]);
    assert!(
        branches.lines().any(|line| line == "feature"),
        "{branches}"
    );
    assert!(
        tmp.git(&["symbolic-ref", "HEAD"]).trim().ends_with("main"),
        "create must not switch HEAD"
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
            .any(|text| *text == "Created tag v1"),
        "{:?}",
        result.effects
    );
    let tags = tmp.git(&["tag", "--list"]);
    assert!(tags.lines().any(|line| line == "v1"), "{tags}");
    assert!(
        tmp.git(&["symbolic-ref", "HEAD"]).trim().ends_with("main"),
        "creating a tag must not switch HEAD"
    );
    assert!(!test.session.has_pending_prompt());
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
        .resume_prompt(
            &first.prompt_id,
            Some(PromptValue::Text("main".into())),
        )
        .unwrap()
    else {
        panic!("expected rename target prompt");
    };
    let Execution::Done(result) = test
        .session
        .resume_prompt(
            &second.prompt_id,
            Some(PromptValue::Text("renamed".into())),
        )
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
    assert!(
        !branches.lines().any(|line| line == "main"),
        "{branches}"
    );
    assert!(
        branches.lines().any(|line| line == "renamed"),
        "{branches}"
    );
    assert!(
        tmp.git(&["symbolic-ref", "HEAD"]).trim().ends_with("renamed"),
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
        .resume_prompt(
            &prompt.prompt_id,
            Some(PromptValue::Text("feature".into())),
        )
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
        .resume_prompt(
            &prompt.prompt_id,
            Some(PromptValue::Text("main".into())),
        )
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
        .resume_prompt(
            &first.prompt_id,
            Some(PromptValue::Text("main".into())),
        )
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
            .resume_prompt(
                &first.prompt_id,
                Some(PromptValue::Text("main".into())),
            )
            .unwrap()
        else {
            panic!("expected description prompt");
        };
        let Execution::Done(result) = test
            .session
            .resume_prompt(
                &second.prompt_id,
                Some(PromptValue::Text(answer.into())),
            )
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
        .resume_prompt(
            &first.prompt_id,
            Some(PromptValue::Text("feature".into())),
        )
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
        assert_eq!(snapshot(&tmp), before, "answer {answer} must change nothing");
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
        ("tag.create", &["-l"]),
        ("tag.create", &["--delete"]),
        ("tag.create", &["-d"]),
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
    let result = run(&mut test, "tag.create", &["v1"]);
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
        .resume_prompt(
            &confirm.prompt_id,
            Some(PromptValue::Text("no".into())),
        )
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
        .resume_prompt(
            &confirm.prompt_id,
            Some(PromptValue::Text("yes".into())),
        )
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
