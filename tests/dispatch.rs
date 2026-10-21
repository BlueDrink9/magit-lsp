mod common;

use common::*;
use magit_lsp::dispatch::{self, Command, LineRange, NodeKind, Resolution};
use magit_lsp::errors;
use magit_lsp::patch::{self, FileLayout};
use magit_lsp::protocol::{Effect, MessageLevel};

fn message_text(resolution: &Resolution) -> &str {
    match resolution {
        Resolution::Message(Effect::Message { level, text, .. }) => {
            assert_eq!(*level, MessageLevel::Info, "expected an info message");
            text
        }
        other => panic!("expected a message resolution, got {other:?}"),
    }
}

fn file_target() -> NodeKind {
    NodeKind::FileHeader {
        path: "f.txt".to_string(),
    }
}

fn hunk_target() -> NodeKind {
    NodeKind::HunkHeader {
        path: "f.txt".to_string(),
        hunk_id: "h0".to_string(),
    }
}

fn diff_line_target(changed: bool) -> NodeKind {
    NodeKind::DiffLine {
        changed,
        path: "f.txt".to_string(),
        hunk_id: "h0".to_string(),
    }
}

// --- IMM-6 table: status.stage -------------------------------------------

#[test]
fn stage_unstaged_section_stages_all() {
    assert_eq!(
        dispatch::resolve(Command::Stage, NodeKind::SectionUnstaged, None),
        Resolution::StageAllUnstaged
    );
}

#[test]
fn stage_staged_section_is_na() {
    let resolution = dispatch::resolve(Command::Stage, NodeKind::SectionStaged, None);
    assert_eq!(message_text(&resolution), dispatch::NOTHING_TO_STAGE);
}

#[test]
fn stage_untracked_section_stages_all_untracked() {
    assert_eq!(
        dispatch::resolve(Command::Stage, NodeKind::SectionUntracked, None),
        Resolution::StageAllUntracked
    );
}

#[test]
fn stage_file_header_stages_file() {
    assert_eq!(
        dispatch::resolve(Command::Stage, file_target(), None),
        Resolution::StageFile {
            path: "f.txt".to_string()
        }
    );
}

#[test]
fn stage_hunk_header_stages_hunk() {
    assert_eq!(
        dispatch::resolve(Command::Stage, hunk_target(), None),
        Resolution::StageHunk {
            path: "f.txt".to_string(),
            hunk_id: "h0".to_string()
        }
    );
}

#[test]
fn stage_changed_line_stages_whole_hunk() {
    assert_eq!(
        dispatch::resolve(Command::Stage, diff_line_target(true), None),
        Resolution::StageHunk {
            path: "f.txt".to_string(),
            hunk_id: "h0".to_string()
        }
    );
}

#[test]
fn stage_context_line_stages_whole_hunk() {
    assert_eq!(
        dispatch::resolve(Command::Stage, diff_line_target(false), None),
        Resolution::StageHunk {
            path: "f.txt".to_string(),
            hunk_id: "h0".to_string()
        }
    );
}

#[test]
fn stage_other_row_is_na() {
    let resolution = dispatch::resolve(Command::Stage, NodeKind::Other, None);
    assert_eq!(message_text(&resolution), dispatch::NOTHING_TO_STAGE);
}

#[test]
fn stage_selection_delegates_to_imm2() {
    assert_eq!(
        dispatch::resolve(
            Command::Stage,
            NodeKind::Other,
            Some(LineRange { start: 4, end: 5 })
        ),
        Resolution::Selection {
            start: 4,
            end: 5,
            reverse: false
        }
    );
}

// --- IMM-6 table: status.unstage -----------------------------------------

#[test]
fn unstage_unstaged_section_is_na() {
    let resolution = dispatch::resolve(Command::Unstage, NodeKind::SectionUnstaged, None);
    assert_eq!(message_text(&resolution), dispatch::NOTHING_TO_UNSTAGE);
}

