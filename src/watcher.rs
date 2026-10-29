//! Repository watching that auto-refreshes open views (R-3.5).
//!
//! One [`RepoWatcher`] watches a repository's worktree and `.git` internals.
//! `notify` events are filtered (reads and Git noise such as `objects/` and
//! `*.lock` never count), then debounced: one quiet burst becomes a single
//! [`Refresh`] on the watcher thread. A live `.git/index.lock` defers the
//! refresh with bounded retries; a lock that outlives them reports
//! [`Refresh::IndexLocked`] once instead of failing, and the pending burst
//! still fires when the lock clears.
//!
//! The caller supplies the refresh action; [`crate::server`] routes it through
//! the normal incremental-update path, so renders are edits only when the
//! client has acknowledged the base version.

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::Duration;

/// Watcher timing; production values are defaults and tests shrink them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatcherConfig {
    /// Quiet time after the last relevant event before the refresh fires.
    pub debounce: Duration,
    /// One lock wait, applied [`WatcherConfig::lock_retries`] times.
    pub lock_retry_delay: Duration,
    /// How many times a pending burst waits for `.git/index.lock` to clear.
    pub lock_retries: usize,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(250),
            lock_retry_delay: Duration::from_millis(200),
            lock_retries: 5,
        }
    }
}

/// One debounced burst the watcher hands to its owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refresh {
    /// The repository changed; the paths below drove the burst (deduplicated
    /// and sorted).
    Changed(Vec<PathBuf>),
    /// A relevant burst stayed blocked behind `.git/index.lock` past the
    /// retries; the owner reports this to the user once and refreshes when the
    /// lock clears.
    IndexLocked,
}

/// A running watcher for one repository.
///
/// Dropping it stops the event source; the thread exits on its own and
/// [`RepoWatcher::join`] can wait for it (used on shutdown and in tests).
pub struct RepoWatcher {
    /// Keeps the event source alive: dropping it closes the channel and ends
    /// the thread.
    watcher: Option<RecommendedWatcher>,
    thread: Option<JoinHandle<()>>,
}

impl RepoWatcher {
    /// Watch `repo` and call `on_refresh` once per quiet burst of relevant
    /// events. `on_refresh` runs on the watcher thread; it must not block
    /// indefinitely, and nothing it borrows may live longer than the watcher.
    pub fn start<F>(repo: &Path, config: WatcherConfig, on_refresh: F) -> notify::Result<Self>
    where
        F: FnMut(Refresh) + Send + 'static,
    {
        let repo = repo.to_path_buf();
        let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
        let mut watcher = notify::recommended_watcher(tx)?;
        watcher.watch(&repo, RecursiveMode::Recursive)?;
        let thread = std::thread::Builder::new()
            .name("magit-lsp-watcher".to_string())
            .spawn(move || RepoWatcher::run(repo, config, rx, on_refresh))?;
        Ok(Self {
            watcher: Some(watcher),
            thread: Some(thread),
        })
    }

