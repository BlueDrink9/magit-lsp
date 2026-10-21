use magit_lsp::git::GitRepo;
use magit_lsp::patch::{self, FileLayout, Selection, SelectionOp};
use proptest::prelude::*;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
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
    write(&tmp.path().join("f.txt"), &(lines.join("\n") + "\n"));
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

/// A repository whose only content is a staged new file (no commits yet).
fn init_unborn_repo() -> (TempDir, GitRepo) {
    let tmp = TempDir::new().expect("tempdir");
    git(tmp.path(), &["init", "-q", "-b", "main"]);
    git(tmp.path(), &["config", "user.email", "t@example.com"]);
    git(tmp.path(), &["config", "user.name", "t"]);
    write(&tmp.path().join("f.txt"), "a\nb\nc\n");
    git(tmp.path(), &["add", "f.txt"]);

    let repo = GitRepo::open(tmp.path()).expect("open repo");
    (tmp, repo)
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).expect("read file")
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
    assert!(staged
        .hunks
        .iter()
        .any(|h| { h.lines.iter().any(|l| l.text == "line3 CHANGED") }));
    assert!(unstaged
        .hunks
        .iter()
        .any(|h| { h.lines.iter().any(|l| l.text == "NEW line") }));
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

#[test]
fn unstage_file_works_without_commits() {
    let (_tmp, repo) = init_unborn_repo();
    assert_eq!(repo.status().expect("status").staged.len(), 1);

    repo.unstage_file("f.txt")
        .expect("unstage file without HEAD");

    let status = repo.status().expect("status");
    assert!(status.staged.is_empty());
    assert_eq!(status.unstaged.len(), 1);
    assert_eq!(status.unstaged[0].status, "?");
}

#[test]
fn unstage_file_keeps_worktree_changes_without_commits() {
    let (tmp, repo) = init_unborn_repo();
    // The index still holds a/b/c while the working tree holds a/B/c.
    write(&tmp.path().join("f.txt"), "a\nB\nc\n");

    repo.unstage_file("f.txt")
        .expect("unstage file without HEAD");

    assert_eq!(read(&tmp.path().join("f.txt")), "a\nB\nc\n");
    let status = repo.status().expect("status");
    assert!(status.staged.is_empty());
    assert_eq!(status.unstaged.len(), 1);
    assert_eq!(status.unstaged[0].status, "?");
}

#[test]
fn unstage_hunk_works_without_commits() {
    let (tmp, repo) = init_unborn_repo();
    write(&tmp.path().join("f.txt"), "a\nB\nc\n");

    repo.unstage_hunk("f.txt", 0)
        .expect("unstage hunk without HEAD");

    assert_eq!(read(&tmp.path().join("f.txt")), "a\nB\nc\n");
    assert!(repo.status().expect("status").staged.is_empty());
}

#[test]
fn unstage_lines_works_without_commits() {
    let (tmp, repo) = init_unborn_repo();
    write(&tmp.path().join("f.txt"), "a\nB\nc\n");

    // Hunk lines are +a, +b, +c; unstage only +b.
    repo.unstage_lines("f.txt", 0, 1, 1)
        .expect("unstage lines without HEAD");

    assert_eq!(read(&tmp.path().join("f.txt")), "a\nB\nc\n");
    let status = repo.status().expect("status");
    assert_eq!(status.staged.len(), 1);
    let staged = &status.staged[0];
    assert_eq!(staged.hunks.len(), 1);
    let text: Vec<&str> = staged.hunks[0]
        .lines
        .iter()
        .map(|l| l.text.as_str())
        .collect();
    assert_eq!(text, vec!["a", "c"]);
}

#[test]
fn stage_hunk_works_without_commits() {
    let (tmp, repo) = init_unborn_repo();
    write(&tmp.path().join("f.txt"), "a\nB\nc\n");

    repo.stage_hunk("f.txt", 0)
        .expect("stage hunk without HEAD");

    assert_eq!(git(tmp.path(), &["show", ":f.txt"]), "a\nB\nc\n");
    assert!(repo.status().expect("status").unstaged.is_empty());
}

#[test]
fn stage_lines_works_without_commits() {
    let (tmp, repo) = init_unborn_repo();
    write(&tmp.path().join("f.txt"), "a\nB\nc\n");

    // Hunk lines are context a, -b, +B, context c.
    repo.stage_lines("f.txt", 0, 1, 2)
        .expect("stage lines without HEAD");

    assert_eq!(git(tmp.path(), &["show", ":f.txt"]), "a\nB\nc\n");
    assert!(repo.status().expect("status").unstaged.is_empty());
}

