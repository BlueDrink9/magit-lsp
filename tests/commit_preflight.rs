//! `commit.create` pre-flight (R-UI-5, U1.10).
//!
//! A clean tree never starts Git, an unstaged-only tree asks before staging,
//! and a staged tree commits without a prompt. These are L1 assertions on the
//! effects and the repository state; the real screen is covered by
//! `tests/ui/scenarios/commit-*.scn`.

mod common;

use common::*;
use magit_lsp::config::Profile;
use magit_lsp::git::{CommitOperation, CommitOptions};
use magit_lsp::protocol::{Effect, MessageLevel, PromptKind};
use magit_lsp::session::InvokeResult;

#[test]
fn preflight_rejects_changed_head_index_or_worktree_and_closed_targets() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    for change in ["head", "index", "worktree", "closed"] {
        let (tmp, _repo) = RepoFixture::new().commit("init", [("f.txt", TEN_LINES)]).modify_standard().build();
        let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
        let view = test.view_id().to_string();
        let step = test.session.start_invoke(&ViewInvokeParams {
            view_id: view.clone(), command: "commit.create".to_string(), version: None,
            cursor: None, selection: None, args: None,
        }).unwrap();
        let Execution::Prompt(prompt) = step else { panic!("expected preflight"); };
        match change {
            "head" => { tmp.git(&["commit", "--allow-empty", "-m", "external"]); }
            "index" => { tmp.git(&["add", "f.txt"]); }
            "worktree" => { std::fs::write(tmp.path().join("f.txt"), "external content\n").unwrap(); }
            "closed" => { test.session.close(&view).unwrap(); }
            _ => unreachable!(),
        }
        let before = tmp.git(&["diff", "--cached"]);
        let worktree = std::fs::read(tmp.path().join("f.txt")).unwrap();
        let Execution::Done(result) = test.session.resume_prompt(&prompt.prompt_id, Some(PromptValue::Bool(true))).unwrap() else { panic!("expected completion"); };
        assert!(result.commit.is_none(), "{change}");
        assert_eq!(tmp.git(&["diff", "--cached"]), before, "{change}");
        assert_eq!(std::fs::read(tmp.path().join("f.txt")).unwrap(), worktree, "{change}");
        assert!(!test.session.has_pending_prompt());
    }
}

fn message_effects(outcome: &InvokeResult) -> Vec<(&MessageLevel, &str, Option<&str>)> {
    outcome
        .effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Message {
                level, text, hint, ..
            } => Some((level, text.as_str(), hint.as_deref())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_clean_tree_reports_nothing_to_commit_without_git() {
    let (tmp, _repo) = init_repo();
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();

    let outcome = test.invoke_ui("commit.create");

    assert!(outcome.commit.is_none(), "no commit worker is started");
    assert!(
        test.ui().requests().is_empty(),
        "the clean tree never prompts"
    );
    let messages = message_effects(&outcome);
    assert_eq!(messages.len(), 1, "{messages:?}");
    let (level, text, hint) = messages[0];
    assert_eq!(*level, MessageLevel::Info);
    assert_eq!(text, "Nothing to commit (working tree clean)");
    assert!(
        hint.is_some_and(|hint| hint.contains("--allow-empty")),
        "the hint mentions --allow-empty: {hint:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|(_, text, _)| text.contains("Committing")),
        "a clean tree shows no progress: {messages:?}"
    );
}

#[test]
fn unstaged_changes_prompt_then_stage_and_commit() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    test.ui().reply_confirm(true);

    let outcome = test.invoke_ui("commit.create");

    let request = test.ui().last_request().expect("a confirm prompt");
    assert_eq!(request.kind, PromptKind::Confirm);
    assert_eq!(
        request.title,
        "Nothing staged. Stage all changes and commit?"
    );
    assert_eq!(
        outcome.commit,
        Some(CommitOperation::Create(CommitOptions::default())),
        "the commit continues after staging"
    );
    assert_eq!(
        tmp.git(&["status", "--porcelain"]),
        "M  f.txt\n",
        "the tracked change is staged"
    );
}

#[test]
fn declining_the_prompt_cancels_quietly() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    test.ui().reply_confirm(false);

    let outcome = test.invoke_ui("commit.create");

    assert!(outcome.commit.is_none(), "no commit worker is started");
    assert!(outcome.effects.is_empty(), "{:?}", outcome.effects);
    assert_eq!(
        tmp.git(&["status", "--porcelain"]),
        " M f.txt\n",
        "nothing was staged"
    );
}

#[test]
fn a_dismissed_prompt_cancels_quietly() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    test.ui().reply_cancel();

    let outcome = test.invoke_ui("commit.create");

    assert!(outcome.commit.is_none(), "no commit worker is started");
    assert!(outcome.effects.is_empty(), "{:?}", outcome.effects);
    assert_eq!(tmp.git(&["status", "--porcelain"]), " M f.txt\n");
}

#[test]
fn staged_changes_commit_without_a_prompt() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .staged_file("f.txt", standard_changes())
        .build();
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();

    let outcome = test.invoke_ui("commit.create");

    assert!(
        test.ui().requests().is_empty(),
        "a staged tree needs no prompt"
    );
    assert_eq!(
        outcome.commit,
        Some(CommitOperation::Create(CommitOptions::default())),
        "the commit worker is started"
    );
}

