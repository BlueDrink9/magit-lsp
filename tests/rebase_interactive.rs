mod common;

use common::{git, write, TestSession, STATUS_VIEWPORT};
use magit_lsp::config::Profile;
use magit_lsp::git::GitRepo;
use magit_lsp::rebase_plan::PreparedRebase;
use magit_lsp::rebase_sequence_editor::{command_for, SequenceEditorPolicy, PLAN_FILE_ENV};
use magit_lsp::rebase_todo::TodoAction;
use magit_lsp::rebase_todo_view::RebaseTodoView;
use tempfile::TempDir;
use std::process::Command;

#[test]
fn sequence_editor_helper_copies_exact_bytes_for_paths_with_quotes() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("plan ' source");
    let destination = dir.path().join("todo ' destination");
    let bytes = b"pick abc\0def\n";
    std::fs::write(&source, bytes).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_magit-lsp"))
        .args(["--sequence-editor-helper", "prepared"]).arg(&destination)
        .env(PLAN_FILE_ENV, &source).status().unwrap();
    assert!(status.success());
    assert_eq!(std::fs::read(destination).unwrap(), bytes);
}

#[test]
fn sequence_editor_helper_fails_closed_on_missing_inputs_and_bad_arguments() {
    let dir = TempDir::new().unwrap();
    let destination = dir.path().join("todo");
    std::fs::write(&destination, b"unrelated").unwrap();
    let binary = env!("CARGO_BIN_EXE_magit-lsp");
    for (args, source) in [
        (vec!["--sequence-editor-helper", "prepared"], Some(dir.path().join("source"))),
        (vec!["--sequence-editor-helper", "prepared", "one", "extra"], Some(dir.path().join("source"))),
        (vec!["--sequence-editor-helper", "prepared", "todo"], None),
        (vec!["--sequence-editor-helper", "prepared", "todo"], Some(dir.path().join("missing-source"))),
    ] {
        let mut command = Command::new(binary);
        command.args(args);
        if let Some(source) = source { command.env(PLAN_FILE_ENV, source); }
        else { command.env_remove(PLAN_FILE_ENV); }
        assert!(!command.current_dir(dir.path()).status().unwrap().success());
        assert_eq!(std::fs::read(&destination).unwrap(), b"unrelated");
    }
    let source = dir.path().join("source");
    std::fs::write(&source, b"plan").unwrap();
    assert!(!Command::new(binary).args(["--sequence-editor-helper", "prepared", "missing/todo"])
        .env(PLAN_FILE_ENV, source).current_dir(dir.path()).status().unwrap().success());
    assert_eq!(std::fs::read(&destination).unwrap(), b"unrelated");

    let destination = dir.path().join("todo ' preserve");
    let bytes = b"# transformed\npick abc fixup! subject\0\n";
    std::fs::write(&destination, bytes).unwrap();
    assert!(Command::new(binary).args(["--sequence-editor-helper", "preserve"]).arg(&destination)
        .env_remove(PLAN_FILE_ENV).status().unwrap().success());
    assert_eq!(std::fs::read(&destination).unwrap(), bytes);
    assert!(!Command::new(binary).args(["--sequence-editor-helper", "unknown"]).arg(&destination)
        .env_remove(PLAN_FILE_ENV).status().unwrap().success());
}

#[test]
fn sequence_editor_command_quotes_spaces_and_apostrophes() {
    assert_eq!(command_for(std::path::Path::new("/tmp/a path/it's magit-lsp"), SequenceEditorPolicy::PreparedReplacement),
        "'/tmp/a path/it'\\''s magit-lsp' --sequence-editor-helper prepared");
}

fn repo() -> (TempDir, GitRepo, String) {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-b", "main"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    write(&dir.path().join("file"), "zero\n");
    git(dir.path(), &["add", "file"]);
    git(dir.path(), &["commit", "-m", "zero"]);
    let base = git(dir.path(), &["rev-parse", "HEAD"]).trim().to_owned();
    for message in ["one", "two", "three"] {
        write(&dir.path().join("file"), &format!("{message}\n"));
        git(dir.path(), &["commit", "-am", message]);
    }
    let repo = GitRepo::open(dir.path()).unwrap();
    (dir, repo, base)
}

