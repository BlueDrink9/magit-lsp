//! Diff-shape cases that stress header preservation in `src/patch.rs`.

mod common;

use common::*;
use magit_lsp::git::GitRepo;
use magit_lsp::patch::FileLayout;
use magit_lsp::ErrorKind;
use std::path::Path;
use std::process::Command;

/// Parse one side's diff for `path` into a layout.
fn layout_for(repo: &GitRepo, path: &str, staged: bool) -> FileLayout {
    let diff = if staged {
        repo.diff_staged(path).expect("staged diff")
    } else {
        repo.diff_unstaged(path).expect("unstaged diff")
    };
    FileLayout::from_diff(&diff, 0).expect("file layout")
}

/// The index blob at `path`, as text.
fn index_text(dir: &Path, path: &str) -> String {
    git(dir, &["show", &format!(":{path}")])
}

/// The index blob at `path`, as raw bytes.
fn index_bytes(dir: &Path, path: &str) -> Vec<u8> {
    apply_determinism_env();
    let out = Command::new("git")
        .args(["show", &format!(":{path}")])
        .current_dir(dir)
        .output()
        .expect("run git show");
    assert!(
        out.status.success(),
        "git show :{path} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

/// The index mode of `path`, e.g. `100644`.
fn index_mode(dir: &Path, path: &str) -> String {
    let raw = git(dir, &["ls-files", "-s", "--", path]);
    raw.split_whitespace().next().unwrap_or("").to_string()
}

/// Write raw bytes to a fixture path.
fn write_bytes(dir: &Path, path: &str, bytes: &[u8]) {
    std::fs::write(dir.join(path), bytes).expect("write bytes");
}

fn line3_changed() -> String {
    TEN_LINES.replace("line3\n", "line3 CHANGED\n")
}

#[test]
fn rename_with_modifications_stages_hunks_and_unstages() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    std::fs::rename(tmp.path().join("f.txt"), tmp.path().join("g.txt")).expect("rename");
    write(&tmp.path().join("g.txt"), &standard_changes());
    // Intent-to-add is what lets `git diff` see the deletion and the new path
    // as one rename instead of a delete plus an untracked file.
    git(tmp.path(), &["add", "-N", "g.txt"]);

    let status = repo.status().expect("status");
    assert!(status.staged.is_empty());
    let entry = status
        .unstaged
        .iter()
        .find(|file| file.path == "g.txt")
        .expect("renamed file is unstaged");
    assert_eq!(entry.orig_path.as_deref(), Some("f.txt"));
    assert_eq!(entry.status, "R");
    assert_eq!(entry.hunks.len(), 1);

    let hunk_id = entry.hunks[0].id.clone();
    repo.stage_hunk("g.txt", &hunk_id).expect("stage rename hunk");
    let cached = git(tmp.path(), &["diff", "--cached", "-M", "-U3"]);
    assert!(cached.contains("rename from f.txt"), "got: {cached}");
    assert!(cached.contains("rename to g.txt"), "got: {cached}");
    assert!(cached.contains("+line3 CHANGED"), "got: {cached}");
    assert_eq!(index_text(tmp.path(), "g.txt"), standard_changes());
    assert!(git(tmp.path(), &["ls-files", "--", "f.txt"]).is_empty());
    assert_eq!(repo.status().expect("status").staged.len(), 1);

    let staged_id = repo.status().expect("status").staged[0].hunks[0]
        .id
        .clone();
    repo.unstage_hunk("g.txt", &staged_id)
        .expect("unstage rename hunk");
    assert_eq!(index_text(tmp.path(), "f.txt"), TEN_LINES);
    assert!(git(tmp.path(), &["ls-files", "--", "g.txt"]).is_empty());

    git(tmp.path(), &["add", "-N", "g.txt"]);
    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    repo.stage_lines("g.txt", &hunk_id, 2, 3)
        .expect("stage rename lines");
    let cached = git(tmp.path(), &["diff", "--cached", "-M", "-U3"]);
    assert!(cached.contains("rename from f.txt"), "got: {cached}");
    assert!(cached.contains("+line3 CHANGED"), "got: {cached}");
    assert!(!cached.contains("NEW line"), "got: {cached}");
    assert_eq!(index_text(tmp.path(), "g.txt"), line3_changed());
    // The rename is staged, the remaining edits are still unstaged.
    let status = repo.status().expect("status");
    assert!(status.staged.iter().any(|file| file.path == "g.txt"));
    assert!(status.unstaged.iter().any(|file| file.path == "g.txt"));

    repo.unstage_file("g.txt").expect("unstage rename file");
    assert_eq!(index_text(tmp.path(), "f.txt"), TEN_LINES);
    assert!(git(tmp.path(), &["ls-files", "--", "g.txt"]).is_empty());
}

#[cfg(unix)]
#[test]
fn mode_change_only_stages_and_unstages_whole_file() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    set_mode(&tmp.path().join("f.txt"), 0o755);

    let status = repo.status().expect("status");
    assert!(status.staged.is_empty());
    let entry = &status.unstaged[0];
    assert_eq!(entry.path, "f.txt");
    assert_eq!(entry.status, "M");
    assert!(entry.hunks.is_empty());

    let layout = layout_for(&repo, "f.txt", false);
    assert!(layout.hunks.is_empty());
    assert!(
        layout.header.contains("old mode 100644"),
        "{}",
        layout.header
    );
    assert!(
        layout.header.contains("new mode 100755"),
        "{}",
        layout.header
    );
    assert_eq!(layout.whole_file_patch(), layout.header);

    repo.stage_file("f.txt").expect("stage mode change");
    assert_eq!(index_mode(tmp.path(), "f.txt"), "100755");
    assert_eq!(index_text(tmp.path(), "f.txt"), TEN_LINES);
    let cached = git(tmp.path(), &["diff", "--cached", "-U3", "--", "f.txt"]);
    assert!(cached.contains("old mode 100644"), "got: {cached}");
    assert!(cached.contains("new mode 100755"), "got: {cached}");
    let status = repo.status().expect("status");
    assert_eq!(status.staged.len(), 1);
    assert!(status.staged[0].hunks.is_empty());

    repo.unstage_file("f.txt").expect("unstage mode change");
    assert_eq!(index_mode(tmp.path(), "f.txt"), "100644");
    assert_eq!(repo.status().expect("status").unstaged.len(), 1);
}

