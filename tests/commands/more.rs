use super::additional::{change_identity_file, file_identity_matrix};
use super::*;

#[test]
fn notes_menu_opens_through_dispatch_and_reads_explicit_commit_note() {
    let (tmp, _) = RepoFixture::new()
        .commit("first", [("base.txt", "base\n")])
        .commit("second", [("base.txt", "second\n")])
        .build();
    let target = tmp
        .git(&["rev-list", "--max-parents=0", "HEAD"])
        .trim()
        .to_string();
    tmp.git(&["config", "core.notesRef", "refs/notes/custom"]);
    let head_target = tmp.git(&["rev-parse", "HEAD"]).trim().to_string();
    tmp.git(&[
        "notes",
        "--ref=refs/notes/custom",
        "add",
        "-m",
        "kept",
        &head_target,
    ]);
    tmp.git(&[
        "notes",
        "--ref=refs/notes/custom",
        "add",
        "-m",
        "target note",
        &target,
    ]);

    let head = tmp.git(&["rev-parse", "HEAD"]);
    let index = tmp.git(&["write-tree"]);
    let status = tmp.git(&["status", "--porcelain"]);
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Standard);
    test.ack();
    test.press_ui("?");
    test.ack();
    test.press_ui("N");
    test.ack();

    assert!(test.text().iter().any(|line| line.contains("Notes")));
    test.ui.reply_text(Some(target.clone()));
    let result = test.press_ui("r");
    assert!(message_texts(&result.effects)
        .iter()
        .any(|message| message.contains("target note")));
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["write-tree"]), index);
    assert_eq!(tmp.git(&["status", "--porcelain"]), status);
}

fn open_notes_menu(tmp: &RepoFixture) -> TestSession {
    let (mut test, _) = TestSession::open_status(tmp, Profile::Standard);
    test.ack();
    test
}

fn notes_state(tmp: &RepoFixture) -> (String, String, String) {
    (
        tmp.git(&["rev-parse", "HEAD"]),
        tmp.git(&["write-tree"]),
        tmp.git(&["status", "--porcelain"]),
    )
}

fn assert_notes_state(tmp: &RepoFixture, state: &(String, String, String)) {
    assert_eq!(
        notes_state(tmp),
        *state,
        "notes operation changed HEAD, index, or worktree"
    );
}

fn invoke_notes(
    test: &mut TestSession,
    command: &str,
    answers: Vec<Option<magit_lsp::protocol::PromptValue>>,
) -> magit_lsp::session::Execution {
    use magit_lsp::protocol::ViewInvokeParams;
    use magit_lsp::session::Execution;
    let started = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: command.into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap();
    let Execution::Prompt(mut prompt) = started else {
        panic!("expected notes prompt, got {started:?}");
    };
    let mut execution = Execution::Prompt(prompt.clone());
    for answer in answers {
        execution = test
            .session
            .resume_prompt(&prompt.prompt_id, answer)
            .unwrap();
        match execution {
            Execution::Prompt(ref next) => prompt = next.clone(),
            Execution::Done(_) => break,
            Execution::Pick(_) => panic!("unexpected notes picker"),
        }
    }
    execution
}

#[test]
fn notes_add_edit_remove_target_explicit_commit_and_preserve_other_state() {
    let (tmp, _) = RepoFixture::new()
        .commit("first", [("base.txt", "base\n")])
        .commit("second", [("base.txt", "second\n")])
        .build();
    let target = tmp
        .git(&["rev-list", "--max-parents=0", "HEAD"])
        .trim()
        .to_string();
    let head_target = tmp.git(&["rev-parse", "HEAD"]).trim().to_string();
    tmp.git(&["config", "core.notesRef", "refs/notes/custom"]);
    tmp.git(&[
        "notes",
        "--ref=refs/notes/custom",
        "add",
        "-m",
        "head note",
        &head_target,
    ]);
    let state = notes_state(&tmp);
    let mut test = open_notes_menu(&tmp);

    use magit_lsp::protocol::PromptValue;
    let added = invoke_notes(
        &mut test,
        "notes.add",
        vec![
            Some(PromptValue::Text(target.clone())),
            Some(PromptValue::Text("created note".into())),
        ],
    );
    let magit_lsp::session::Execution::Done(added) = added else {
        panic!("expected completion");
    };
    assert!(message_texts(&added.effects)
        .iter()
        .any(|text| text.contains("Note added")));
    assert_eq!(
        tmp.git(&["notes", "--ref=refs/notes/custom", "show", &target])
            .trim(),
        "created note"
    );
    assert_eq!(
        tmp.git(&["notes", "--ref=refs/notes/custom", "show", &head_target])
            .trim(),
        "head note"
    );

    let magit_lsp::session::Execution::Done(duplicate) = invoke_notes(
        &mut test,
        "notes.add",
        vec![
            Some(PromptValue::Text(target.clone())),
            Some(PromptValue::Text("duplicate".into())),
        ],
    ) else {
        panic!("expected completion");
    };
    assert!(message_texts(&duplicate.effects)
        .iter()
        .any(|text| text.to_lowercase().contains("note") || text.to_lowercase().contains("git")));
    assert_eq!(
        tmp.git(&["notes", "--ref=refs/notes/custom", "show", &target])
            .trim(),
        "created note"
    );

    use magit_lsp::protocol::ViewInvokeParams;
    let magit_lsp::session::Execution::Prompt(edit_prompt) = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "notes.edit".into(),
            version: Some(test.version()),
            cursor: None,
            selection: None,
            args: None,
        })
        .unwrap()
    else {
        panic!("expected edit target prompt");
    };
    let magit_lsp::session::Execution::Prompt(content_prompt) = test
        .session
        .resume_prompt(
            &edit_prompt.prompt_id,
            Some(PromptValue::Text(target.clone())),
        )
        .unwrap()
    else {
        panic!("expected edit content prompt");
    };
    assert_eq!(
        content_prompt
            .default
            .as_ref()
            .and_then(serde_json::Value::as_str),
        Some("created note\n")
    );
    let magit_lsp::session::Execution::Done(edited) = test
        .session
        .resume_prompt(
            &content_prompt.prompt_id,
            Some(PromptValue::Text("edited note".into())),
        )
        .unwrap()
    else {
        panic!("expected completion");
    };
    assert!(message_texts(&edited.effects)
        .iter()
        .any(|text| text.contains("Note updated")));
    assert_eq!(
        tmp.git(&["notes", "--ref=refs/notes/custom", "show", &target])
            .trim(),
        "edited note"
    );
    assert_notes_state(&tmp, &state);

    let magit_lsp::session::Execution::Done(declined) = invoke_notes(
        &mut test,
        "notes.remove",
        vec![
            Some(PromptValue::Text(target.clone())),
            Some(PromptValue::Bool(false)),
        ],
    ) else {
        panic!("expected completion");
    };
    assert!(!message_texts(&declined.effects)
        .iter()
        .any(|text| text.contains("Note removed")));
    assert_eq!(
        tmp.git(&["notes", "--ref=refs/notes/custom", "show", &target])
            .trim(),
        "edited note"
    );
    let magit_lsp::session::Execution::Done(removed) = invoke_notes(
        &mut test,
        "notes.remove",
        vec![
            Some(PromptValue::Text(target.clone())),
            Some(PromptValue::Bool(true)),
        ],
    ) else {
        panic!("expected completion");
    };
    assert!(message_texts(&removed.effects)
        .iter()
        .any(|text| text.contains("Note removed")));
    assert!(!tmp
        .git(&["notes", "--ref=refs/notes/custom", "list"])
        .contains(&target));
    assert_eq!(
        tmp.git(&["notes", "--ref=refs/notes/custom", "show", &head_target])
            .trim(),
        "head note"
    );
    assert_notes_state(&tmp, &state);
}

#[test]
fn notes_cancel_missing_invalid_and_unborn_targets_do_not_write() {
    let (tmp, _) = RepoFixture::new()
        .commit("first", [("base.txt", "base\n")])
        .commit("second", [("base.txt", "second\n")])
        .build();
    let target = tmp
        .git(&["rev-list", "--max-parents=0", "HEAD"])
        .trim()
        .to_string();
    tmp.git(&["config", "core.notesRef", "refs/notes/custom"]);
    let state = notes_state(&tmp);
    let mut test = open_notes_menu(&tmp);

    use magit_lsp::protocol::PromptValue;
    let _ = invoke_notes(&mut test, "notes.add", vec![None]);
    let _ = invoke_notes(
        &mut test,
        "notes.add",
        vec![Some(PromptValue::Text(target.clone())), None],
    );
    let _ = invoke_notes(
        &mut test,
        "notes.add",
        vec![
            Some(PromptValue::Text(target.clone())),
            Some(PromptValue::Text("   ".into())),
        ],
    );
    assert!(tmp
        .git(&["notes", "--ref=refs/notes/custom", "list"])
        .trim()
        .is_empty());
    assert_notes_state(&tmp, &state);

    let magit_lsp::session::Execution::Done(invalid) = invoke_notes(
        &mut test,
        "notes.read",
        vec![Some(PromptValue::Text("not-a-commit".into()))],
    ) else {
        panic!("expected completion");
    };
    assert!(message_texts(&invalid.effects)
        .iter()
        .any(|text| text.contains("Invalid commit target")));
    let _ = invoke_notes(
        &mut test,
        "notes.edit",
        vec![Some(PromptValue::Text("HEAD~99".into()))],
    );
    let magit_lsp::session::Execution::Done(missing) = invoke_notes(
        &mut test,
        "notes.edit",
        vec![Some(PromptValue::Text(target.clone()))],
    ) else {
        panic!("expected completion");
    };
    assert!(message_texts(&missing.effects)
        .iter()
        .any(|text| text.contains("No note for commit")));
    let magit_lsp::session::Execution::Done(missing_read) = invoke_notes(
        &mut test,
        "notes.read",
        vec![Some(PromptValue::Text(target.clone()))],
    ) else {
        panic!("expected completion");
    };
    assert!(message_texts(&missing_read.effects)
        .iter()
        .any(|text| text.contains("No note for commit")));
    assert_notes_state(&tmp, &state);

    let _ = invoke_notes(
        &mut test,
        "notes.remove",
        vec![Some(PromptValue::Text(target.clone())), None],
    );
    assert_notes_state(&tmp, &state);

    let (unborn, _) = RepoFixture::new().unborn().build();
    let mut unborn_test = open_notes_menu(&unborn);
    let magit_lsp::session::Execution::Done(result) = invoke_notes(
        &mut unborn_test,
        "notes.read",
        vec![Some(PromptValue::Text("HEAD".into()))],
    ) else {
        panic!("expected completion");
    };
    assert!(message_texts(&result.effects)
        .iter()
        .any(|text| text.contains("Invalid commit target")));
    assert!(unborn.git(&["notes", "list"]).trim().is_empty());
}