fn state(dir: &std::path::Path) -> (String, String, Vec<u8>, Vec<u8>, String, String, bool) {
    (git(dir, &["rev-parse", "HEAD"]), git(dir, &["write-tree"]), std::fs::read(dir.join("file")).unwrap(), std::fs::read(dir.join(".git/HEAD")).unwrap(), git(dir, &["status", "--porcelain"]), git(dir, &["show-ref"]), dir.join(".git/rebase-merge").exists() || dir.join(".git/rebase-apply").exists())
}

#[test]
fn exclusive_base_plan_has_full_oid_membership_and_discard_is_read_only() {
    let (dir, repo, base) = repo();
    let before = state(dir.path());
    let mut plan = PreparedRebase::prepare(&repo, &base).unwrap();
    assert_eq!(plan.entries().len(), 3);
    assert!(plan.entries().iter().all(|entry| entry.oid.len() == 40));
    let chosen = plan.entries()[1].oid.clone();
    plan.set_action(&chosen, TodoAction::Edit).unwrap();
    plan.move_before(&chosen, None).unwrap();
    plan.validate_launch(&repo).unwrap();
    drop(plan);
    assert_eq!(state(dir.path()), before);
}

#[test]
fn prepared_message_actions_refuse_before_launch_without_mutation() {
    let (dir, repo, base) = repo();
    let baseline = state(dir.path());
    for action in [TodoAction::Reword, TodoAction::Squash] {
        let mut plan = PreparedRebase::prepare(&repo, &base).unwrap();
        let chosen = plan.entries()[1].oid.clone();
        plan.set_action(&chosen, action).unwrap();
        let entries = plan.entries().to_vec();
        assert!(plan.validate_launch(&repo).unwrap_err().to_string().contains("prepared rebase message editing is not supported yet"));
        let error = plan.launch_outcome(&repo, std::path::Path::new(env!("CARGO_BIN_EXE_magit-lsp"))).unwrap_err();
        assert!(error.to_string().contains("prepared rebase message editing is not supported yet"));
        assert_eq!(plan.entries(), entries);
        assert_eq!(state(dir.path()), baseline);
    }
}

#[test]
fn prepared_plan_rejected_edits_preserve_complete_entries() {
    let (_dir, repo, base) = repo();
    let mut plan = PreparedRebase::prepare(&repo, &base).unwrap();
    let ids = plan.entries().iter().map(|entry| entry.oid.clone()).collect::<Vec<_>>();

    let before = plan.entries().to_vec();
    assert!(plan.move_before(&ids[0], Some("unknown")).is_err());
    assert_eq!(plan.entries(), before);
    assert!(plan.move_before(&ids[0], Some(&ids[0])).is_err());
    assert_eq!(plan.entries(), before);

    plan.set_action(&ids[1], TodoAction::Fixup).unwrap();
    let before = plan.entries().to_vec();
    assert!(plan.move_before(&ids[0], None).is_err());
    assert_eq!(plan.entries(), before);

    plan.set_action(&ids[0], TodoAction::Reword).unwrap();
    let before = plan.entries().to_vec();
    assert!(plan.set_action(&ids[0], TodoAction::Squash).is_err());
    assert_eq!(plan.entries(), before);
}

