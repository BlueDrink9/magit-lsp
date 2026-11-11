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

#[test]
fn detects_idle_rebase_and_non_rebase_operations() {
    let dir = repo();
    let git = GitRepo::open(dir.path()).unwrap();
    assert_eq!(git.rebase_state().unwrap(), None);
    std::fs::create_dir_all(git.git_dir().join("sequencer")).unwrap();
    assert_eq!(git.rebase_state().unwrap(), None);
}

#[test]
fn detects_active_and_conflicted_rebase_apply_state() {
    let dir = repo();
    git(dir.path(), &["checkout", "-b", "topic"]);
    write(&dir.path().join("file"), "topic\n");
    git(dir.path(), &["commit", "-am", "topic"]);
    git(dir.path(), &["checkout", "main"]);
    write(&dir.path().join("file"), "main\n");
    git(dir.path(), &["commit", "-am", "main"]);
    git(dir.path(), &["checkout", "topic"]);
    let repo = GitRepo::open(dir.path()).unwrap();
    let output = std::process::Command::new("git").args(["rebase", "main"]).current_dir(dir.path()).output().unwrap();
    assert!(!output.status.success());
    assert_eq!(repo.rebase_state().unwrap().unwrap().conflicted, true);
    git(dir.path(), &["rebase", "--abort"]);
}

#[test]
fn merge_backend_is_detected_and_cherry_pick_revert_are_not_rebases() {
    let dir = repo();
    let repo = GitRepo::open(dir.path()).unwrap();
    std::fs::create_dir_all(repo.git_dir().join("rebase-merge")).unwrap();
    assert_eq!(repo.rebase_state().unwrap().unwrap().conflicted, false);
    std::fs::remove_dir_all(repo.git_dir().join("rebase-merge")).unwrap();
    std::fs::create_dir_all(repo.git_dir().join("rebase-apply")).unwrap();
    assert_eq!(repo.rebase_state().unwrap().unwrap().conflicted, false);
}
