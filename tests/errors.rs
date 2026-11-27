use magit_lsp_test_support as common;

use common::*;
use magit_lsp::config::Profile;
use magit_lsp::errors::map_git_failure;
use magit_lsp::git::GitFailure;
use magit_lsp::git::GitRepo;
use magit_lsp::protocol::Cursor;
use magit_lsp::protocol::{Effect, MessageLevel};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
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
fn a_real_non_fast_forward_push_is_readable_without_a_process_log() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", "a\nb\nc\n")])
        .build();
    let remote = TempDir::new().expect("bare remote");
    git(remote.path(), &["init", "--bare", "-q"]);
    // Match the fixture's branch so a clone checks out `main`.
    git(remote.path(), &["symbolic-ref", "HEAD", "refs/heads/main"]);
    let remote_url = remote.path().to_str().expect("utf8 remote");
    tmp.git(&["remote", "add", "origin", remote_url]);
    tmp.git(&["push", "-q", "-u", "origin", "main"]);

    // Advance the remote from a clone so the next local push is rejected.
    let other = TempDir::new().expect("other clone");
    git(other.path(), &["clone", "-q", remote_url, "."]);
    write(&other.path().join("f.txt"), "a\nREMOTE\nc\n");
    git(other.path(), &["commit", "-q", "-am", "remote advance"]);
    git(other.path(), &["push", "-q", "origin", "main"]);

    write(&tmp.path().join("f.txt"), "a\nLOCAL\nc\n");
    tmp.git(&["commit", "-q", "-am", "local advance"]);
    let failure = git_failure(tmp.path(), &["push", "origin", "main"], None);
    assert_ne!(
        failure.exit_code, 128,
        "a rejected push must exercise Git's own exit code, not the table's 128"
    );

    let effect = Effect::from_git_failure(&failure);
    let Effect::Message { text, hint, .. } = &effect else {
        panic!("expected a message effect, got {effect:?}");
    };
    assert_eq!(
        text, "The remote has commits you do not have. Fetch or pull first.",
        "stderr: {}",
        failure.stderr
    );
    assert!(hint.is_none());
    let wire = serde_json::to_value(&effect).expect("serialise the effect");
    assert!(
        wire.get("processLogId").is_none(),
        "internal process-log identifiers must stay off the wire: {wire}"
    );

    // The mapping follows stderr, not the particular exit code Git used.
    for exit_code in [1, 128, 255] {
        let user = map_git_failure(exit_code, &failure.stderr, "push");
        assert_eq!(user.summary, *text, "exit code {exit_code}");
        assert!(user.hint.is_none());
    }
}

#[test]
fn unmapped_failure_uses_the_fallback_with_stderr_detail() {
    let stderr = "fatal: an unusual Git failure";
    let user = map_git_failure(128, stderr, "add");

    assert_eq!(user.summary, "`git add` failed.");
    assert_eq!(user.hint, None);
    assert_eq!(user.detail.as_deref(), Some(stderr));
    assert_eq!(user.command.as_deref(), Some("git add"));
}

#[test]
fn a_missing_pathspec_has_a_refreshable_summary() {
    let (tmp, _repo) = init_repo();
    let failure = git_failure(tmp.path(), &["add", "--", "missing.txt"], None);
    assert_eq!(
        Effect::from_git_failure(&failure),
        message(
            MessageLevel::Error,
            "The requested file is no longer available to Git.",
            Some("Refresh the status view and try again."),
            None
        )
    );
}

#[test]
fn unborn_head_from_a_real_repository_is_readable() {
    let (tmp, _repo) = RepoFixture::new()
        .unborn()
        .staged_file("f.txt", "hello\n")
        .build();

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
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    let line = test.goto_file("unstaged", "f.txt");
    write(&tmp.path().join(".git/index.lock"), "");

    let outcome = test.invoke_at("status.stage", Cursor { line, column: 0 });

    assert_eq!(
        outcome.effects,
        vec![message(
            MessageLevel::Error,
            "Another Git process is running in this repository.",
            Some(
                "If no Git process is running, remove the stale .git/index.lock file and try again."
            ),
            None
        )]
    );
    let status = GitRepo::open(tmp.path())
        .expect("open repo")
        .status()
        .expect("status");
    assert!(status.staged.is_empty());
}

