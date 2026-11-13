mod common;

use common::{git, write};
use magit_lsp::git::GitRepo;
use magit_lsp::instant_rebase::{InstantAutosquashKind, InstantRebaseAuthority, InstantRebaseOutcome};
use magit_lsp::rebase_sequence_editor::SequenceEditorPolicy;
use std::path::Path;
use tempfile::TempDir;

fn repo() -> (TempDir, GitRepo, Vec<String>) {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-b", "main"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    let mut ids = Vec::new();
    for value in ["zero\n", "one\n", "two\n", "three\n"] {
        write(&dir.path().join("file"), value);
        git(dir.path(), &["add", "file"]);
        git(dir.path(), &["commit", "-m", value.trim()]);
        ids.push(git(dir.path(), &["rev-parse", "HEAD"]).trim().to_owned());
    }
    write(&dir.path().join("file"), "three\nstaged\n");
    git(dir.path(), &["add", "file"]);
    let repo = GitRepo::open(dir.path()).unwrap();
    (dir, repo, ids)
}

fn state(path: &Path) -> (String, String, String, Vec<u8>, Vec<u8>, Vec<u8>) {
    (
        git(path, &["rev-parse", "HEAD"]),
        git(path, &["show-ref"]),
        git(path, &["status", "--porcelain=v1", "--untracked-files=all"]),
        std::fs::read(path.join(".git/HEAD")).unwrap(),
        std::fs::read(path.join(".git/index")).unwrap(),
        std::fs::read(path.join("file")).unwrap(),
    )
}

#[test]
fn captured_fixup_executes_marker_and_real_autosquash() {
    let (dir, _repo, ids) = repo();
    git(dir.path(), &["reset", "--hard", "HEAD"]);
    write(&dir.path().join("fixup-only"), "folded\n");
    git(dir.path(), &["add", "fixup-only"]);
    let repo = GitRepo::open(dir.path()).unwrap();
    let authority = InstantRebaseAuthority::prepare(&repo, &ids[2], InstantAutosquashKind::Fixup).unwrap();
    let operation = magit_lsp::git::RepositoryOperation::InstantRebase(authority);
    let result = magit_lsp::editor::execute_operation(
        &dir.path().to_string_lossy(), "instant-fixup", Path::new("unused-socket"),
        &operation, Path::new(env!("CARGO_BIN_EXE_magit-lsp")),
    ).unwrap();
    let marker = match result {
        magit_lsp::git::RepositoryOperationOutcome::InstantRebase(InstantRebaseOutcome::Completed { marker, warning: None }) => marker,
        other => panic!("unexpected outcome: {other:?}"),
    };
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD^^" ]).trim(), ids[1]);
    assert_ne!(git(dir.path(), &["rev-parse", "HEAD"]).trim(), marker);
    assert_eq!(git(dir.path(), &["log", "--format=%s", &format!("{}..HEAD", ids[1])]), "three\ntwo\n");
    assert_eq!(git(dir.path(), &["show", "-s", "--format=%s", "HEAD^"]), "two\n");
    assert_eq!(git(dir.path(), &["show", "HEAD:fixup-only"]), "folded\n");
    assert!(git(dir.path(), &["status", "--porcelain"]).is_empty());
    assert!(!dir.path().join(".git/rebase-merge").exists());
    assert!(!dir.path().join(".git/rebase-apply").exists());
}

#[test]
fn instant_fixup_with_duplicate_subject_folds_selected_full_oid() {
    let (dir, _repo, ids) = repo();
    git(dir.path(), &["reset", "--hard", &ids[1]]);
    write(&dir.path().join("later-two"), "later\n");
    git(dir.path(), &["add", "later-two"]);
    git(dir.path(), &["commit", "-m", "two"]);
    let selected = git(dir.path(), &["rev-parse", "HEAD"]).trim().to_owned();
    write(&dir.path().join("following"), "following\n");
    git(dir.path(), &["add", "following"]);
    git(dir.path(), &["commit", "-m", "following"]);
    write(&dir.path().join("fixup-only"), "folded\n");
    git(dir.path(), &["add", "fixup-only"]);
    let repo = GitRepo::open(dir.path()).unwrap();
    let authority = InstantRebaseAuthority::prepare(&repo, &selected, InstantAutosquashKind::Fixup).unwrap();
    let result = authority.execute(&repo, Path::new(env!("CARGO_BIN_EXE_magit-lsp")), &[]);
    assert!(matches!(result, InstantRebaseOutcome::Completed { .. }), "unexpected outcome: {result:?}");
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD~2"]).trim(), ids[1]);
    assert_eq!(git(dir.path(), &["log", "--format=%s", &format!("{}..HEAD", ids[1])]), "following\ntwo\n");
    assert_eq!(git(dir.path(), &["show", "HEAD~1:later-two"]), "later\n");
    assert_eq!(git(dir.path(), &["show", "HEAD:fixup-only"]), "folded\n");
    assert_eq!(git(dir.path(), &["show", "HEAD:following"]), "following\n");
    assert!(!git(dir.path(), &["rev-list", "--all"]).lines().any(|oid| oid == selected));
}