#[cfg(unix)]
#[test]
fn mode_and_content_change_keeps_mode_lines_in_hunk_headers() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    set_mode(&tmp.path().join("f.txt"), 0o755);
    write(&tmp.path().join("f.txt"), &standard_changes());

    let layout = layout_for(&repo, "f.txt", false);
    assert!(
        layout.header.contains("old mode 100644"),
        "{}",
        layout.header
    );
    assert!(
        layout.header.contains("new mode 100755"),
        "{}",
        layout.header
    );
    let hunk_id = layout.hunks[0].hunk.id.clone();
    let whole = layout.whole_hunk_patch(&hunk_id).expect("hunk patch");
    assert!(whole.contains("old mode 100644"), "got: {whole}");
    assert!(whole.contains("new mode 100755"), "got: {whole}");
    let lines = layout
        .lines_patch(&hunk_id, &[2, 3], false)
        .expect("lines patch");
    assert!(lines.contains("old mode 100644"), "got: {lines}");
    assert!(lines.contains("new mode 100755"), "got: {lines}");

    repo.stage_lines("f.txt", &hunk_id, 2, 3)
        .expect("stage lines");
    let cached = git(tmp.path(), &["diff", "--cached", "-U3", "--", "f.txt"]);
    assert!(cached.contains("old mode 100644"), "got: {cached}");
    assert!(cached.contains("new mode 100755"), "got: {cached}");
    assert!(cached.contains("+line3 CHANGED"), "got: {cached}");
    assert_eq!(index_text(tmp.path(), "f.txt"), line3_changed());
    assert_eq!(index_mode(tmp.path(), "f.txt"), "100755");
}

