//! Process logging and cancellation for the synchronous Git backend.
//!
//! Git still runs through `std::process` inside `spawn_blocking` (the sync
//! `GitRepo` API is deliberately kept); this module supplies the two pieces an
//! async runner would otherwise own: a bounded record of every finished
//! invocation and a handle that can kill the one child a request is running.

use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::Path;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// How many finished invocations the global log keeps.
pub const DEFAULT_CAPACITY: usize = 256;

/// How many bytes of stdout and stderr one record keeps per field.
pub const DEFAULT_OUTPUT_CAP: usize = 16 * 1024;

/// One finished Git invocation, as kept by the process log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessRecord {
    pub id: u64,
    pub cwd: String,
    pub args: Vec<String>,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub stdout: String,
    pub stderr: String,
}

struct State {
    next_id: u64,
    records: VecDeque<ProcessRecord>,
}

/// A thread-safe, bounded ring buffer of [`ProcessRecord`]s.
///
/// Ids are allocated under the same lock as the insertion, so the ring is
/// always ordered by id and a record can be looked up without a scan by time.
pub struct ProcessLog {
    capacity: usize,
    output_cap: usize,
    state: Mutex<State>,
}

impl ProcessLog {
    /// Create a log that keeps at most `capacity` records and at most
    /// `output_cap` bytes of each output field.
    pub fn new(capacity: usize, output_cap: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            output_cap,
            state: Mutex::new(State {
                next_id: 1,
                records: VecDeque::new(),
            }),
        }
    }

    /// Append one finished invocation and return its id.
    pub fn record(
        &self,
        cwd: &Path,
        args: &[&str],
        exit_code: i32,
        duration: Duration,
        stdout: &[u8],
        stderr: &[u8],
    ) -> u64 {
        let mut state = self.lock();
        let id = state.next_id;
        state.next_id = state.next_id.wrapping_add(1);
        let record = ProcessRecord {
            id,
            cwd: cwd.display().to_string(),
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
            exit_code,
            duration_ms: u64::try_from(duration.as_millis()).unwrap_or(u64::MAX),
            stdout: truncate(stdout, self.output_cap),
            stderr: truncate(stderr, self.output_cap),
        };
        state.records.push_back(record);
        while state.records.len() > self.capacity {
            state.records.pop_front();
        }
        id
    }

    /// One record by id, if the ring still holds it.
    pub fn get(&self, id: u64) -> Option<ProcessRecord> {
        self.lock()
            .records
            .iter()
            .find(|record| record.id == id)
            .cloned()
    }

    /// Every retained record, oldest first.
    pub fn iter(&self) -> Vec<ProcessRecord> {
        self.lock().records.iter().cloned().collect()
    }

    /// How many records the ring currently holds.
    pub fn len(&self) -> usize {
        self.lock().records.len()
    }

    /// Whether the ring holds no records.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drop every record, keeping the id counter monotonic.
    pub fn clear(&self) {
        self.lock().records.clear();
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The process log every Git invocation records into.
pub fn global_log() -> &'static ProcessLog {
    static LOG: OnceLock<ProcessLog> = OnceLock::new();
    LOG.get_or_init(|| ProcessLog::new(DEFAULT_CAPACITY, DEFAULT_OUTPUT_CAP))
}

/// Copy at most `cap` bytes of `bytes`, never splitting a UTF-8 sequence.
fn truncate(bytes: &[u8], cap: usize) -> String {
    let text = String::from_utf8_lossy(bytes);
    if text.len() <= cap {
        return text.into_owned();
    }
    let mut end = cap;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.get(..end).unwrap_or_default().to_string()
}

#[derive(Debug, Default)]
struct CancelState {
    child: Mutex<Option<u32>>,
    cancelled: AtomicBool,
}

/// A handle that can kill the one child the current request is running.
///
/// Clones share the same state, so the server can stash one while the blocking
/// command thread registers the spawned child.
#[derive(Debug, Clone, Default)]
pub struct CancelHandle {
    state: Arc<CancelState>,
}

impl CancelHandle {
    /// Create an uncancelled handle.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark the request cancelled and kill the child if one is running.
    pub fn cancel(&self) {
        self.state.cancelled.store(true, Ordering::SeqCst);
        let pid = *self.lock_child();
        if let Some(pid) = pid {
            kill(pid);
        }
    }