#[test]
fn unstage_staged_section_unstages_all() {
    assert_eq!(
        dispatch::resolve(Command::Unstage, NodeKind::SectionStaged, None),
        Resolution::UnstageAllStaged
    );
}

#[test]
fn unstage_untracked_section_is_na() {
    let resolution = dispatch::resolve(Command::Unstage, NodeKind::SectionUntracked, None);
    assert_eq!(message_text(&resolution), dispatch::NOTHING_TO_UNSTAGE);
}

#[test]
fn unstage_file_header_unstages_file() {
    assert_eq!(
        dispatch::resolve(Command::Unstage, file_target(), None),
        Resolution::UnstageFile {
            path: "f.txt".to_string()
        }
    );
}

#[test]
fn unstage_hunk_header_unstages_hunk() {
    assert_eq!(
        dispatch::resolve(Command::Unstage, hunk_target(), None),
        Resolution::UnstageHunk {
            path: "f.txt".to_string(),
            hunk_id: "h0".to_string()
        }
    );
}

#[test]
fn unstage_changed_line_unstages_whole_hunk() {
    assert_eq!(
        dispatch::resolve(Command::Unstage, diff_line_target(true), None),
        Resolution::UnstageHunk {
            path: "f.txt".to_string(),
            hunk_id: "h0".to_string()
        }
    );
}

#[test]
fn unstage_context_line_unstages_whole_hunk() {
    assert_eq!(
        dispatch::resolve(Command::Unstage, diff_line_target(false), None),
        Resolution::UnstageHunk {
            path: "f.txt".to_string(),
            hunk_id: "h0".to_string()
        }
    );
}

#[test]
fn unstage_other_row_is_na() {
    let resolution = dispatch::resolve(Command::Unstage, NodeKind::Other, None);
    assert_eq!(message_text(&resolution), dispatch::NOTHING_TO_UNSTAGE);
}

#[test]
fn unstage_selection_delegates_to_imm2() {
    assert_eq!(
        dispatch::resolve(
            Command::Unstage,
            NodeKind::Other,
            Some(LineRange { start: 4, end: 5 })
        ),
        Resolution::Selection {
            start: 4,
            end: 5,
            reverse: true
        }
    );
}

// --- IMM-6 table: status.discard -----------------------------------------

#[test]
fn discard_unstaged_section_needs_confirm() {
    assert_eq!(
        dispatch::resolve(Command::Discard, NodeKind::SectionUnstaged, None),
        Resolution::DiscardAllUnstaged {
            needs_confirm: true
        }
    );
}

#[test]
fn discard_staged_section_is_na() {
    let resolution = dispatch::resolve(Command::Discard, NodeKind::SectionStaged, None);
    assert_eq!(message_text(&resolution), dispatch::NOTHING_TO_DISCARD);
}

#[test]
fn discard_untracked_section_needs_confirm() {
    assert_eq!(
        dispatch::resolve(Command::Discard, NodeKind::SectionUntracked, None),
        Resolution::DiscardAllUntracked {
            needs_confirm: true
        }
    );
}

#[test]
fn discard_file_header_needs_confirm() {
    assert_eq!(
        dispatch::resolve(Command::Discard, file_target(), None),
        Resolution::DiscardFile {
            path: "f.txt".to_string(),
            needs_confirm: true
        }
    );
}

#[test]
fn discard_hunk_header_needs_confirm() {
    assert_eq!(
        dispatch::resolve(Command::Discard, hunk_target(), None),
        Resolution::DiscardHunk {
            path: "f.txt".to_string(),
            hunk_id: "h0".to_string(),
            needs_confirm: true
        }
    );
}

#[test]
fn discard_changed_line_needs_confirm() {
    assert_eq!(
        dispatch::resolve(Command::Discard, diff_line_target(true), None),
        Resolution::DiscardHunk {
            path: "f.txt".to_string(),
            hunk_id: "h0".to_string(),
            needs_confirm: true
        }
    );
}

