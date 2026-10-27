//! Process logging and cancellation for the synchronous Git backend.
//!
//! Git still runs through `std::process` inside `spawn_blocking` (the sync
//! `GitRepo` API is deliberately kept); this module supplies the two pieces an
//! async runner would otherwise own: a bounded record of every finished
//! invocation and a handle that can kill the one child a request is running.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod owned;
pub use owned::{run_owned, OwnedOutcome, OwnedOutput};

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

/// Environment variable naming the client trace file (ui-test-plan §A.4).
pub const TRACE_ENV: &str = "MAGIT_LSP_TRACE";

thread_local! {
    /// The client key trace this thread's Git calls belong to; `0` when none.
    static TRACE_ID: Cell<u64> = const { Cell::new(0) };
}

/// The server's trace file, next to the client's, or `None` when untraced.
///
/// The two processes append to separate files: each has its own monotonic
/// clock, so merging happens in `scripts/ui` by the wall-clock field instead.
fn trace_file() -> Option<&'static Path> {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    PATH.get_or_init(|| {
        std::env::var_os(TRACE_ENV)
            .filter(|value| !value.is_empty())
            .map(|value| PathBuf::from(format!("{}.server", PathBuf::from(value).display())))
    })
    .as_deref()
}

fn trace_start() -> &'static Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now)
}

/// Append one server trace event for key trace `id` (§A.4).
///
/// A missing trace file or an unwritable one is ignored: tracing must never
/// change what the server does.
pub fn trace(id: u64, event: &str, detail: Value) {
    let Some(path) = trace_file() else {
        return;
    };
    let t_ms = trace_start().elapsed().as_secs_f64() * 1000.0;
    let t_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs_f64() * 1000.0)
        .unwrap_or(0.0);
    let line = serde_json::json!({
        "t_ms": t_ms,
        "t_unix_ms": t_unix_ms,
        "id": id,
        "event": event,
        "detail": detail,
        "source": "server",
    });
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{line}");
    }
}

/// Run `f` with `id` as the trace id its Git calls report.
pub fn with_trace_id<T>(id: u64, f: impl FnOnce() -> T) -> T {
    struct Restore(u64);
    impl Drop for Restore {
        fn drop(&mut self) {
            TRACE_ID.with(|slot| slot.set(self.0));
        }
    }

    let previous = TRACE_ID.with(|slot| slot.replace(id));
    let _restore = Restore(previous);
    f()
}

/// The trace id installed on this thread, or `0`.
pub fn current_trace_id() -> u64 {
    TRACE_ID.with(Cell::get)
}

/// Trace an event on the current thread, when a key trace is being followed.
pub fn trace_current(event: &str, detail: Value) {
    let id = current_trace_id();
    if id > 0 {
        trace(id, event, detail);
    }
}

#[derive(Debug, Default)]
struct CancelState {
    cancelled: AtomicBool,
}

/// A handle that enqueues cancellation intent for the current operation.
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

    /// Mark the request cancelled; the process owner observes and handles it.
    pub fn cancel(&self) {
        self.state.cancelled.store(true, Ordering::SeqCst);
    }

    /// Whether [`Self::cancel`] has been called.
    pub fn is_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::SeqCst)
    }

    pub(super) fn take_cancelled(&self) -> bool {
        self.state.cancelled.swap(false, Ordering::SeqCst)
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