#[test]
fn launch_uses_edited_order_and_drop_despite_autosquash_config() {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-b", "main"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    write(&dir.path().join("base"), "base\n");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-m", "base"]);
    let base = git(dir.path(), &["rev-parse", "HEAD"]).trim().to_owned();
    for (name, value) in [("one", "1"), ("two", "2"), ("three", "3")] {
        write(&dir.path().join(name), value);
        git(dir.path(), &["add", name]);
        git(dir.path(), &["commit", "-m", name]);
    }
    let repo = GitRepo::open(dir.path()).unwrap();
    git(dir.path(), &["config", "rebase.autoSquash", "true"]);
    let mut plan = PreparedRebase::prepare(&repo, &base).unwrap();
    let oids = plan.entries().iter().map(|entry| entry.oid.clone()).collect::<Vec<_>>();
    plan.set_action(&oids[1], TodoAction::Drop).unwrap();
    plan.move_before(&oids[2], Some(&oids[0])).unwrap();
    plan.launch(&repo, std::path::Path::new(env!("CARGO_BIN_EXE_magit-lsp"))).unwrap();
    assert_eq!(git(dir.path(), &["rev-list", "--reverse", &format!("{base}..HEAD")]).lines().count(), 2);
    assert_eq!(git(dir.path(), &["show", "HEAD:three"]).trim(), "3");
    assert_eq!(git(dir.path(), &["log", "-3", "--format=%s", "--reverse"]).lines().collect::<Vec<_>>(), vec!["base", "three", "one"]);
    assert!(!dir.path().join("two").exists());
}

#[test]
fn prepared_plan_refuses_same_oid_branch_switch_and_detach() {
    for detach in [false, true] {
        let (dir, repo, base) = repo();
        let mut plan = PreparedRebase::prepare(&repo, &base).unwrap();
        let oid = plan.entries()[1].oid.clone();
        plan.set_action(&oid, TodoAction::Drop).unwrap();
        let head = git(dir.path(), &["rev-parse", "HEAD"]);
        if detach {
            git(dir.path(), &["checkout", "--detach", "HEAD"]);
        } else {
            git(dir.path(), &["branch", "side", head.trim()]);
            git(dir.path(), &["checkout", "side"]);
        }
        let before = state(dir.path());
        let entries = plan.entries().to_vec();
        let error = plan.launch(&repo, std::path::Path::new(env!("CARGO_BIN_EXE_magit-lsp"))).unwrap_err();
        assert!(error.to_string().contains("HEAD attachment changed"));
        assert_eq!(plan.entries(), entries);
        assert_eq!(state(dir.path()), before);
    }
}

fn conflicting_plan() -> (TempDir, GitRepo, String, PreparedRebase) {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-b", "main"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    write(&dir.path().join("file"), "base\n");
    git(dir.path(), &["add", "file"]);
    git(dir.path(), &["commit", "-m", "base"]);
    let base = git(dir.path(), &["rev-parse", "HEAD"]).trim().to_owned();
    for (content, message) in [("first\n", "first"), ("second\n", "second")] {
        write(&dir.path().join("file"), content);
        git(dir.path(), &["commit", "-am", message]);
    }
    let repo = GitRepo::open(dir.path()).unwrap();
    let mut plan = PreparedRebase::prepare(&repo, &base).unwrap();
    let entries = plan.entries().iter().map(|entry| entry.oid.clone()).collect::<Vec<_>>();
    plan.move_before(&entries[1], Some(&entries[0])).unwrap();
    (dir, repo, base, plan)
}

#[test]
fn edited_plan_conflict_survives_resolution_and_continue() {
    let (dir, repo, _base, plan) = conflicting_plan();
    let error = plan.launch(&repo, std::path::Path::new(env!("CARGO_BIN_EXE_magit-lsp"))).unwrap_err();
    assert!(!error.to_string().trim().is_empty());
    assert_eq!(git(dir.path(), &["show", "-s", "--format=%s", "HEAD"]).trim(), "base");
    assert_eq!(git(dir.path(), &["status", "--porcelain"]).trim(), "UU file");
    let conflicted = String::from_utf8(std::fs::read(dir.path().join("file")).unwrap()).unwrap();
    assert!(conflicted.starts_with("<<<<<<< HEAD\nbase\n=======\nsecond\n>>>>>>> "));
    assert_eq!(repo.rebase_state().unwrap().unwrap().conflicted, true);
    while repo.rebase_state().unwrap().is_some() {
        write(&dir.path().join("file"), "resolved\n");
        git(dir.path(), &["add", "file"]);
        repo.rebase_continue().unwrap_or_else(|error| {
            assert!(repo.rebase_state().unwrap().unwrap().conflicted, "{error}");
        });
    }
    assert_eq!(git(dir.path(), &["show", "HEAD:file"]).trim(), "resolved");
    assert_eq!(git(dir.path(), &["log", "-3", "--format=%s", "--reverse"]).lines().collect::<Vec<_>>(), vec!["base", "second"]);
    assert_eq!(repo.rebase_state().unwrap(), None);
}