#[test]
fn notes_git_write_failure_is_readable_and_preserves_existing_note() {
    let (tmp, _) = RepoFixture::new()
        .commit("first", [("base.txt", "base\n")])
        .commit("second", [("base.txt", "second\n")])
        .build();
    let target = tmp
        .git(&["rev-list", "--max-parents=0", "HEAD"])
        .trim()
        .to_string();
    tmp.git(&["config", "core.notesRef", "refs/notes/custom"]);
    tmp.git(&[
        "notes",
        "--ref=refs/notes/custom",
        "add",
        "-m",
        "preserve me",
        &target,
    ]);
    let state = notes_state(&tmp);
    std::fs::create_dir_all(tmp.path().join(".git/refs/notes")).unwrap();
    std::fs::write(tmp.path().join(".git/refs/notes/custom.lock"), "locked").unwrap();
    let mut test = open_notes_menu(&tmp);
    use magit_lsp::protocol::PromptValue;
    let magit_lsp::session::Execution::Done(result) = invoke_notes(
        &mut test,
        "notes.edit",
        vec![
            Some(PromptValue::Text(target.clone())),
            Some(PromptValue::Text("replacement".into())),
        ],
    ) else {
        panic!("expected completion");
    };
    let messages = message_texts(&result.effects);
    assert!(
        messages.iter().any(|text| *text == "`git notes` failed."),
        "{messages:?}"
    );
    assert!(!messages.iter().any(|text| text.contains("Note updated")));
    assert_eq!(
        tmp.git(&["notes", "--ref=refs/notes/custom", "show", &target])
            .trim(),
        "preserve me"
    );
    assert_notes_state(&tmp, &state);
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

#[test]
fn ignore_root_literal_uses_selected_path() {
    use magit_lsp::protocol::PromptChoiceAnswer;
    let (tmp, _) = RepoFixture::new()
        .commit(
            "init",
            [
                ("base.txt", "base\n"),
                ("dir/a*b.txt", "original\n"),
                ("dir/axxb.txt", "decoy\n"),
                ("other/other.txt", "other\n"),
            ],
        )
        .modify("dir/a*b.txt", "changed\n")
        .modify("other/other.txt", "changed too\n")
        .build();
    std::fs::write(tmp.path().join(".gitignore"), "# retained\n").unwrap();

    let head = tmp.git(&["rev-parse", "HEAD"]);
    let index = tmp.git(&["write-tree"]);
    let base_before = std::fs::read_to_string(tmp.path().join("base.txt")).unwrap();
    let decoy_before = std::fs::read_to_string(tmp.path().join("dir/axxb.txt")).unwrap();
    let other_before = std::fs::read_to_string(tmp.path().join("other/other.txt")).unwrap();

    let (mut test, _) = TestSession::open_status(&tmp, Profile::Standard);
    test.ack();
    let line = test.goto_file("unstaged", "dir/a*b.txt");
    assert_eq!(test.cursor().line, line);
    // A different modified file supplies a menu cursor whose line resolves to
    // another file in the source view, so the action can only succeed if it
    // reads the source identity rather than the menu's own cursor.
    let other_line = test.goto_file("unstaged", "other/other.txt");
    assert_ne!(other_line, line);
    test.goto(line);

    // Real key ingress: the `i` opener, then the `t` root action. Queue the
    // choice answer before pressing `t` so the ScriptedUi can drive it through
    // the pending prompt.
    test.ui().reply_choice(Some(PromptChoiceAnswer::Id {
        id: "literal".into(),
    }));
    test.press_ui("i");
    // Move the menu cursor away from the retained source cursor.
    test.goto(other_line);
    assert_eq!(test.cursor().line, other_line);
    let result = test.press_ui("t");

    assert!(
        message_texts(&result.effects)
            .iter()
            .any(|text| text.contains("root .gitignore")),
        "{:?}",
        result.effects
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap(),
        "# retained\n/dir/a\\*b.txt\n"
    );

    let ignored = |path: &str| -> bool {
        std::process::Command::new("git")
            .args(["check-ignore", "--no-index", path])
            .current_dir(tmp.path())
            .output()
            .expect("run git check-ignore")
            .status
            .success()
    };
    assert!(ignored("dir/a*b.txt"), "the literal file must be ignored");
    assert!(
        !ignored("dir/axxb.txt"),
        "the wildcard must be escaped, not expanded"
    );
    assert!(
        !ignored("base.txt"),
        "an unrelated file must not be ignored"
    );
    assert!(
        !ignored("other/other.txt"),
        "the menu cursor's file must not be ignored"
    );

    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["write-tree"]), index);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("base.txt")).unwrap(),
        base_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("dir/axxb.txt")).unwrap(),
        decoy_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("other/other.txt")).unwrap(),
        other_before
    );
}

#[test]
fn ignore_subdir_literal_is_relative_and_scoped() {
    use magit_lsp::protocol::PromptChoiceAnswer;
    let (tmp, _) = RepoFixture::new()
        .commit(
            "init",
            [
                ("base.txt", "base\n"),
                ("dir/nested/a*b.txt", "original\n"),
                ("other/other.txt", "other\n"),
            ],
        )
        .modify("dir/nested/a*b.txt", "changed\n")
        .modify("other/other.txt", "changed too\n")
        .build();
    // Pre-seed the selected parent's ignore file and place untracked decoys:
    // one with the same basename outside the parent, one with a non-literal
    // name inside it. The rule must be relative to and scoped to this parent.
    std::fs::write(tmp.path().join("dir/nested/.gitignore"), "# retained\n").unwrap();
    std::fs::write(tmp.path().join("dir/nested/axxb.txt"), "decoy inside\n").unwrap();
    std::fs::write(tmp.path().join("dir/a*b.txt"), "decoy outside\n").unwrap();

    let head = tmp.git(&["rev-parse", "HEAD"]);
    let index = tmp.git(&["write-tree"]);
    let base_before = std::fs::read_to_string(tmp.path().join("base.txt")).unwrap();
    let selected_before = std::fs::read_to_string(tmp.path().join("dir/nested/a*b.txt")).unwrap();
    let inside_before = std::fs::read_to_string(tmp.path().join("dir/nested/axxb.txt")).unwrap();
    let outside_before = std::fs::read_to_string(tmp.path().join("dir/a*b.txt")).unwrap();
    let other_before = std::fs::read_to_string(tmp.path().join("other/other.txt")).unwrap();
    let root_ignore_before = std::fs::read_to_string(tmp.path().join(".gitignore")).ok();
    let repo_exclude_before =
        std::fs::read_to_string(tmp.path().join(".git/info/exclude")).unwrap_or_default();
    let user_ignore_path =
        std::path::PathBuf::from(std::env::var("XDG_CONFIG_HOME").unwrap()).join("git/ignore");
    let user_ignore_before = std::fs::read_to_string(&user_ignore_path).ok();

    let (mut test, _) = TestSession::open_status(&tmp, Profile::Standard);
    test.ack();
    let line = test.goto_file("unstaged", "dir/nested/a*b.txt");
    assert_eq!(test.cursor().line, line);
    // A different modified file supplies a menu cursor whose line resolves to
    // another file in the source view, so the action can only succeed if it
    // reads the retained source identity rather than the menu's own cursor.
    let other_line = test.goto_file("unstaged", "other/other.txt");
    assert_ne!(other_line, line);
    test.goto(line);

    // Real key ingress: the `i` opener, then the `s` subdir action. Queue the
    // literal choice before pressing `s` so ScriptedUi drives the prompt.
    test.ui().reply_choice(Some(PromptChoiceAnswer::Id {
        id: "literal".into(),
    }));
    test.press_ui("i");
    // Move the menu cursor away from the retained source cursor.
    test.goto(other_line);
    assert_eq!(test.cursor().line, other_line);
    let result = test.press_ui("s");

    assert!(
        message_texts(&result.effects)
            .iter()
            .any(|text| text.contains("parent .gitignore")),
        "{:?}",
        result.effects
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("dir/nested/.gitignore")).unwrap(),
        "# retained\n/a\\*b.txt\n"
    );

    // No other scope was written: root ignore, repository exclude and the
    // isolated user excludes file all remain byte-for-byte unchanged.
    assert_eq!(
        std::fs::read_to_string(tmp.path().join(".gitignore")).ok(),
        root_ignore_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join(".git/info/exclude")).unwrap_or_default(),
        repo_exclude_before
    );
    assert_eq!(
        std::fs::read_to_string(&user_ignore_path).ok(),
        user_ignore_before
    );

    let ignored = |path: &str| -> bool {
        std::process::Command::new("git")
            .args(["check-ignore", "--no-index", path])
            .current_dir(tmp.path())
            .output()
            .expect("run git check-ignore")
            .status
            .success()
    };
    assert!(
        ignored("dir/nested/a*b.txt"),
        "the literal file in the selected parent must be ignored"
    );
    assert!(
        !ignored("dir/nested/axxb.txt"),
        "the wildcard must be escaped, not expanded"
    );
    assert!(
        !ignored("dir/a*b.txt"),
        "the same basename outside the selected parent must not be ignored"
    );
    assert!(
        !ignored("base.txt"),
        "an unrelated file must not be ignored"
    );
    assert!(
        !ignored("other/other.txt"),
        "the menu cursor's file must not be ignored"
    );

    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["write-tree"]), index);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("base.txt")).unwrap(),
        base_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("dir/nested/a*b.txt")).unwrap(),
        selected_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("dir/nested/axxb.txt")).unwrap(),
        inside_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("dir/a*b.txt")).unwrap(),
        outside_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("other/other.txt")).unwrap(),
        other_before
    );
}

