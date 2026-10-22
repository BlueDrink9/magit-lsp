mod common;

use common::*;
use magit_lsp::errors::map_git_failure;
use magit_lsp::git::GitFailure;
use magit_lsp::protocol::{Effect, MessageLevel};
use magit_lsp::server;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn git_failure(dir: &Path, args: &[&str], stdin: Option<&str>) -> GitFailure {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().expect("spawn git");
    if let Some(input) = stdin {
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(input.as_bytes())
            .expect("write stdin");
    }
    let out = child.wait_with_output().expect("wait for git");
    assert!(
        !out.status.success(),
        "git {} unexpectedly succeeded",
        args.join(" ")
    );
    GitFailure::from_output(args, out.status, &out.stderr)
}

fn message(level: MessageLevel, text: &str, hint: Option<&str>, detail: Option<&str>) -> Effect {
    Effect::Message {
        level,
        text: text.to_string(),
        hint: hint.map(str::to_string),
        detail: detail.map(str::to_string),
        process_log_id: None,
    }
}

#[test]
fn maps_every_table_row() {
    let cases: &[(&str, &str, &str, Option<&str>)] = &[
        (
            "fatal: could not resolve HEAD to a revision",
            "reset",
            "This repository has no commits yet.",
            None,
        ),
        (
            "error: corrupt patch at <stdin>:4",
            "apply",
            "Git could not apply that selection. Try selecting whole lines that are changes.",
            None,
        ),
        (
            "fatal: Unable to create '.git/index.lock': File exists.",
            "add",
            "Another Git process is running in this repository.",
            Some(
                "If no Git process is running, remove the stale .git/index.lock file and try again.",
            ),
        ),
        (
            "fatal: not a git repository (or any of the parent directories): .git",
            "rev-parse",
            "This folder is not inside a Git repository.",
            None,
        ),
        (
            "error: Your local changes to the following files would be overwritten by merge:",
            "merge",
            "Your local changes would be overwritten. Commit, stash or discard them first.",
            None,
        ),
        (
            "! [rejected] main -> main (non-fast-forward)",
            "push",
            "The remote has commits you do not have. Fetch or pull first.",
            None,
        ),
        (
            "fatal: Authentication failed for 'https://example.com/repo'",
            "fetch",
            "Git could not authenticate with the remote.",
            None,
        ),
        (
            "fatal: Permission denied (publickey).",
            "push",
            "Git could not authenticate with the remote.",
            None,
        ),
        (
            "CONFLICT (content): Merge conflict in f.txt",
            "merge",
            "The operation stopped because of conflicts.",
            Some("Resolve the conflicts, then continue the operation."),
        ),
    ];

    for (stderr, subcommand, summary, hint) in cases {
        let user = map_git_failure(128, stderr, subcommand);
        assert_eq!(user.summary, *summary, "stderr: {stderr}");
        assert_eq!(user.hint.as_deref(), *hint, "stderr: {stderr}");
        assert_eq!(user.command, Some(format!("git {subcommand}")));
        assert_eq!(user.detail, None, "stderr: {stderr}");
    }
}

#[test]
fn unmapped_failure_uses_the_fallback_with_stderr_detail() {
    let stderr = "fatal: pathspec 'missing' did not match any files";
    let user = map_git_failure(128, stderr, "add");

    assert_eq!(user.summary, "`git add` failed.");
    assert_eq!(user.hint, None);
    assert_eq!(user.detail.as_deref(), Some(stderr));
    assert_eq!(user.command.as_deref(), Some("git add"));
}

#[test]
fn unborn_head_from_a_real_repository_is_readable() {
    let tmp = TempDir::new().expect("tempdir");
    git(tmp.path(), &["init", "-q", "-b", "main"]);
    git(tmp.path(), &["config", "user.email", "t@example.com"]);
    git(tmp.path(), &["config", "user.name", "t"]);
    write(&tmp.path().join("f.txt"), "hello\n");
    git(tmp.path(), &["add", "f.txt"]);

    let failure = git_failure(tmp.path(), &["reset", "HEAD"], None);
    assert_eq!(
        Effect::from_git_failure(&failure),
        message(
            MessageLevel::Error,
            "This repository has no commits yet.",
            None,
            None
        )
    );
}

#[test]
fn corrupt_patch_from_a_real_repository_is_readable() {
    let (tmp, _repo) = init_repo();
    let failure = git_failure(
        tmp.path(),
        &["apply", "--cached", "--unidiff-zero", "-"],
        Some("diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -x,y +1,1 @@\n-a\n+b\n"),
    );

    assert_eq!(
        Effect::from_git_failure(&failure),
        message(
            MessageLevel::Error,
            "Git could not apply that selection. Try selecting whole lines that are changes.",
            None,
            None
        )
    );
}

#[test]
fn index_lock_failure_returns_a_message_effect_and_status() {
    let (tmp, _repo) = init_repo();
    write(&tmp.path().join("f.txt"), "line1\nCHANGED\nline3\n");
    write(&tmp.path().join(".git/index.lock"), "");

    let (status, effects) = server::mutation(tmp.path().to_str().expect("utf8 path"), |repo| {
        repo.stage_file("f.txt").map(|_| true)
    })
    .expect("mutation response");

    assert_eq!(
        effects,
        vec![message(
            MessageLevel::Error,
            "Another Git process is running in this repository.",
            Some(
                "If no Git process is running, remove the stale .git/index.lock file and try again."
            ),
            None
        )]
    );
    assert_eq!(status.repo_path, tmp.path().to_string_lossy());
}

