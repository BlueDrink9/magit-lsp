//! The bounded process log and cancellation of a running Git child.

#![cfg(unix)]

mod common;

use common::fake_git::{FakeGit, CHILD_ENV, MARKER_ENV};
use common::*;
use magit_lsp::git::GitRepo;
use magit_lsp::runner::{self, CancelHandle, ProcessLog};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Run `test` alone in a child process whose `PATH` starts with a fake `git`.
///
/// The environment lives on the child's `Command`, so the multithreaded test
/// binary never mutates its own `PATH`.
fn run_with_fake_git(test: &str) {
    let fake = FakeGit::new();
    let markers = TempDir::new().expect("marker tempdir");
    let marker = markers.path().join("started");
    let status = Command::new(std::env::current_exe().expect("current test binary"))
        .args(["--exact", test])
        .env(CHILD_ENV, "1")
        .env(MARKER_ENV, &marker)
        .env("PATH", &fake.path)
        .output()
        .expect("spawn the fake-git child");
    assert!(
        status.status.success(),
        "the fake-git child for {test} failed: {}{}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
}

/// Whether this process was re-executed by [`run_with_fake_git`].
fn in_fake_git_child() -> bool {
    std::env::var_os(CHILD_ENV).is_some()
}

/// The marker path the fake `git` writes when it starts.
fn marker_path() -> PathBuf {
    PathBuf::from(std::env::var_os(MARKER_ENV).expect("the marker env is set by the parent"))
}

fn wait_for_start(marker: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !marker.exists() {
        assert!(
            Instant::now() < deadline,
            "the fake git never signalled its start"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_record_captures_every_field() {
    let log = ProcessLog::new(4, 1024);
    let id = log.record(
        Path::new("/repo"),
        &["diff", "--cached"],
        7,
        Duration::from_millis(42),
        b"out\n",
        b"err\n",
    );

    let record = log.get(id).expect("the record is retained");
    assert_eq!(record.id, id);
    assert_eq!(record.cwd, "/repo");
    assert_eq!(
        record.args,
        vec!["diff".to_string(), "--cached".to_string()]
    );
    assert_eq!(record.exit_code, 7);
    assert_eq!(record.duration_ms, 42);
    assert_eq!(record.stdout, "out\n");
    assert_eq!(record.stderr, "err\n");
}

#[test]
fn oversized_output_is_truncated_per_field() {
    let log = ProcessLog::new(4, 4);
    let id = log.record(
        Path::new("/repo"),
        &["status"],
        0,
        Duration::ZERO,
        b"0123456789",
        b"abcdefghij",
    );

    let record = log.get(id).expect("the record is retained");
    assert_eq!(record.stdout, "0123");
    assert_eq!(record.stderr, "abcd");
}

#[test]
fn capacity_evicts_the_oldest_and_ids_strictly_increase() {
    let log = ProcessLog::new(2, 8);
    let ids: Vec<u64> = (0..4)
        .map(|_| log.record(Path::new("/repo"), &["status"], 0, Duration::ZERO, b"", b""))
        .collect();

    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(log.len(), 2);
    assert!(
        log.get(ids[0]).is_none(),
        "the oldest record must be evicted"
    );
    assert!(log.get(ids[1]).is_none());
    assert_eq!(
        log.iter()
            .iter()
            .map(|record| record.id)
            .collect::<Vec<_>>(),
        vec![ids[2], ids[3]]
    );
}

#[test]
fn cancelling_a_slow_child_returns_early_with_a_failed_record() {
    if !in_fake_git_child() {
        run_with_fake_git("cancelling_a_slow_child_returns_early_with_a_failed_record");
        return;
    }
    let (fixture, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let marker = marker_path();
    let root = fixture.path().to_path_buf();

    let handle = CancelHandle::new();
    let worker_handle = handle.clone();
    let worker_root = root.clone();
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let repo = GitRepo::open(&worker_root).expect("open repo");
        let result = runner::with_cancel(Some(worker_handle), || repo.stage_all());
        let _ = tx.send(result);
    });

    wait_for_start(&marker);
    let cancelled_at = Instant::now();
    handle.cancel();

    let result = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("a cancelled Git child must return promptly");
    assert!(
        cancelled_at.elapsed() < Duration::from_secs(5),
        "cancellation took {:?}",
        cancelled_at.elapsed()
    );
    assert!(result.is_err(), "a killed stage must fail: {result:?}");
    worker.join().expect("worker thread");

    let cwd = root.display().to_string();
    let record = runner::global_log()
        .iter()
        .into_iter()
        .rev()
        .find(|record| {
            record.cwd == cwd && record.args == vec!["add".to_string(), "-u".to_string()]
        })
        .expect("the killed invocation is recorded");
    assert_ne!(record.exit_code, 0, "a killed child has no success code");
    assert!(record.duration_ms < 30_000);
}

#[test]
fn a_read_completes_while_a_slow_mutation_runs() {
    if !in_fake_git_child() {
        run_with_fake_git("a_read_completes_while_a_slow_mutation_runs");
        return;
    }
    let (fixture, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let marker = marker_path();
    let root = fixture.path().to_path_buf();

    let handle = CancelHandle::new();
    let worker_handle = handle.clone();
    let worker_root = root.clone();
    let worker = thread::spawn(move || {
        let repo = GitRepo::open(&worker_root).expect("open repo");
        runner::with_cancel(Some(worker_handle), || repo.stage_all())
    });

    wait_for_start(&marker);

    let reader_root = root.clone();
    let (tx, rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        let repo = GitRepo::open(&reader_root).expect("open repo for read");
        let _ = tx.send(repo.status());
    });
    let status = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("a read must not wait for the slow mutation")
        .expect("status");
    assert_eq!(status.unstaged.len(), 1);
    reader.join().expect("reader thread");

    // The log is readable while the child still runs: recording never holds a
    // lock across the process lifetime.
    let cwd = root.display().to_string();
    assert!(runner::global_log()
        .iter()
        .iter()
        .any(|record| record.cwd == cwd));

    handle.cancel();
    let result = worker.join().expect("worker thread");
    assert!(result.is_err(), "the cancelled stage must fail");
}

#[test]
fn cancellation_after_exit_does_not_cancel_the_next_command() {
    let markers = TempDir::new().expect("marker tempdir");
    let marker = markers.path().join("ready");
    let handle = CancelHandle::new();
    let canceller_handle = handle.clone();
    let canceller_marker = marker.clone();
    let canceller = thread::spawn(move || {
        wait_for_start(&canceller_marker);
        thread::sleep(Duration::from_millis(50));
        canceller_handle.cancel();
    });

    runner::with_cancel(Some(handle), || {
        let first = runner::run_owned(
            Command::new("sh")
                .arg("-c")
                .arg(format!("touch {}; exit 0", marker.display())),
            None,
        )
        .expect("first command");
        assert!(first.status.expect("exit status").success());
        assert!(!first.outcome.cancelled);

        let second = runner::run_owned(Command::new("sh").arg("-c").arg("exit 0"), None)
            .expect("second command");
        assert!(second.status.expect("exit status").success());
        assert!(!second.outcome.cancelled);
    });
    canceller.join().expect("canceller thread");
}
