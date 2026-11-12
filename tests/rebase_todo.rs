mod common;

use common::{git, message_texts, write, TestSession, STATUS_VIEWPORT};
use magit_lsp::config::Profile;
use magit_lsp::git::GitRepo;
use magit_lsp::rebase_todo::{RebaseTodo, TodoAction};
use magit_lsp::rebase_todo_view::RebaseTodoView;
use tempfile::TempDir;

fn repo() -> (TempDir, GitRepo, String) {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-b", "main"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    write(&dir.path().join("file"), "one\n");
    git(dir.path(), &["add", "file"]);
    git(dir.path(), &["commit", "-m", "one"]);
    write(&dir.path().join("file"), "two\n");
    git(dir.path(), &["commit", "-am", "two"]);
    let oid = git(dir.path(), &["rev-parse", "HEAD"]).trim().to_owned();
    let repo = GitRepo::open(dir.path()).unwrap();
    (dir, repo, oid)
}

fn paused_repo() -> (TempDir, GitRepo) {
    let (dir, _, _) = repo();
    write(&dir.path().join("file"), "three\n");
    git(dir.path(), &["commit", "-am", "three"]);
    write(&dir.path().join("file"), "four\n");
    git(dir.path(), &["commit", "-am", "four"]);
    let output = std::process::Command::new("git").args(["rebase", "-i", "HEAD~3"]).current_dir(dir.path())
        .env("GIT_SEQUENCE_EDITOR", "sed -i '1s/^pick /edit /'").output().unwrap();
    let repo = GitRepo::open(dir.path()).unwrap();
    assert!(repo.rebase_state().unwrap().is_some(), "edit rebase must pause: {}", String::from_utf8_lossy(&output.stderr));
    (dir, repo)
}

#[test]
fn parses_supported_actions_and_abbreviated_identity() {
    let (dir, repo, oid) = repo();
    let prefix = &oid[..8];
    let mut ids = vec![oid.clone(), git(dir.path(), &["rev-parse", "HEAD^"]).trim().to_owned()];
    for index in 0..4 {
        write(&dir.path().join("file"), &format!("{index}\n"));
        git(dir.path(), &["commit", "-am", &format!("extra-{index}")]);
        ids.push(git(dir.path(), &["rev-parse", "HEAD"]).trim().to_owned());
    }
    let source = format!("# comment\n\np   {prefix}\ttwo\nreword {} one\ne {} two\ns {} three\nfixup {} four\ndrop {} five\n", ids[1], ids[2], ids[3], ids[4], ids[5]);
    let parsed = RebaseTodo::parse(&repo, source.as_bytes()).unwrap();
    assert_eq!(parsed.entries.iter().map(|entry| entry.action).collect::<Vec<_>>(), vec![TodoAction::Pick, TodoAction::Reword, TodoAction::Edit, TodoAction::Squash, TodoAction::Fixup, TodoAction::Drop]);
    assert_eq!(parsed.entries[0].oid, oid);
    assert_eq!(parsed.source(), source.as_bytes());
}

#[test]
fn subject_starting_with_dash_is_not_treated_as_an_option() {
    let (_dir, repo, oid) = repo();
    let parsed = RebaseTodo::parse(&repo, format!("pick {oid} -subject\n").as_bytes()).unwrap();
    assert_eq!(parsed.entries[0].subject, "-subject");
}

#[test]
fn save_reorders_changes_action_and_writes_omissions_as_drop() {
    let (dir, repo) = paused_repo();
    let mut todo = RebaseTodo::load(&repo).unwrap();
    let identities = todo.entries.iter().map(|entry| entry.oid.clone()).collect::<Vec<_>>();
    assert_eq!(identities.len(), 2);
    let first = todo.entries[0].clone();
    let second = todo.entries[1].clone();
    todo.entries.reverse();
    todo.entries[0].action = TodoAction::Reword;
    todo.entries.pop();
    todo.save(&repo).unwrap();
    let saved = std::fs::read_to_string(repo.rebase_todo_path().unwrap()).unwrap();
    assert_eq!(saved, format!("reword {} {}\ndrop {} {}\n", second.oid, second.subject, first.oid, first.subject));
    let mut identities = identities;
    identities.sort();
    let mut written = RebaseTodo::parse(&repo, saved.as_bytes()).unwrap().entries.into_iter().map(|entry| entry.oid).collect::<Vec<_>>();
    written.sort();
    assert_eq!(written, identities);
    git(dir.path(), &["rebase", "--abort"]);
}