#[test]
fn stage_file_works_without_commits() {
    let (tmp, repo) = init_unborn_repo();
    write(&tmp.path().join("new.txt"), "hello\n");

    repo.stage_file("new.txt").expect("stage file without HEAD");

    let status = repo.status().expect("status");
    assert_eq!(status.staged.len(), 2);
    assert!(status.staged.iter().any(|f| f.path == "new.txt"));
}

#[test]
fn stage_all_works_without_commits() {
    let (tmp, repo) = init_unborn_repo();
    write(&tmp.path().join("f.txt"), "a\nB\nc\n");

    repo.stage_all().expect("stage all without HEAD");

    assert_eq!(git(tmp.path(), &["show", ":f.txt"]), "a\nB\nc\n");
    assert!(repo.status().expect("status").unstaged.is_empty());
}

fn layout_for(repo: &GitRepo, path: &str, staged: bool) -> FileLayout {
    let diff = if staged {
        repo.diff_staged(path).expect("staged diff")
    } else {
        repo.diff_unstaged(path).expect("unstaged diff")
    };
    FileLayout::from_diff(&diff, 0).expect("file layout")
}

/// A repo with a 30-line file, so two distant edits produce two hunks.
fn init_long_repo() -> (TempDir, GitRepo) {
    let tmp = TempDir::new().expect("tempdir");
    git(tmp.path(), &["init", "-q", "-b", "main"]);
    git(tmp.path(), &["config", "user.email", "t@example.com"]);
    git(tmp.path(), &["config", "user.name", "t"]);
    let lines: Vec<String> = (1..=30).map(|i| format!("line{i}")).collect();
    write(&tmp.path().join("f.txt"), &(lines.join("\n") + "\n"));
    git(tmp.path(), &["add", "."]);
    git(tmp.path(), &["commit", "-q", "-m", "init"]);
    let repo = GitRepo::open(tmp.path()).expect("open repo");
    (tmp, repo)
}

/// A repo with two tracked files, both ten lines long.
fn init_two_file_repo() -> (TempDir, GitRepo) {
    let tmp = TempDir::new().expect("tempdir");
    git(tmp.path(), &["init", "-q", "-b", "main"]);
    git(tmp.path(), &["config", "user.email", "t@example.com"]);
    git(tmp.path(), &["config", "user.name", "t"]);
    let lines: Vec<String> = (1..=10).map(|i| format!("line{i}")).collect();
    let body = lines.join("\n") + "\n";
    write(&tmp.path().join("f.txt"), &body);
    write(&tmp.path().join("g.txt"), &body);
    git(tmp.path(), &["add", "."]);
    git(tmp.path(), &["commit", "-q", "-m", "init"]);
    let repo = GitRepo::open(tmp.path()).expect("open repo");
    (tmp, repo)
}

fn apply_check(dir: &Path, patch_text: &str) -> Result<(), String> {
    let mut child = Command::new("git")
        .args([
            "apply",
            "--cached",
            "--unidiff-zero",
            "--recount",
            "--check",
            "-",
        ])
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    child
        .stdin
        .as_mut()
        .ok_or("no stdin")?
        .write_all(patch_text.as_bytes())
        .map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

#[test]
fn selection_context_only_is_noop() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());
    let layout = layout_for(&repo, "f.txt", false);
    let context_line = layout.hunks[0].first_line;

    let applied = repo
        .apply_selection(&[layout], context_line, context_line, false)
        .expect("apply selection");

    assert!(!applied, "context-only selection must not apply anything");
    assert!(cached_diff(tmp.path()).is_empty());
}

#[test]
fn selection_context_plus_change_stages_only_change() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());
    let layout = layout_for(&repo, "f.txt", false);
    let first = layout.hunks[0].first_line;

    // Context line1 through the line3 replacement.
    let applied = repo
        .apply_selection(&[layout], first, first + 3, false)
        .expect("apply selection");

    assert!(applied);
    let cached = cached_diff(tmp.path());
    assert!(cached.contains("-line3"));
    assert!(cached.contains("+line3 CHANGED"));
    assert!(!cached.contains("NEW line"));
    assert!(!cached.contains("-line8"));
}

#[test]
fn selection_header_plus_change_stages_whole_file() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());
    let layout = layout_for(&repo, "f.txt", false);
    let end = layout.hunks[0].first_line + 3;

    let applied = repo
        .apply_selection(&[layout], 0, end, false)
        .expect("apply selection");

    assert!(applied);
    let cached = cached_diff(tmp.path());
    assert!(cached.contains("+line3 CHANGED"));
    assert!(cached.contains("+NEW line"));
    assert!(cached.contains("-line8"));
}