#[test]
fn ignore_repository_literal_is_private_and_scoped() {
    use magit_lsp::protocol::PromptChoiceAnswer;
    let (tmp, _) = RepoFixture::new()
        .commit(
            "init",
            [
                ("base.txt", "base\n"),
                ("dir/a*b.txt", "original\n"),
                ("dir/axxb.txt", "decoy\n"),
                ("other/other.txt", "other\n"),
            ],
        )
        .modify("dir/a*b.txt", "changed\n")
        .modify("other/other.txt", "changed too\n")
        .build();
    // Pre-seed the repository exclude file and place an untracked decoy with
    // the same basename outside the selected directory. The rule is private
    // to this repository, relative to its root and anchored to the selected
    // path, so it must not affect that decoy.
    std::fs::write(tmp.path().join(".git/info/exclude"), "# retained\n").unwrap();
    std::fs::write(tmp.path().join("other/a*b.txt"), "decoy outside\n").unwrap();

    let head = tmp.git(&["rev-parse", "HEAD"]);
    let index = tmp.git(&["write-tree"]);
    let base_before = std::fs::read_to_string(tmp.path().join("base.txt")).unwrap();
    let selected_before = std::fs::read_to_string(tmp.path().join("dir/a*b.txt")).unwrap();
    let decoy_before = std::fs::read_to_string(tmp.path().join("dir/axxb.txt")).unwrap();
    let outside_before = std::fs::read_to_string(tmp.path().join("other/a*b.txt")).unwrap();
    let other_before = std::fs::read_to_string(tmp.path().join("other/other.txt")).unwrap();
    let root_ignore_before = std::fs::read_to_string(tmp.path().join(".gitignore")).ok();
    let subdir_ignore_before = std::fs::read_to_string(tmp.path().join("dir/.gitignore")).ok();
    let user_ignore_path =
        std::path::PathBuf::from(std::env::var("XDG_CONFIG_HOME").unwrap()).join("git/ignore");
    let user_ignore_before = std::fs::read_to_string(&user_ignore_path).ok();

    let (mut test, _) = TestSession::open_status(&tmp, Profile::Standard);
    test.ack();
    let line = test.goto_file("unstaged", "dir/a*b.txt");
    assert_eq!(test.cursor().line, line);
    // A different modified file supplies a menu cursor whose line resolves to
    // another file in the source view, so the action can only succeed if it
    // reads the retained source identity rather than the menu's own cursor.
    let other_line = test.goto_file("unstaged", "other/other.txt");
    assert_ne!(other_line, line);
    test.goto(line);

    // Real key ingress: the `i` opener, then the `p` repository-private
    // action. Queue the literal choice before pressing `p` so ScriptedUi
    // drives it through the pending prompt.
    test.ui().reply_choice(Some(PromptChoiceAnswer::Id {
        id: "literal".into(),
    }));
    test.press_ui("i");
    // Move the menu cursor away from the retained source cursor.
    test.goto(other_line);
    assert_eq!(test.cursor().line, other_line);
    let result = test.press_ui("p");

    assert!(
        message_texts(&result.effects)
            .iter()
            .any(|text| text.contains("repository info/exclude")),
        "{:?}",
        result.effects
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join(".git/info/exclude")).unwrap(),
        "# retained\n/dir/a\\*b.txt\n"
    );

    // No other scope was written: root/subdir ignores and the isolated user
    // excludes file all remain byte-for-byte unchanged.
    assert_eq!(
        std::fs::read_to_string(tmp.path().join(".gitignore")).ok(),
        root_ignore_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("dir/.gitignore")).ok(),
        subdir_ignore_before
    );
    assert_eq!(
        std::fs::read_to_string(&user_ignore_path).ok(),
        user_ignore_before
    );

    let ignored = |path: &str| -> bool {
        std::process::Command::new("git")
            .args(["check-ignore", "--no-index", path])
            .current_dir(tmp.path())
            .output()
            .expect("run git check-ignore")
            .status
            .success()
    };
    assert!(
        ignored("dir/a*b.txt"),
        "the literal file must be ignored via repository excludes"
    );
    assert!(
        !ignored("dir/axxb.txt"),
        "the wildcard must be escaped, not expanded"
    );
    assert!(
        !ignored("other/a*b.txt"),
        "the same basename outside the selected path must not be ignored"
    );
    assert!(
        !ignored("base.txt"),
        "an unrelated file must not be ignored"
    );
    assert!(
        !ignored("other/other.txt"),
        "the menu cursor's file must not be ignored"
    );

    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["write-tree"]), index);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("base.txt")).unwrap(),
        base_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("dir/a*b.txt")).unwrap(),
        selected_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("dir/axxb.txt")).unwrap(),
        decoy_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("other/a*b.txt")).unwrap(),
        outside_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("other/other.txt")).unwrap(),
        other_before
    );
}