#[test]
fn stale_or_invalid_save_leaves_todo_bytes_unchanged() {
    let (dir, repo) = paused_repo();
    let mut todo = RebaseTodo::load(&repo).unwrap();
    let path = repo.rebase_todo_path().unwrap();
    std::fs::write(&path, b"external edit\n").unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(todo.save(&repo).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);

    let source = todo.source().to_vec();
    std::fs::write(&path, &source).unwrap();
    todo.entries[0].oid = "forged".into();
    let before = std::fs::read(&path).unwrap();
    assert!(todo.save(&repo).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    todo.entries[0].oid = RebaseTodo::parse(&repo, &source).unwrap().entries[0].oid.clone();
    todo.entries[0].subject.push_str("\nexec touch injected");
    assert!(todo.save(&repo).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    git(dir.path(), &["rebase", "--abort"]);
}

#[test]
fn old_todo_model_cannot_save_into_identical_restarted_rebase() {
    let (dir, repo) = paused_repo();
    let old = RebaseTodo::load(&repo).unwrap();
    let path = repo.rebase_todo_path().unwrap();
    let source = std::fs::read(&path).unwrap();
    let metadata = ["orig-head", "onto", "head-name"].map(|name| std::fs::read(dir.path().join(".git/rebase-merge").join(name)).unwrap());
    git(dir.path(), &["rebase", "--abort"]);
    let output = std::process::Command::new("git").args(["rebase", "-i", "HEAD~3"]).current_dir(dir.path())
        .env("GIT_SEQUENCE_EDITOR", "sed -i '1s/^pick /edit /'").output().unwrap();
    assert!(output.status.success(), "restart paused rebase: {}", String::from_utf8_lossy(&output.stderr));
    let restarted = std::fs::read(&path).unwrap();
    assert_eq!(restarted, source);
    assert_eq!(["orig-head", "onto", "head-name"].map(|name| std::fs::read(dir.path().join(".git/rebase-merge").join(name)).unwrap()), metadata);
    assert!(old.save(&repo).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), source);
    let mut current = RebaseTodo::load(&repo).unwrap();
    current.entries[0].action = TodoAction::Reword;
    current.save(&repo).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), format!("reword {} {}\npick {} {}\n", current.entries[0].oid, current.entries[0].subject, current.entries[1].oid, current.entries[1].subject).as_bytes());
    git(dir.path(), &["rebase", "--abort"]);
}

#[test]
fn todo_view_edits_and_reorders_by_captured_identity() {
    let (dir, repo) = paused_repo();
    let mut view = RebaseTodoView::load(repo).unwrap();
    let initial = view.render();
    let ids = initial.nodes.iter().map(|node| node.commit.clone().unwrap()).collect::<Vec<_>>();
    assert_eq!(ids.len(), 2);
    let second = ids[1].clone();
    view.set_action(&second, TodoAction::Reword).unwrap();
    view.move_before(&second, Some(&ids[0])).unwrap();
    let changed = view.render();
    assert_eq!(changed.nodes[0].commit.as_deref(), Some(second.as_str()));
    assert_eq!(changed.nodes[0].status.as_deref(), Some("reword"));
    assert_eq!(changed.nodes[1].commit.as_deref(), Some(ids[0].as_str()));
    let path = dir.path().join(".git/rebase-merge/git-rebase-todo");
    let before = std::fs::read(&path).unwrap();
    assert!(view.move_before("unknown", None).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    view.save().unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.starts_with(&format!("reword {second} ")));
    git(dir.path(), &["rebase", "--abort"]);
}

