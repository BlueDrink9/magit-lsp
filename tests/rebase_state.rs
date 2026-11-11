mod common;

use common::{git, write};
use magit_lsp::git::GitRepo;
use tempfile::TempDir;

fn repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-b", "main"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    write(&dir.path().join("file"), "base\n");
    git(dir.path(), &["add", "file"]);
    git(dir.path(), &["commit", "-m", "base"]);
    dir
}

fn conflicting_history(dir: &std::path::Path) {
    git(dir, &["checkout", "-b", "topic"]);
    write(&dir.join("file"), "topic\n");
    git(dir, &["commit", "-am", "topic"]);
    git(dir, &["checkout", "main"]);
    write(&dir.join("file"), "main\n");
    git(dir, &["commit", "-am", "main"]);
}

fn failed_git(dir: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git").args(args).current_dir(dir).output().unwrap();
    assert!(!output.status.success(), "git {:?} unexpectedly succeeded", args);
}

#[test]
fn real_operations_distinguish_rebase_cherry_pick_revert_and_am() {
    let dir = repo();
    conflicting_history(dir.path());
    let main_commit = git(dir.path(), &["rev-parse", "main"]).trim().to_owned();
    let topic_commit = git(dir.path(), &["rev-parse", "topic"]).trim().to_owned();
    let repo = GitRepo::open(dir.path()).unwrap();
    assert_eq!(repo.rebase_state().unwrap(), None);

    git(dir.path(), &["checkout", "topic"]);
    failed_git(dir.path(), &["rebase", "main"]);
    assert_eq!(repo.rebase_state().unwrap().unwrap().conflicted, true);
    git(dir.path(), &["rebase", "--abort"]);

    git(dir.path(), &["checkout", "main"]);
    failed_git(dir.path(), &["cherry-pick", &topic_commit]);
    assert_eq!(repo.rebase_state().unwrap(), None);
    git(dir.path(), &["cherry-pick", "--abort"]);

    write(&dir.path().join("file"), "later\n");
    git(dir.path(), &["commit", "-am", "later"]);
    failed_git(dir.path(), &["revert", "--no-edit", &main_commit]);
    assert!(dir.path().join(".git/REVERT_HEAD").exists());
    assert_eq!(repo.rebase_state().unwrap(), None);
    git(dir.path(), &["revert", "--abort"]);

    let patch = std::process::Command::new("git").args(["format-patch", "--stdout", "main..topic"]).current_dir(dir.path()).output().unwrap();
    assert!(patch.status.success());
    std::fs::write(dir.path().join("patch"), patch.stdout).unwrap();
    failed_git(dir.path(), &["am", "patch"]);
    assert_eq!(repo.rebase_state().unwrap(), None);
    git(dir.path(), &["am", "--abort"]);
}

#[test]
fn apply_rebase_detected_and_linked_worktree_state_isolated() {
    let dir = repo();
    conflicting_history(dir.path());
    let linked = dir.path().join("linked");
    git(dir.path(), &["worktree", "add", "-b", "other", linked.to_str().unwrap(), "main"]);
    let main_repo = GitRepo::open(dir.path()).unwrap();
    let linked_repo = GitRepo::open(&linked).unwrap();
    assert_eq!(main_repo.rebase_state().unwrap(), None);
    assert_eq!(linked_repo.rebase_state().unwrap(), None);

    git(dir.path(), &["checkout", "topic"]);
    failed_git(dir.path(), &["rebase", "--apply", "main"]);
    assert_eq!(main_repo.rebase_state().unwrap().unwrap().conflicted, true);
    assert_eq!(linked_repo.rebase_state().unwrap(), None);
    git(dir.path(), &["rebase", "--abort"]);
}

#[test]
fn rebase_operations_guard_idle_and_handle_conflict_skip_abort_and_continue() {
    let dir = repo();
    let idle = GitRepo::open(dir.path()).unwrap();
    assert!(idle.rebase_continue().unwrap_err().to_string().contains("No rebase operation"));
    assert!(idle.rebase_skip().is_err());
    assert!(idle.rebase_abort().is_err());
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]).trim(), git(dir.path(), &["rev-parse", "main"]).trim());

    conflicting_history(dir.path());
    git(dir.path(), &["checkout", "topic"]);
    failed_git(dir.path(), &["rebase", "main"]);
    let active = GitRepo::open(dir.path()).unwrap();
    assert!(active.rebase_continue().is_err());
    assert!(active.rebase_state().unwrap().is_some());
    active.rebase_abort().unwrap();
    assert_eq!(active.rebase_state().unwrap(), None);

    failed_git(dir.path(), &["rebase", "main"]);
    write(&dir.path().join("file"), "resolved\n");
    git(dir.path(), &["add", "file"]);
    active.rebase_continue().unwrap();
    assert_eq!(active.rebase_state().unwrap(), None);

    let skip_dir = repo();
    conflicting_history(skip_dir.path());
    git(skip_dir.path(), &["checkout", "topic"]);
    failed_git(skip_dir.path(), &["rebase", "main"]);
    let skip_repo = GitRepo::open(skip_dir.path()).unwrap();
    assert!(skip_repo.rebase_state().unwrap().unwrap().conflicted);
    skip_repo.rebase_skip().unwrap();
    assert_eq!(skip_repo.rebase_state().unwrap(), None);
}