#[test]
fn discard_context_line_needs_confirm() {
    assert_eq!(
        dispatch::resolve(Command::Discard, diff_line_target(false), None),
        Resolution::DiscardHunk {
            path: "f.txt".to_string(),
            hunk_id: "h0".to_string(),
            needs_confirm: true
        }
    );
}

#[test]
fn discard_other_row_is_na() {
    let resolution = dispatch::resolve(Command::Discard, NodeKind::Other, None);
    assert_eq!(message_text(&resolution), dispatch::NOTHING_TO_DISCARD);
}

#[test]
fn discard_selection_needs_confirm() {
    assert_eq!(
        dispatch::resolve(
            Command::Discard,
            NodeKind::Other,
            Some(LineRange { start: 4, end: 5 })
        ),
        Resolution::DiscardSelection {
            start: 4,
            end: 5,
            needs_confirm: true
        }
    );
}

#[test]
fn selection_wins_over_node_kind() {
    assert_eq!(
        dispatch::resolve(
            Command::Stage,
            file_target(),
            Some(LineRange { start: 10, end: 20 })
        ),
        Resolution::Selection {
            start: 10,
            end: 20,
            reverse: false
        }
    );
}

// --- Executor: supported operations --------------------------------------

#[test]
fn execute_stage_file_stages_it() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());

    let effects = dispatch::execute(
        &repo,
        &Resolution::StageFile {
            path: "f.txt".to_string(),
        },
        &[],
    )
    .expect("execute");

    assert!(effects.is_empty());
    assert!(!cached_diff(tmp.path()).is_empty());
    assert_eq!(repo.status().expect("status").staged.len(), 1);
}

#[test]
fn execute_unstage_file_unstages_it() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());
    repo.stage_file("f.txt").expect("stage");

    let effects = dispatch::execute(
        &repo,
        &Resolution::UnstageFile {
            path: "f.txt".to_string(),
        },
        &[],
    )
    .expect("execute");

    assert!(effects.is_empty());
    assert!(cached_diff(tmp.path()).is_empty());
    assert!(repo.status().expect("status").staged.is_empty());
}

#[test]
fn execute_stage_hunk_stages_only_that_hunk() {
    let (tmp, repo) = init_repo_40();
    make_two_hunks(tmp.path());
    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();

    dispatch::execute(
        &repo,
        &Resolution::StageHunk {
            path: "f.txt".to_string(),
            hunk_id,
        },
        &[],
    )
    .expect("execute");

    let cached = cached_diff(tmp.path());
    assert_eq!(hunk_count(&cached), 1, "got: {cached}");
    assert!(cached.contains("+line5 CHANGED"), "got: {cached}");
    assert!(!cached.contains("line35 CHANGED"), "got: {cached}");
}

#[test]
fn execute_unstage_hunk_unstages_only_that_hunk() {
    let (tmp, repo) = init_repo_40();
    make_two_hunks(tmp.path());
    repo.stage_file("f.txt").expect("stage");
    let hunk_id = repo.status().expect("status").staged[0].hunks[0]
        .id
        .clone();

    dispatch::execute(
        &repo,
        &Resolution::UnstageHunk {
            path: "f.txt".to_string(),
            hunk_id,
        },
        &[],
    )
    .expect("execute");

    let cached = cached_diff(tmp.path());
    assert_eq!(hunk_count(&cached), 1, "got: {cached}");
    assert!(cached.contains("+line35 CHANGED"), "got: {cached}");
    assert!(!cached.contains("line5 CHANGED"), "got: {cached}");
}