#[test]
fn paused_rebase_menu_opens_todo_and_cancel_preserves_repository() {
    let (dir, repo) = paused_repo();
    let todo_path = repo.rebase_todo_path().unwrap();
    let todo_before = std::fs::read(&todo_path).unwrap();
    let head_before = git(dir.path(), &["rev-parse", "HEAD"]);
    let index_before = git(dir.path(), &["write-tree"]);
    let expected = RebaseTodo::load(&repo).unwrap().entries.into_iter().map(|entry| entry.oid).collect::<Vec<_>>();

    let mut test = TestSession::new(dir.path().to_string_lossy(), Profile::Standard);
    test.open_status_view(STATUS_VIEWPORT);
    test.press("r T");
    assert_eq!(*test.session.views().get(test.view_id()).unwrap().kind(), magit_lsp::view::ViewKind::RebaseTodo);
    let tree = test.tree();
    let rows = tree.nodes.iter().filter_map(|node| node.commit.clone()).collect::<Vec<_>>();
    assert_eq!(rows, expected);
    assert!(rows.iter().all(|oid| oid.len() == 40));

    test.press("Escape");
    assert_eq!(test.view_id(), "v1");
    assert_eq!(std::fs::read(todo_path).unwrap(), todo_before);
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head_before);
    assert_eq!(git(dir.path(), &["write-tree"]), index_before);
    git(dir.path(), &["rebase", "--abort"]);
}

#[test]
fn todo_key_input_changes_action_and_reorders_full_oid_rows() {
    let (dir, repo) = paused_repo();
    let expected = RebaseTodo::load(&repo).unwrap().entries.into_iter().map(|entry| entry.oid).collect::<Vec<_>>();
    let mut test = TestSession::new(dir.path().to_string_lossy(), Profile::Standard);
    test.open_status_view(STATUS_VIEWPORT);
    test.press("r T");
    let first = magit_lsp::protocol::Cursor { line: 0, column: 0 };
    test.set_cursor(first);
    test.press("r");
    assert_eq!(test.tree().nodes[0].status.as_deref(), Some("reword"));
    test.press("J");
    let rows = test.tree().nodes.iter().filter_map(|node| node.commit.as_deref()).collect::<Vec<_>>();
    assert_eq!(rows, vec![expected[1].as_str(), expected[0].as_str()]);
    assert_eq!(test.tree().nodes.iter().find(|node| node.commit.as_deref() == Some(&expected[0])).unwrap().status.as_deref(), Some("reword"));
    test.press("J");
    assert_eq!(test.tree().nodes.iter().filter_map(|node| node.commit.as_deref()).collect::<Vec<_>>(), expected.iter().map(String::as_str).collect::<Vec<_>>());
    test.set_cursor(magit_lsp::protocol::Cursor { line: 1, column: 0 });
    test.press("K");
    assert_eq!(test.tree().nodes.iter().filter_map(|node| node.commit.as_deref()).collect::<Vec<_>>(), vec![expected[1].as_str(), expected[0].as_str()]);
    test.press("K");
    assert_eq!(test.tree().nodes.iter().filter_map(|node| node.commit.as_deref()).collect::<Vec<_>>(), expected.iter().map(String::as_str).collect::<Vec<_>>());
    test.press("Escape");
    git(dir.path(), &["rebase", "--abort"]);
}

#[test]
fn todo_key_movement_boundaries_submit_and_cancel_preserve_caller_and_repository() {
    let (dir, repo) = paused_repo();
    let path = repo.rebase_todo_path().unwrap();
    let entries = RebaseTodo::load(&repo).unwrap().entries;
    let identities = entries.iter().map(|entry| entry.oid.clone()).collect::<Vec<_>>();
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    let index = git(dir.path(), &["write-tree"]);
    let mut test = TestSession::new(dir.path().to_string_lossy(), Profile::Standard);
    test.open_status_view(STATUS_VIEWPORT);
    test.press("r T");
    let order = |test: &TestSession| test.tree().nodes.iter().filter_map(|node| node.commit.clone()).collect::<Vec<_>>();

    test.set_cursor(magit_lsp::protocol::Cursor { line: 0, column: 0 });
    test.press("K");
    assert_eq!(order(&test), identities);
    test.set_cursor(magit_lsp::protocol::Cursor { line: 1, column: 0 });
    test.press("J");
    assert_eq!(order(&test), identities);
    test.set_cursor(magit_lsp::protocol::Cursor { line: 0, column: 0 });
    test.press("J");
    assert_eq!(order(&test), vec![identities[1].clone(), identities[0].clone()]);
    test.press("ctrl+s");
    assert_eq!(test.view_id(), "v1");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), format!("pick {} {}\npick {} {}\n", entries[1].oid, entries[1].subject, entries[0].oid, entries[0].subject));
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(git(dir.path(), &["write-tree"]), index);
    let refresh = test.invoke("common.refresh");
    assert!(message_texts(&refresh.effects).is_empty());
    git(dir.path(), &["rebase", "--abort"]);

    let (dir, repo) = paused_repo();
    let path = repo.rebase_todo_path().unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    let index = git(dir.path(), &["write-tree"]);
    let mut test = TestSession::new(dir.path().to_string_lossy(), Profile::Standard);
    test.open_status_view(STATUS_VIEWPORT);
    test.press("r T");
    test.set_cursor(magit_lsp::protocol::Cursor { line: 0, column: 0 });
    test.press("r");
    test.press("J");
    test.press("Escape");
    assert_eq!(test.view_id(), "v1");
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(git(dir.path(), &["write-tree"]), index);
    let refresh = test.invoke("common.refresh");
    assert!(message_texts(&refresh.effects).is_empty());
    git(dir.path(), &["rebase", "--abort"]);
}

