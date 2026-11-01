//! Commit document core checks.
#![cfg(unix)]

use magit_lsp::editor::draft::{Draft, DraftContext, MAX_DRAFT_BYTES};
use magit_lsp::editor::{Reply, COMMIT_MESSAGE_KIND};
use magit_lsp::git::GitRepo;
use magit_lsp::protocol::DraftFinishReason::{Confirm, NormalClose};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const TEMPLATE: &str = "\n# initial template\n# café 🦀\n";

struct Access {
    owner: String,
    run: String,
    uri: String,
    repository: PathBuf,
}

impl Access {
    fn new(draft: &Draft) -> Self {
        Self {
            owner: "connection-one".into(),
            run: draft.run_id().into(),
            uri: draft.uri().into(),
            repository: draft.repository().into(),
        }
    }

    fn context(&self) -> DraftContext<'_> {
        DraftContext {
            owner: &self.owner,
            run_id: &self.run,
            uri: &self.uri,
            repository: &self.repository,
        }
    }
}

struct Fixture {
    _dir: TempDir,
    repo: GitRepo,
    target: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        let repo = GitRepo::open(dir.path()).unwrap();
        let target = repo.commit_message_path().unwrap();
        fs::write(&target, TEMPLATE).unwrap();
        Self {
            _dir: dir,
            repo,
            target,
        }
    }

    fn awaiting(&self) -> (Draft, Access) {
        let draft = Draft::new(&self.repo, "connection-one").unwrap();
        let access = Access::new(&draft);
        (draft, access)
    }

    fn joined(&self) -> (Draft, Access) {
        let (mut draft, access) = self.awaiting();
        let token = draft.helper_token().to_string();
        draft
            .accept_helper(access.context(), &token, COMMIT_MESSAGE_KIND, &self.target)
            .unwrap();
        (draft, access)
    }

    fn opened(&self) -> (Draft, Access) {
        let (mut draft, access) = self.joined();
        draft
            .did_open(access.context(), 1, TEMPLATE.as_bytes())
            .unwrap();
        (draft, access)
    }
}