#[test]
fn edited_plan_conflict_abort_restores_original_repository() {
    let (dir, repo, _, plan) = conflicting_plan();
    let before = state(dir.path());
    assert!(plan.launch(&repo, std::path::Path::new(env!("CARGO_BIN_EXE_magit-lsp"))).is_err());
    assert!(repo.rebase_state().unwrap().is_some());
    repo.rebase_abort().unwrap();
    assert_eq!(state(dir.path()), before);
    assert_eq!(repo.rebase_state().unwrap(), None);
}

#[test]
fn intentional_edit_pause_abort_restores_original_repository() {
    let (dir, repo, _, mut plan) = conflicting_plan();
    let entries = plan.entries().iter().map(|entry| entry.oid.clone()).collect::<Vec<_>>();
    plan.move_before(&entries[1], Some(&entries[0])).unwrap();
    let first = plan.entries()[0].oid.clone();
    plan.set_action(&first, TodoAction::Edit).unwrap();
    let before = state(dir.path());
    plan.launch(&repo, std::path::Path::new(env!("CARGO_BIN_EXE_magit-lsp"))).unwrap();
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
    assert_eq!(repo.rebase_state().unwrap().unwrap().conflicted, false);
    repo.rebase_abort().unwrap();
    assert_eq!(state(dir.path()), before);
    assert_eq!(repo.rebase_state().unwrap(), None);
}

#[test]
fn prepared_todo_view_edits_in_memory_and_cannot_save_paused_file() {
    let (dir, repo, base) = repo();
    let before = state(dir.path());
    let plan = PreparedRebase::prepare(&repo, &base).unwrap();
    let membership = plan.entries().iter().map(|entry| entry.oid.clone()).collect::<Vec<_>>();
    let mut view = RebaseTodoView::prepared(repo, plan);
    let tree = view.render();
    assert_eq!(tree.nodes.iter().map(|node| node.commit.clone().unwrap()).collect::<Vec<_>>(), membership);
    view.set_action(&membership[1], TodoAction::Reword).unwrap();
    view.move_before(&membership[2], Some(&membership[0])).unwrap();
    assert_eq!(view.render().nodes.iter().map(|node| node.commit.clone().unwrap()).collect::<Vec<_>>(), vec![membership[2].clone(), membership[0].clone(), membership[1].clone()]);
    assert!(view.save().unwrap_err().to_string().contains("cannot be saved"));
    drop(view);
    assert_eq!(state(dir.path()), before);
}

#[test]
fn interactive_rebase_confirmation_opens_prepared_rows_for_selected_base() {
    let (dir, repo, base) = repo();
    let expected = PreparedRebase::prepare(&repo, &base).unwrap().entries().iter().map(|entry| entry.oid.clone()).collect::<Vec<_>>();
    let mut test = TestSession::new(dir.path().to_string_lossy(), Profile::Standard);
    test.open_status_view(STATUS_VIEWPORT);
    test.press("r");
    assert_eq!(*test.session.views().get(test.view_id()).unwrap().kind(), magit_lsp::view::ViewKind::Menu("menu.rebase".to_string()));
    let selected = test.press("i");
    assert_eq!(*test.session.views().get(test.view_id()).unwrap().kind(), magit_lsp::view::ViewKind::Log, "effects: {:?}", selected.effects);
    test.ack();
    let selected_base = test.tree().line_nodes.iter().position(|line| test.tree().node(&line.node).is_some_and(|node| node.kind == magit_lsp::tree::NodeKind::Commit && node.commit.as_deref() == Some(base.as_str()))).unwrap();
    test.set_cursor(magit_lsp::protocol::Cursor { line: selected_base, column: 0 });
    let confirmed = test.invoke_here("common.confirm");
    assert_eq!(*test.session.views().get(test.view_id()).unwrap().kind(), magit_lsp::view::ViewKind::RebaseTodo, "effects: {:?}", confirmed.effects);
    let rows = test.tree().nodes.iter().filter_map(|node| node.commit.clone()).collect::<Vec<_>>();
    assert_eq!(rows, expected);
    assert!(rows.iter().all(|oid| oid.len() == 40));
}

