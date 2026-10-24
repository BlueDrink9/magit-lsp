mod common;

use common::*;
use magit_lsp::git::GitRepo;
use magit_lsp::patch::{self, FileLayout};
use magit_lsp::ErrorKind;
use proptest::prelude::*;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn identical_fixtures_produce_identical_commit_hashes() {
    let (first, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (second, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();

    assert_eq!(
        first.git(&["rev-parse", "HEAD"]),
        second.git(&["rev-parse", "HEAD"])
    );
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
    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();

    repo.stage_hunk("f.txt", &hunk_id).expect("stage hunk");

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
fn stage_hunk_by_id_survives_line_shifts() {
    let (tmp, repo) = init_40_line_repo();
    let mut lines: Vec<String> = (1..=40).map(|i| format!("line{i}")).collect();
    lines[34] = "line35 CHANGED".into();
    write(&tmp.path().join("f.txt"), &(lines.join("\n") + "\n"));
    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();

    // Insert lines above the hunk: its line numbers move, its content does not.
    let mut shifted = vec!["inserted1".to_string(), "inserted2".to_string()];
    shifted.extend(lines);
    write(&tmp.path().join("f.txt"), &(shifted.join("\n") + "\n"));

    repo.stage_hunk("f.txt", &hunk_id)
        .expect("stage the shifted hunk by id");

    let cached = cached_diff(tmp.path());
    assert!(cached.contains("+line35 CHANGED"), "got: {cached}");
    assert!(!cached.contains("inserted1"), "got: {cached}");
    assert!(!cached.contains("inserted2"), "got: {cached}");
}

#[test]
fn stage_hunk_rejects_hunk_changed_since_render() {
    let (tmp, repo) = init_40_line_repo();
    let mut lines: Vec<String> = (1..=40).map(|i| format!("line{i}")).collect();
    lines[34] = "line35 CHANGED".into();
    write(&tmp.path().join("f.txt"), &(lines.join("\n") + "\n"));
    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();

    lines[34] = "line35 DIFFERENT".into();
    write(&tmp.path().join("f.txt"), &(lines.join("\n") + "\n"));

    let err = repo
        .stage_hunk("f.txt", &hunk_id)
        .expect_err("the old hunk id must not resolve");
    assert_eq!(err.kind, ErrorKind::HunkNotFound);
    assert!(
        cached_diff(tmp.path()).is_empty(),
        "the index must stay untouched"
    );
}

#[test]
fn duplicate_content_hunks_get_distinct_ids_and_stage_independently() {
    let block = ["A", "B", "C", "D", "target", "E", "F", "G", "H", "I"];
    let filler: Vec<String> = (1..=10).map(|i| format!("J{i}")).collect();
    let mut original: Vec<String> = block.iter().map(|s| s.to_string()).collect();
    original.extend(filler.iter().cloned());
    original.extend(block.iter().map(|s| s.to_string()));
    let original = original.join("\n") + "\n";
    let modified = original.replace("target\n", "changed\n");

    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", original)])
        .build();
    write(&tmp.path().join("f.txt"), &modified);

    let status = repo.status().expect("status");
    let hunks = &status.unstaged[0].hunks;
    assert_eq!(hunks.len(), 2, "the edits are far enough apart");
    assert_ne!(hunks[0].id, hunks[1].id);
    assert_eq!(hunks[1].id, format!("{}-1", hunks[0].id));

    // Stage only the second occurrence.
    repo.stage_hunk("f.txt", &hunks[1].id)
        .expect("stage the second duplicate");
    let cached = cached_diff(tmp.path());
    assert_eq!(cached.matches("+changed").count(), 1, "got: {cached}");
    assert_eq!(
        repo.status().expect("status").unstaged[0].hunks.len(),
        1,
        "the first duplicate stays unstaged"
    );

    // The remaining occurrence still has the base id and can be staged.
    repo.stage_hunk("f.txt", &hunks[0].id)
        .expect("stage the first duplicate");
    let cached = cached_diff(tmp.path());
    assert_eq!(cached.matches("+changed").count(), 2, "got: {cached}");
}

#[test]
fn stage_lines_stages_only_modification() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());

    // line3 modification is hunk lines 2..=3
    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    repo.stage_lines("f.txt", &hunk_id, 2, 3)
        .expect("stage lines");

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

    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    repo.stage_lines("f.txt", &hunk_id, 6, 6)
        .expect("stage lines");

    let cached = cached_diff(tmp.path());
    assert!(cached.contains("+NEW line"));
    assert!(!cached.contains("line3 CHANGED"));
    assert!(!cached.contains("-line8"));
}

#[test]
fn stage_lines_stages_only_deletion() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());

    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    repo.stage_lines("f.txt", &hunk_id, 9, 9)
        .expect("stage lines");

    let cached = cached_diff(tmp.path());
    assert!(cached.contains("-line8"));
    assert!(!cached.contains("line3 CHANGED"));
    assert!(!cached.contains("NEW line"));
}