#[test]
fn execute_stage_hunk_reports_hunk_changed_since_display() {
    let (tmp, repo) = init_40_line_repo();
    let mut lines: Vec<String> = (1..=40).map(|i| format!("line{i}")).collect();
    lines[34] = "line35 CHANGED".into();
    write(&tmp.path().join("f.txt"), &(lines.join("\n") + "\n"));
    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();

    // The rendered hunk no longer exists once its content changes.
    lines[34] = "line35 DIFFERENT".into();
    write(&tmp.path().join("f.txt"), &(lines.join("\n") + "\n"));

    let effects = dispatch::execute(
        &repo,
        &Resolution::StageHunk {
            path: "f.txt".to_string(),
            hunk_id,
        },
        &[],
    )
    .expect("execute");

    assert_eq!(
        effects,
        vec![Effect::message(MessageLevel::Warn, errors::HUNK_CHANGED)]
    );
    assert!(
        cached_diff(tmp.path()).is_empty(),
        "the index must stay untouched"
    );
}

#[test]
fn server_mutation_returns_fresh_status_with_hunk_changed_message() {
    let (tmp, repo) = init_40_line_repo();
    let mut lines: Vec<String> = (1..=40).map(|i| format!("line{i}")).collect();
    lines[34] = "line35 CHANGED".into();
    write(&tmp.path().join("f.txt"), &(lines.join("\n") + "\n"));
    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    lines[34] = "line35 DIFFERENT".into();
    write(&tmp.path().join("f.txt"), &(lines.join("\n") + "\n"));

    let response = magit_lsp::server::mutation(tmp.path().to_str().expect("utf-8 path"), |repo| {
        repo.stage_hunk("f.txt", &hunk_id).map(|_| true)
    })
    .expect("mutation");

    assert_eq!(
        response.effects,
        vec![Effect::message(MessageLevel::Warn, errors::HUNK_CHANGED)]
    );
    assert!(response.status.staged.is_empty());
    assert_eq!(response.status.unstaged.len(), 1);
}

#[test]
fn execute_stage_all_unstaged_stages_tracked_changes() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());
    write(&tmp.path().join("g.txt"), "g1\n");
    git(tmp.path(), &["add", "g.txt"]);
    git(tmp.path(), &["commit", "-q", "-m", "g"]);
    write(&tmp.path().join("g.txt"), "g1 changed\n");

    dispatch::execute(&repo, &Resolution::StageAllUnstaged, &[]).expect("execute");

    let status = repo.status().expect("status");
    assert_eq!(status.staged.len(), 2);
    assert!(status.unstaged.is_empty());
}

#[test]
fn execute_unstage_all_staged_unstages_everything() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());
    repo.stage_file("f.txt").expect("stage");

    dispatch::execute(&repo, &Resolution::UnstageAllStaged, &[]).expect("execute");

    assert!(repo.status().expect("status").staged.is_empty());
    assert!(cached_diff(tmp.path()).is_empty());
}

#[test]
fn execute_stage_all_untracked_stages_only_untracked() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());
    write(&tmp.path().join("new.txt"), "brand new\n");
    write(&tmp.path().join("spaced name.txt"), "spaced\n");

    dispatch::execute(&repo, &Resolution::StageAllUntracked, &[]).expect("execute");

    let porcelain = git(tmp.path(), &["status", "--porcelain"]);
    assert!(porcelain.contains("A  new.txt"), "got: {porcelain}");
    assert!(porcelain.contains("spaced name.txt"), "got: {porcelain}");
    // The tracked modification is left unstaged.
    assert!(porcelain.contains(" M f.txt"), "got: {porcelain}");

    let status = repo.status().expect("status");
    assert!(status.untracked.is_empty());
    assert_eq!(status.unstaged.len(), 1);
    assert_eq!(status.unstaged[0].path, "f.txt");
}