#[test]
fn edited_initial_rebase_cancel_preserves_repository_and_restores_caller() {
    let (dir, _repo, base) = repo();
    let before = state(dir.path());
    let mut test = TestSession::new(dir.path().to_string_lossy(), Profile::Standard);
    test.open_status_view(STATUS_VIEWPORT);
    test.press("r");
    test.press("i");
    test.ack();
    let selected_base = test.tree().line_nodes.iter().position(|line| test.tree().node(&line.node).is_some_and(|node| node.kind == magit_lsp::tree::NodeKind::Commit && node.commit.as_deref() == Some(base.as_str()))).unwrap();
    test.set_cursor(magit_lsp::protocol::Cursor { line: selected_base, column: 0 });
    test.invoke_here("common.confirm");
    let first = test.tree().nodes.iter().find(|node| node.commit.is_some()).unwrap().commit.clone().unwrap();
    let row = test.tree().line_nodes.iter().position(|line| test.tree().node(&line.node).is_some_and(|node| node.commit.as_deref() == Some(first.as_str()))).unwrap();
    test.set_cursor(magit_lsp::protocol::Cursor { line: row, column: 0 });
    test.press("r");
    assert!(test.tree().nodes.iter().any(|node| node.commit.as_deref() == Some(first.as_str()) && node.status.as_deref() == Some("reword")));
    test.press("Escape");
    assert_eq!(test.view_id(), "v1");
    assert_eq!(state(dir.path()), before);
    let refresh = test.invoke("common.refresh");
    assert!(common::message_texts(&refresh.effects).is_empty());
    assert_eq!(*test.session.views().get(test.view_id()).unwrap().kind(), magit_lsp::view::ViewKind::Status);
}

#[test]
fn initial_rebase_selector_cancel_and_stale_preparation_keep_caller_usable() {
    let (dir, _, _) = repo();
    let before = state(dir.path());
    let mut test = TestSession::new(dir.path().to_string_lossy(), Profile::Standard);
    test.open_status_view(STATUS_VIEWPORT);
    test.press("r");
    test.press("i");
    test.press("Escape");
    assert_eq!(test.view_id(), "v1");
    assert_eq!(state(dir.path()), before);
    let refresh = test.invoke("common.refresh");
    assert!(common::message_texts(&refresh.effects).is_empty());
    assert_eq!(*test.session.views().get(test.view_id()).unwrap().kind(), magit_lsp::view::ViewKind::Status);
}

#[test]
fn initial_rebase_invalid_selection_keeps_caller_usable() {
    let (dir, _, base) = repo();
    let before = state(dir.path());
    let mut test = TestSession::new(dir.path().to_string_lossy(), Profile::Standard);
    test.open_status_view(STATUS_VIEWPORT);
    test.press("r");
    test.press("i");
    test.ack();
    test.set_cursor(magit_lsp::protocol::Cursor { line: 0, column: 0 });
    let invalid = test.invoke_here("common.confirm");
    assert!(common::message_texts(&invalid.effects).iter().any(|message| !message.trim().is_empty()), "effects: {:?}", invalid.effects);
    assert_eq!(*test.session.views().get(test.view_id()).unwrap().kind(), magit_lsp::view::ViewKind::Log);
    assert_eq!(state(dir.path()), before);
    assert!(common::message_texts(&test.invoke("common.refresh").effects).is_empty());
}

