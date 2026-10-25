//! Integration tests for the repository file watcher (R-3.5).
//!
//! Deterministic by construction: every wait polls with a bounded timeout and
//! small debounce values, never with sleeps.

mod common;

use magit_lsp::watcher::{Refresh, RepoWatcher, WatcherConfig};
use std::sync::mpsc;
use std::time::Duration;
use common::{init_repo, write};

/// Test timing: a burst is debounced after 20 ms, and a lock is retried three
/// times 10 ms apart, so the whole test stays well inside a bounded wait.
fn test_config() -> WatcherConfig {
    WatcherConfig {
        debounce: Duration::from_millis(20),
        lock_retry_delay: Duration::from_millis(10),
        lock_retries: 3,
    }
}

/// How long any bounded poll may run before the test fails.
const WAIT: Duration = Duration::from_secs(5);

/// Wait for the next `Refresh` matching `predicate`, failing after `WAIT`.
fn wait_for(
    rx: &mpsc::Receiver<Refresh>,
    description: &str,
    predicate: impl Fn(&Refresh) -> bool,
) -> Refresh {
    let deadline = std::time::Instant::now() + WAIT;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        assert!(
            !remaining.is_zero(),
            "timed out waiting for {description}"
        );
        match rx.recv_timeout(remaining.min(Duration::from_millis(50))) {
            Ok(refresh) if predicate(&refresh) => return refresh,
            Ok(Refresh::IndexLocked) => continue, // not the event under test
            Ok(Refresh::Changed(_)) => continue,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(error) => panic!("watcher channel broke while waiting for {description}: {error}"),
        }
    }
}

/// Assert no `Refresh` arrives within `quiet`, then drain and return whether
/// anything did.
fn nothing_within(rx: &mpsc::Receiver<Refresh>, quiet: Duration) -> bool {
    match rx.recv_timeout(quiet) {
        Ok(_) => false,
        Err(mpsc::RecvTimeoutError::Timeout) => true,
        Err(_) => false,
    }
}

#[test]
fn modifying_a_tracked_file_outside_the_editor_refreshes() {
    let (fixture, _repo) = init_repo();
    let repo = fixture.path().to_path_buf();
    let (tx, rx) = mpsc::channel();
    let mut watcher = RepoWatcher::start(&repo, test_config(), move |refresh| {
        tx.send(refresh).expect("send refresh");
    })
    .expect("start watcher");

    // The change happens outside the editor, like another process writing.
    write(&repo.join("f.txt"), "line1\nCHANGED\nline3\n");

    let refresh = wait_for(&rx, "the modified file", |refresh| match refresh {
        Refresh::Changed(paths) => paths
            .iter()
            .any(|path| path.file_name().is_some_and(|name| name == "f.txt")),
        Refresh::IndexLocked => false,
    });
    let Refresh::Changed(paths) = refresh else {
        unreachable!("wait_for returned only Changed");
    };
    assert!(paths.iter().any(|path| path.ends_with("f.txt")));

    watcher.join();
}

#[test]
fn debounce_collapses_a_burst_into_one_refresh() {
    let (fixture, _repo) = init_repo();
    let repo = fixture.path().to_path_buf();
    let (tx, rx) = mpsc::channel();
    let mut watcher = RepoWatcher::start(&repo, test_config(), move |refresh| {
        tx.send(refresh).expect("send refresh");
    })
    .expect("start watcher");

    // Several writes inside one debounce window become a single burst.
    for line in ["a", "b", "c", "d"] {
        write(&repo.join("f.txt"), &format!("{line}\n"));
    }

    wait_for(&rx, "the burst's refresh", |refresh| {
        matches!(refresh, Refresh::Changed(_))
    });
    assert!(
        nothing_within(&rx, Duration::from_millis(100)),
        "one burst produced more than one refresh"
    );

    watcher.join();
}

#[test]
fn git_internals_do_not_trigger_a_refresh() {
    let (fixture, _repo) = init_repo();
    let repo = fixture.path().to_path_buf();
    let (tx, rx) = mpsc::channel();
    let mut watcher = RepoWatcher::start(&repo, test_config(), move |refresh| {
        tx.send(refresh).expect("send refresh");
    })
    .expect("start watcher");

    // Noise: `.git` internals that never change what a view renders.
    std::fs::create_dir_all(repo.join(".git/objects/ab")).expect("create objects dir");
    write(&repo.join(".git/objects/ab/cd"), "blob");
    write(&repo.join(".git/config"), "[core]\n\trepositoryformatversion = 0\n");
    write(&repo.join(".git/refs/heads/main.lock"), "");

    match rx.recv_timeout(Duration::from_millis(150)) {
        Err(mpsc::RecvTimeoutError::Timeout) => {}
        Ok(other) => panic!("git internals triggered a refresh: repo={repo:?} other={other:?}"),
        Err(error) => panic!("{error}"),
    }

    watcher.join();
}

#[test]
fn index_lock_defers_the_refresh_until_it_clears() {
    let (fixture, _repo) = init_repo();
    let repo = fixture.path().to_path_buf();
    let (tx, rx) = mpsc::channel();
    let mut watcher = RepoWatcher::start(&repo, test_config(), move |refresh| {
        tx.send(refresh).expect("send refresh");
    })
    .expect("start watcher");

    // A concurrent Git mutation holds the lock.
    write(&repo.join(".git/index.lock"), "");
    write(&repo.join("f.txt"), "line1\nline2 CHANGED\nline3\n");

    // The retries cannot outlive the lock, so the watcher reports it once
    // instead of racing the mutation or failing.
    let first = wait_for(&rx, "the lock report", |refresh| {
        matches!(refresh, Refresh::IndexLocked)
    });
    assert_eq!(first, Refresh::IndexLocked);
    assert!(
        nothing_within(&rx, Duration::from_millis(80)),
        "the lock was reported more than once"
    );

    // Git releases the lock; the deferred burst now fires.
    std::fs::remove_file(repo.join(".git/index.lock")).expect("remove lock");
    let refresh = wait_for(&rx, "the deferred refresh", |refresh| {
        matches!(refresh, Refresh::Changed(_))
    });
    let Refresh::Changed(paths) = refresh else {
        unreachable!("wait_for returned only Changed");
    };
    assert!(paths.iter().any(|path| path.ends_with("f.txt")));

    watcher.join();
}

#[test]
fn stopping_the_watcher_ends_its_thread() {
    let (fixture, _repo) = init_repo();
    let repo = fixture.path().to_path_buf();
    let (tx, rx) = mpsc::channel();
    let mut watcher = RepoWatcher::start(&repo, test_config(), move |refresh| {
        tx.send(refresh).expect("send refresh");
    })
    .expect("start watcher");

    watcher.join();
    // After a stop the channel is closed; further changes must not arrive.
    assert!(matches!(
        rx.try_recv(),
        Err(mpsc::TryRecvError::Disconnected)
    ));
}