#[test]
fn execute_selection_stages_only_changed_lines() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());
    let diff = repo.diff_unstaged("f.txt").expect("diff");
    let layout = FileLayout::from_diff(&diff, 0).expect("layout");

    // Document line 4..=5 is the line3 replacement; line 6 is context.
    let effects = dispatch::execute(
        &repo,
        &Resolution::Selection {
            start: 4,
            end: 5,
            reverse: false,
        },
        &[layout],
    )
    .expect("execute");

    assert!(effects.is_empty());
    let cached = cached_diff(tmp.path());
    assert!(cached.contains("+line3 CHANGED"), "got: {cached}");
    assert!(!cached.contains("NEW line"), "got: {cached}");
}

#[test]
fn execute_selection_without_changes_reports_message() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());
    let diff = repo.diff_unstaged("f.txt").expect("diff");
    let layout = FileLayout::from_diff(&diff, 0).expect("layout");

    // Document line 2 is the first context line of the hunk.
    let effects = dispatch::execute(
        &repo,
        &Resolution::Selection {
            start: 2,
            end: 2,
            reverse: false,
        },
        &[layout],
    )
    .expect("execute");

    assert_eq!(effects.len(), 1);
    match &effects[0] {
        Effect::Message { level, text, .. } => {
            assert_eq!(*level, MessageLevel::Info);
            assert_eq!(text, patch::NO_CHANGED_LINES);
        }
    }
    assert!(cached_diff(tmp.path()).is_empty());
}

// --- Executor: discard is deferred ---------------------------------------

#[test]
fn execute_discard_file_changes_nothing() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());
    let before = repo.status().expect("status");
    let before_content = std::fs::read_to_string(tmp.path().join("f.txt")).expect("read");

    let effects = dispatch::execute(
        &repo,
        &Resolution::DiscardFile {
            path: "f.txt".to_string(),
            needs_confirm: true,
        },
        &[],
    )
    .expect("execute");

    assert_eq!(repo.status().expect("status"), before);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("f.txt")).expect("read"),
        before_content
    );
    assert_eq!(effects.len(), 1);
    match &effects[0] {
        Effect::Message { level, text, .. } => {
            assert_eq!(*level, MessageLevel::Info);
            assert!(
                text.starts_with("Not implemented yet:"),
                "got: {text}"
            );
        }
    }
}

#[test]
fn execute_every_discard_variant_is_deferred() {
    let (tmp, repo) = init_repo();
    make_standard_changes(tmp.path());
    let before = repo.status().expect("status");

    let variants = [
        Resolution::DiscardAllUnstaged {
            needs_confirm: true,
        },
        Resolution::DiscardAllUntracked {
            needs_confirm: true,
        },
        Resolution::DiscardFile {
            path: "f.txt".to_string(),
            needs_confirm: true,
        },
        Resolution::DiscardHunk {
            path: "f.txt".to_string(),
            hunk_id: "h0".to_string(),
            needs_confirm: true,
        },
        Resolution::DiscardSelection {
            start: 4,
            end: 5,
            needs_confirm: true,
        },
    ];

    for variant in variants {
        let effects = dispatch::execute(&repo, &variant, &[]).expect("execute");
        assert_eq!(effects.len(), 1, "for {variant:?}");
        match &effects[0] {
            Effect::Message { level, text, .. } => {
                assert_eq!(*level, MessageLevel::Info);
                assert!(
                    text.starts_with("Not implemented yet:"),
                    "for {variant:?} got: {text}"
                );
            }
        }
        assert_eq!(repo.status().expect("status"), before, "for {variant:?}");
    }
}

#[test]
fn execute_message_resolution_returns_the_same_message() {
    let (_tmp, repo) = init_repo();
    let resolution = dispatch::resolve(Command::Stage, NodeKind::Other, None);
    let effects = dispatch::execute(&repo, &resolution, &[]).expect("execute");
    assert_eq!(effects, vec![resolution_message(&resolution)]);
}

fn resolution_message(resolution: &Resolution) -> Effect {
    match resolution {
        Resolution::Message(effect) => effect.clone(),
        other => panic!("expected a message resolution, got {other:?}"),
    }
}