#[test]
fn commit_msg_hook_suffix_rejects_marker_before_rebase_and_retains_hook_result() {
    let (dir, repo, ids) = repo();
    write(&dir.path().join("hook-only"), "retained\n");
    git(dir.path(), &["add", "hook-only"]);
    let hook = dir.path().join(".git/hooks/commit-msg");
    write(&hook, "#!/bin/sh\nprintf ' extra' >> \"$1\"\n");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let authority = InstantRebaseAuthority::prepare(&repo, &ids[2], InstantAutosquashKind::Fixup).unwrap();
    let outcome = authority.execute(&repo, Path::new(env!("CARGO_BIN_EXE_magit-lsp")), &[]);
    let marker = match outcome {
        InstantRebaseOutcome::RebaseStopped { marker, phase, error } => {
            assert_eq!(phase, "marker verification");
            assert!(error.contains("created autosquash marker does not match"));
            marker
        }
        other => panic!("unexpected outcome: {other:?}"),
    };
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]).trim(), marker);
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD^"]).trim(), ids[3]);
    assert_eq!(git(dir.path(), &["show", "-s", "--format=%s", &marker]).trim(), format!("fixup! {}  extra", ids[2]));
    assert_eq!(git(dir.path(), &["show", &format!("{marker}:hook-only")]), "retained\n");
    assert!(!dir.path().join(".git/rebase-merge").exists());
    assert!(!dir.path().join(".git/rebase-apply").exists());
}

#[test]
fn stale_refusal_and_post_marker_launch_failure_preserve_real_state() {
    let (dir, repo, ids) = repo();
    let stale = InstantRebaseAuthority::prepare(&repo, &ids[2], InstantAutosquashKind::Fixup).unwrap();
    write(&dir.path().join("file"), "changed\n");
    let before = state(dir.path());
    assert!(matches!(stale.execute(&repo, Path::new(env!("CARGO_BIN_EXE_magit-lsp")), &[]), InstantRebaseOutcome::Refused(_)));
    assert_eq!(state(dir.path()), before);

    git(dir.path(), &["reset", "--hard", "HEAD"]);
    write(&dir.path().join("failure-only"), "retained\n");
    git(dir.path(), &["add", "failure-only"]);
    let repo = GitRepo::open(dir.path()).unwrap();
    let authority = InstantRebaseAuthority::prepare(&repo, &ids[2], InstantAutosquashKind::Fixup).unwrap();
    let missing_helper = dir.path().join("missing-helper");
    let operation = magit_lsp::git::RepositoryOperation::InstantRebase(authority);
    let outcome = magit_lsp::editor::execute_operation(
        &dir.path().to_string_lossy(), "instant-fixup", Path::new("unused-socket"),
        &operation, &missing_helper,
    ).unwrap();
    let marker = match outcome {
        magit_lsp::git::RepositoryOperationOutcome::InstantRebase(
            InstantRebaseOutcome::RebaseStopped { marker, phase, .. },
        ) => { assert_eq!(phase, "autosquash"); marker },
        other => panic!("unexpected outcome: {other:?}"),
    };
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]).trim(), marker);
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD^"]).trim(), ids[3]);
    assert!(git(dir.path(), &["status", "--porcelain"]).is_empty());
    assert!(!dir.path().join(".git/rebase-merge").exists());
}

#[test]
fn shared_autosquash_launcher_preserves_git_transformed_fixup_todo() {
    let (dir, repo, ids) = repo();
    git(dir.path(), &["reset", "--hard", "HEAD"]);
    write(&dir.path().join("fixup-only"), "folded\n");
    git(dir.path(), &["add", "fixup-only"]);
    git(dir.path(), &["commit", "--fixup", &ids[2]]);
    let marker = git(dir.path(), &["rev-parse", "HEAD"]).trim().to_owned();
    let marker_message = git(dir.path(), &["show", "-s", "--format=%s", &marker]);
    assert!(marker_message.starts_with("fixup! two"));
    repo.launch_interactive_rebase(&ids[1], None, SequenceEditorPolicy::PreserveAutosquash,
        Path::new(env!("CARGO_BIN_EXE_magit-lsp"))).unwrap();
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD~2"]).trim(), ids[1]);
    assert_eq!(git(dir.path(), &["show", "-s", "--format=%s", "HEAD~1"]).trim(), "two");
    assert_eq!(git(dir.path(), &["show", "-s", "--format=%s", "HEAD"]).trim(), "three");
    assert_eq!(std::fs::read(dir.path().join("file")).unwrap(), b"three\n");
    assert_eq!(std::fs::read(dir.path().join("fixup-only")).unwrap(), b"folded\n");
    assert!(!git(dir.path(), &["rev-list", "--all"]).lines().any(|oid| oid == marker));
    assert!(!dir.path().join(".git/rebase-merge").exists());
}