#[test]
fn ignore_user_literal_requires_deliberate_confirmation() {
    use magit_lsp::protocol::{PromptChoiceAnswer, PromptKind};

    // A fixture-only private directory stands in for the user-wide excludes
    // file. The repository's LOCAL config points at it, so neither the real
    // HOME/XDG config nor any global file is ever touched.
    let private = tempfile::TempDir::new().expect("tempdir for private user excludes");
    let user_ignore = private.path().join("user-ignore");
    std::fs::write(&user_ignore, "# retained\n").unwrap();

    let (tmp, _) = RepoFixture::new()
        .commit(
            "init",
            [
                ("base.txt", "base\n"),
                ("dir/a*b.txt", "original\n"),
                ("dir/axxb.txt", "decoy\n"),
                ("other/a*b.txt", "decoy outside\n"),
                ("other/other.txt", "other\n"),
            ],
        )
        .modify("dir/a*b.txt", "changed\n")
        .modify("other/other.txt", "changed too\n")
        .build();
    let excludes_arg = user_ignore.to_string_lossy().into_owned();
    tmp.git(&["config", "core.excludesFile", &excludes_arg]);

    let head = tmp.git(&["rev-parse", "HEAD"]);
    let index = tmp.git(&["write-tree"]);
    let base_before = std::fs::read_to_string(tmp.path().join("base.txt")).unwrap();
    let selected_before = std::fs::read_to_string(tmp.path().join("dir/a*b.txt")).unwrap();
    let lookalike_before = std::fs::read_to_string(tmp.path().join("dir/axxb.txt")).unwrap();
    let outside_before = std::fs::read_to_string(tmp.path().join("other/a*b.txt")).unwrap();
    let other_before = std::fs::read_to_string(tmp.path().join("other/other.txt")).unwrap();
    let root_ignore_before = std::fs::read_to_string(tmp.path().join(".gitignore")).ok();
    let subdir_ignore_before = std::fs::read_to_string(tmp.path().join("dir/.gitignore")).ok();
    let repo_exclude_before =
        std::fs::read_to_string(tmp.path().join(".git/info/exclude")).unwrap_or_default();
    let user_ignore_before = std::fs::read_to_string(&user_ignore).unwrap();

    let ignored = |path: &str| -> bool {
        std::process::Command::new("git")
            .args(["check-ignore", "--no-index", path])
            .current_dir(tmp.path())
            .output()
            .expect("run git check-ignore")
            .status
            .success()
    };

    // First real ingress: literal choice, then DECLINE the user-wide
    // confirmation. The confirmation wording must be visible to the user and
    // declining must leave every byte untouched.
    {
        let (mut test, _) = TestSession::open_status(&tmp, Profile::Standard);
        test.ack();
        let line = test.goto_file("unstaged", "dir/a*b.txt");
        assert_eq!(test.cursor().line, line);
        test.ui()
            .reply_choice(Some(PromptChoiceAnswer::Id {
                id: "literal".into(),
            }))
            .reply_confirm(false);
        test.press_ui("i");
        let result = test.press_ui("g");

        let confirm = test
            .ui()
            .last_request()
            .expect("a user-wide confirmation prompt was shown");
        assert_eq!(confirm.kind, PromptKind::Confirm);
        assert_eq!(confirm.title, "Write user-wide ignore rule?");
        assert_eq!(confirm.yes_label.as_deref(), Some("Write user excludes"));
        let detail = confirm.detail.as_deref().unwrap_or_default();
        assert!(detail.contains("user-wide excludes file"), "{detail}");
        assert!(detail.contains("dir/a*b.txt"), "{detail}");
        assert!(
            !message_texts(&result.effects)
                .iter()
                .any(|text| text.contains("Added ignore rule")),
            "{:?}",
            result.effects
        );
    }
    assert_eq!(
        std::fs::read_to_string(&user_ignore).unwrap(),
        user_ignore_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join(".gitignore")).ok(),
        root_ignore_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("dir/.gitignore")).ok(),
        subdir_ignore_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join(".git/info/exclude")).unwrap_or_default(),
        repo_exclude_before
    );
    assert!(
        !ignored("dir/a*b.txt"),
        "a declined confirmation must not write a rule"
    );
    assert!(!ignored("dir/axxb.txt"));
    assert!(!ignored("other/a*b.txt"));
    assert!(!ignored("base.txt"));
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["write-tree"]), index);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("base.txt")).unwrap(),
        base_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("dir/a*b.txt")).unwrap(),
        selected_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("dir/axxb.txt")).unwrap(),
        lookalike_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("other/a*b.txt")).unwrap(),
        outside_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("other/other.txt")).unwrap(),
        other_before
    );

    // Third real ingress: custom pattern choice, then DECLINE the user-wide
    // confirmation. The prompt must name the pinned selected source path AND
    // the differing proposed pattern, state user-wide scope exactly once, and
    // declining must leave every byte untouched.
    {
        let (mut test, _) = TestSession::open_status(&tmp, Profile::Standard);
        test.ack();
        let line = test.goto_file("unstaged", "dir/a*b.txt");
        assert_eq!(test.cursor().line, line);
        let pattern = "dir/*.log";
        assert_ne!(
            pattern, "dir/a*b.txt",
            "pattern must differ from the selected source path"
        );
        test.ui()
            .reply_choice(Some(PromptChoiceAnswer::Id {
                id: "pattern".into(),
            }))
            .reply_text(Some(pattern.into()))
            .reply_confirm(false);
        test.press_ui("i");
        let result = test.press_ui("g");

        let confirm = test
            .ui()
            .last_request()
            .expect("a user-wide confirmation prompt was shown");
        assert_eq!(confirm.kind, PromptKind::Confirm);
        assert_eq!(confirm.title, "Write user-wide ignore rule?");
        assert_eq!(confirm.yes_label.as_deref(), Some("Write user excludes"));
        let detail = confirm.detail.as_deref().unwrap_or_default();
        assert_eq!(
            detail.matches("user-wide excludes file").count(),
            1,
            "user-wide scope must be named exactly once: {detail}"
        );
        assert!(
            detail.contains("dir/a*b.txt"),
            "selected source path must be visible: {detail}"
        );
        assert!(
            detail.contains(pattern),
            "proposed custom pattern must be visible: {detail}"
        );
        assert!(
            !message_texts(&result.effects)
                .iter()
                .any(|text| text.contains("Added ignore rule")),
            "{:?}",
            result.effects
        );
    }
    assert_eq!(
        std::fs::read_to_string(&user_ignore).unwrap(),
        user_ignore_before
    );
    assert!(
        !ignored("dir/a*b.txt"),
        "a declined custom-pattern confirmation must not write a rule"
    );

    // Second real ingress: literal choice, then ACCEPT the deliberate
    // confirmation. Only the configured private user excludes file changes.
    {
        let (mut test, _) = TestSession::open_status(&tmp, Profile::Standard);
        test.ack();
        let line = test.goto_file("unstaged", "dir/a*b.txt");
        assert_eq!(test.cursor().line, line);
        test.ui()
            .reply_choice(Some(PromptChoiceAnswer::Id {
                id: "literal".into(),
            }))
            .reply_confirm(true);
        test.press_ui("i");
        let result = test.press_ui("g");

        assert!(
            message_texts(&result.effects)
                .iter()
                .any(|text| text.contains("the user-wide excludes file")),
            "{:?}",
            result.effects
        );
        let confirm = test
            .ui()
            .last_request()
            .expect("a user-wide confirmation prompt was shown");
        assert_eq!(confirm.kind, PromptKind::Confirm);
        assert_eq!(confirm.title, "Write user-wide ignore rule?");
    }
    assert_eq!(
        std::fs::read_to_string(&user_ignore).unwrap(),
        "# retained\n/dir/a\\*b.txt\n"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join(".gitignore")).ok(),
        root_ignore_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("dir/.gitignore")).ok(),
        subdir_ignore_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join(".git/info/exclude")).unwrap_or_default(),
        repo_exclude_before
    );
    assert!(
        ignored("dir/a*b.txt"),
        "the accepted literal must be ignored via user excludes"
    );
    assert!(
        !ignored("dir/axxb.txt"),
        "the wildcard must be escaped, not expanded"
    );
    assert!(
        !ignored("other/a*b.txt"),
        "the same basename outside the selected path must not be ignored"
    );
    assert!(
        !ignored("base.txt"),
        "an unrelated file must not be ignored"
    );
    assert!(!ignored("other/other.txt"), "a decoy must not be ignored");
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["write-tree"]), index);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("base.txt")).unwrap(),
        base_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("dir/a*b.txt")).unwrap(),
        selected_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("dir/axxb.txt")).unwrap(),
        lookalike_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("other/a*b.txt")).unwrap(),
        outside_before
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("other/other.txt")).unwrap(),
        other_before
    );
}

#[test]
fn ignore_root_custom_pattern_is_deliberate() {
    use magit_lsp::protocol::{Cursor, PromptChoiceAnswer, PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit(
            "init",
            [
                ("base.txt", "base\n"),
                ("dir/a*b.txt", "original\n"),
                ("cache.tmp", "cache\n"),
                ("dir/nested.tmp", "nested\n"),
                ("keep.txt", "keep\n"),
            ],
        )
        .modify("dir/a*b.txt", "changed\n")
        .build();

    let head = tmp.git(&["rev-parse", "HEAD"]);
    let index = tmp.git(&["write-tree"]);
    let target_before = std::fs::read_to_string(tmp.path().join("dir/a*b.txt")).unwrap();

    let (mut test, _) = TestSession::open_status(&tmp, Profile::Standard);
    test.ack();
    let line = test.goto_file("unstaged", "dir/a*b.txt");

    let started = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "ignore.topdir".into(),
            version: Some(test.version()),
            cursor: Some(Cursor { line, column: 0 }),
            selection: None,
            args: None,
        })
        .unwrap();
    let Execution::Prompt(kind) = started else {
        panic!("expected ignore-rule-kind prompt, got {started:?}");
    };
    assert_eq!(kind.kind, magit_lsp::protocol::PromptKind::Choice);
    let items = kind.items.as_ref().expect("choice items");
    assert!(items.iter().any(|item| item.id == "literal"));
    assert!(items.iter().any(|item| item.id == "pattern"));
    assert!(
        kind.detail
            .as_deref()
            .is_some_and(|detail| detail.contains("dir/a*b.txt")),
        "{:?}",
        kind.detail
    );

    let Execution::Prompt(value) = test
        .session
        .resume_prompt(
            &kind.prompt_id,
            Some(PromptValue::Choice(PromptChoiceAnswer::Id {
                id: "pattern".into(),
            })),
        )
        .unwrap()
    else {
        panic!("expected custom-pattern prompt");
    };
    assert!(
        value
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("dir/a*b.txt")),
        "the selected path stays pinned across prompts: {:?}",
        value.detail
    );
    let Execution::Done(result) = test
        .session
        .resume_prompt(&value.prompt_id, Some(PromptValue::Text("*.tmp".into())))
        .unwrap()
    else {
        panic!("custom pattern must finish without another prompt");
    };
    assert!(
        message_texts(&result.effects)
            .iter()
            .any(|text| text.contains("root .gitignore")),
        "{:?}",
        result.effects
    );

    assert_eq!(
        std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap(),
        "*.tmp\n"
    );
    let ignored = |path: &str| -> bool {
        std::process::Command::new("git")
            .args(["check-ignore", "--no-index", path])
            .current_dir(tmp.path())
            .output()
            .expect("run git check-ignore")
            .status
            .success()
    };
    assert!(ignored("cache.tmp"), "the custom pattern matches at root");
    assert!(
        ignored("dir/nested.tmp"),
        "the custom pattern matches in a subdirectory"
    );
    assert!(!ignored("keep.txt"), "the custom pattern is selective");
    assert!(
        !ignored("dir/a*b.txt"),
        "a pattern is distinct from A1's literal selected path"
    );

    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["write-tree"]), index);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("dir/a*b.txt")).unwrap(),
        target_before
    );
}