#[test]
fn todo_action_keys_change_captured_full_oid_row() {
    let (dir, repo) = paused_repo();
    let selected = RebaseTodo::load(&repo).unwrap().entries[1].oid.clone();
    assert_eq!(selected.len(), 40);
    let mut test = TestSession::new(dir.path().to_string_lossy(), Profile::Standard);
    test.open_status_view(STATUS_VIEWPORT);
    test.press("r T");
    let tree = test.tree();
    let node = tree.nodes.iter().find(|node| node.commit.as_deref() == Some(&selected)).unwrap();
    let line = tree.line_nodes.iter().position(|line| line.node == node.id).unwrap();
    test.set_cursor(magit_lsp::protocol::Cursor { line, column: 0 });

    for (key, action) in [("p", "pick"), ("r", "reword"), ("e", "edit"), ("s", "squash"), ("f", "fixup"), ("d", "drop")] {
        test.press(key);
        assert_eq!(test.tree().nodes.iter().find(|node| node.commit.as_deref() == Some(&selected)).unwrap().status.as_deref(), Some(action));
    }

    test.press("Escape");
    git(dir.path(), &["rebase", "--abort"]);
}

#[test]
fn todo_key_safety_rejects_leading_folds_without_mutation_and_remains_usable() {
    let (dir, repo) = paused_repo();
    let path = repo.rebase_todo_path().unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let entries = RebaseTodo::load(&repo).unwrap().entries;
    let identities = entries.iter().map(|entry| entry.oid.clone()).collect::<Vec<_>>();
    let mut test = TestSession::new(dir.path().to_string_lossy(), Profile::Standard);
    test.open_status_view(STATUS_VIEWPORT);
    test.press("r T");
    let tree = test.tree();
    let node = tree.nodes.iter().find(|node| node.commit.as_deref() == Some(&identities[0])).unwrap();
    let line = tree.line_nodes.iter().position(|line| line.node == node.id).unwrap();
    test.set_cursor(magit_lsp::protocol::Cursor { line, column: 0 });

    for key in ["s", "f"] {
        let result = test.press(key);
        assert!(message_texts(&result.effects).iter().any(|message| message.contains("first todo action cannot squash or fixup")));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(test.tree().nodes.iter().filter_map(|node| node.commit.as_ref()).collect::<Vec<_>>(), identities.iter().collect::<Vec<_>>());
        assert!(test.tree().nodes.iter().filter(|node| node.commit.is_some()).all(|node| node.status.as_deref() == Some("pick")));
    }

    test.press("r");
    assert_eq!(test.tree().nodes.iter().find(|node| node.commit.as_deref() == Some(&identities[0])).unwrap().status.as_deref(), Some("reword"));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    test.press("Escape");
    git(dir.path(), &["rebase", "--abort"]);
}

