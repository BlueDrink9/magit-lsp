//! Status parsing: odd paths, renames, and branch reporting (R-0.2).

mod common;

use common::*;
use rstest::rstest;

fn status_paths(status: &magit_lsp::git::Status, staged: bool) -> Vec<String> {
    let files = if staged {
        &status.staged
    } else {
        &status.unstaged
    };
    files.iter().map(|f| f.path.clone()).collect()
}

fn untracked_paths(status: &magit_lsp::git::Status) -> Vec<String> {
    let mut paths: Vec<String> = status.untracked.iter().map(|f| f.path.clone()).collect();
    paths.sort();
    paths
}

#[rstest]
#[case::status_reports_path_with_spaces("a b.txt")]
#[case::status_reports_path_with_quote_and_backslash("a\"b\\c.txt")]
#[case::status_reports_unicode_path("héllo wörld/日本.txt")]
fn status_reports_unusual_path(#[case] path: &str) {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [(path, TEN_LINES)])
        .build();
    write(&tmp.path().join(path), &standard_changes());

    let status = repo.status().expect("status");
    assert_eq!(status_paths(&status, false), vec![path]);
    assert_eq!(status.unstaged[0].hunks.len(), 1);
}

#[test]
fn stage_lines_works_on_spaced_path() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("a b.txt", TEN_LINES)])
        .build();
    write(&tmp.path().join("a b.txt"), &standard_changes());

    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    repo.stage_lines("a b.txt", &hunk_id, 2, 3)
        .expect("stage lines");

    let cached = git(tmp.path(), &["diff", "--cached", "-U3", "--", "a b.txt"]);
    assert!(cached.contains("-line3"), "got: {cached}");
    assert!(cached.contains("+line3 CHANGED"), "got: {cached}");
    assert!(!cached.contains("NEW line"), "got: {cached}");
    assert!(!cached.contains("-line8"), "got: {cached}");
}

#[test]
fn unstage_hunk_works_on_unicode_path() {
    let path = "héllo wörld/日本.txt";
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [(path, TEN_LINES)])
        .build();
    write(&tmp.path().join(path), &standard_changes());
    repo.stage_file(path).expect("stage file");

    let hunk_id = repo.status().expect("status").staged[0].hunks[0]
        .id
        .clone();
    repo.unstage_hunk(path, &hunk_id).expect("unstage hunk");

    let cached = git(tmp.path(), &["diff", "--cached", "-U3", "--", path]);
    assert!(cached.is_empty(), "got: {cached}");
    let status = repo.status().expect("status");
    assert!(status.staged.is_empty());
    assert_eq!(status_paths(&status, false), vec![path]);
}

#[test]
fn stage_lines_works_on_quote_and_backslash_path() {
    let path = "a\"b\\c.txt";
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [(path, TEN_LINES)])
        .build();
    write(&tmp.path().join(path), &standard_changes());

    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    repo.stage_lines(path, &hunk_id, 2, 3)
        .expect("stage lines");

    let cached = git(tmp.path(), &["diff", "--cached", "-U3", "--", path]);
    assert!(cached.contains("+line3 CHANGED"), "got: {cached}");
    assert!(!cached.contains("NEW line"), "got: {cached}");
}

#[test]
fn stage_lines_partial_deletion_works_on_spaced_path() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("a b.txt", TEN_LINES)])
        .build();
    std::fs::remove_file(tmp.path().join("a b.txt")).expect("remove file");

    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    let applied = repo
        .stage_lines("a b.txt", &hunk_id, 2, 2)
        .expect("stage lines");

    assert!(applied);
    let cached = git(tmp.path(), &["diff", "--cached", "-U3", "--", "a b.txt"]);
    assert!(cached.contains("-line3"), "got: {cached}");
    assert!(!cached.contains("-line1"), "got: {cached}");
    assert!(!cached.contains("-line10"), "got: {cached}");
}