#[cfg(unix)]
#[test]
fn symlink_type_change_is_whole_file_only() {
    use std::os::unix::fs::symlink;

    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("regular", "x\n"), ("target.txt", "target\n")])
        .build();
    std::fs::remove_file(tmp.path().join("regular")).expect("remove regular file");
    symlink("target.txt", tmp.path().join("regular")).expect("create symlink");

    let diff = repo.diff_unstaged("regular").expect("diff");
    assert!(
        diff.matches("diff --git ").count() >= 2,
        "a type change is two file sections:\n{diff}"
    );
    assert!(
        FileLayout::from_diff(&diff, 0).is_none(),
        "a two-section diff has no single header"
    );
    let status = repo.status().expect("status");
    let entry = status
        .unstaged
        .iter()
        .find(|file| file.path == "regular")
        .expect("type change is unstaged");
    assert_eq!(entry.status, "T");
    assert!(entry.hunks.is_empty());

    // Hunk staging must refuse rather than apply only the deletion half.
    assert!(repo.stage_hunk("regular", "0000000000000000").is_err());
    assert_eq!(index_text(tmp.path(), "regular"), "x\n");
    assert_eq!(index_mode(tmp.path(), "regular"), "100644");

    repo.stage_file("regular").expect("stage type change");
    assert_eq!(index_mode(tmp.path(), "regular"), "120000");
    assert_eq!(index_text(tmp.path(), "regular"), "target.txt");
    let cached = git(tmp.path(), &["diff", "--cached", "-U3", "--", "regular"]);
    assert!(cached.contains("deleted file mode 100644"), "got: {cached}");
    assert!(cached.contains("new file mode 120000"), "got: {cached}");
    let status = repo.status().expect("status");
    assert_eq!(status.staged.len(), 1);
    assert!(status.staged[0].hunks.is_empty());

    repo.unstage_file("regular").expect("unstage type change");
    assert_eq!(index_mode(tmp.path(), "regular"), "100644");
    assert_eq!(index_text(tmp.path(), "regular"), "x\n");

    symlink("target.txt", tmp.path().join("link")).expect("create new symlink");
    repo.stage_file("link").expect("stage new symlink");
    assert_eq!(index_mode(tmp.path(), "link"), "120000");
    assert_eq!(index_text(tmp.path(), "link"), "target.txt");
}

#[test]
fn binary_file_is_whole_file_only_and_hunk_requests_fail_cleanly() {
    let binary = b"\x00\x01\x02\xffbinary\n";
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    write_bytes(tmp.path(), "f.txt", binary);

    let diff = repo.diff_unstaged("f.txt").expect("diff");
    assert!(diff.contains("Binary files"), "got: {diff}");
    let layout = FileLayout::from_diff(&diff, 0).expect("header-only layout");
    assert!(layout.hunks.is_empty());
    assert!(layout.header.contains("Binary files"), "{}", layout.header);

    let status = repo.status().expect("status");
    let entry = &status.unstaged[0];
    assert_eq!(entry.path, "f.txt");
    assert!(entry.hunks.is_empty());

    // A binary file exposes no hunks, so any hunk id is stale by definition.
    let err = repo
        .stage_hunk("f.txt", "0000000000000000")
        .expect_err("no hunk to stage");
    assert_eq!(err.kind, ErrorKind::HunkNotFound);
    assert_eq!(index_bytes(tmp.path(), "f.txt"), TEN_LINES.as_bytes());

    let err = repo
        .stage_lines("f.txt", "0000000000000000", 0, 0)
        .expect_err("no lines to stage");
    assert_eq!(err.kind, ErrorKind::HunkNotFound);
    assert_eq!(index_bytes(tmp.path(), "f.txt"), TEN_LINES.as_bytes());

    repo.stage_file("f.txt").expect("stage binary file");
    assert_eq!(index_bytes(tmp.path(), "f.txt"), binary);
    let cached = git(tmp.path(), &["diff", "--cached", "-U3", "--", "f.txt"]);
    assert!(cached.contains("Binary files"), "got: {cached}");
    let status = repo.status().expect("status");
    assert_eq!(status.staged.len(), 1);
    assert!(status.staged[0].hunks.is_empty());

    repo.unstage_file("f.txt").expect("unstage binary file");
    assert_eq!(index_bytes(tmp.path(), "f.txt"), TEN_LINES.as_bytes());
}