#[test]
fn not_a_repository_returns_a_message_effect() {
    magit_lsp_test_support::cargo_server_binary!();
    let tmp = TempDir::new().expect("tempdir");
    let mut server = common::stdio::TestServer::spawn(&[]);
    server.initialize(serde_json::json!({}));
    server.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "view/open",
        "params": {
            "kind": "status",
            "args": {"repo_path": tmp.path().to_string_lossy()},
            "viewport": {"cols": 80, "rows": 24}
        }
    }));
    let response = server.response_with_id(2);
    assert_eq!(
        response["error"]["message"], "This folder is not inside a Git repository.",
        "{response}"
    );
    server.shutdown_and_exit();
    let status = server.finish(std::time::Duration::from_secs(10));
    assert!(status.success(), "server exited: {status}");
}

#[test]
fn disappeared_displayed_file_returns_a_readable_refusal() {
    let (tmp, _repo) = init_repo();
    write(&tmp.path().join("f.txt"), "line1\nCHANGED\nline3\n");
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    let line = test.goto_marker("f.txt", '+');
    std::fs::remove_file(tmp.path().join("f.txt")).expect("remove displayed file");

    let outcome = test.invoke_at("status.stage", Cursor { line, column: 0 });

    assert_eq!(outcome.effects.len(), 1);
    let Effect::Message {
        level,
        text,
        detail,
        ..
    } = &outcome.effects[0]
    else {
        panic!("expected a message effect, got {:?}", outcome.effects[0]);
    };
    assert_eq!(*level, MessageLevel::Warn);
    assert_eq!(text, magit_lsp::errors::HUNK_CHANGED);
    assert!(detail.is_none());
    let status = GitRepo::open(tmp.path())
        .expect("open repo")
        .status()
        .expect("status");
    assert!(status.staged.is_empty());
}

#[test]
fn no_op_selection_returns_an_info_effect() {
    let (tmp, _repo) = init_repo();
    write(&tmp.path().join("f.txt"), "line1\nline2 CHANGED\nline3\n");

    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    // Resolve the context line from tree metadata, then dispatch a one-line selection.
    let line = test.goto_text("f.txt", "line1");
    let outcome = test.invoke_selection(
        "status.stage",
        Some(Cursor { line, column: 0 }),
        Some(magit_lsp::protocol::Selection {
            start_line: line,
            end_line: line,
        }),
    );

    assert_eq!(
        outcome.effects,
        vec![message(
            MessageLevel::Info,
            magit_lsp::patch::NO_CHANGED_LINES,
            None,
            None
        )]
    );
    assert!(GitRepo::open(tmp.path())
        .expect("open repo")
        .status()
        .expect("status")
        .staged
        .is_empty());
}

#[test]
fn would_be_overwritten_from_a_real_repository_is_readable() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", "a\nb\nc\n")])
        .build();
    tmp.git(&["checkout", "-q", "-b", "other"]);
    write(&tmp.path().join("f.txt"), "a\nOTHER\nc\n");
    tmp.git(&["commit", "-q", "-a", "-m", "other"]);
    tmp.git(&["checkout", "-q", "main"]);
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
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", "a\nb\nc\n")])
        .build();
    tmp.git(&["checkout", "-q", "-b", "other"]);
    write(&tmp.path().join("f.txt"), "a\nOTHER\nc\n");
    tmp.git(&["commit", "-q", "-a", "-m", "other"]);
    tmp.git(&["checkout", "-q", "main"]);
    write(&tmp.path().join("f.txt"), "a\nMAIN\nc\n");
    tmp.git(&["commit", "-q", "-a", "-m", "main"]);

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
        warning: None,
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