#[test]
fn captures_typed_full_oid_authority_and_prepare_validate_discard_are_read_only() {
    for kind in [InstantAutosquashKind::Fixup, InstantAutosquashKind::Squash] {
        let (dir, repo, ids) = repo();
        let before = state(dir.path());
        let authority = InstantRebaseAuthority::prepare(&repo, &ids[1][..8], kind).unwrap();
        assert_eq!(authority.kind(), kind);
        assert_eq!(authority.target(), ids[1]);
        assert_eq!(authority.base(), ids[0]);
        assert_eq!(authority.membership(), &ids[1..]);
        assert!(authority.membership().iter().all(|oid| oid.len() == 40));
        authority.validate(&repo).unwrap();
        assert_eq!(state(dir.path()), before);
        authority.discard();
        assert_eq!(state(dir.path()), before);
    }
}

#[test]
fn validation_rejects_a_different_repository_without_mutation() {
    let (other_dir, other_repo, _) = repo();
    let (dir, primary_repo, ids) = repo();
    let authority = InstantRebaseAuthority::prepare(&primary_repo, &ids[1], InstantAutosquashKind::Fixup).unwrap();
    let before = state(dir.path());
    let other_before = state(other_dir.path());
    assert!(authority.validate(&other_repo).unwrap_err().to_string().contains("repository identity changed"));
    assert_eq!(state(dir.path()), before);
    assert_eq!(state(other_dir.path()), other_before);
}

#[test]
fn validation_rejects_stale_head_attachment_index_and_worktree_without_mutation() {
    for stale in ["head", "attachment", "index", "worktree"] {
        let (dir, repo, ids) = repo();
        let authority = InstantRebaseAuthority::prepare(&repo, &ids[1], InstantAutosquashKind::Fixup).unwrap();
        match stale {
            "head" => {
                git(dir.path(), &["commit", "-m", "stale head"]);
            }
            "attachment" => {
                git(dir.path(), &["branch", "side", "HEAD"]);
                git(dir.path(), &["symbolic-ref", "HEAD", "refs/heads/side"]);
            }
            "index" => {
                write(&dir.path().join("file"), "three\ndifferent staged\n");
                git(dir.path(), &["add", "file"]);
            }
            "worktree" => write(&dir.path().join("file"), "three\nstaged\nunstaged\n"),
            _ => unreachable!(),
        }
        let before = state(dir.path());
        let error = authority.validate(&repo).unwrap_err().to_string();
        assert!(error.contains(match stale {
            "head" => "HEAD changed",
            "attachment" => "HEAD attachment changed",
            "index" => "index changed",
            "worktree" => "tracked unstaged changes",
            _ => unreachable!(),
        }), "{error}");
        assert_eq!(state(dir.path()), before);
    }
}

#[test]
fn preparation_refuses_active_operation_and_empty_staged_change_without_mutation() {
    for refusal in ["operation", "empty"] {
        let (dir, repo, ids) = repo();
        if refusal == "operation" {
            std::fs::write(dir.path().join(".git/MERGE_HEAD"), format!("{}\n", ids[0])).unwrap();
        } else {
            git(dir.path(), &["reset", "--hard", "HEAD"]);
        }
        let before = state(dir.path());
        let error = InstantRebaseAuthority::prepare(&repo, &ids[1], InstantAutosquashKind::Fixup)
            .unwrap_err().to_string();
        assert!(error.contains(if refusal == "operation" { "operation is in progress" } else { "requires a staged change" }), "{error}");
        assert_eq!(state(dir.path()), before);
    }
}

#[test]
fn preparation_refuses_root_nonancestor_and_merge_topology_without_mutation() {
    let (dir, repo, ids) = repo();
    let before = state(dir.path());
    assert!(InstantRebaseAuthority::prepare(&repo, &ids[0], InstantAutosquashKind::Fixup)
        .unwrap_err().to_string().contains("must have a parent"));
    assert_eq!(state(dir.path()), before);

    git(dir.path(), &["reset", "--hard", "HEAD"]);
    git(dir.path(), &["checkout", "-b", "side", &ids[0]]);
    write(&dir.path().join("side"), "side\n");
    git(dir.path(), &["add", "side"]);
    git(dir.path(), &["commit", "-m", "side"]);
    let side = git(dir.path(), &["rev-parse", "HEAD"]).trim().to_owned();
    git(dir.path(), &["checkout", "main"]);
    write(&dir.path().join("file"), "three\nstaged\n");
    git(dir.path(), &["add", "file"]);
    let before = state(dir.path());
    assert!(InstantRebaseAuthority::prepare(&repo, &side, InstantAutosquashKind::Fixup)
        .unwrap_err().to_string().contains("not an ancestor"));
    assert_eq!(state(dir.path()), before);

    git(dir.path(), &["reset", "--hard", "HEAD"]);
    git(dir.path(), &["merge", "--no-ff", "side", "-m", "merge"]);
    write(&dir.path().join("file"), "three\nmerge staged\n");
    git(dir.path(), &["add", "file"]);
    let before = state(dir.path());
    assert!(InstantRebaseAuthority::prepare(&repo, &ids[1], InstantAutosquashKind::Squash)
        .unwrap_err().to_string().contains("supported linear history"));
    assert_eq!(state(dir.path()), before);
}