#[test]
fn dispatched_todo_edits_submit_caller_and_stale_failure_preserves_editor() {
    let (dir, repo) = paused_repo();
    let todo_path = repo.rebase_todo_path().unwrap();
    let original = RebaseTodo::load(&repo).unwrap().entries;
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    let index = git(dir.path(), &["write-tree"]);
    let mut test = TestSession::new(dir.path().to_string_lossy(), Profile::Standard);
    test.open_status_view(STATUS_VIEWPORT);
    test.press("r T");
    let first_cursor = magit_lsp::protocol::Cursor { line: 0, column: 0 };
    let second_cursor = magit_lsp::protocol::Cursor { line: 1, column: 0 };
    let second_oid = test.tree().nodes.iter().filter_map(|node| node.commit.as_deref()).nth(1).unwrap().to_owned();
    let first_oid = test.tree().nodes.iter().filter_map(|node| node.commit.as_deref()).next().unwrap().to_owned();
    let changed = test.invoke_at_args("rebase.todo-set-action", Some(first_cursor), serde_json::json!({"action":"reword"}));
    assert!(message_texts(&changed.effects).is_empty());
    let moved = test.invoke_at_args("rebase.todo-move-before", Some(second_cursor), serde_json::json!({"before":first_oid}));
    assert!(message_texts(&moved.effects).is_empty());
    let ordered = test.tree().nodes.iter().filter_map(|node| node.commit.as_deref()).collect::<Vec<_>>();
    assert_eq!(ordered, vec![second_oid.as_str(), first_oid.as_str()]);
    test.press("ctrl+s");
    assert_eq!(test.view_id(), "v1");
    let saved = std::fs::read_to_string(&todo_path).unwrap();
    assert!(saved.starts_with(&format!("pick {second_oid} ")));
    assert!(saved.contains(&format!("reword {first_oid} ")));
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(git(dir.path(), &["write-tree"]), index);

    git(dir.path(), &["rebase", "--abort"]);
    let (dir, repo) = paused_repo();
    let path = repo.rebase_todo_path().unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    let index = git(dir.path(), &["write-tree"]);
    let mut test = TestSession::new(dir.path().to_string_lossy(), Profile::Standard);
    test.open_status_view(STATUS_VIEWPORT);
    test.press("r T");
    let todo_view_id = test.view_id().to_owned();
    let cursor = magit_lsp::protocol::Cursor { line: 0, column: 0 };
    let invalid_bytes = std::fs::read(&path).unwrap();
    let invalid = test.invoke_at_args("rebase.todo-set-action", Some(cursor), serde_json::json!({"action":"unsupported"}));
    assert!(message_texts(&invalid.effects).iter().any(|message| message.contains("Invalid rebase todo action")));
    assert_eq!(std::fs::read(&path).unwrap(), invalid_bytes);
    let valid = test.invoke_at_args("rebase.todo-set-action", Some(cursor), serde_json::json!({"action":"reword"}));
    assert!(message_texts(&valid.effects).is_empty());
    assert_eq!(test.view_id(), todo_view_id);
    std::fs::write(&path, b"external stale edit\n").unwrap();
    let stale = test.invoke("rebase.todo-submit");
    assert!(message_texts(&stale.effects).iter().any(|message| message.contains("changed")));
    assert_eq!(test.view_id(), todo_view_id);
    assert_eq!(std::fs::read(&path).unwrap(), b"external stale edit\n");
    let usable = test.invoke("rebase.todo-set-action");
    assert!(message_texts(&usable.effects).iter().any(|message| !message.trim().is_empty()));
    assert_eq!(test.view_id(), todo_view_id);
    test.press("Escape");
    assert_eq!(test.view_id(), "v1");
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(git(dir.path(), &["write-tree"]), index);
    let _ = bytes;
    git(dir.path(), &["rebase", "--abort"]);
}

#[test]
fn dispatched_todo_cancel_after_edits_preserves_repository_and_restores_caller() {
    let (dir, repo) = paused_repo();
    let path = repo.rebase_todo_path().unwrap();
    let todo_before = std::fs::read(&path).unwrap();
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    let index = git(dir.path(), &["write-tree"]);
    let mut test = TestSession::new(dir.path().to_string_lossy(), Profile::Standard);
    test.open_status_view(STATUS_VIEWPORT);
    test.press("r T");
    let cursor = magit_lsp::protocol::Cursor { line: 0, column: 0 };
    let edit = test.invoke_at_args("rebase.todo-set-action", Some(cursor), serde_json::json!({"action":"reword"}));
    assert!(message_texts(&edit.effects).is_empty());
    test.press("Escape");
    assert_eq!(test.view_id(), "v1");
    assert_eq!(std::fs::read(&path).unwrap(), todo_before);
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(git(dir.path(), &["write-tree"]), index);
    git(dir.path(), &["rebase", "--abort"]);
}