#[cfg(unix)]
#[test]
fn backend_apply_failure_child() {
    let Ok(root) = std::env::var("ERROR_TEST_ROOT") else {
        return;
    };
    let repo = magit_lsp::git::GitRepo::open(root).unwrap();
    let mode = std::env::var("ERROR_TEST_MODE").expect("test mode");
    if mode == "success" {
        repo.stage_file("f.txt").expect("successful staging");
        return;
    }
    let failure = match mode.as_str() {
        "add" => repo.stage_file("f.txt").unwrap_err(),
        "apply" | "early" => {
            let hunks = repo.file_hunks("f.txt", false).unwrap();
            repo.stage_hunk("f.txt", &hunks[0].id).unwrap_err()
        }
        _ => panic!("unknown test mode: {mode}"),
    };
    let failure = failure.git.as_ref().expect("structured failure");
    assert_eq!(failure.exit_code, 17);
    assert_eq!(failure.stderr, "fatal: preserved fake failure");
    let expected_args: &[&str] = if mode == "add" {
        &["add", "--", "f.txt"]
    } else {
        &["apply", "--cached", "--unidiff-zero", "--recount"]
    };
    assert_eq!(failure.args, expected_args);
    if std::env::var("ERROR_TEST_NO_WARNING").is_ok() {
        assert!(failure.warning.is_none());
        return;
    }
    assert!(failure.warning.as_deref().unwrap_or("").contains("2.19.6"));
    let effect = Effect::from_git_failure(failure);
    let Effect::Message {
        hint: Some(hint), ..
    } = effect
    else {
        panic!("missing advisory: {effect:?}")
    };
    assert!(hint.contains("2.30.9"));
}