    /// Whether [`Self::cancel`] has been called.
    pub fn is_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::SeqCst)
    }

    /// Register the child that [`Self::cancel`] should kill.
    ///
    /// Cancellation is consumed by the first child it kills: a later Git
    /// invocation in the same request (for example the status read that
    /// explains a failed mutation) runs normally. Returns `false` when the
    /// request was already cancelled; the child has then been killed and the
    /// caller should simply reap it.
    pub fn register_child(&self, child: &mut Child) -> bool {
        let pid = child.id();
        if self.is_cancelled() {
            kill(pid);
            self.state.cancelled.store(false, Ordering::SeqCst);
            return false;
        }
        *self.lock_child() = Some(pid);
        // A cancel may have slipped in before the pid was stored.
        if self.is_cancelled() {
            kill(pid);
            self.state.cancelled.store(false, Ordering::SeqCst);
            return false;
        }
        true
    }

    /// Forget the child once it has been reaped.
    ///
    /// This also ends the cancellation: the request keeps running so it can
    /// report the failure with a fresh status read.
    pub fn clear_child(&self) {
        *self.lock_child() = None;
        self.state.cancelled.store(false, Ordering::SeqCst);
    }

    fn lock_child(&self) -> std::sync::MutexGuard<'_, Option<u32>> {
        self.state
            .child
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Kill `pid` with SIGKILL.
///
/// The standard library has no portable kill-by-pid, and the `libc` crate is
/// not a dependency, so use the platform utility. Every target the server
/// supports ships one.
fn kill(pid: u32) {
    #[cfg(unix)]
    {
        for program in ["kill", "/bin/kill", "/usr/bin/kill"] {
            let status = Command::new(program)
                .args(["-KILL", &pid.to_string()])
                .status();
            if status.is_ok_and(|status| status.success()) {
                return;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .status();
    }
}

thread_local! {
    static CURRENT: RefCell<Option<CancelHandle>> = const { RefCell::new(None) };
}

/// The cancellation handle installed on this thread, if any.
pub fn current_cancel() -> Option<CancelHandle> {
    CURRENT.with(|slot| slot.borrow().clone())
}

/// Run `f` with `handle` as this thread's cancellation target.
pub fn with_cancel<T>(handle: Option<CancelHandle>, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<CancelHandle>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CURRENT.with(|slot| *slot.borrow_mut() = self.0.take());
        }
    }

    let previous = CURRENT.with(|slot| slot.replace(handle));
    let _restore = Restore(previous);
    f()
}

/// A per-request runner: the cancellation handle plus a no-op default.
#[derive(Debug, Clone, Default)]
pub struct Runner {
    cancel: Option<CancelHandle>,
}

impl Runner {
    /// A runner that cannot be cancelled; the default for existing call sites.
    pub fn detached() -> Self {
        Self::default()
    }

    /// A runner whose child can be killed through `cancel`.
    pub fn with_cancel(cancel: CancelHandle) -> Self {
        Self {
            cancel: Some(cancel),
        }
    }

    /// The handle this runner installs, if any.
    pub fn cancel_handle(&self) -> Option<CancelHandle> {
        self.cancel.clone()
    }

    /// Run `f` with this runner's handle installed on the current thread.
    pub fn run<T>(&self, f: impl FnOnce() -> T) -> T {
        with_cancel(self.cancel.clone(), f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_monotonic_and_eviction_drops_the_oldest() {
        let log = ProcessLog::new(2, 64);
        let first = log.record(Path::new("/repo"), &["status"], 0, Duration::ZERO, b"", b"");
        let second = log.record(Path::new("/repo"), &["diff"], 0, Duration::ZERO, b"", b"");
        let third = log.record(Path::new("/repo"), &["apply"], 0, Duration::ZERO, b"", b"");

        assert!(first < second && second < third);
        assert_eq!(log.len(), 2);
        assert!(log.get(first).is_none());
        assert_eq!(log.get(third).map(|record| record.id), Some(third));
        assert_eq!(
            log.iter()
                .iter()
                .map(|record| record.id)
                .collect::<Vec<_>>(),
            vec![second, third]
        );
    }

    #[test]
    fn output_is_truncated_per_field() {
        let log = ProcessLog::new(4, 3);
        let id = log.record(
            Path::new("/repo"),
            &["status"],
            0,
            Duration::from_millis(1),
            b"abcdef",
            b"ghijkl",
        );
        let record = log.get(id).expect("record");
        assert_eq!(record.stdout, "abc");
        assert_eq!(record.stderr, "ghi");
    }
}
