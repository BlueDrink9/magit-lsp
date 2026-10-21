//! Status parsing: odd paths, renames, and branch reporting (R-0.2).

mod common;

use common::*;

fn status_paths(status: &magit_lsp::git::Status, staged: bool) -> Vec<String> {
    let files = if staged {
        &status.staged
    } else {
        &status.unstaged
    };
    files.iter().map(|f| f.path.clone()).collect()
}

#[test]
fn status_reports_path_with_spaces() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("a b.txt", TEN_LINES)])
        .build();
    write(&tmp.path().join("a b.txt"), &standard_changes());

    let status = repo.status().expect("status");
    assert_eq!(status_paths(&status, false), vec!["a b.txt"]);
    assert_eq!(status.unstaged[0].hunks.len(), 1);
}

#[test]
fn status_reports_path_with_quote_and_backslash() {
    let path = "a\"b\\c.txt";
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [(path, TEN_LINES)])
        .build();
    write(&tmp.path().join(path), &standard_changes());

    let status = repo.status().expect("status");
    assert_eq!(status_paths(&status, false), vec![path]);
    assert_eq!(status.unstaged[0].hunks.len(), 1);
}

#[test]
fn status_reports_unicode_path() {
    let path = "héllo wörld/日本.txt";
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

    repo.stage_lines("a b.txt", 0, 2, 3).expect("stage lines");

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

    repo.unstage_hunk(path, 0).expect("unstage hunk");

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

    repo.stage_lines(path, 0, 2, 3).expect("stage lines");

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

    let applied = repo.stage_lines("a b.txt", 0, 2, 2).expect("stage lines");

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
    assert_eq!(
        status_paths(&status, false),
        vec!["f.txt", "renamed file.txt"]
    );
    assert_eq!(status.unstaged[0].status, "D");
    assert_eq!(status.unstaged[1].status, "?");
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
