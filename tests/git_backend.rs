use magit_lsp::git::GitRepo;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run git");
    if !out.status.success() {
        panic!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn write(path: &Path, content: &str) {
    std::fs::write(path, content).expect("write file");
}

fn init_repo() -> (TempDir, GitRepo) {
    let tmp = TempDir::new().expect("tempdir");
    git(tmp.path(), &["init", "-q", "-b", "main"]);
    git(tmp.path(), &["config", "user.email", "t@example.com"]);
    git(tmp.path(), &["config", "user.name", "t"]);
    let lines: Vec<String> = (1..=10).map(|i| format!("line{i}")).collect();
    write(
        &tmp.path().join("f.txt"),
        &(lines.join("\n") + "\n"),
    );
    git(tmp.path(), &["add", "."]);
    git(tmp.path(), &["commit", "-q", "-m", "init"]);

    let repo = GitRepo::open(tmp.path()).expect("open repo");
    (tmp, repo)
}

fn make_standard_changes(tmp: &Path) {
    let mut lines: Vec<String> = (1..=10).map(|i| format!("line{i}")).collect();
    lines[2] = "line3 CHANGED".into();
    lines.insert(5, "NEW line".into());
    lines.remove(8);
    write(&tmp.join("f.txt"), &(lines.join("\n") + "\n"));
}

fn cached_diff(tmp: &Path) -> String {
    git(tmp, &["diff", "--cached", "-U3", "--", "f.txt"])
}

#[test]
fn status_reports_modified_file_with_hunk() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());

    let status = repo.status().expect("status");
    assert_eq!(status.branch, "main");
    assert!(status.staged.is_empty());
    assert_eq!(status.unstaged.len(), 1);
    let entry = &status.unstaged[0];
    assert_eq!(entry.path, "f.txt");
    assert_eq!(entry.hunks.len(), 1);
    assert_eq!(entry.hunks[0].lines.len(), 12);
}

#[test]
fn diff_unstaged_returns_full_diff() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());
    let diff = repo.diff_unstaged("f.txt").expect("diff");
    assert!(diff.contains("-line3"));
    assert!(diff.contains("+line3 CHANGED"));
    assert!(diff.contains("+NEW line"));
    assert!(diff.contains("-line8"));
}

#[test]
fn stage_hunk_stages_entire_file() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());

    repo.stage_hunk("f.txt", 0).expect("stage hunk");

    let cached = cached_diff(tmp.path());
    assert!(cached.contains("-line3"));
    assert!(cached.contains("+line3 CHANGED"));
    assert!(cached.contains("+NEW line"));
    assert!(cached.contains("-line8"));

    let status = repo.status().expect("status");
    assert!(status.unstaged.is_empty());
    assert_eq!(status.staged.len(), 1);
}

#[test]
fn stage_lines_stages_only_modification() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());

    // line3 modification is hunk lines 2..=3
    repo.stage_lines("f.txt", 0, 2, 3).expect("stage lines");

    let cached = cached_diff(tmp.path());
    assert!(cached.contains("-line3"));
    assert!(cached.contains("+line3 CHANGED"));
    assert!(!cached.contains("NEW line"));
    assert!(!cached.contains("-line8"));
}

#[test]
fn stage_lines_stages_only_addition() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());

    repo.stage_lines("f.txt", 0, 6, 6).expect("stage lines");

    let cached = cached_diff(tmp.path());
    assert!(cached.contains("+NEW line"));
    assert!(!cached.contains("line3 CHANGED"));
    assert!(!cached.contains("-line8"));
}

#[test]
fn stage_lines_stages_only_deletion() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());

    repo.stage_lines("f.txt", 0, 9, 9).expect("stage lines");

    let cached = cached_diff(tmp.path());
    assert!(cached.contains("-line8"));
    assert!(!cached.contains("line3 CHANGED"));
    assert!(!cached.contains("NEW line"));
}

#[test]
fn unstage_hunk_reverts_staged_hunk() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());
    repo.stage_hunk("f.txt", 0).expect("stage");
    assert!(!cached_diff(tmp.path()).is_empty());

    repo.unstage_hunk("f.txt", 0).expect("unstage");

    let cached = cached_diff(tmp.path());
    assert!(!cached.contains("line3 CHANGED"));
    let status = repo.status().expect("status");
    assert!(status.staged.is_empty());
    assert_eq!(status.unstaged.len(), 1);
}

#[test]
fn stage_file_stages_untracked_file() {
    let (tmp, repo) = init_repo();
    write(&tmp.path().join("new.txt"), "hello\n");

    let status = repo.status().expect("status");
    assert_eq!(status.unstaged.len(), 1);
    assert_eq!(status.unstaged[0].status, "?");

    repo.stage_file("new.txt").expect("stage file");

    let status = repo.status().expect("status");
    assert!(status.unstaged.is_empty());
    assert_eq!(status.staged.len(), 1);
    assert_eq!(status.staged[0].path, "new.txt");
}

#[test]
fn unstage_file_unstages_staged_file() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());
    repo.stage_file("f.txt").expect("stage file");
    assert_eq!(repo.status().expect("status").staged.len(), 1);

    repo.unstage_file("f.txt").expect("unstage file");

    let status = repo.status().expect("status");
    assert!(status.staged.is_empty());
    assert_eq!(status.unstaged.len(), 1);
}

#[test]
fn status_reports_partially_staged_as_both() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());
    repo.stage_lines("f.txt", 0, 2, 3).expect("stage lines");

    let status = repo.status().expect("status");
    assert_eq!(status.staged.len(), 1);
    assert_eq!(status.unstaged.len(), 1);
    let staged = &status.staged[0];
    let unstaged = &status.unstaged[0];
    assert_eq!(staged.path, "f.txt");
    assert_eq!(unstaged.path, "f.txt");
    // staged side only has the line3 change; unstaged side has the rest
    assert!(staged.hunks.iter().any(|h| {
        h.lines.iter().any(|l| l.text == "line3 CHANGED")
    }));
    assert!(unstaged.hunks.iter().any(|h| {
        h.lines.iter().any(|l| l.text == "NEW line")
    }));
}

#[test]
fn stage_all_stages_everything() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());

    repo.stage_all().expect("stage all");

    let status = repo.status().expect("status");
    assert_eq!(status.staged.len(), 1);
    assert!(status.unstaged.is_empty());
}

#[test]
fn unstage_all_resets_the_index() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());
    repo.stage_all().expect("stage all");

    repo.unstage_all().expect("unstage all");

    let status = repo.status().expect("status");
    assert!(status.staged.is_empty());
    assert_eq!(status.unstaged.len(), 1);
}

#[test]
fn unstage_all_works_without_commits() {
    let tmp = TempDir::new().expect("tempdir");
    git(tmp.path(), &["init", "-q", "-b", "main"]);
    git(tmp.path(), &["config", "user.email", "t@example.com"]);
    git(tmp.path(), &["config", "user.name", "t"]);
    write(&tmp.path().join("f.txt"), "hello\n");
    git(tmp.path(), &["add", "f.txt"]);

    let repo = GitRepo::open(tmp.path()).expect("open repo");
    assert_eq!(repo.status().expect("status").staged.len(), 1);

    repo.unstage_all().expect("unstage all without HEAD");

    let status = repo.status().expect("status");
    assert!(status.staged.is_empty());
    assert_eq!(status.unstaged.len(), 1);
    assert_eq!(status.unstaged[0].status, "?");
}