fn git(path: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git fixture setup failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn snapshots(draft: &Draft) -> (String, i32, String, i32, String, bool) {
    (
        draft.initial().unwrap().into(),
        draft.live().unwrap().version(),
        draft.live().unwrap().text().into(),
        draft.saved().unwrap().version(),
        draft.saved().unwrap().text().into(),
        draft.is_terminal(),
    )
}

#[test]
fn opaque_run_uri_and_helper_are_distinct_and_repository_bound() {
    let first = Fixture::new();
    let second = Fixture::new();
    let (draft, access) = first.awaiting();
    let (other, _) = second.awaiting();
    assert_ne!(draft.run_id(), other.run_id());
    assert_ne!(draft.helper_token(), other.helper_token());
    assert_ne!(draft.run_id(), draft.helper_token());
    assert_eq!(draft.uri(), format!("file://{}", first.target.display()));
    assert!(draft
        .uri()
        .contains(&first.repo.root().to_string_lossy().to_string()));
    assert_eq!(
        draft.repository(),
        fs::canonicalize(first.repo.root()).unwrap()
    );
    assert_ne!(draft.repository(), other.repository());
    assert_eq!(access.run.len(), 64);
}

#[test]
fn initial_live_saved_are_exact_and_save_never_writes_or_resolves() {
    let fixture = Fixture::new();
    let (mut draft, access) = fixture.opened();
    assert_eq!(draft.initial(), Some(TEMPLATE));
    assert_eq!(draft.live().unwrap().text(), TEMPLATE);
    assert_eq!(draft.saved().unwrap().text(), TEMPLATE);
    let text = "  exact café 🦀\r\n\n# comment retained\nno final newline";
    draft
        .did_change(access.context(), 7, text.as_bytes())
        .unwrap();
    assert_eq!(draft.saved().unwrap().version(), 1);
    assert_eq!(draft.save(access.context(), 7).unwrap(), 7);
    draft
        .did_change(access.context(), 20, b"later unsaved text")
        .unwrap();
    assert_eq!(draft.initial(), Some(TEMPLATE));
    assert_eq!(draft.saved().unwrap().text(), text);
    assert_eq!(draft.saved().unwrap().version(), 7);
    assert_eq!(draft.live().unwrap().version(), 20);
    assert!(!draft.is_terminal());
    assert_eq!(fs::read_to_string(&fixture.target).unwrap(), TEMPLATE);
    fs::write(&fixture.target, text).unwrap();
    assert_eq!(
        draft.finish(access.context(), 7, NormalClose).unwrap(),
        Reply::Ok
    );
    assert_eq!(fs::read_to_string(&fixture.target).unwrap(), text);
    assert!(draft.is_terminal());
}

#[test]
fn first_open_requires_exact_initial_version_and_text() {
    let fixture = Fixture::new();
    let (mut draft, access) = fixture.joined();
    let before = snapshots(&draft);
    for version in [-1] {
        assert!(draft
            .did_open(access.context(), version, TEMPLATE.as_bytes())
            .is_err());
    }
    assert!(draft
        .did_open(access.context(), 1, b"different template")
        .is_err());
    assert!(draft
        .did_change(access.context(), 2, b"not yet opened")
        .is_err());
    assert_eq!(snapshots(&draft), before);
    draft
        .did_open(access.context(), 1, TEMPLATE.as_bytes())
        .unwrap();
    assert!(draft
        .did_open(access.context(), 1, TEMPLATE.as_bytes())
        .is_err());
}

#[test]
fn wrong_owner_run_uri_and_repository_reject_every_operation_atomically() {
    let fixture = Fixture::new();
    let other_repo = Fixture::new();
    let (mut draft, access) = fixture.opened();
    let before = snapshots(&draft);
    let token = draft.helper_token().to_string();
    for wrong in [
        DraftContext {
            owner: "wrong",
            ..access.context()
        },
        DraftContext {
            run_id: "wrong",
            ..access.context()
        },
        DraftContext {
            uri: "magit-commit://wrong/message",
            ..access.context()
        },
        DraftContext {
            repository: other_repo.repo.root(),
            ..access.context()
        },
    ] {
        assert!(draft
            .accept_helper(wrong, &token, COMMIT_MESSAGE_KIND, &fixture.target)
            .is_err());
        assert!(draft.did_open(wrong, 1, TEMPLATE.as_bytes()).is_err());
        assert!(draft.did_change(wrong, 2, b"wrong").is_err());
        assert!(draft.save(wrong, 1).is_err());
        assert!(draft.did_close(wrong).is_err());
        assert!(draft.finish(wrong, 1, NormalClose).is_err());
        assert!(draft.abort(wrong).is_err());
        assert_eq!(snapshots(&draft), before);
    }
    draft
        .did_change(access.context(), 2, b"still attached")
        .unwrap();
    assert_eq!(fs::read_to_string(&fixture.target).unwrap(), TEMPLATE);
}

#[test]
fn stale_duplicate_changes_and_unapplied_save_versions_are_atomic() {
    let fixture = Fixture::new();
    let (mut draft, access) = fixture.opened();
    draft
        .did_change(access.context(), 10, b"accepted skipped version")
        .unwrap();
    let before = snapshots(&draft);
    for version in [i32::MIN, -1, 0, 1, 9, 10] {
        assert!(draft
            .did_change(access.context(), version, b"rejected")
            .is_err());
        assert_eq!(snapshots(&draft), before);
    }
    for version in [1, 9, 11, i32::MAX] {
        assert!(draft.save(access.context(), version).is_err());
        assert_eq!(snapshots(&draft), before);
    }
    draft
        .did_change(access.context(), i32::MAX, b"last version")
        .unwrap();
    assert_eq!(draft.save(access.context(), i32::MAX).unwrap(), i32::MAX);
}

#[test]
fn utf8_byte_limit_is_independent_and_rejections_leave_snapshots_unchanged() {
    let fixture = Fixture::new();
    let (mut draft, access) = fixture.opened();
    let at_limit = "🦀".repeat(MAX_DRAFT_BYTES / 4);
    assert_eq!(at_limit.len(), MAX_DRAFT_BYTES);
    draft
        .did_change(access.context(), 2, at_limit.as_bytes())
        .unwrap();
    draft.save(access.context(), 2).unwrap();
    draft
        .did_change(access.context(), 3, b"small live")
        .unwrap();
    assert_eq!(draft.saved().unwrap().text().len(), MAX_DRAFT_BYTES);
    let before = snapshots(&draft);
    for invalid in [
        vec![b'x'; MAX_DRAFT_BYTES + 1],
        vec![0xff],
        vec![0xf0, 0x9f],
    ] {
        assert!(draft.did_change(access.context(), 4, &invalid).is_err());
        assert_eq!(snapshots(&draft), before);
    }
    draft
        .did_change(access.context(), 4, b"valid retry at same version")
        .unwrap();
}

#[test]
fn invalid_initial_utf8_or_size_does_not_consume_helper() {
    let fixture = Fixture::new();
    let (mut draft, access) = fixture.awaiting();
    let token = draft.helper_token().to_string();
    for invalid in [vec![0xff], vec![b'x'; MAX_DRAFT_BYTES + 1]] {
        fs::write(&fixture.target, invalid).unwrap();
        assert!(draft
            .accept_helper(
                access.context(),
                &token,
                COMMIT_MESSAGE_KIND,
                &fixture.target
            )
            .is_err());
        assert!(draft.initial().is_none());
        assert!(!draft.is_terminal());
    }
    let limit = "é".repeat(MAX_DRAFT_BYTES / 2);
    fs::write(&fixture.target, &limit).unwrap();
    draft
        .accept_helper(
            access.context(),
            &token,
            COMMIT_MESSAGE_KIND,
            &fixture.target,
        )
        .unwrap();
    assert_eq!(draft.initial(), Some(limit.as_str()));
    assert_eq!(draft.saved().unwrap().text(), limit);
}

#[test]
fn detach_reopen_preserves_versions_saved_and_helper() {
    let fixture = Fixture::new();
    let (mut draft, access) = fixture.opened();
    draft.did_change(access.context(), 5, b"saved").unwrap();
    draft.save(access.context(), 5).unwrap();
    draft.did_change(access.context(), 8, b"unsaved").unwrap();
    let before = snapshots(&draft);
    draft.did_close(access.context()).unwrap();
    assert!(draft.did_close(access.context()).is_err());
    assert!(draft.did_change(access.context(), 9, b"detached").is_err());
    assert!(draft
        .did_open(access.context(), 1, TEMPLATE.as_bytes())
        .is_err());
    assert!(draft.did_open(access.context(), 8, b"saved").is_err());
    assert!(draft.did_open(access.context(), 1, b"unsaved").is_err());
    assert_eq!(snapshots(&draft), before);
    assert_eq!(fs::read_to_string(&fixture.target).unwrap(), TEMPLATE);
    draft.did_open(access.context(), 8, b"unsaved").unwrap();
    draft
        .did_change(access.context(), 9, b"reattached")
        .unwrap();
    draft.did_close(access.context()).unwrap();
    assert_eq!(
        draft.finish(access.context(), 5, NormalClose).unwrap(),
        Reply::Ok
    );
    assert_eq!(fs::read_to_string(&fixture.target).unwrap(), TEMPLATE);
}

#[test]
fn confirm_requires_exact_current_version_and_writes_without_a_checkpoint() {
    let fixture = Fixture::new();
    let (mut draft, access) = fixture.opened();
    draft.did_change(access.context(), 2, b"first").unwrap();
    assert!(draft.finish(access.context(), 1, Confirm).is_err());
    draft.save(access.context(), 2).unwrap();
    draft.did_change(access.context(), 3, b"newer").unwrap();
    assert!(draft.finish(access.context(), 2, Confirm).is_err());
    assert!(!draft.is_terminal());
    assert_eq!(fs::read_to_string(&fixture.target).unwrap(), TEMPLATE);
    assert_eq!(
        draft.finish(access.context(), 3, Confirm).unwrap(),
        Reply::Ok
    );
    assert_eq!(fs::read(&fixture.target).unwrap(), b"newer");
}

#[test]
fn normal_close_without_save_writes_initial_not_unsaved_text() {
    let fixture = Fixture::new();
    let (mut draft, access) = fixture.opened();
    draft
        .did_change(access.context(), 2, b"never submitted")
        .unwrap();
    draft.did_close(access.context()).unwrap();
    assert_eq!(
        draft.finish(access.context(), 1, NormalClose).unwrap(),
        Reply::Ok
    );
    assert_eq!(fs::read_to_string(&fixture.target).unwrap(), TEMPLATE);
}

#[test]
fn saved_close_reads_native_safe_save_and_retains_helper_on_unsafe_read() {
    let fixture = Fixture::new();
    let (mut draft, access) = fixture.opened();
    draft
        .did_change(access.context(), 2, b"unsaved must never land")
        .unwrap();
    let replacement = fixture.target.with_file_name("native-save");
    fs::write(&replacement, "Saved native bytes\r\n# exact comment\n").unwrap();
    fs::rename(&replacement, &fixture.target).unwrap();
    let neighbor = fixture.target.with_file_name("neighbor");
    fs::rename(&fixture.target, &neighbor).unwrap();
    symlink(&neighbor, &fixture.target).unwrap();
    assert!(draft.finish(access.context(), 2, NormalClose).is_err());
    assert!(!draft.is_terminal());
    fs::remove_file(&fixture.target).unwrap();
    fs::rename(&neighbor, &fixture.target).unwrap();
    assert_eq!(
        draft.finish(access.context(), 2, NormalClose).unwrap(),
        Reply::Ok
    );
    assert_eq!(
        draft.saved().unwrap().text(),
        "Saved native bytes\r\n# exact comment\n"
    );
    assert_eq!(
        fs::read_to_string(&fixture.target).unwrap(),
        draft.saved().unwrap().text()
    );
    assert!(draft.finish(access.context(), 2, NormalClose).is_err());
}

#[test]
fn empty_and_comment_only_text_are_git_owned_not_abort() {
    for text in ["", "# comment only\n", "\n\n"] {
        let fixture = Fixture::new();
        let (mut draft, access) = fixture.opened();
        draft
            .did_change(access.context(), 2, text.as_bytes())
            .unwrap();
        draft.save(access.context(), 2).unwrap();
        assert_eq!(
            draft.finish(access.context(), 2, Confirm).unwrap(),
            Reply::Ok
        );
        assert_eq!(fs::read_to_string(&fixture.target).unwrap(), text);
    }
}

#[test]
fn terminal_operations_reject_without_touching_text_or_neighbor() {
    for abort in [false, true] {
        let fixture = Fixture::new();
        let (mut draft, access) = fixture.opened();
        let token = draft.helper_token().to_string();
        draft.did_change(access.context(), 2, b"submitted").unwrap();
        draft.save(access.context(), 2).unwrap();
        if abort {
            assert_eq!(draft.abort(access.context()).unwrap(), Reply::Abort);
        } else {
            draft.finish(access.context(), 2, Confirm).unwrap();
        }
        let before = snapshots(&draft);
        let file_before = fs::read(&fixture.target).unwrap();
        assert!(draft.did_open(access.context(), 2, b"submitted").is_err());
        assert!(draft.did_change(access.context(), 3, b"late").is_err());
        assert!(draft.did_close(access.context()).is_err());
        assert!(draft.save(access.context(), 2).is_err());
        assert!(draft.finish(access.context(), 2, Confirm).is_err());
        assert!(draft.abort(access.context()).is_err());
        assert!(draft
            .accept_helper(
                access.context(),
                &token,
                COMMIT_MESSAGE_KIND,
                &fixture.target
            )
            .is_err());
        assert_eq!(snapshots(&draft), before);
        assert_eq!(fs::read(&fixture.target).unwrap(), file_before);
        if abort {
            assert_eq!(file_before, TEMPLATE.as_bytes());
        }
    }
}

#[test]
fn abort_before_helper_join_rejects_late_helper() {
    let fixture = Fixture::new();
    let (mut draft, access) = fixture.awaiting();
    let token = draft.helper_token().to_string();
    assert_eq!(draft.abort(access.context()).unwrap(), Reply::Abort);
    assert!(draft
        .accept_helper(
            access.context(),
            &token,
            COMMIT_MESSAGE_KIND,
            &fixture.target
        )
        .is_err());
    assert!(draft.initial().is_none());
    assert_eq!(fs::read_to_string(&fixture.target).unwrap(), TEMPLATE);
}

#[test]
fn helper_rejects_wrong_kind_token_path_and_duplicate_join() {
    let fixture = Fixture::new();
    let other_repo = Fixture::new();
    let neighbor = fixture.target.with_file_name("MERGE_MSG");
    fs::write(&neighbor, "neighbor").unwrap();
    let (mut draft, access) = fixture.awaiting();
    let token = draft.helper_token().to_string();
    for kind in ["merge-message", "tag-message", "rebase-todo", "other", ""] {
        assert!(draft
            .accept_helper(access.context(), &token, kind, &fixture.target)
            .is_err());
    }
    assert!(draft
        .accept_helper(
            access.context(),
            "wrong-token",
            COMMIT_MESSAGE_KIND,
            &fixture.target
        )
        .is_err());
    for wrong in [&neighbor, &other_repo.target, Path::new("COMMIT_EDITMSG")] {
        assert!(draft
            .accept_helper(access.context(), &token, COMMIT_MESSAGE_KIND, wrong)
            .is_err());
    }
    assert!(draft.initial().is_none());
    draft
        .accept_helper(
            access.context(),
            &token,
            COMMIT_MESSAGE_KIND,
            &fixture.target,
        )
        .unwrap();
    assert!(draft
        .accept_helper(
            access.context(),
            &token,
            COMMIT_MESSAGE_KIND,
            &fixture.target
        )
        .is_err());
    assert_eq!(fs::read_to_string(neighbor).unwrap(), "neighbor");
}

#[test]
fn symlinks_hardlinks_directories_and_fifo_are_not_helper_targets() {
    let fixture = Fixture::new();
    let (mut draft, access) = fixture.awaiting();
    let token = draft.helper_token().to_string();
    let neighbor = fixture.target.with_file_name("neighbor");
    fs::write(&neighbor, "protected").unwrap();
    fs::remove_file(&fixture.target).unwrap();
    symlink(&neighbor, &fixture.target).unwrap();
    assert!(draft
        .accept_helper(
            access.context(),
            &token,
            COMMIT_MESSAGE_KIND,
            &fixture.target
        )
        .is_err());
    fs::remove_file(&fixture.target).unwrap();
    fs::hard_link(&neighbor, &fixture.target).unwrap();
    assert!(draft
        .accept_helper(
            access.context(),
            &token,
            COMMIT_MESSAGE_KIND,
            &fixture.target
        )
        .is_err());
    fs::remove_file(&fixture.target).unwrap();
    fs::create_dir(&fixture.target).unwrap();
    assert!(draft
        .accept_helper(
            access.context(),
            &token,
            COMMIT_MESSAGE_KIND,
            &fixture.target
        )
        .is_err());
    fs::remove_dir(&fixture.target).unwrap();
    let c_path = std::ffi::CString::new(fixture.target.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
    assert!(draft
        .accept_helper(
            access.context(),
            &token,
            COMMIT_MESSAGE_KIND,
            &fixture.target
        )
        .is_err());
    fs::remove_file(&fixture.target).unwrap();
    fs::write(&fixture.target, TEMPLATE).unwrap();
    draft
        .accept_helper(
            access.context(),
            &token,
            COMMIT_MESSAGE_KIND,
            &fixture.target,
        )
        .unwrap();
    assert_eq!(fs::read_to_string(neighbor).unwrap(), "protected");
}

#[test]
fn failed_write_keeps_pending_checkpoint_and_recovers_without_neighbor_mutation() {
    let fixture = Fixture::new();
    let (mut draft, access) = fixture.opened();
    draft
        .did_change(access.context(), 4, b"exact saved bytes\r\n")
        .unwrap();
    draft.save(access.context(), 4).unwrap();
    let before = snapshots(&draft);
    let retained = fixture.target.with_file_name("retained-original");
    let neighbor = fixture.target.with_file_name("neighbor");
    fs::write(&neighbor, "protected").unwrap();
    fs::rename(&fixture.target, &retained).unwrap();
    assert!(draft.finish(access.context(), 4, Confirm).is_err());
    assert!(!fixture.target.exists());
    symlink(&neighbor, &fixture.target).unwrap();
    assert!(draft.finish(access.context(), 4, Confirm).is_err());
    fs::remove_file(&fixture.target).unwrap();
    assert_eq!(snapshots(&draft), before);
    assert_eq!(fs::read_to_string(&neighbor).unwrap(), "protected");
    // Native safe-save replaces the inode in the pinned, owner-checked directory.
    fs::write(&fixture.target, "replacement inode").unwrap();
    assert_eq!(
        draft.finish(access.context(), 4, Confirm).unwrap(),
        Reply::Ok
    );
    assert_eq!(fs::read(&fixture.target).unwrap(), b"exact saved bytes\r\n");
    assert_eq!(fs::read_to_string(neighbor).unwrap(), "protected");
}

#[test]
fn hardlink_added_after_validation_cannot_mutate_neighbor() {
    let fixture = Fixture::new();
    let (mut draft, access) = fixture.opened();
    draft
        .did_change(access.context(), 2, b"never written")
        .unwrap();
    draft.save(access.context(), 2).unwrap();
    let neighbor = fixture.target.with_file_name("neighbor");
    fs::hard_link(&fixture.target, &neighbor).unwrap();
    assert!(draft.finish(access.context(), 2, Confirm).is_err());
    assert!(!draft.is_terminal());
    assert_eq!(fs::read_to_string(&neighbor).unwrap(), TEMPLATE);
    fs::remove_file(neighbor).unwrap();
    draft.finish(access.context(), 2, Confirm).unwrap();
}

#[test]
fn replaced_git_directory_is_rejected_even_if_target_inode_is_preserved() {
    let fixture = Fixture::new();
    let (mut draft, access) = fixture.opened();
    draft
        .did_change(access.context(), 2, b"never written")
        .unwrap();
    draft.save(access.context(), 2).unwrap();
    let directory = fixture.target.parent().unwrap();
    let retained = fixture.repo.root().join("original-gitdir");
    fs::rename(directory, &retained).unwrap();
    symlink(&retained, directory).unwrap();
    assert!(draft.finish(access.context(), 2, Confirm).is_err());
    assert!(!draft.is_terminal());
    assert_eq!(fs::read_to_string(&fixture.target).unwrap(), TEMPLATE);
    fs::remove_file(directory).unwrap();
    fs::rename(retained, directory).unwrap();
    draft.finish(access.context(), 2, Confirm).unwrap();
}

#[test]
fn linked_worktree_uses_actual_gitdir_not_common_or_dotgit_file() {
    let main = Fixture::new();
    git(
        main.repo.root(),
        &[
            "-c",
            "user.name=Draft Test",
            "-c",
            "user.email=draft@example.invalid",
            "commit",
            "--allow-empty",
            "-qm",
            "initial",
        ],
    );
    fs::write(&main.target, TEMPLATE).unwrap();
    let linked_dir = tempfile::tempdir().unwrap();
    let linked = linked_dir.path().join("linked");
    git(
        main.repo.root(),
        &["worktree", "add", "--detach", linked.to_str().unwrap()],
    );
    let repo = GitRepo::open(&linked).unwrap();
    let target = repo.commit_message_path().unwrap();
    assert!(linked.join(".git").is_file());
    assert_ne!(target, main.target);
    assert!(target.starts_with(main.repo.root().join(".git/worktrees")));
    fs::write(&target, TEMPLATE).unwrap();
    let mut draft = Draft::new(&repo, "connection-one").unwrap();
    let access = Access::new(&draft);
    let token = draft.helper_token().to_string();
    for wrong in [
        &main.target,
        &linked.join(".git"),
        &linked.join("COMMIT_EDITMSG"),
    ] {
        assert!(draft
            .accept_helper(access.context(), &token, COMMIT_MESSAGE_KIND, wrong)
            .is_err());
    }
    draft
        .accept_helper(access.context(), &token, COMMIT_MESSAGE_KIND, &target)
        .unwrap();
    draft
        .did_open(access.context(), 1, TEMPLATE.as_bytes())
        .unwrap();
    draft
        .did_change(access.context(), 3, b"linked only")
        .unwrap();
    draft.save(access.context(), 3).unwrap();
    draft.finish(access.context(), 3, Confirm).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"linked only");
    assert_eq!(fs::read_to_string(main.target).unwrap(), TEMPLATE);
    assert!(linked.join(".git").is_file());
}