#[test]
fn head_edits_reject_unborn_and_conflicting_options_without_mutation() {
    use serde_json::json;
    for unborn in [true, false] {
        let fixture = if unborn { RepoFixture::new().unborn() } else {
            RepoFixture::new().commit("init", [("f.txt", TEN_LINES)])
        };
        let (tmp, _) = fixture.staged_file("f.txt", "staged\n").modify("f.txt", "unstaged\n").build();
        let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
        let index = tmp.git(&["ls-files", "--stage"]);
        let status = tmp.git(&["status", "--porcelain"]);
        for command in ["commit.amend", "commit.extend", "commit.reword"] {
            let arguments = if unborn { vec![json!(null)] } else { vec![
                json!({"switches": [], "options": {"=C": "HEAD"}}),
                json!({"switches": [], "options": {"=c": "HEAD"}}),
                json!({"switches": ["-R"], "options": {"=C": "HEAD"}}),
                json!({"switches": ["--only"], "options": {}}),
                json!({"switches": ["--no-edit"], "options": {}}),
            ] };
            for args in arguments {
                let result = test.invoke_args(command, args);
                assert!(result.commit.is_none(), "{command}");
                assert!(!message_effects(&result).is_empty());
                if unborn { assert!(message_effects(&result)[0].1.contains("no commit yet")); }
                assert_eq!(tmp.git(&["ls-files", "--stage"]), index);
                assert_eq!(tmp.git(&["status", "--porcelain"]), status);
            }
        }
        if !unborn {
            let head = tmp.git(&["rev-parse", "HEAD"]);
            assert!(test.invoke_args("commit.reword", json!({"switches": ["-a"], "options": {}})).commit.is_none());
            assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
        }
    }
}

#[test]
fn published_head_confirmation_captures_options_and_revalidates_state() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    use serde_json::json;
    for command in ["commit.amend", "commit.extend", "commit.reword"] {
        for change in ["accept", "decline", "dismiss", "head", "index", "worktree", "closed"] {
            let (tmp, _) = RepoFixture::new().commit("init", [("f.txt", TEN_LINES)])
                .staged_file("f.txt", "staged\n").modify("f.txt", "unstaged\n").build();
            tmp.git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
            let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
            let view = test.view_id().to_string();
            let Execution::Prompt(prompt) = test.session.start_invoke(&ViewInvokeParams {
                view_id: view.clone(), command: command.into(), version: None, cursor: None, selection: None,
                args: Some(json!({"switches": ["-n"], "options": {"=A": "Captured <captured@example.com>"}})),
            }).unwrap() else { panic!("published HEAD must prompt: {command}"); };
            assert!(prompt.title.contains("published HEAD"));
            match change {
                "head" => { tmp.git(&["commit", "--allow-empty", "-m", "external"]); }
                "index" => { tmp.git(&["add", "f.txt"]); }
                "worktree" => { write(&tmp.path().join("f.txt"), "external\n"); }
                "closed" => { test.session.close(&view).unwrap(); }
                _ => {}
            }
            let head = tmp.git(&["rev-parse", "HEAD"]);
            let index = tmp.git(&["ls-files", "--stage"]);
            let worktree = read(&tmp.path().join("f.txt"));
            let answer = match change { "dismiss" => None, "decline" => Some(PromptValue::Bool(false)), _ => Some(PromptValue::Bool(true)) };
            let Execution::Done(result) = test.session.resume_prompt(&prompt.prompt_id, answer).unwrap() else { panic!("completed"); };
            if change == "accept" {
                let operation = result.commit.expect("accepted captured operation");
                let mut expected = match command {
                    "commit.amend" => vec!["--amend"],
                    "commit.extend" => vec!["--amend", "--no-edit"],
                    _ => vec!["--amend", "--only", "--allow-empty"],
                };
                expected.extend(["--no-verify", "--author=Captured <captured@example.com>"]);
                assert_eq!(operation.argv(), expected);
            } else { assert!(result.commit.is_none(), "{command}: {change}"); }
            assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
            assert_eq!(tmp.git(&["ls-files", "--stage"]), index);
            assert_eq!(read(&tmp.path().join("f.txt")), worktree);
        }
    }
}

#[test]
fn extend_clean_guard_and_tracked_all_prompt_never_stage_implicitly() {
    use magit_lsp::protocol::{PromptValue, ViewInvokeParams};
    use magit_lsp::session::Execution;
    for change in ["accept", "decline", "head", "index", "worktree"] {
        let (tmp, _) = RepoFixture::new().commit("init", [("f.txt", TEN_LINES)]).build();
        let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
        let clean = test.invoke_ui("commit.extend");
        assert!(clean.commit.is_none());
        assert!(message_effects(&clean)[0].1.contains("Nothing staged"));
        write(&tmp.path().join("f.txt"), "unstaged\n");
        write(&tmp.path().join("untracked.txt"), "not added\n");
        let Execution::Prompt(prompt) = test.session.start_invoke(&ViewInvokeParams {
            view_id: test.view_id().into(), command: "commit.extend".into(), version: None,
            cursor: None, selection: None, args: None,
        }).unwrap() else { panic!("tracked all prompt"); };
        match change {
            "head" => { tmp.git(&["commit", "--allow-empty", "-m", "external"]); }
            "index" => { tmp.git(&["add", "f.txt"]); }
            "worktree" => { write(&tmp.path().join("f.txt"), "external\n"); }
            _ => {}
        }
        let head = tmp.git(&["rev-parse", "HEAD"]);
        let index = tmp.git(&["ls-files", "--stage"]);
        let status = tmp.git(&["status", "--porcelain"]);
        let Execution::Done(result) = test.session.resume_prompt(&prompt.prompt_id, Some(PromptValue::Bool(change != "decline"))).unwrap()
            else { panic!("completed"); };
        if change == "accept" { assert_eq!(result.commit.unwrap().argv(), ["--amend", "--no-edit", "--all"]); }
        else { assert!(result.commit.is_none()); }
        assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
        assert_eq!(tmp.git(&["ls-files", "--stage"]), index);
        assert_eq!(tmp.git(&["status", "--porcelain"]), status);
    }
}
