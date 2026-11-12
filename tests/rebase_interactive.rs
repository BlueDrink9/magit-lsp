mod common;

use common::{git, write};
use magit_lsp::git::GitRepo;
use magit_lsp::rebase_plan::PreparedRebase;
use magit_lsp::rebase_todo::TodoAction;
use tempfile::TempDir;

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
    plan.set_action(&chosen, TodoAction::Reword).unwrap();
    plan.move_before(&chosen, None).unwrap();
    plan.validate_launch(&repo).unwrap();
    drop(plan);
    assert_eq!(state(dir.path()), before);
}

#[test]
fn invalid_and_stale_inputs_fail_without_losing_plan_or_writing_repository() {
    let (dir, repo, base) = repo();
    assert!(PreparedRebase::prepare(&repo, "not-a-revision").is_err());
    let mut head_plan = PreparedRebase::prepare(&repo, &base).unwrap();
    let head_edit = head_plan.entries()[0].oid.clone();
    head_plan.set_action(&head_edit, TodoAction::Reword).unwrap();
    write(&dir.path().join("file"), "head-change\n");
    git(dir.path(), &["commit", "-am", "head-change"]);
    let head_instructions = head_plan.entries().to_vec();
    assert!(head_plan.validate_launch(&repo).unwrap_err().to_string().contains("HEAD changed"));
    assert_eq!(head_plan.entries(), head_instructions);
    git(dir.path(), &["reset", "--hard", "HEAD~1"]);
    let mut plan = PreparedRebase::prepare(&repo, &base).unwrap();
    let chosen = plan.entries()[0].oid.clone();
    plan.set_action(&chosen, TodoAction::Reword).unwrap();
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