#[test]
fn ignore_rule_cancellation_never_writes() {
    use magit_lsp::protocol::{
        Cursor, PromptChoiceAnswer, PromptKind, PromptValue, ViewInvokeParams,
    };
    use magit_lsp::session::Execution;

    // A fixture-only private file stands in for the user-wide excludes file;
    // neither the real HOME/XDG config nor any global file is ever touched.
    let private = tempfile::TempDir::new().expect("tempdir for private user excludes");
    let user_ignore = private.path().join("user-ignore");
    std::fs::write(&user_ignore, "# retained\n").unwrap();

    let (tmp, _) = RepoFixture::new()
        .commit(
            "init",
            [
                ("base.txt", "base\n"),
                ("dir/a*b.txt", "original\n"),
                ("dir/axxb.txt", "decoy\n"),
                ("other/other.txt", "other\n"),
            ],
        )
        .modify("dir/a*b.txt", "changed\n")
        .build();
    let excludes_arg = user_ignore.to_string_lossy().into_owned();
    tmp.git(&["config", "core.excludesFile", &excludes_arg]);

    // Seed all four possible ignore destinations with exact retained bytes.
    std::fs::write(tmp.path().join(".gitignore"), "# retained\n").unwrap();
    std::fs::write(tmp.path().join("dir/.gitignore"), "# retained\n").unwrap();
    std::fs::write(tmp.path().join(".git/info/exclude"), "# retained\n").unwrap();

    let head = tmp.git(&["rev-parse", "HEAD"]);
    let index = tmp.git(&["write-tree"]);
    let status = tmp.git(&["status", "--porcelain=v2", "-z"]);
    let root_before = std::fs::read(tmp.path().join(".gitignore")).unwrap();
    let subdir_before = std::fs::read(tmp.path().join("dir/.gitignore")).unwrap();
    let repo_before = std::fs::read(tmp.path().join(".git/info/exclude")).unwrap();
    let user_before = std::fs::read(&user_ignore).unwrap();
    let selected_before = std::fs::read(tmp.path().join("dir/a*b.txt")).unwrap();
    let base_before = std::fs::read(tmp.path().join("base.txt")).unwrap();
    let decoy_before = std::fs::read(tmp.path().join("dir/axxb.txt")).unwrap();
    let other_before = std::fs::read(tmp.path().join("other/other.txt")).unwrap();

    let assert_unchanged = |stage: &str| {
        assert_eq!(
            std::fs::read(tmp.path().join(".gitignore")).unwrap(),
            root_before,
            "{stage}: root .gitignore"
        );
        assert_eq!(
            std::fs::read(tmp.path().join("dir/.gitignore")).unwrap(),
            subdir_before,
            "{stage}: subdir .gitignore"
        );
        assert_eq!(
            std::fs::read(tmp.path().join(".git/info/exclude")).unwrap(),
            repo_before,
            "{stage}: repo exclude"
        );
        assert_eq!(
            std::fs::read(&user_ignore).unwrap(),
            user_before,
            "{stage}: user excludes"
        );
        assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head, "{stage}: HEAD");
        assert_eq!(tmp.git(&["write-tree"]), index, "{stage}: index");
        assert_eq!(
            tmp.git(&["status", "--porcelain=v2", "-z"]),
            status,
            "{stage}: status"
        );
        assert_eq!(
            std::fs::read(tmp.path().join("dir/a*b.txt")).unwrap(),
            selected_before,
            "{stage}: selected file"
        );
        assert_eq!(
            std::fs::read(tmp.path().join("base.txt")).unwrap(),
            base_before,
            "{stage}: base file"
        );
        assert_eq!(
            std::fs::read(tmp.path().join("dir/axxb.txt")).unwrap(),
            decoy_before,
            "{stage}: decoy file"
        );
        assert_eq!(
            std::fs::read(tmp.path().join("other/other.txt")).unwrap(),
            other_before,
            "{stage}: other file"
        );
    };
    let assert_no_success = |effects: &[Effect], stage: &str| {
        let messages = message_texts(effects);
        assert!(
            !messages
                .iter()
                .any(|text| text.contains("Added ignore rule")),
            "{stage}: unexpected success message {messages:?}"
        );
    };
    let start = |test: &mut TestSession, command: &str, line: usize| {
        test.session
            .start_invoke(&ViewInvokeParams {
                view_id: test.view_id().to_string(),
                command: command.into(),
                version: Some(test.version()),
                cursor: Some(Cursor { line, column: 0 }),
                selection: None,
                args: None,
            })
            .unwrap()
    };
    let open = || {
        let (mut test, _) = TestSession::open_status(&tmp, Profile::Standard);
        test.ack();
        let line = test.goto_file("unstaged", "dir/a*b.txt");
        (test, line)
    };

    // (1) Cancel at the initial kind choice for ignore.topdir (None answer).
    {
        let (mut test, line) = open();
        let started = start(&mut test, "ignore.topdir", line);
        let Execution::Prompt(kind) = started else {
            panic!("expected ignore-rule-kind prompt, got {started:?}");
        };
        assert_eq!(kind.kind, PromptKind::Choice);
        let done = test.session.resume_prompt(&kind.prompt_id, None).unwrap();
        let Execution::Done(result) = done else {
            panic!("cancelling the kind prompt must finish, got {done:?}");
        };
        assert_no_success(&result.effects, "kind-choice cancel");
    }
    assert_unchanged("after kind-choice cancellation");

    // (2) Select "pattern", then cancel at the text-pattern prompt for ignore.topdir.
    {
        let (mut test, line) = open();
        let started = start(&mut test, "ignore.topdir", line);
        let Execution::Prompt(kind) = started else {
            panic!("expected ignore-rule-kind prompt, got {started:?}");
        };
        let Execution::Prompt(value) = test
            .session
            .resume_prompt(
                &kind.prompt_id,
                Some(PromptValue::Choice(PromptChoiceAnswer::Id {
                    id: "pattern".into(),
                })),
            )
            .unwrap()
        else {
            panic!("expected custom-pattern prompt");
        };
        let done = test.session.resume_prompt(&value.prompt_id, None).unwrap();
        let Execution::Done(result) = done else {
            panic!("cancelling the pattern prompt must finish, got {done:?}");
        };
        assert_no_success(&result.effects, "pattern-prompt cancel");
    }
    assert_unchanged("after pattern-prompt cancellation");

    // (3) Choose "literal", then cancel at the user-wide confirmation for ignore.system.
    {
        let (mut test, line) = open();
        let started = start(&mut test, "ignore.system", line);
        let Execution::Prompt(kind) = started else {
            panic!("expected ignore-rule-kind prompt, got {started:?}");
        };
        let Execution::Prompt(confirm) = test
            .session
            .resume_prompt(
                &kind.prompt_id,
                Some(PromptValue::Choice(PromptChoiceAnswer::Id {
                    id: "literal".into(),
                })),
            )
            .unwrap()
        else {
            panic!("expected user-wide confirmation prompt");
        };
        assert_eq!(confirm.kind, PromptKind::Confirm);
        assert_eq!(confirm.title, "Write user-wide ignore rule?");
        let done = test
            .session
            .resume_prompt(&confirm.prompt_id, None)
            .unwrap();
        let Execution::Done(result) = done else {
            panic!("cancelling the user-wide confirmation must finish, got {done:?}");
        };
        assert_no_success(&result.effects, "user-confirm cancel");
    }
    assert_unchanged("after user-confirm cancellation");
}