    /// The debounce-and-retry loop.
    fn run(
        repo: PathBuf,
        config: WatcherConfig,
        rx: mpsc::Receiver<notify::Result<Event>>,
        mut on_refresh: impl FnMut(Refresh),
    ) {
        let git_dir = repo.join(".git");
        let mut pending: Option<Vec<PathBuf>> = None;
        let mut lock_reported = false;
        loop {
            match rx.recv_timeout(config.debounce) {
                Ok(Ok(event)) => {
                    if !relevant_event(&event, &repo) {
                        continue;
                    }
                    let paths: Vec<PathBuf> =
                        event.paths.iter().cloned().collect();
                    match pending.as_mut() {
                        Some(batch) => batch.extend(paths),
                        None => pending = (!paths.is_empty()).then_some(paths),
                    }
                }
                Ok(Err(error)) => {
                    tracing::debug!(%error, "watcher event failed");
                }
                Err(RecvTimeoutError::Timeout) => {
                    let Some(batch) = pending.as_ref() else {
                        continue;
                    };
                    // A concurrent Git mutation (ours or another process) holds
                    // the index lock: retry briefly instead of racing it.
                    if !RepoWatcher::wait_for_index_lock(&git_dir, &config) {
                        if !lock_reported {
                            lock_reported = true;
                            on_refresh(Refresh::IndexLocked);
                        }
                        // Keep the burst; it fires when the lock clears.
                        continue;
                    }
                    lock_reported = false;
                    on_refresh(Refresh::Changed(dedup(batch.clone())));
                    pending = None;
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }

    /// Wait for `.git/index.lock` to clear, bounded by the retry config.
    fn wait_for_index_lock(git_dir: &Path, config: &WatcherConfig) -> bool {
        let lock = git_dir.join("index.lock");
        for _ in 0..config.lock_retries {
            if !lock.exists() {
                return true;
            }
            std::thread::sleep(config.lock_retry_delay);
        }
        !lock.exists()
    }

    /// Whether the repository is currently locked for an index write.
    pub fn index_locked(repo: &Path) -> bool {
        repo.join(".git").join("index.lock").is_file()
    }

    /// Stop the watcher and wait for its thread.
    pub fn join(&mut self) {
        self.watcher.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for RepoWatcher {
    fn drop(&mut self) {
        // Dropping the watcher closes the channel; the thread exits on its own.
        self.watcher.take();
    }
}

/// Deduplicate and sort a burst's paths for stable, testable reports.
fn dedup(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Whether one notify event should make the open views stale (§3.16).
///
/// Access events (reads) are ignored. Everything else counts through the paths
/// of [`relevant_path`].
pub fn relevant_event(event: &Event, repo: &Path) -> bool {
    match event.kind {
        notify::event::EventKind::Access(_) => false,
        _ => event.paths.iter().any(|path| relevant_path(path, repo)),
    }
}

/// Whether one watched path makes the views stale.
///
/// Worktree paths are relevant. Inside `.git`, only state files that change
/// what the views render count (`HEAD`, `index`, `packed-refs`, `refs/`,
/// merge/rebase/cherry-pick state); lock files and `objects/` are noise, and so
/// is every other `.git` internal.
pub fn relevant_path(path: &Path, repo: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(repo) else {
        return false;
    };
    let components: Vec<&OsStr> = rel
        .components()
        .map(|component| component.as_os_str())
        .collect();
    if components.iter().any(|component| *component == OsStr::new(".magit")) {
        return false;
    }
    match components.first() {
        None => false,
        Some(first) if *first == OsStr::new(".git") => relevant_git_path(&components[1..]),
        _ => true,
    }
}

/// Whether a path inside `.git` changes repository state the views render.
fn relevant_git_path(rel: &[&OsStr]) -> bool {
    match rel.first() {
        None => false, // the `.git` directory itself
        Some(first) => {
            let first = first.to_string_lossy();
            if first.ends_with(".lock") || first == "objects" {
                return false;
            }
            matches!(
                first.as_ref(),
                "HEAD"
                    | "index"
                    | "packed-refs"
                    | "MERGE_HEAD"
                    | "MERGE_MSG"
                    | "MERGE_MODE"
                    | "CHERRY_PICK_HEAD"
                    | "REVERT_HEAD"
                    | "BISECT_LOG"
                    | "rebase-merge"
                    | "rebase-apply"
                    | "sequencer"
            ) || (first == "refs"
                && !rel.iter().any(|name| name.to_string_lossy().ends_with(".lock")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, DataChange, ModifyKind};

    fn path(relative: &str) -> PathBuf {
        Path::new("/repo").join(relative)
    }

    #[test]
    fn worktree_changes_are_relevant() {
        assert!(relevant_path(&path("f.txt"), Path::new("/repo")));
        assert!(relevant_path(&path("dir/sub/g.txt"), Path::new("/repo")));
        assert!(!relevant_path(&path(".git"), Path::new("/repo")));
        assert!(!relevant_path(Path::new("/other/f.txt"), Path::new("/repo")));
    }

    #[test]
    fn git_state_files_are_relevant_but_noise_is_not() {
        let relevant = [
            ".git/HEAD",
            ".git/index",
            ".git/packed-refs",
            ".git/refs/heads/main",
            ".git/MERGE_HEAD",
            ".git/rebase-merge/msgnum",
            ".git/sequencer/todo",
        ];
        for rel in relevant {
            assert!(relevant_path(&path(rel), Path::new("/repo")), "{rel}");
        }
        let noise = [
            ".git/config",
            ".git/description",
            ".git/hooks/pre-commit.sample",
            ".git/logs/HEAD",
            ".git/logs/refs/heads/main",
            ".git/objects/ab/cdef1234",
            ".git/index.lock",
            ".git/refs/heads/main.lock",
            ".git/refs/tags/v1.lock",
            ".git/ORIG_HEAD",
            ".git/FETCH_HEAD",
        ];
        for rel in noise {
            assert!(!relevant_path(&path(rel), Path::new("/repo")), "{rel}");
        }
    }

    #[test]
    fn access_events_never_refresh() {
        let event = Event::new(notify::event::EventKind::Access(AccessKind::Read))
            .add_path(path("f.txt"));
        assert!(!relevant_event(&event, Path::new("/repo")));
    }

    #[test]
    fn modify_events_refresh_through_relevant_paths() {
        let event = Event::new(notify::event::EventKind::Modify(ModifyKind::Data(
            DataChange::Any,
        )))
        .add_path(path(".git/objects/00/00"))
        .add_path(path("f.txt"));
        assert!(relevant_event(&event, Path::new("/repo")));
    }
}