#[test]
fn idle_rebase_todo_opener_reports_error_and_keeps_caller_usable() {
    let (dir, _repo, _) = repo();
    let head_before = git(dir.path(), &["rev-parse", "HEAD"]);
    let index_before = git(dir.path(), &["write-tree"]);

    let mut test = TestSession::new(dir.path().to_string_lossy(), Profile::Standard);
    test.open_status_view(STATUS_VIEWPORT);
    let result = test.press("r T");
    let messages = message_texts(&result.effects);
    assert!(!messages.is_empty(), "idle opener must emit a readable message");
    assert!(messages.iter().any(|text| !text.trim().is_empty()));
    assert_eq!(test.view_id(), "v1");
    assert_eq!(test.view_ids(), vec!["v1"]);

    let refresh = test.invoke("common.refresh");
    let _ = refresh;
    assert_eq!(test.view_id(), "v1");
    assert!(!test.text().is_empty());
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head_before);
    assert_eq!(git(dir.path(), &["write-tree"]), index_before);
}

#[test]
fn parse_only_and_cross_repository_models_cannot_write() {
    let (dir, repo) = paused_repo();
    let path = repo.rebase_todo_path().unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let parsed = RebaseTodo::parse(&repo, &bytes).unwrap();
    assert!(parsed.save(&repo).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);

    let (other_dir, other) = paused_repo();
    let other_path = other.rebase_todo_path().unwrap();
    let other_bytes = std::fs::read(&other_path).unwrap();
    let loaded = RebaseTodo::load(&repo).unwrap();
    assert!(loaded.save(&other).is_err());
    assert_eq!(std::fs::read(&other_path).unwrap(), other_bytes);
    git(dir.path(), &["rebase", "--abort"]);
    git(other_dir.path(), &["rebase", "--abort"]);
}

#[test]
fn rejects_bad_commands_objects_duplicates_and_leading_fold_without_mutation() {
    let (dir, repo, oid) = repo();
    let file_before = std::fs::read(dir.path().join("file")).unwrap();
    let head_before = git(dir.path(), &["rev-parse", "HEAD"]);
    let index_before = git(dir.path(), &["write-tree"]);
    let one = git(dir.path(), &["rev-parse", "HEAD^"]).trim().to_owned();
    for invalid in [
        format!("exec echo bad {oid}\n"), format!("pick {oid}\n"),
        format!("pick {} missing\n", "deadbeef"), format!("pick {oid} one\npick {oid} duplicate\n"),
        format!("squash {oid} first\n"), format!("fixup {oid} first\n"),
        format!("pick {one} first\nupdate-ref refs/heads/x\n"),
        format!("pick {} tree\n", git(dir.path(), &["rev-parse", "HEAD:file"]).trim()),
    ] {
        assert!(RebaseTodo::parse(&repo, invalid.as_bytes()).is_err(), "accepted {invalid:?}");
        assert_eq!(std::fs::read(dir.path().join("file")).unwrap(), file_before);
        assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head_before);
        assert_eq!(git(dir.path(), &["write-tree"]), index_before);
    }
}

#[test]
fn rejects_ambiguous_abbreviation() {
    let (dir, repo, _) = repo();
    let mut prefixes = std::collections::HashMap::<String, usize>::new();
    let mut ambiguous = None;
    for index in 0..5000 {
        let data = format!("fixture-{index}");
        let path = dir.path().join(format!("fixture-{index}"));
        write(&path, &data);
        let oid = git(dir.path(), &["hash-object", "-w", path.to_str().unwrap()]).trim().to_owned();
        let prefix = oid[..4].to_owned();
        let count = prefixes.entry(prefix.clone()).or_default();
        *count += 1;
        if *count > 1 { ambiguous = Some(prefix); break; }
    }
    let prefix = ambiguous.expect("bounded fixture must produce 4-hex collision");
    let matching = git(dir.path(), &["rev-parse", &format!("--disambiguate={prefix}")]);
    assert!(matching.lines().count() > 1, "Git must prove ambiguity");
    assert!(RebaseTodo::parse(&repo, format!("pick {prefix} ambiguous\n").as_bytes()).is_err());
}