#[test]
fn ignore_rule_invalid_inputs_never_write() {
    use magit_lsp::protocol::{Cursor, PromptChoiceAnswer, PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    use magit_lsp::tree::NodeKind;

    // Fixture-only private user excludes; never the real HOME/XDG/global files.
    let private = tempfile::TempDir::new().expect("tempdir for private user excludes");
    let user_ignore = private.path().join("user-ignore");
    std::fs::write(&user_ignore, "# retained\n").unwrap();

    let (tmp, _) = RepoFixture::new()
        .commit(
            "init",
            [
                ("base.txt", "base\n"),
                ("dir/a*b.txt", "original\n"),
                ("dir/axxb.txt", "decoy\n"),
                ("other/other.txt", "other\n"),
            ],
        )
        .modify("dir/a*b.txt", "changed\n")
        .build();
    let excludes_arg = user_ignore.to_string_lossy().into_owned();
    tmp.git(&["config", "core.excludesFile", &excludes_arg]);

    std::fs::write(tmp.path().join(".gitignore"), "# retained\n").unwrap();
    std::fs::write(tmp.path().join("dir/.gitignore"), "# retained\n").unwrap();
    std::fs::write(tmp.path().join(".git/info/exclude"), "# retained\n").unwrap();

    let head = tmp.git(&["rev-parse", "HEAD"]);
    let index = tmp.git(&["write-tree"]);
    let status = tmp.git(&["status", "--porcelain=v2", "-z"]);
    let root_before = std::fs::read(tmp.path().join(".gitignore")).unwrap();
    let subdir_before = std::fs::read(tmp.path().join("dir/.gitignore")).unwrap();
    let repo_before = std::fs::read(tmp.path().join(".git/info/exclude")).unwrap();
    let user_before = std::fs::read(&user_ignore).unwrap();
    let selected_before = std::fs::read(tmp.path().join("dir/a*b.txt")).unwrap();
    let base_before = std::fs::read(tmp.path().join("base.txt")).unwrap();
    let decoy_before = std::fs::read(tmp.path().join("dir/axxb.txt")).unwrap();
    let other_before = std::fs::read(tmp.path().join("other/other.txt")).unwrap();

    let assert_unchanged = |stage: &str| {
        assert_eq!(
            std::fs::read(tmp.path().join(".gitignore")).unwrap(),
            root_before,
            "{stage}: root .gitignore"
        );
        assert_eq!(
            std::fs::read(tmp.path().join("dir/.gitignore")).unwrap(),
            subdir_before,
            "{stage}: subdir .gitignore"
        );
        assert_eq!(
            std::fs::read(tmp.path().join(".git/info/exclude")).unwrap(),
            repo_before,
            "{stage}: repo exclude"
        );
        assert_eq!(
            std::fs::read(&user_ignore).unwrap(),
            user_before,
            "{stage}: user excludes"
        );
        assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head, "{stage}: HEAD");
        assert_eq!(tmp.git(&["write-tree"]), index, "{stage}: index");
        assert_eq!(
            tmp.git(&["status", "--porcelain=v2", "-z"]),
            status,
            "{stage}: status"
        );
        assert_eq!(
            std::fs::read(tmp.path().join("dir/a*b.txt")).unwrap(),
            selected_before,
            "{stage}: selected file"
        );
        assert_eq!(
            std::fs::read(tmp.path().join("base.txt")).unwrap(),
            base_before,
            "{stage}: base file"
        );
        assert_eq!(
            std::fs::read(tmp.path().join("dir/axxb.txt")).unwrap(),
            decoy_before,
            "{stage}: decoy file"
        );
        assert_eq!(
            std::fs::read(tmp.path().join("other/other.txt")).unwrap(),
            other_before,
            "{stage}: other file"
        );
    };
    let assert_no_success = |effects: &[Effect], stage: &str| {
        let messages = message_texts(effects);
        assert!(
            !messages
                .iter()
                .any(|text| text.contains("Added ignore rule")),
            "{stage}: unexpected success message {messages:?}"
        );
    };
    let start = |test: &mut TestSession, command: &str, line: usize| {
        test.session
            .start_invoke(&ViewInvokeParams {
                view_id: test.view_id().to_string(),
                command: command.into(),
                version: Some(test.version()),
                cursor: Some(Cursor { line, column: 0 }),
                selection: None,
                args: None,
            })
            .unwrap()
    };
    let open_pattern = || {
        let (mut test, _) = TestSession::open_status(&tmp, Profile::Standard);
        test.ack();
        let line = test.goto_file("unstaged", "dir/a*b.txt");
        let started = start(&mut test, "ignore.topdir", line);
        let Execution::Prompt(kind) = started else {
            panic!("expected ignore-rule-kind prompt, got {started:?}");
        };
        let Execution::Prompt(value) = test
            .session
            .resume_prompt(
                &kind.prompt_id,
                Some(PromptValue::Choice(PromptChoiceAnswer::Id {
                    id: "pattern".into(),
                })),
            )
            .unwrap()
        else {
            panic!("expected custom-pattern prompt");
        };
        (test, value.prompt_id)
    };

    // (a) No File node under the status cursor: readable refusal, no prompt.
    {
        let (mut test, _) = TestSession::open_status(&tmp, Profile::Standard);
        test.ack();
        let header = test.tree().navigation_node(0);
        assert!(
            header.map_or(true, |node| node.kind != NodeKind::File),
            "line 0 must not be a File node for the missing-selection case"
        );
        let started = start(&mut test, "ignore.topdir", 0);
        let Execution::Done(result) = started else {
            panic!("missing selection must finish without a prompt, got {started:?}");
        };
        assert!(
            message_texts(&result.effects)
                .iter()
                .any(|text| text.contains("Select a file")),
            "expected readable selection message, got {:?}",
            message_texts(&result.effects)
        );
        assert_no_success(&result.effects, "missing selection");
    }
    assert_unchanged("after missing selection");

    // (b) Empty custom pattern: deliberate no-op with a readable message.
    {
        let (mut test, pattern_id) = open_pattern();
        let done = test
            .session
            .resume_prompt(&pattern_id, Some(PromptValue::Text("".into())))
            .unwrap();
        let Execution::Done(result) = done else {
            panic!("empty pattern must finish, got {done:?}");
        };
        assert!(
            message_texts(&result.effects)
                .iter()
                .any(|text| text.contains("Enter one non-empty ignore pattern")),
            "expected invalid-pattern message, got {:?}",
            message_texts(&result.effects)
        );
        assert_no_success(&result.effects, "empty pattern");
    }
    assert_unchanged("after empty pattern");

    // (b) Whitespace-only custom pattern is likewise refused.
    {
        let (mut test, pattern_id) = open_pattern();
        let done = test
            .session
            .resume_prompt(&pattern_id, Some(PromptValue::Text("   \t ".into())))
            .unwrap();
        let Execution::Done(result) = done else {
            panic!("whitespace pattern must finish, got {done:?}");
        };
        assert!(
            message_texts(&result.effects)
                .iter()
                .any(|text| text.contains("Enter one non-empty ignore pattern")),
            "expected invalid-pattern message, got {:?}",
            message_texts(&result.effects)
        );
        assert_no_success(&result.effects, "whitespace pattern");
    }
    assert_unchanged("after whitespace pattern");

    // (c) Custom pattern containing a newline is refused.
    {
        let (mut test, pattern_id) = open_pattern();
        let done = test
            .session
            .resume_prompt(&pattern_id, Some(PromptValue::Text("foo\nbar".into())))
            .unwrap();
        let Execution::Done(result) = done else {
            panic!("newline pattern must finish, got {done:?}");
        };
        assert!(
            message_texts(&result.effects)
                .iter()
                .any(|text| text.contains("Enter one non-empty ignore pattern")),
            "expected invalid-pattern message, got {:?}",
            message_texts(&result.effects)
        );
        assert_no_success(&result.effects, "newline pattern");
    }
    assert_unchanged("after newline pattern");
}

#[test]
fn ignore_rule_write_failure_is_readable_and_never_claims_success() {
    use magit_lsp::protocol::{Cursor, PromptChoiceAnswer, PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;

    // Fixture-only private user excludes; never the real HOME/XDG/global files.
    let private = tempfile::TempDir::new().expect("tempdir for private user excludes");
    let user_ignore = private.path().join("user-ignore");
    std::fs::write(&user_ignore, "# retained\n").unwrap();

    let (tmp, _) = RepoFixture::new()
        .commit(
            "init",
            [
                ("base.txt", "base\n"),
                ("dir/a*b.txt", "original\n"),
                ("dir/axxb.txt", "decoy\n"),
                ("other/other.txt", "other\n"),
            ],
        )
        .modify("dir/a*b.txt", "changed\n")
        .build();
    let excludes_arg = user_ignore.to_string_lossy().into_owned();
    tmp.git(&["config", "core.excludesFile", &excludes_arg]);

    // Make the root .gitignore destination a directory holding a sentinel file.
    // Opening a directory for append fails deterministically, with no dependence
    // on chmod/permission semantics; the sentinel proves the directory is intact.
    let root_ignore = tmp.path().join(".gitignore");
    std::fs::create_dir(&root_ignore).unwrap();
    let sentinel = root_ignore.join("sentinel");
    let sentinel_bytes = b"sentinel-bytes\n".to_vec();
    std::fs::write(&sentinel, &sentinel_bytes).unwrap();

    std::fs::write(tmp.path().join("dir/.gitignore"), "# retained\n").unwrap();
    std::fs::write(tmp.path().join(".git/info/exclude"), "# retained\n").unwrap();

    // Snapshot AFTER fixture seeding: the root destination is a directory, so
    // assert it via is_dir plus the sentinel file rather than reading a directory.
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let index = tmp.git(&["write-tree"]);
    let status = tmp.git(&["status", "--porcelain=v2", "-z"]);
    let subdir_before = std::fs::read(tmp.path().join("dir/.gitignore")).unwrap();
    let repo_before = std::fs::read(tmp.path().join(".git/info/exclude")).unwrap();
    let user_before = std::fs::read(&user_ignore).unwrap();
    let selected_before = std::fs::read(tmp.path().join("dir/a*b.txt")).unwrap();
    let base_before = std::fs::read(tmp.path().join("base.txt")).unwrap();
    let decoy_before = std::fs::read(tmp.path().join("dir/axxb.txt")).unwrap();
    let other_before = std::fs::read(tmp.path().join("other/other.txt")).unwrap();

    let assert_unchanged = |stage: &str| {
        assert!(
            root_ignore.is_dir(),
            "{stage}: root .gitignore must remain a directory"
        );
        assert_eq!(
            std::fs::read(&sentinel).unwrap(),
            sentinel_bytes,
            "{stage}: sentinel bytes"
        );
        assert_eq!(
            std::fs::read(tmp.path().join("dir/.gitignore")).unwrap(),
            subdir_before,
            "{stage}: subdir .gitignore"
        );
        assert_eq!(
            std::fs::read(tmp.path().join(".git/info/exclude")).unwrap(),
            repo_before,
            "{stage}: repo exclude"
        );
        assert_eq!(
            std::fs::read(&user_ignore).unwrap(),
            user_before,
            "{stage}: user excludes"
        );
        assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head, "{stage}: HEAD");
        assert_eq!(tmp.git(&["write-tree"]), index, "{stage}: index");
        assert_eq!(
            tmp.git(&["status", "--porcelain=v2", "-z"]),
            status,
            "{stage}: status"
        );
        assert_eq!(
            std::fs::read(tmp.path().join("dir/a*b.txt")).unwrap(),
            selected_before,
            "{stage}: selected file"
        );
        assert_eq!(
            std::fs::read(tmp.path().join("base.txt")).unwrap(),
            base_before,
            "{stage}: base file"
        );
        assert_eq!(
            std::fs::read(tmp.path().join("dir/axxb.txt")).unwrap(),
            decoy_before,
            "{stage}: decoy file"
        );
        assert_eq!(
            std::fs::read(tmp.path().join("other/other.txt")).unwrap(),
            other_before,
            "{stage}: other file"
        );
    };

    let (mut test, _) = TestSession::open_status(&tmp, Profile::Standard);
    test.ack();
    let line = test.goto_file("unstaged", "dir/a*b.txt");
    let started = test
        .session
        .start_invoke(&ViewInvokeParams {
            view_id: test.view_id().to_string(),
            command: "ignore.topdir".into(),
            version: Some(test.version()),
            cursor: Some(Cursor { line, column: 0 }),
            selection: None,
            args: None,
        })
        .unwrap();
    let Execution::Prompt(kind) = started else {
        panic!("expected ignore-rule-kind prompt, got {started:?}");
    };
    let done = test
        .session
        .resume_prompt(
            &kind.prompt_id,
            Some(PromptValue::Choice(PromptChoiceAnswer::Id {
                id: "literal".into(),
            })),
        )
        .unwrap();
    let Execution::Done(result) = done else {
        panic!("literal ignore write must finish without another prompt, got {done:?}");
    };

    let messages = message_texts(&result.effects);
    // A readable, nonempty user-facing error: not empty and not raw JSON or a
    // Rust debug rendering of an error/result value.
    let readable = messages.iter().copied().find(|text| {
        let trimmed = text.trim();
        !trimmed.is_empty()
            && !trimmed.starts_with('{')
            && !trimmed.contains("\"summary\"")
            && !trimmed.starts_with("Error(")
            && !trimmed.starts_with("Ok(")
            && !trimmed.starts_with("Err(")
    });
    assert!(
        readable.is_some(),
        "expected a readable nonempty failure message, got {messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|text| text.contains("Added ignore rule")),
        "a failed write must not claim success, got {messages:?}"
    );
    assert_unchanged("after failed root ignore write");
}