#[test]
fn selection_crossing_two_hunks_stages_both() {
    let (tmp, repo) = init_long_repo();
    let mut lines: Vec<String> = (1..=30).map(|i| format!("line{i}")).collect();
    lines[1] = "line2 CHANGED".into();
    lines[27] = "line28 CHANGED".into();
    write(&tmp.path().join("f.txt"), &(lines.join("\n") + "\n"));

    let layout = layout_for(&repo, "f.txt", false);
    assert_eq!(layout.hunks.len(), 2);
    let start = layout.hunks[0].first_line + 1;
    let end = layout.hunks[1].first_line + 1;

    let applied = repo
        .apply_selection(&[layout], start, end, false)
        .expect("apply selection");

    assert!(applied);
    let cached = cached_diff(tmp.path());
    assert!(cached.contains("+line2 CHANGED"), "got: {cached}");
    assert!(cached.contains("+line28 CHANGED"), "got: {cached}");
}

#[test]
fn selection_crossing_two_files_stages_both() {
    let (tmp, repo) = init_two_file_repo();
    let mut f: Vec<String> = (1..=10).map(|i| format!("line{i}")).collect();
    f[2] = "f3 CHANGED".into();
    write(&tmp.path().join("f.txt"), &(f.join("\n") + "\n"));
    let mut g: Vec<String> = (1..=10).map(|i| format!("line{i}")).collect();
    g[2] = "g3 CHANGED".into();
    write(&tmp.path().join("g.txt"), &(g.join("\n") + "\n"));

    let first = layout_for(&repo, "f.txt", false);
    let second =
        FileLayout::from_diff(&repo.diff_unstaged("g.txt").expect("diff"), 100).expect("layout");
    let start = first.hunks[0].first_line + 1;
    let end = second.hunks[0].first_line + 1;

    let applied = repo
        .apply_selection(&[first, second], start, end, false)
        .expect("apply selection");

    assert!(applied);
    assert!(git(tmp.path(), &["diff", "--cached", "--", "f.txt"]).contains("f3 CHANGED"));
    assert!(git(tmp.path(), &["diff", "--cached", "--", "g.txt"]).contains("g3 CHANGED"));
}

#[test]
fn selection_inside_staged_hunk_unstages_only_that_change() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());
    repo.stage_file("f.txt").expect("stage file");
    let layout = layout_for(&repo, "f.txt", true);
    let first = layout.hunks[0].first_line;

    // The staged hunk has the same shape as the unstaged sample.
    let applied = repo
        .apply_selection(&[layout], first + 2, first + 3, true)
        .expect("apply selection");

    assert!(applied);
    let cached = cached_diff(tmp.path());
    assert!(!cached.contains("line3 CHANGED"), "got: {cached}");
    assert!(cached.contains("+NEW line"), "got: {cached}");
    assert!(cached.contains("-line8"), "got: {cached}");
}

#[test]
fn stage_lines_context_only_returns_false() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());

    let applied = repo.stage_lines("f.txt", 0, 0, 0).expect("stage lines");

    assert!(!applied);
    assert!(cached_diff(tmp.path()).is_empty());
}

fn random_content(edits: &[u8]) -> (String, String) {
    let mut original = String::new();
    let mut modified = String::new();
    for (i, edit) in edits.iter().enumerate() {
        original.push_str(&format!("L{i}\n"));
        match edit % 4 {
            0 => modified.push_str(&format!("L{i}\n")),
            1 => modified.push_str(&format!("M{i}\n")),
            2 => {}
            _ => {
                modified.push_str(&format!("L{i}\n"));
                modified.push_str(&format!("I{i}\n"));
            }
        }
    }
    (original, modified)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn random_selections_always_build_applicable_patches(
        edits in prop::collection::vec(0u8..4, 1..25),
        a in 0usize..60,
        b in 0usize..60,
    ) {
        let (original, modified) = random_content(&edits);
        prop_assume!(original != modified);

        let tmp = TempDir::new().expect("tempdir");
        git(tmp.path(), &["init", "-q", "-b", "main"]);
        git(tmp.path(), &["config", "user.email", "t@example.com"]);
        git(tmp.path(), &["config", "user.name", "t"]);
        write(&tmp.path().join("f.txt"), &original);
        git(tmp.path(), &["add", "."]);
        git(tmp.path(), &["commit", "-q", "-m", "init"]);
        write(&tmp.path().join("f.txt"), &modified);

        let repo = GitRepo::open(tmp.path()).expect("open repo");
        let diff = repo.diff_unstaged("f.txt").expect("diff");
        prop_assume!(!diff.is_empty());
        let layout = FileLayout::from_diff(&diff, 0).expect("layout");
        let max_line = layout
            .hunks
            .last()
            .map(|h| h.first_line + h.hunk.lines.len() as u32 - 1)
            .unwrap_or(0);
        let a = (a as u32).min(max_line);
        let b = (b as u32).min(max_line);
        let (start, end) = if a <= b { (a, b) } else { (b, a) };

        match patch::normalise_selection(&[layout.clone()], start, end) {
            Selection::NoChanges => {}
            Selection::Ops(ops) => {
                for op in ops {
                    let text = match op {
                        SelectionOp::WholeFile { .. } => layout.whole_file_patch(),
                        SelectionOp::WholeHunk { hunk_index, .. } => {
                            layout.whole_hunk_patch(hunk_index).expect("hunk patch")
                        }
                        SelectionOp::Lines { hunk_index, lines, .. } => {
                            layout.lines_patch(hunk_index, &lines, false).expect("lines patch")
                        }
                    };
                    if let Err(e) = apply_check(tmp.path(), &text) {
                        prop_assert!(false, "patch failed: {e}\n{text}");
                    }
                }
            }
        }
    }
}