#[test]
fn no_trailing_newline_markers_survive_line_staging() {
    let (tmp, repo) = RepoFixture::new()
        .commit(
            "init",
            [
                ("old_side.txt", "a\nb"),
                ("new_side.txt", "a\nb\n"),
                ("both.txt", "a\nb"),
            ],
        )
        .build();
    write(&tmp.path().join("old_side.txt"), "a\nB\n");
    write(&tmp.path().join("new_side.txt"), "a\nB");
    write(&tmp.path().join("both.txt"), "a\nB");

    for path in ["old_side.txt", "new_side.txt", "both.txt"] {
        let hunk_id = repo
            .status()
            .expect("status")
            .unstaged
            .iter()
            .find(|file| file.path == path)
            .expect("unstaged path")
            .hunks[0]
            .id
            .clone();
        repo.stage_lines(path, &hunk_id, 1, 2)
            .unwrap_or_else(|err| panic!("stage {path}: {err}"));
    }

    assert_eq!(index_text(tmp.path(), "old_side.txt"), "a\nB\n");
    assert_eq!(index_text(tmp.path(), "new_side.txt"), "a\nB");
    assert_eq!(index_text(tmp.path(), "both.txt"), "a\nB");

    let old_cached = git(
        tmp.path(),
        &["diff", "--cached", "-U3", "--", "old_side.txt"],
    );
    assert_eq!(marker_count(&old_cached), 1, "got: {old_cached}");
    let new_cached = git(
        tmp.path(),
        &["diff", "--cached", "-U3", "--", "new_side.txt"],
    );
    assert_eq!(marker_count(&new_cached), 1, "got: {new_cached}");
    let both_cached = git(tmp.path(), &["diff", "--cached", "-U3", "--", "both.txt"]);
    assert_eq!(marker_count(&both_cached), 2, "got: {both_cached}");
}

#[test]
fn crlf_line_staging_preserves_carriage_returns() {
    let original = "line1\r\nline2\r\nline3\r\n";
    let replacement = "line1\r\nline2 CHANGED\r\nline3\r\n";
    let addition = "line1\r\nline2\r\nline2b\r\nline3\r\n";
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("replace.txt", original), ("add.txt", original)])
        .build();
    write(&tmp.path().join("replace.txt"), replacement);
    write(&tmp.path().join("add.txt"), addition);

    // Replace line2: hunk lines are context, -line2, +line2 CHANGED, context.
    let replace_id = repo
        .status()
        .expect("status")
        .unstaged
        .iter()
        .find(|file| file.path == "replace.txt")
        .expect("replace.txt is unstaged")
        .hunks[0]
        .id
        .clone();
    repo.stage_lines("replace.txt", &replace_id, 1, 2)
        .expect("stage replacement");
    // Add line2b: select the single added line.
    let add_id = repo
        .status()
        .expect("status")
        .unstaged
        .iter()
        .find(|file| file.path == "add.txt")
        .expect("add.txt is unstaged")
        .hunks[0]
        .id
        .clone();
    repo.stage_lines("add.txt", &add_id, 2, 2)
        .expect("stage addition");

    assert_eq!(index_text(tmp.path(), "replace.txt"), replacement);
    assert_eq!(index_text(tmp.path(), "add.txt"), addition);

    let cached = git(
        tmp.path(),
        &["diff", "--cached", "-U3", "--", "replace.txt"],
    );
    assert!(cached.contains("-line2\r\n"), "got: {cached:?}");
    assert!(cached.contains("+line2 CHANGED\r\n"), "got: {cached:?}");
    let cached = git(tmp.path(), &["diff", "--cached", "-U3", "--", "add.txt"]);
    assert!(cached.contains("+line2b\r\n"), "got: {cached:?}");
}

/// Number of `\ No newline at end of file` markers in a diff.
fn marker_count(diff: &str) -> usize {
    diff.lines()
        .filter(|line| *line == "\\ No newline at end of file")
        .count()
}

/// Change a file's mode bits.
#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("chmod");
}