#[test]
fn ignore_root_stale_origin_is_refused_without_redirecting() {
    use magit_lsp::errors::STALE_VIEW;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("base.txt", "base\n")])
        .build();
    std::fs::write(tmp.path().join("first.txt"), "first\n").unwrap();
    std::fs::write(tmp.path().join("second.txt"), "second\n").unwrap();

    let head = tmp.git(&["rev-parse", "HEAD"]);
    let index = tmp.git(&["write-tree"]);

    let (mut test, _) = TestSession::open_status(&tmp, Profile::Standard);
    test.ack();
    let status_id = test.view_id().to_string();
    let first_line = test.goto_file("untracked", "first.txt");
    let second_line = test.goto_file("untracked", "second.txt");
    assert!(second_line > first_line);
    test.goto(first_line);

    test.press_ui("i");

    // The client applies a refreshed status view: the origin file is gone and
    // the retained line now names a different file in the acknowledged tree.
    std::fs::remove_file(tmp.path().join("first.txt")).unwrap();
    test.session.refresh(&status_id).expect("refresh status");
    let refreshed_version = test
        .session
        .views()
        .get(&status_id)
        .expect("status view")
        .version();
    test.session
        .ack(&status_id, refreshed_version)
        .expect("ack refreshed status");
    let acked = test
        .session
        .views()
        .get(&status_id)
        .expect("status view")
        .acked_tree()
        .expect("acked status tree")
        .clone();
    let redirected = &acked.line_nodes[first_line];
    assert_eq!(redirected.kind, magit_lsp::tree::NodeKind::File);
    assert_eq!(redirected.path.as_deref(), Some("second.txt"));

    let result = test.press_ui("t");
    assert_eq!(message_texts(&result.effects), vec![STALE_VIEW]);
    assert!(
        !tmp.path().join(".gitignore").exists(),
        "a stale origin must not write any ignore rule"
    );
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["write-tree"]), index);
}

/// Read one file's index flag from `git ls-files -v -z` output (`S`/`h`/`H`).
fn skip_worktree_flag<'a>(listing: &'a str, target: &str) -> Option<char> {
    listing.split('\0').find_map(|entry| {
        let tag = entry.chars().next()?;
        let path = entry[tag.len_utf8()..].strip_prefix(' ')?;
        (path == target).then_some(tag)
    })
}

#[test]
fn skip_worktree_commands_set_and_clear_explicit_path() {
    use magit_lsp::protocol::PromptKind;

    let path = "dir/a b*.txt";
    let (tmp, _) = RepoFixture::new()
        .commit(
            "init",
            [
                (path, "original\n"),
                ("dir/other.txt", "other\n"),
                ("base.txt", "base\n"),
            ],
        )
        .modify(path, "changed\n")
        .build();
    let flags = || tmp.git(&["ls-files", "-v", "-z"]);
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let staged = tmp.git(&["ls-files", "--stage"]);
    let target_before = std::fs::read(tmp.path().join(path)).unwrap();
    let other_before = std::fs::read(tmp.path().join("dir/other.txt")).unwrap();
    let base_before = std::fs::read(tmp.path().join("base.txt")).unwrap();
    assert_eq!(
        skip_worktree_flag(&flags(), path),
        Some('H'),
        "precondition"
    );
    assert_eq!(
        skip_worktree_flag(&flags(), "dir/other.txt"),
        Some('H'),
        "precondition"
    );

    let (mut test, _) = TestSession::open_status(&tmp, Profile::Standard);
    test.ack();
    let line = test.goto_file("unstaged", path);
    assert_eq!(test.cursor().line, line);

    // Real `i`/`w` ingress with the selected wildcard-and-space path; the
    // answer is typed literally and must never be split or expanded.
    test.ui().reply_text(Some(path.to_string()));
    test.press_ui("i");
    let set = test.press_ui("w");
    let asked = test.ui().last_request().expect("one text prompt");
    assert_eq!(asked.kind, PromptKind::Text);
    assert_eq!(asked.default.as_ref().and_then(|v| v.as_str()), Some(path));
    assert!(
        message_texts(&set.effects)
            .iter()
            .any(|text| text.contains("Set skip-worktree") && text.contains(path)),
        "{:?}",
        set.effects
    );
    assert_eq!(skip_worktree_flag(&flags(), path), Some('S'), "{}", flags());
    assert_eq!(skip_worktree_flag(&flags(), "dir/other.txt"), Some('H'));

    // An independent assume-unchanged bit on the same target must survive the
    // clear below; setting it also hides the file from status, as in Magit.
    tmp.git(&["update-index", "--assume-unchanged", "--", path]);
    assert_eq!(skip_worktree_flag(&flags(), path), Some('s'), "{}", flags());

    // Clear from the status header: no file row under the cursor, so the
    // default is blank and the now-hidden path is entered by hand.
    test.goto(0);
    assert_ne!(
        test.tree().line_nodes.first().map(|node| node.kind),
        Some(magit_lsp::tree::NodeKind::File)
    );
    test.ui().reply_text(Some(path.to_string()));
    test.press_ui("i");
    let clear = test.press_ui("W");
    let asked = test.ui().last_request().expect("second text prompt");
    assert_eq!(asked.kind, PromptKind::Text);
    assert!(
        asked.default.is_none(),
        "blank default without a selected file: {:?}",
        asked.default
    );
    assert!(
        message_texts(&clear.effects)
            .iter()
            .any(|text| text.contains("Cleared skip-worktree") && text.contains(path)),
        "{:?}",
        clear.effects
    );
    assert_eq!(skip_worktree_flag(&flags(), path), Some('h'), "{}", flags());
    assert_eq!(skip_worktree_flag(&flags(), "dir/other.txt"), Some('H'));

    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["ls-files", "--stage"]), staged);
    assert_eq!(std::fs::read(tmp.path().join(path)).unwrap(), target_before);
    assert_eq!(
        std::fs::read(tmp.path().join("dir/other.txt")).unwrap(),
        other_before
    );
    assert_eq!(
        std::fs::read(tmp.path().join("base.txt")).unwrap(),
        base_before
    );
}

#[test]
fn assume_unchanged_commands_set_and_clear_explicit_path() {
    use magit_lsp::protocol::PromptKind;

    let path = "dir/a b*.txt";
    let (tmp, _) = RepoFixture::new()
        .commit("init", [(path, "original\n"), ("dir/other.txt", "other\n")])
        .modify(path, "changed\n")
        .build();
    let flags = || tmp.git(&["ls-files", "-v", "-z"]);
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let staged = tmp.git(&["ls-files", "--stage"]);
    let target_before = std::fs::read(tmp.path().join(path)).unwrap();
    let other_before = std::fs::read(tmp.path().join("dir/other.txt")).unwrap();
    assert_eq!(skip_worktree_flag(&flags(), path), Some('H'));
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Standard);
    test.ack();
    test.goto_file("unstaged", path);
    tmp.git(&["update-index", "--skip-worktree", "--", path]);
    test.ui().reply_text(Some(path.to_string()));
    test.press_ui("i");
    let set = test.press_ui("u");
    let asked = test.ui().last_request().expect("path prompt");
    assert_eq!(asked.kind, PromptKind::Text);
    assert_eq!(asked.default.as_ref().and_then(|v| v.as_str()), Some(path));
    assert!(message_texts(&set.effects)
        .iter()
        .any(|m| m.contains("Set assume-unchanged") && m.contains(path)));
    assert_eq!(skip_worktree_flag(&flags(), path), Some('s'));
    assert_eq!(skip_worktree_flag(&flags(), "dir/other.txt"), Some('H'));
    test.goto(0);
    test.ui().reply_text(Some(path.to_string()));
    test.press_ui("i");
    let clear = test.press_ui("U");
    assert!(test.ui().last_request().unwrap().default.is_none());
    assert!(message_texts(&clear.effects)
        .iter()
        .any(|m| m.contains("Cleared assume-unchanged") && m.contains(path)));
    assert_eq!(skip_worktree_flag(&flags(), path), Some('S'));
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["ls-files", "--stage"]), staged);
    assert_eq!(std::fs::read(tmp.path().join(path)).unwrap(), target_before);
    assert_eq!(
        std::fs::read(tmp.path().join("dir/other.txt")).unwrap(),
        other_before
    );
}

