//! The bounded process log and cancellation of a running Git child.

#![cfg(unix)]

mod common;

use common::*;
use magit_lsp::git::GitRepo;
use magit_lsp::runner::{self, CancelHandle, ProcessLog};
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// `PATH` is process-global, so only one fake-git case may run at a time.
static PATH_LOCK: Mutex<()> = Mutex::new(());

fn path_lock() -> std::sync::MutexGuard<'static, ()> {
    PATH_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Put a fake `git` first on `PATH`; it delegates everything to the real Git
/// except `add`, which signals its start and then sleeps until it is killed.
struct FakeGit {
    _dir: TempDir,
    previous_path: Option<OsString>,
}

impl FakeGit {
    fn install(marker: &Path) -> Self {
        let real_git = real_git_path();
        let dir = TempDir::new().expect("fake git tempdir");
        let script = dir.path().join("git");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\n\
                 if [ \"$1\" = \"add\" ]; then\n\
                   printf 'started\\n' > '{}'\n\
                   exec sleep 30\n\
                 fi\n\
                 exec '{}' \"$@\"\n",
                marker.display(),
                real_git.display()
            ),
        )
        .expect("write fake git");
        let mut permissions = std::fs::metadata(&script)
            .expect("fake git metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).expect("chmod fake git");

        let previous_path = std::env::var_os("PATH");
        let mut paths = vec![dir.path().to_path_buf()];
        if let Some(path) = &previous_path {
            paths.extend(std::env::split_paths(path));
        }
        std::env::set_var("PATH", std::env::join_paths(paths).expect("join PATH"));

        Self {
            _dir: dir,
            previous_path,
        }
    }
}

impl Drop for FakeGit {
    fn drop(&mut self) {
        match &self.previous_path {
            Some(path) => std::env::set_var("PATH", path),
            None => std::env::remove_var("PATH"),
        }
    }
}

fn real_git_path() -> PathBuf {
    let output = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .expect("locate git");
    assert!(output.status.success(), "git is not on PATH");
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert!(!path.is_empty(), "git is not on PATH");
    PathBuf::from(path)
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
    let _lock = path_lock();
    let (fixture, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let marker = fixture.path().join("fake-git-started");
    let _fake = FakeGit::install(&marker);
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
    let _lock = path_lock();
    let (fixture, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let marker = fixture.path().join("fake-git-started");
    let _fake = FakeGit::install(&marker);
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