#[test]
fn staged_rename_reports_both_paths() {
    let (_tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .rename("f.txt", "renamed file.txt")
        .build();

    let status = repo.status().expect("status");
    assert_eq!(status.staged.len(), 1);
    let entry = &status.staged[0];
    assert_eq!(entry.path, "renamed file.txt");
    assert_eq!(entry.status, "R");
    assert_eq!(entry.orig_path.as_deref(), Some("f.txt"));
    assert!(status.unstaged.is_empty());
}

#[test]
fn unstage_file_on_rename_restores_both_paths() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .rename("f.txt", "renamed file.txt")
        .build();

    repo.unstage_file("renamed file.txt").expect("unstage file");

    let status = repo.status().expect("status");
    assert!(status.staged.is_empty(), "staged: {:?}", status.staged);
    assert_eq!(status_paths(&status, false), vec!["f.txt"]);
    assert_eq!(status.unstaged[0].status, "D");
    assert_eq!(untracked_paths(&status), vec!["renamed file.txt"]);
    // The working tree keeps the new name.
    assert!(tmp.path().join("renamed file.txt").exists());
}

#[test]
fn detached_head_is_reported_as_head() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .detached()
        .build();
    let oid = tmp.git(&["rev-parse", "HEAD"]).trim().to_string();

    let status = repo.status().expect("status");
    assert_eq!(status.branch, "HEAD");
    assert_eq!(status.head.as_deref(), Some(oid.as_str()));
}

#[test]
fn unborn_head_reports_the_configured_branch() {
    let (_tmp, repo) = init_unborn_repo();

    let status = repo.status().expect("status");
    assert_eq!(status.branch, "main");
    assert!(status.head.is_none());
}

#[test]
fn status_reports_renamed_path_hunks() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .rename("f.txt", "renamed file.txt")
        .build();
    write(&tmp.path().join("renamed file.txt"), &standard_changes());

    let status = repo.status().expect("status");
    let staged = status
        .staged
        .iter()
        .find(|f| f.path == "renamed file.txt")
        .expect("staged rename");
    assert_eq!(staged.orig_path.as_deref(), Some("f.txt"));
    assert_eq!(status.unstaged.len(), 1);
    assert_eq!(status.unstaged[0].path, "renamed file.txt");
    assert_eq!(status.unstaged[0].hunks.len(), 1);
}

#[test]
fn untracked_files_are_separate_from_unstaged() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    write(&tmp.path().join("new.txt"), "new\n");
    write(&tmp.path().join("spaced name.txt"), "spaced\n");

    let status = repo.status().expect("status");
    assert_eq!(status_paths(&status, false), vec!["f.txt"]);
    assert_eq!(
        untracked_paths(&status),
        vec!["new.txt", "spaced name.txt"]
    );
    for file in &status.untracked {
        assert_eq!(file.status, "?");
        assert!(file.hunks.is_empty());
    }
}

#[test]
fn stage_untracked_stages_only_untracked_files() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    write(&tmp.path().join("new.txt"), "new\n");
    write(&tmp.path().join("spaced name.txt"), "spaced\n");

    repo.stage_untracked().expect("stage untracked");

    let status = repo.status().expect("status");
    assert!(status.untracked.is_empty());
    let staged: Vec<String> = status.staged.iter().map(|f| f.path.clone()).collect();
    assert_eq!(staged, vec!["new.txt", "spaced name.txt"]);
    // The tracked modification stays in the working tree.
    assert_eq!(status_paths(&status, false), vec!["f.txt"]);
    let cached = git(tmp.path(), &["diff", "--cached", "--name-only"]);
    assert!(!cached.contains("f.txt"), "got: {cached}");
}

#[test]
fn stage_untracked_works_on_unborn_head() {
    let (tmp, repo) = RepoFixture::new().unborn().build();
    write(&tmp.path().join("new.txt"), "new\n");

    repo.stage_untracked().expect("stage untracked");

    let status = repo.status().expect("status");
    assert!(status.untracked.is_empty());
    assert_eq!(status.staged.len(), 1);
    assert_eq!(status.staged[0].path, "new.txt");
    assert_eq!(status.staged[0].status, "A");
}