#[test]
fn assume_unchanged_commands_refuse_cancel_invalid_and_locked_writes() {
    let path = "dir/a b*.txt";
    let (tmp, _) = RepoFixture::new()
        .commit("init", [(path, "original\n")])
        .modify(path, "changed\n")
        .build();
    std::fs::write(tmp.path().join("untracked.txt"), "untracked\n").unwrap();
    let flags = || tmp.git(&["ls-files", "-v", "-z"]);
    let before = std::fs::read(tmp.path().join(".git/index")).unwrap();
    let initial_flags = flags();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Standard);
    test.ack();
    let line = test.goto_file("unstaged", path);
    test.ui().reply_cancel();
    test.press_ui("i");
    let cancelled = test.press_ui("u");
    assert!(!message_texts(&cancelled.effects)
        .iter()
        .any(|m| m.contains("Set assume-unchanged")));
    for input in ["untracked.txt", "../escape"] {
        test.goto(line);
        test.ui().reply_text(Some(input.into()));
        test.press_ui("i");
        let result = test.press_ui("u");
        let texts = message_texts(&result.effects);
        assert!(texts
            .iter()
            .any(|m| !m.trim().is_empty() && !m.contains('{')));
        assert!(!texts.iter().any(|m| m.contains("Set assume-unchanged")));
    }
    std::fs::write(tmp.path().join(".git/index.lock"), "locked").unwrap();
    test.goto(line);
    test.ui().reply_text(Some(path.into()));
    test.press_ui("i");
    let locked = test.press_ui("u");
    assert!(locked.effects.iter().any(|e| matches!(
        e,
        Effect::Message {
            level: MessageLevel::Error,
            ..
        }
    )));
    assert!(!message_texts(&locked.effects)
        .iter()
        .any(|m| m.contains("Set assume-unchanged")));
    assert_eq!(
        std::fs::read(tmp.path().join(".git/index")).unwrap(),
        before
    );
    assert_eq!(flags(), initial_flags);
}

#[test]
fn skip_worktree_commands_cancel_never_writes() {
    let path = "dir/a b*.txt";
    let (tmp, _) = RepoFixture::new()
        .commit("init", [(path, "original\n"), ("base.txt", "base\n")])
        .modify(path, "changed\n")
        .build();
    let flags = || tmp.git(&["ls-files", "-v", "-z"]);
    let index_before = std::fs::read(tmp.path().join(".git/index")).unwrap();
    let flags_before = flags();
    let staged = tmp.git(&["ls-files", "--stage"]);
    let target_before = std::fs::read(tmp.path().join(path)).unwrap();
    let base_before = std::fs::read(tmp.path().join("base.txt")).unwrap();
    let assert_preserved = |stage: &str| {
        assert_eq!(
            std::fs::read(tmp.path().join(".git/index")).unwrap(),
            index_before,
            "{stage}: index bytes"
        );
        assert_eq!(flags(), flags_before, "{stage}: index flags");
        assert_eq!(
            tmp.git(&["ls-files", "--stage"]),
            staged,
            "{stage}: staged entries and modes"
        );
        assert_eq!(
            std::fs::read(tmp.path().join(path)).unwrap(),
            target_before,
            "{stage}: target bytes"
        );
        assert_eq!(
            std::fs::read(tmp.path().join("base.txt")).unwrap(),
            base_before,
            "{stage}: base bytes"
        );
    };

    let (mut test, _) = TestSession::open_status(&tmp, Profile::Standard);
    test.ack();
    test.goto_file("unstaged", path);

    // Cancel the set prompt with an explicit `None` text answer.
    test.ui().reply_text(None);
    test.press_ui("i");
    let cancelled = test.press_ui("w");
    assert!(
        !message_texts(&cancelled.effects)
            .iter()
            .any(|text| text.contains("Set skip-worktree")),
        "{:?}",
        cancelled.effects
    );
    assert!(!test.session.has_pending_prompt());
    assert_preserved("cancel set");

    // Cancel the clear prompt through the generic cancel answer.
    test.goto(0);
    test.ui().reply_cancel();
    test.press_ui("i");
    let cancelled = test.press_ui("W");
    assert!(
        !message_texts(&cancelled.effects)
            .iter()
            .any(|text| text.contains("Cleared skip-worktree")),
        "{:?}",
        cancelled.effects
    );
    assert!(!test.session.has_pending_prompt());
    assert_preserved("cancel clear");
}

#[test]
fn skip_worktree_commands_invalid_and_untracked_paths_do_not_write() {
    let path = "dir/a b*.txt";
    let (tmp, _) = RepoFixture::new()
        .commit("init", [(path, "original\n"), ("base.txt", "base\n")])
        .modify(path, "changed\n")
        .build();
    std::fs::write(tmp.path().join("untracked.txt"), "not in index\n").unwrap();
    let flags = || tmp.git(&["ls-files", "-v", "-z"]);
    let index_before = std::fs::read(tmp.path().join(".git/index")).unwrap();
    let flags_before = flags();
    let staged = tmp.git(&["ls-files", "--stage"]);
    let target_before = std::fs::read(tmp.path().join(path)).unwrap();
    let base_before = std::fs::read(tmp.path().join("base.txt")).unwrap();
    let untracked_before = std::fs::read(tmp.path().join("untracked.txt")).unwrap();
    let assert_preserved = |stage: &str| {
        assert_eq!(
            std::fs::read(tmp.path().join(".git/index")).unwrap(),
            index_before,
            "{stage}: index bytes"
        );
        assert_eq!(flags(), flags_before, "{stage}: index flags");
        assert_eq!(
            tmp.git(&["ls-files", "--stage"]),
            staged,
            "{stage}: staged entries and modes"
        );
        assert_eq!(
            std::fs::read(tmp.path().join(path)).unwrap(),
            target_before,
            "{stage}: target bytes"
        );
        assert_eq!(
            std::fs::read(tmp.path().join("base.txt")).unwrap(),
            base_before,
            "{stage}: base bytes"
        );
        assert_eq!(
            std::fs::read(tmp.path().join("untracked.txt")).unwrap(),
            untracked_before,
            "{stage}: untracked bytes"
        );
    };

    let (mut test, _) = TestSession::open_status(&tmp, Profile::Standard);
    test.ack();
    let line = test.goto_file("unstaged", path);
    for (case, input) in ["", "   ", "untracked.txt", "../escape.txt", "dir/nope.txt"]
        .into_iter()
        .enumerate()
    {
        test.goto(line);
        test.ui().reply_text(Some(input.to_string()));
        test.press_ui("i");
        let result = test.press_ui("w");
        let texts = message_texts(&result.effects);
        assert!(
            texts.iter().any(|text| {
                !text.trim().is_empty() && !text.contains('{') && !text.contains('}')
            }),
            "case {case} ({input:?}) must give a readable refusal: {texts:?}"
        );
        assert!(
            !texts
                .iter()
                .any(|text| text.contains("Set skip-worktree")
                    || text.contains("Cleared skip-worktree")),
            "case {case} ({input:?}) claimed success: {texts:?}"
        );
        assert_preserved(&format!("invalid {input:?}"));
    }
}

#[test]
fn skip_worktree_commands_index_lock_failure_is_readable() {
    let path = "dir/a b*.txt";
    let (tmp, _) = RepoFixture::new()
        .commit("init", [(path, "original\n"), ("base.txt", "base\n")])
        .modify(path, "changed\n")
        .build();
    let flags = || tmp.git(&["ls-files", "-v", "-z"]);
    let index_before = std::fs::read(tmp.path().join(".git/index")).unwrap();
    let flags_before = flags();
    let staged = tmp.git(&["ls-files", "--stage"]);
    let target_before = std::fs::read(tmp.path().join(path)).unwrap();
    let base_before = std::fs::read(tmp.path().join("base.txt")).unwrap();

    let (mut test, _) = TestSession::open_status(&tmp, Profile::Standard);
    test.ack();
    test.goto_file("unstaged", path);
    let lock = tmp.path().join(".git/index.lock");
    std::fs::write(&lock, "locked").unwrap();

    test.ui().reply_text(Some(path.to_string()));
    test.press_ui("i");
    let result = test.press_ui("w");
    let texts = message_texts(&result.effects);
    assert!(
        result.effects.iter().any(|effect| matches!(
            effect,
            Effect::Message {
                level: MessageLevel::Error,
                ..
            }
        )),
        "a Git failure must be an error effect: {:?}",
        result.effects
    );
    assert!(
        texts
            .iter()
            .any(|text| !text.trim().is_empty() && !text.contains('{')),
        "readable failure, not raw data: {texts:?}"
    );
    assert!(
        !texts.iter().any(|text| text.contains("Set skip-worktree")),
        "a failed write must not claim success: {texts:?}"
    );
    std::fs::remove_file(&lock).unwrap();

    assert_eq!(
        std::fs::read(tmp.path().join(".git/index")).unwrap(),
        index_before,
        "index bytes"
    );
    assert_eq!(flags(), flags_before, "index flags");
    assert_eq!(
        tmp.git(&["ls-files", "--stage"]),
        staged,
        "staged entries and modes"
    );
    assert_eq!(
        std::fs::read(tmp.path().join(path)).unwrap(),
        target_before,
        "target bytes"
    );
    assert_eq!(
        std::fs::read(tmp.path().join("base.txt")).unwrap(),
        base_before,
        "base bytes"
    );
}