#[test]
fn not_a_repository_returns_a_message_effect() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().to_str().expect("utf8 path");

    let (status, effects) = server::mutation(path, |_| Ok(true)).expect("mutation response");

    assert_eq!(
        effects,
        vec![message(
            MessageLevel::Error,
            "This folder is not inside a Git repository.",
            None,
            None
        )]
    );
    assert_eq!(status.repo_path, path);
    assert!(status.staged.is_empty());
    assert!(status.unstaged.is_empty());
}

#[test]
fn unmapped_git_failure_returns_the_fallback_effect() {
    let (tmp, _repo) = init_repo();

    let (_status, effects) = server::mutation(tmp.path().to_str().expect("utf8 path"), |repo| {
        repo.stage_file("missing.txt").map(|_| true)
    })
    .expect("mutation response");

    assert_eq!(effects.len(), 1);
    let Effect::Message {
        level, text, detail, ..
    } = &effects[0]
    else {
        panic!("expected a message effect, got {:?}", effects[0]);
    };
    assert_eq!(*level, MessageLevel::Error);
    assert_eq!(text, "`git add` failed.");
    assert!(detail
        .as_deref()
        .unwrap_or("")
        .contains("did not match any files"));
}

#[test]
fn no_op_selection_returns_an_info_effect() {
    let (tmp, repo) = init_repo();
    write(&tmp.path().join("f.txt"), "line1\nline2 CHANGED\nline3\n");

    // The first hunk line is context, so nothing is selected.
    let hunk_id = repo.status().expect("status").unstaged[0].hunks[0]
        .id
        .clone();
    let (_status, effects) = server::mutation(tmp.path().to_str().expect("utf8 path"), |repo| {
        repo.stage_lines("f.txt", &hunk_id, 0, 0)
    })
    .expect("mutation response");

    assert_eq!(
        effects,
        vec![message(
            MessageLevel::Info,
            magit_lsp::patch::NO_CHANGED_LINES,
            None,
            None
        )]
    );
    assert!(repo.status().expect("status").staged.is_empty());
}

#[test]
fn would_be_overwritten_from_a_real_repository_is_readable() {
    let tmp = TempDir::new().expect("tempdir");
    git(tmp.path(), &["init", "-q", "-b", "main"]);
    git(tmp.path(), &["config", "user.email", "t@example.com"]);
    git(tmp.path(), &["config", "user.name", "t"]);
    write(&tmp.path().join("f.txt"), "a\nb\nc\n");
    git(tmp.path(), &["add", "."]);
    git(tmp.path(), &["commit", "-q", "-m", "init"]);
    git(tmp.path(), &["checkout", "-q", "-b", "other"]);
    write(&tmp.path().join("f.txt"), "a\nOTHER\nc\n");
    git(tmp.path(), &["commit", "-q", "-a", "-m", "other"]);
    git(tmp.path(), &["checkout", "-q", "main"]);
    write(&tmp.path().join("f.txt"), "a\nLOCAL\nc\n");

    let failure = git_failure(tmp.path(), &["merge", "other"], None);
    assert_eq!(
        Effect::from_git_failure(&failure),
        message(
            MessageLevel::Error,
            "Your local changes would be overwritten. Commit, stash or discard them first.",
            None,
            None
        )
    );
}

#[test]
fn conflicts_from_a_real_repository_are_readable() {
    let tmp = TempDir::new().expect("tempdir");
    git(tmp.path(), &["init", "-q", "-b", "main"]);
    git(tmp.path(), &["config", "user.email", "t@example.com"]);
    git(tmp.path(), &["config", "user.name", "t"]);
    write(&tmp.path().join("f.txt"), "a\nb\nc\n");
    git(tmp.path(), &["add", "."]);
    git(tmp.path(), &["commit", "-q", "-m", "init"]);
    git(tmp.path(), &["checkout", "-q", "-b", "other"]);
    write(&tmp.path().join("f.txt"), "a\nOTHER\nc\n");
    git(tmp.path(), &["commit", "-q", "-a", "-m", "other"]);
    git(tmp.path(), &["checkout", "-q", "main"]);
    write(&tmp.path().join("f.txt"), "a\nMAIN\nc\n");
    git(tmp.path(), &["commit", "-q", "-a", "-m", "main"]);

    // Merge reports conflicts on stdout, so combine both streams.
    let out = Command::new("git")
        .args(["merge", "other"])
        .current_dir(tmp.path())
        .output()
        .expect("run git merge");
    assert!(!out.status.success());
    let stderr = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let failure = GitFailure {
        exit_code: out.status.code().unwrap_or(-1),
        stderr: stderr.trim_end().to_string(),
        args: vec!["merge".to_string(), "other".to_string()],
        subcommand: "merge".to_string(),
    };
    assert_eq!(
        Effect::from_git_failure(&failure),
        message(
            MessageLevel::Error,
            "The operation stopped because of conflicts.",
            Some("Resolve the conflicts, then continue the operation."),
            None
        )
    );
}