#[test]
fn unstage_hunk_reverts_staged_hunk() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());
    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    repo.stage_hunk("f.txt", &hunk_id).expect("stage");
    assert!(!cached_diff(tmp.path()).is_empty());

    let staged_id = repo.status().expect("status").staged[0].hunks[0]
        .id
        .clone();
    repo.unstage_hunk("f.txt", &staged_id).expect("unstage");

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
    assert!(status.unstaged.is_empty());
    assert_eq!(status.untracked.len(), 1);
    assert_eq!(status.untracked[0].status, "?");

    repo.stage_file("new.txt").expect("stage file");

    let status = repo.status().expect("status");
    assert!(status.unstaged.is_empty());
    assert!(status.untracked.is_empty());
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
    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    repo.stage_lines("f.txt", &hunk_id, 2, 3)
        .expect("stage lines");

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
    git(tmp.path(), &["init", "-q"]);
    git(tmp.path(), &["symbolic-ref", "HEAD", "refs/heads/main"]);
    git(tmp.path(), &["config", "user.email", "t@example.com"]);
    git(tmp.path(), &["config", "user.name", "t"]);
    write(&tmp.path().join("f.txt"), "hello\n");
    git(tmp.path(), &["add", "f.txt"]);

    let repo = GitRepo::open(tmp.path()).expect("open repo");
    assert_eq!(repo.status().expect("status").staged.len(), 1);

    repo.unstage_all().expect("unstage all without HEAD");

    let status = repo.status().expect("status");
    assert!(status.staged.is_empty());
    assert!(status.unstaged.is_empty());
    assert_eq!(status.untracked.len(), 1);
    assert_eq!(status.untracked[0].status, "?");
}

#[test]
fn unstage_file_works_without_commits() {
    let (_tmp, repo) = init_unborn_repo();
    assert_eq!(repo.status().expect("status").staged.len(), 1);

    repo.unstage_file("f.txt")
        .expect("unstage file without HEAD");

    let status = repo.status().expect("status");
    assert!(status.staged.is_empty());
    assert!(status.unstaged.is_empty());
    assert_eq!(status.untracked.len(), 1);
    assert_eq!(status.untracked[0].status, "?");
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
    assert!(status.unstaged.is_empty());
    assert_eq!(status.untracked.len(), 1);
    assert_eq!(status.untracked[0].status, "?");
}

#[test]
fn unstage_hunk_works_without_commits() {
    let (tmp, repo) = init_unborn_repo();
    write(&tmp.path().join("f.txt"), "a\nB\nc\n");

    let hunk_id = repo.status().expect("status").staged[0].hunks[0]
        .id
        .clone();
    repo.unstage_hunk("f.txt", &hunk_id)
        .expect("unstage hunk without HEAD");

    assert_eq!(read(&tmp.path().join("f.txt")), "a\nB\nc\n");
    assert!(repo.status().expect("status").staged.is_empty());
}

#[test]
fn unstage_lines_works_without_commits() {
    let (tmp, repo) = init_unborn_repo();
    write(&tmp.path().join("f.txt"), "a\nB\nc\n");

    // Hunk lines are +a, +b, +c; unstage only +b.
    let hunk_id = repo.status().expect("status").staged[0].hunks[0]
        .id
        .clone();
    repo.unstage_lines("f.txt", &hunk_id, 1, 1)
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

    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    repo.stage_hunk("f.txt", &hunk_id)
        .expect("stage hunk without HEAD");

    assert_eq!(git(tmp.path(), &["show", ":f.txt"]), "a\nB\nc\n");
    assert!(repo.status().expect("status").unstaged.is_empty());
}