#[test]
fn initial_rebase_preparation_failure_keeps_caller_usable() {
    let (dir, _, base) = repo();
    let mut test = TestSession::new(dir.path().to_string_lossy(), Profile::Standard);
    test.open_status_view(STATUS_VIEWPORT);
    test.press("r");
    test.press("i");
    test.ack();
    write(&dir.path().join("file"), "changed after selector opened\n");
    let before = state(dir.path());
    let selected_base = test.tree().line_nodes.iter().position(|line| test.tree().node(&line.node).is_some_and(|node| node.kind == magit_lsp::tree::NodeKind::Commit && node.commit.as_deref() == Some(base.as_str()))).unwrap();
    test.set_cursor(magit_lsp::protocol::Cursor { line: selected_base, column: 0 });
    let result = test.invoke_here("common.confirm");
    assert!(common::message_texts(&result.effects).iter().any(|message| message.contains("cannot prepare rebase")), "effects: {:?}", result.effects);
    assert_eq!(state(dir.path()), before);
    assert_eq!(*test.session.views().get(test.view_id()).unwrap().kind(), magit_lsp::view::ViewKind::Log);
    assert!(common::message_texts(&test.invoke("common.refresh").effects).is_empty());
}

#[test]
fn invalid_and_stale_inputs_fail_without_losing_plan_or_writing_repository() {
    let (dir, repo, base) = repo();
    assert!(PreparedRebase::prepare(&repo, "not-a-revision").is_err());
    let mut head_plan = PreparedRebase::prepare(&repo, &base).unwrap();
    let head_edit = head_plan.entries()[0].oid.clone();
    head_plan.set_action(&head_edit, TodoAction::Edit).unwrap();
    write(&dir.path().join("file"), "head-change\n");
    git(dir.path(), &["commit", "-am", "head-change"]);
    let head_instructions = head_plan.entries().to_vec();
    assert!(head_plan.validate_launch(&repo).unwrap_err().to_string().contains("HEAD changed"));
    assert_eq!(head_plan.entries(), head_instructions);
    let stale_state = state(dir.path());
    assert!(head_plan.launch(&repo, std::path::Path::new(env!("CARGO_BIN_EXE_magit-lsp"))).unwrap_err().to_string().contains("HEAD changed"));
    assert_eq!(head_plan.entries(), head_instructions);
    assert_eq!(state(dir.path()), stale_state);
    git(dir.path(), &["reset", "--hard", "HEAD~1"]);
    let mut plan = PreparedRebase::prepare(&repo, &base).unwrap();
    let chosen = plan.entries()[0].oid.clone();
    plan.set_action(&chosen, TodoAction::Edit).unwrap();
    write(&dir.path().join("file"), "external\n");
    let edited = plan.entries().to_vec();
    assert!(plan.validate_launch(&repo).unwrap_err().to_string().contains("changes"));
    assert_eq!(plan.entries(), edited);
    git(dir.path(), &["checkout", "--", "file"]);
    write(&dir.path().join("file"), "changed-index\n");
    git(dir.path(), &["add", "file"]);
    assert!(plan.validate_launch(&repo).is_err());
    assert_eq!(plan.entries(), edited);
}

#[test]
fn merge_base_and_active_operation_are_rejected_readably() {
    let (dir, repo, _) = repo();
    let head = git(dir.path(), &["rev-parse", "HEAD"]).trim().to_owned();
    assert!(PreparedRebase::prepare(&repo, &head).unwrap_err().to_string().contains("no commits"));
    let output = std::process::Command::new("git").args(["rebase", "-i", "HEAD~2"]).current_dir(dir.path()).env("GIT_SEQUENCE_EDITOR", "sed -i '1s/^pick /edit /'").output().unwrap();
    assert!(output.status.success(), "rebase fixture: {}", String::from_utf8_lossy(&output.stderr));
    assert!(PreparedRebase::prepare(&repo, "HEAD~1").unwrap_err().to_string().contains("operation"));
    git(dir.path(), &["rebase", "--abort"]);
}

#[test]
fn nonlinear_replay_range_is_rejected() {
    let (dir, repo, base) = repo();
    git(dir.path(), &["branch", "side", &base]);
    git(dir.path(), &["checkout", "side"]);
    write(&dir.path().join("side"), "side\n");
    git(dir.path(), &["add", "side"]);
    git(dir.path(), &["commit", "-m", "side"]);
    git(dir.path(), &["checkout", "main"]);
    git(dir.path(), &["merge", "--no-ff", "side", "-m", "merge"]);
    assert!(PreparedRebase::prepare(&repo, &base).unwrap_err().to_string().contains("linear"));
}