#[test]
fn stage_lines_partial_deletion_uses_normal_header() {
    let (tmp, repo) = init_repo();
    std::fs::remove_file(tmp.path().join("f.txt")).expect("remove file");

    // The deletion hunk is -line1 .. -line10; stage only -line3.
    let applied = repo.stage_lines("f.txt", 0, 2, 2).expect("stage lines");

    assert!(applied);
    let cached = cached_diff(tmp.path());
    assert!(cached.contains("-line3"), "got: {cached}");
    assert!(!cached.contains("-line1"), "got: {cached}");
    assert!(!cached.contains("-line10"), "got: {cached}");
}

#[test]
fn unstage_selection_context_only_is_noop() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());
    repo.stage_file("f.txt").expect("stage file");
    let layout = layout_for(&repo, "f.txt", true);
    let context_line = layout.hunks[0].first_line;

    let applied = repo
        .apply_selection(&[layout], context_line, context_line, true)
        .expect("apply selection");

    assert!(!applied, "context-only selection must not apply anything");
    assert!(cached_diff(tmp.path()).contains("line3 CHANGED"));
}

fn apply_check_reverse(dir: &Path, patch_text: &str) -> Result<(), String> {
    let mut child = Command::new("git")
        .args([
            "apply",
            "--cached",
            "--unidiff-zero",
            "--recount",
            "-R",
            "--check",
            "-",
        ])
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    child
        .stdin
        .as_mut()
        .ok_or("no stdin")?
        .write_all(patch_text.as_bytes())
        .map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn random_reverse_selections_always_build_applicable_patches(
        edits in prop::collection::vec(0u8..4, 1..25),
        a in 0usize..60,
        b in 0usize..60,
    ) {
        let (original, modified) = random_content(&edits);
        prop_assume!(original != modified);

        let tmp = TempDir::new().expect("tempdir");
        git(tmp.path(), &["init", "-q", "-b", "main"]);
        git(tmp.path(), &["config", "user.email", "t@example.com"]);
        git(tmp.path(), &["config", "user.name", "t"]);
        write(&tmp.path().join("f.txt"), &original);
        git(tmp.path(), &["add", "."]);
        git(tmp.path(), &["commit", "-q", "-m", "init"]);
        write(&tmp.path().join("f.txt"), &modified);
        git(tmp.path(), &["add", "f.txt"]);

        let repo = GitRepo::open(tmp.path()).expect("open repo");
        let diff = repo.diff_staged("f.txt").expect("diff");
        prop_assume!(!diff.is_empty());
        let layout = FileLayout::from_diff(&diff, 0).expect("layout");
        let max_line = layout
            .hunks
            .last()
            .map(|h| h.first_line + h.hunk.lines.len() as u32 - 1)
            .unwrap_or(0);
        let a = (a as u32).min(max_line);
        let b = (b as u32).min(max_line);
        let (start, end) = if a <= b { (a, b) } else { (b, a) };

        match patch::normalise_selection(&[layout.clone()], start, end) {
            Selection::NoChanges => {}
            Selection::Ops(ops) => {
                for op in ops {
                    let text = match op {
                        SelectionOp::WholeFile { .. } => layout.whole_file_patch(),
                        SelectionOp::WholeHunk { hunk_index, .. } => {
                            layout.whole_hunk_patch(hunk_index).expect("hunk patch")
                        }
                        SelectionOp::Lines { hunk_index, lines, .. } => {
                            layout.lines_patch(hunk_index, &lines, true).expect("lines patch")
                        }
                    };
                    if let Err(e) = apply_check_reverse(tmp.path(), &text) {
                        prop_assert!(false, "patch failed: {e}\n{text}");
                    }
                }
            }
        }
    }
}