#[test]
fn stage_lines_works_without_commits() {
    let (tmp, repo) = init_unborn_repo();
    write(&tmp.path().join("f.txt"), "a\nB\nc\n");

    // Hunk lines are context a, -b, +B, context c.
    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    repo.stage_lines("f.txt", &hunk_id, 1, 2)
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

/// Stage the replacement of `old` by `new`, recomputing the unstaged layout
/// first so earlier stagings are reflected.
fn stage_replacement(repo: &GitRepo, path: &str, old: &str, new: &str) {
    let layout = layout_for(repo, path, false);
    let (hunk_id, lines) = layout
        .hunks
        .iter()
        .find_map(|hunk| {
            let lines: Vec<usize> = hunk
                .hunk
                .lines
                .iter()
                .enumerate()
                .filter(|(_, line)| {
                    (line.marker == '-' && line.text == old)
                        || (line.marker == '+' && line.text == new)
                })
                .map(|(index, _)| index)
                .collect();
            (!lines.is_empty()).then_some((hunk.hunk.id.clone(), lines))
        })
        .unwrap_or_else(|| panic!("no change {old} -> {new}"));
    repo.stage_lines(path, &hunk_id, lines[0], lines[lines.len() - 1])
        .expect("stage change");
}

/// Assert the status view's staged hunks equal what `git diff --cached -U3`
/// printed for `f.txt`.
fn assert_staged_hunks_match_git(repo: &GitRepo, tmp: &Path) {
    let cached = cached_diff(tmp);
    let expected = patch::parse_unified_diff(&cached);
    let status = repo.status().expect("status");
    let staged = status
        .staged
        .iter()
        .find(|file| file.path == "f.txt")
        .expect("f.txt is staged");
    assert_eq!(
        staged.hunks, expected,
        "status hunks must match git diff --cached -U3\n{cached}"
    );
}

#[test]
fn two_distant_line_stagings_show_two_staged_hunks() {
    let (tmp, repo) = init_40_line_repo();
    let mut lines: Vec<String> = (1..=40).map(|i| format!("line{i}")).collect();
    lines[4] = "line5 CHANGED".into();
    lines[34] = "line35 CHANGED".into();
    write(&tmp.path().join("f.txt"), &(lines.join("\n") + "\n"));

    stage_replacement(&repo, "f.txt", "line5", "line5 CHANGED");
    stage_replacement(&repo, "f.txt", "line35", "line35 CHANGED");

    let cached = cached_diff(tmp.path());
    assert!(cached.contains("-line5\n"), "got: {cached}");
    assert!(cached.contains("+line5 CHANGED\n"), "got: {cached}");
    assert!(cached.contains("-line35\n"), "got: {cached}");
    assert!(cached.contains("+line35 CHANGED\n"), "got: {cached}");
    assert_eq!(hunk_count(&cached), 2, "got: {cached}");
    assert_staged_hunks_match_git(&repo, tmp.path());
}

#[test]
fn nearby_line_stagings_show_one_staged_hunk() {
    let (tmp, repo) = init_40_line_repo();
    let mut lines: Vec<String> = (1..=40).map(|i| format!("line{i}")).collect();
    lines[4] = "line5 CHANGED".into();
    lines[8] = "line9 CHANGED".into();
    write(&tmp.path().join("f.txt"), &(lines.join("\n") + "\n"));

    stage_replacement(&repo, "f.txt", "line5", "line5 CHANGED");
    stage_replacement(&repo, "f.txt", "line9", "line9 CHANGED");

    let cached = cached_diff(tmp.path());
    assert!(cached.contains("-line9\n"), "got: {cached}");
    assert!(cached.contains("+line9 CHANGED\n"), "got: {cached}");
    assert_eq!(hunk_count(&cached), 1, "got: {cached}");
    assert_staged_hunks_match_git(&repo, tmp.path());
}

#[test]
fn status_hunks_match_git_when_staged_together() {
    for (first, second) in [(4usize, 34usize), (4, 8)] {
        let (tmp, repo) = init_40_line_repo();
        let mut lines: Vec<String> = (1..=40).map(|i| format!("line{i}")).collect();
        lines[first] = format!("line{} CHANGED", first + 1);
        lines[second] = format!("line{} CHANGED", second + 1);
        write(&tmp.path().join("f.txt"), &(lines.join("\n") + "\n"));

        let layout = layout_for(&repo, "f.txt", false);
        let start = layout.hunks.first().expect("hunk").first_line;
        let end = layout
            .hunks
            .last()
            .map(|hunk| hunk.first_line + hunk.hunk.lines.len() as u32 - 1)
            .expect("hunk");
        let applied = repo
            .apply_selection(&[layout], start, end, false)
            .expect("apply selection");
        assert!(applied);

        let cached = cached_diff(tmp.path());
        let expected_hunks = if second - first - 1 > 6 { 2 } else { 1 };
        assert_eq!(hunk_count(&cached), expected_hunks, "got: {cached}");
        assert_staged_hunks_match_git(&repo, tmp.path());
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

    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    let applied = repo
        .stage_lines("f.txt", &hunk_id, 0, 0)
        .expect("stage lines");

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

fn model_selection(
    current: &str,
    layout: &FileLayout,
    start: u32,
    end: u32,
    reverse: bool,
) -> String {
    let (start, end) = if start <= end { (start, end) } else { (end, start) };
    let mut source: Vec<String> = current.lines().map(str::to_owned).collect();
    let mut result = Vec::new();
    let mut cursor = 0usize;

    for hunk in &layout.hunks {
        let whole_file = start <= layout.header_line && layout.header_line <= end;
        let whole_hunk = start <= hunk.header_line && hunk.header_line <= end;
        let source_start = if reverse {
            hunk.hunk.new_start
        } else {
            hunk.hunk.old_start
        } as usize;
        let before = source_start.saturating_sub(1);
        result.extend(source[cursor..before].iter().cloned());
        cursor = before;

        for (index, line) in hunk.hunk.lines.iter().enumerate() {
            let document_line = hunk.first_line + index as u32;
            let selected = whole_file || whole_hunk || (document_line >= start && document_line <= end);
            let source_line = if reverse { line.new_no } else { line.old_no };
            let is_source = if reverse { line.marker == '+' } else { line.marker == '-' };
            let is_target = if reverse { line.marker == '-' } else { line.marker == '+' };

            if line.marker == ' ' {
                if let Some(number) = source_line {
                    let position = number as usize - 1;
                    result.extend(source[cursor..position].iter().cloned());
                    result.push(source[position].clone());
                    cursor = position + 1;
                }
            } else if is_source {
                if let Some(number) = source_line {
                    let position = number as usize - 1;
                    result.extend(source[cursor..position].iter().cloned());
                    if !selected {
                        result.push(source[position].clone());
                    }
                    cursor = position + 1;
                }
            } else if is_target && selected {
                result.push(line.text.clone());
            }
        }
    }
    result.extend(source.drain(cursor..));
    if !result.is_empty() {
        result.join("\n") + "\n"
    } else {
        result.join("\n")
    }
}

fn index_content(repo: &RepoFixture) -> String {
    repo.git(&["show", ":f.txt"])
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn random_selections_apply_expected_content_and_reverse_back(
        edits in prop::collection::vec(0u8..4, 1..25),
        a in 0usize..60,
        b in 0usize..60,
        reverse in any::<bool>(),
    ) {
        let (original, modified) = random_content(&edits);
        prop_assume!(original != modified);

        let (tmp, repo) = RepoFixture::new()
            .commit("init", [("f.txt", original.clone())])
            .modify("f.txt", modified.clone())
            .build();
        if reverse {
            tmp.git(&["add", "f.txt"]);
        }
        let initial = if reverse { &modified } else { &original };
        let diff = if reverse { repo.diff_staged("f.txt") } else { repo.diff_unstaged("f.txt") }.expect("diff");
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

        let expected = model_selection(initial, &layout, start, end, reverse);
        let has_changes = layout.header_line >= start && layout.header_line <= end || layout.hunks.iter().any(|hunk| {
            let whole_hunk = hunk.header_line >= start && hunk.header_line <= end;
            hunk.hunk.lines.iter().enumerate().any(|(i, line)| {
                (line.marker == '+' || line.marker == '-') && (whole_hunk || (hunk.first_line + i as u32 >= start && hunk.first_line + i as u32 <= end))
            })
        });
        let applied = repo.apply_selection(&[layout], start, end, reverse).expect("apply selection");
        prop_assert_eq!(applied, has_changes, "selection {}..={}", start, end);
        prop_assert_eq!(index_content(&tmp), expected);

        let staged = if reverse { repo.diff_unstaged("f.txt") } else { repo.diff_staged("f.txt") }.expect("inverse diff");
        if staged.is_empty() {
            prop_assert_eq!(index_content(&tmp), initial.as_str());
        } else {
            let staged_layout = FileLayout::from_diff(&staged, 0).expect("staged layout");
            let changed: Vec<u32> = staged_layout.hunks.iter().flat_map(|hunk| {
                hunk.hunk.lines.iter().enumerate().filter_map(move |(i, line)| {
                    (line.marker == '+' || line.marker == '-').then_some(hunk.first_line + i as u32)
                })
            }).collect();
            let (reverse_start, reverse_end) = (*changed.first().unwrap(), *changed.last().unwrap());
            let applied = repo.apply_selection(&[staged_layout], reverse_start, reverse_end, !reverse).expect("inverse selection");
            prop_assert!(applied);
            prop_assert_eq!(index_content(&tmp), initial.as_str());
        }
    }
}

#[test]
fn stage_lines_partial_deletion_uses_normal_header() {
    let (tmp, repo) = init_repo();
    std::fs::remove_file(tmp.path().join("f.txt")).expect("remove file");

    // The deletion hunk is -line1 .. -line10; stage only -line3.
    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    let applied = repo
        .stage_lines("f.txt", &hunk_id, 2, 2)
        .expect("stage lines");

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