#[cfg(unix)]
#[test]
fn normal_and_piped_apply_failures_keep_original_details() {
    let initial = (0..20_000)
        .map(|n| format!("line {n}\n"))
        .collect::<String>();

    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", initial.as_str())])
        .build();

    let fake_dir = TempDir::new().unwrap();
    let real = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    let real = String::from_utf8_lossy(&real.stdout).trim().to_string();

    let fake = fake_dir.path().join("git");
    std::fs::write(
        &fake,
        format!(
            "#!/bin/sh
            if [ \"$1\" = -C ]; then cd \"$2\" || exit 1; shift 2; fi
            if [ \"$1\" = --version ]; then
            echo 'git version 2.19.6.windows.1'
            exit 0
            fi
            case \"$1\" in
            add|apply)
        if [ \"$1\" = apply ] && [ \"$EARLY\" != yes ]; then
        cat >/dev/null
        fi
        echo 'fatal: preserved fake failure' >&2
        exit 17
        ;;
        esac
        exec '{real}' \"$@\"
        "
        ),
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

    for (mode, early) in [("add", "no"), ("apply", "no"), ("early", "yes")] {
        let changed = if mode == "add" {
            "modified file\n".to_string()
        } else {
            let mut lines = initial.lines().map(str::to_string).collect::<Vec<_>>();
            if mode == "early" {
                for line in (0..lines.len()).step_by(2) {
                    lines[line] = format!("changed line {line}");
                }
            } else {
                lines[10_000] = "changed middle line".to_string();
            }
            lines.join("\n") + "\n"
        };

        write(&tmp.path().join("f.txt"), &changed);

        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "backend_apply_failure_child"])
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    fake_dir.path().display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .env("ERROR_TEST_ROOT", tmp.path())
            .env("ERROR_TEST_MODE", mode)
            .env("EARLY", early)
            .env("PROBE_MODE", "ok")
            .env("PROBE_VERSION", "git version 2.19.6.windows.1")
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "Child test failed (mode={mode}, early={early}, status={})\n\
                --- stdout ---\n{}\n\
                --- stderr ---\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}

#[test]
fn git_failure_display_shows_exit_status_when_stderr_is_empty() {
    let silent = GitFailure {
        exit_code: 128,
        stderr: String::new(),
        args: vec!["status".to_owned()],
        subcommand: "status".to_owned(),
        warning: None,
    };
    assert_eq!(silent.to_string(), "git status failed (exit code 128)");

    // A signal death carries no exit status; naming the signal keeps the
    // message self-describing instead of a bare negative number.
    let killed = GitFailure {
        exit_code: -15,
        ..silent.clone()
    };
    assert_eq!(
        killed.to_string(),
        "git status failed (killed by signal 15)"
    );

    // Non-empty stderr keeps the user-facing message unchanged.
    let verbose = GitFailure {
        stderr: "fatal: not a git repository".to_owned(),
        ..silent
    };
    assert_eq!(
        verbose.to_string(),
        "git status failed: fatal: not a git repository"
    );
}

#[cfg(unix)]
#[test]
fn a_signal_killed_git_failure_names_the_signal() {
    use std::os::unix::process::ExitStatusExt;

    // Raw 9 is a wait status whose low bits are signal 9, exactly what
    // `ExitStatus` reports for a child the kernel killed.
    let status = std::process::ExitStatus::from_raw(9);
    assert_eq!(status.code(), None, "a killed child has no exit code");
    let failure = GitFailure::from_output(&["status"], status, b"");
    assert_eq!(failure.exit_code, -9);
    assert_eq!(
        failure.to_string(),
        "git status failed (killed by signal 9)"
    );
}

#[cfg(unix)]
#[test]
fn a_signal_killed_git_child_records_the_negated_signal_in_the_process_log() {
    use magit_lsp::runner;

    let (tmp, repo) = init_repo();
    // A hook that SIGKILLs its parent (`git`) makes the child die without an
    // exit status: the case the process log must keep self-describing instead
    // of collapsing the signal death to -1.
    let hook = tmp.path().join(".git/hooks/pre-commit");
    std::fs::create_dir_all(hook.parent().expect("hooks directory")).expect("hooks dir");
    std::fs::write(&hook, "#!/bin/sh\nkill -9 $PPID\n").expect("write hook");
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).expect("chmod hook");
    // Staged work makes `git commit` run the hook rather than abort first.
    std::fs::write(tmp.path().join("signal.txt"), "payload\n").expect("write file");
    tmp.git(&["add", "signal.txt"]);

    let evidence = "signal-killed child logged";
    let error = repo
        .commit(&["-m", evidence], &[])
        .expect_err("SIGKILLed git must fail");
    assert_eq!(error.git.expect("git failure").exit_code, -9);

    let record = runner::global_log()
        .iter()
        .into_iter()
        .rev()
        .find(|record| record.args.contains(&evidence.to_string()))
        .expect("the killed commit is recorded");
    assert_eq!(
        record.exit_code, -9,
        "the process log must keep the negated signal"
    );
}

#[cfg(unix)]
#[test]
fn version_probe_kill_logs_probe_kill_at_debug() {
    // The probe log line goes to the process's tracing subscriber, so the
    // assertion body runs in a child of this test binary: it can install the
    // subscriber and scope a hanging `git --version` to its own PATH.
    const CHILD: &str = "MAGIT_LSP_VERSION_PROBE_LOG_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let real_git = std::env::split_paths(&std::env::var_os("PATH").expect("PATH"))
            .map(|dir| dir.join("git"))
            .find(|path| path.is_file())
            .expect("real git on PATH");
        let fake = TempDir::new().expect("fake git dir");
        let script = fake.path().join("git");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nif [ \"$1\" = -C ]; then cd \"$2\" || exit 1; shift 2; fi\nif [ \"$1\" = \"--version\" ]; then exec sleep 30; fi\nexec '{}' \"$@\"\n",
                real_git.display()
            ),
        )
        .expect("write fake git");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake git");
        let mut paths = vec![fake.path().to_path_buf()];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        let output = Command::new(std::env::current_exe().expect("test binary"))
            .args(["--exact", "version_probe_kill_logs_probe_kill_at_debug"])
            .env(CHILD, "1")
            .env("MAGIT_LSP_LOG", "debug")
            .env("PATH", std::env::join_paths(paths).expect("join PATH"))
            .output()
            .expect("run child");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "child failed: {stderr}{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let probe = stderr
            .lines()
            .find(|line| line.contains("probe kill") && line.contains("subcommand"))
            .unwrap_or_else(|| panic!("no probe kill line: {stderr}"));
        assert!(
            probe.contains("DEBUG"),
            "the probe kill must log at debug: {probe}"
        );
        assert!(
            probe.contains("--version"),
            "the probe kill must name its subcommand: {probe}"
        );
        return;
    }

    magit_lsp::logging::init().expect("logging");
    let (_tmp, repo) = init_repo();
    // Nothing staged: `git commit` fails and triggers the version probe, whose
    // `git --version` child never finishes inside the 500ms deadline.
    let _ = repo.commit(&["-m", "probe"], &[]);
}
