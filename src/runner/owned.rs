//! Sole-owner execution primitive for bounded, cancellable child processes.

use super::current_cancel;
use std::io::{self, Read, Write};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// Maximum captured bytes per output stream.
pub const OUTPUT_LIMIT: usize = 4 * 1024 * 1024;
const GRACE: Duration = Duration::from_secs(2);

/// Capture, drain, cancellation and input status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnedOutcome {
    /// Whether stdout exceeded its capture limit.
    pub stdout_truncated: bool,
    /// Whether stderr exceeded its capture limit.
    pub stderr_truncated: bool,
    /// Whether output pipes could not be fully drained before the deadline.
    pub drain_incomplete: bool,
    /// Whether cancellation intent won arbitration against process exit.
    pub cancelled: bool,
    /// Whether stdin writing failed.
    pub stdin_error: bool,
    /// Whether a killed child was transferred to the background reaper.
    pub reap_incomplete: bool,
}

/// Captured process data; status is absent only when ownership moved to reaper.
#[derive(Debug)]
pub struct OwnedOutput {
    /// Exit status, or `None` when final reap exceeded its deadline.
    pub status: Option<ExitStatus>,
    /// Captured stdout bytes.
    pub stdout: Vec<u8>,
    /// Captured stderr bytes.
    pub stderr: Vec<u8>,
    /// Outcome metadata.
    pub outcome: OwnedOutcome,
}

/// Spawn and own `command`, optionally writing finite bytes to stdin.
/// On Unix this creates a process group and polls pipes nonblocking; the
/// non-Unix fallback makes no cancellation guarantee.
pub fn run_owned(command: &mut Command, stdin: Option<&[u8]>) -> io::Result<OwnedOutput> {
    #[cfg(unix)]
    {
        unix::run(command, stdin)
    }
    #[cfg(not(unix))]
    {
        fallback(command, stdin)
    }
}

fn base_outcome() -> OwnedOutcome {
    OwnedOutcome {
        stdout_truncated: false,
        stderr_truncated: false,
        drain_incomplete: false,
        cancelled: false,
        stdin_error: false,
        reap_incomplete: false,
    }
}

#[cfg(not(unix))]
fn fallback(command: &mut Command, input: Option<&[u8]>) -> io::Result<OwnedOutput> {
    let mut child = command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdin_error = input.is_some_and(|bytes| {
        child
            .stdin
            .take()
            .is_some_and(|mut p| p.write_all(bytes).is_err())
    });
    let output = child.wait_with_output()?;
    let mut outcome = base_outcome();
    outcome.stdout_truncated = output.stdout.len() > OUTPUT_LIMIT;
    outcome.stderr_truncated = output.stderr.len() > OUTPUT_LIMIT;
    Ok(OwnedOutput {
        status: Some(output.status),
        stdout: output.stdout.into_iter().take(OUTPUT_LIMIT).collect(),
        stderr: output.stderr.into_iter().take(OUTPUT_LIMIT).collect(),
        outcome,
    })
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::{fd::AsRawFd, unix::process::CommandExt};

    enum State {
        Running,
        Cancelling(Instant),
        AfterKill(Instant),
        Exited(Instant),
    }

    pub fn run(command: &mut Command, input: Option<&[u8]>) -> io::Result<OwnedOutput> {
        command
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // SAFETY: setsid is async-signal-safe and runs only in the child.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        let mut child = command.spawn()?;
        let pid = child.id() as libc::pid_t;
        let cancel = current_cancel();
        let mut out = child.stdout.take().unwrap();
        let mut err = child.stderr.take().unwrap();
        let mut inp = child.stdin.take();
        for fd in [
            Some(out.as_raw_fd()),
            Some(err.as_raw_fd()),
            inp.as_ref().map(AsRawFd::as_raw_fd),
        ]
        .into_iter()
        .flatten()
        {
            // SAFETY: descriptors are owned pipe fds.
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFL);
                if flags < 0 {
                    let error = io::Error::last_os_error();
                    cleanup(child, pid, true);
                    return Err(error);
                }
                if libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
                    let error = io::Error::last_os_error();
                    cleanup(child, pid, true);
                    return Err(error);
                }
            }
        }
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let (mut out_open, mut err_open) = (true, true);
        let mut at = 0;
        let mut outcome = base_outcome();
        let mut state = State::Running;
        loop {
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            // SAFETY: WNOWAIT observes leader state without relinquishing our Child ownership.
            let result = unsafe {
                libc::waitid(
                    libc::P_PID,
                    pid as libc::id_t,
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            };
            if result < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                cleanup(child, pid, error.raw_os_error() != Some(libc::ECHILD));
                return Err(error);
            }
            let exited = unsafe { info.si_pid() != 0 };
            if exited && matches!(state, State::Running) {
                event(pid, "exit_observed");
                // Exit wins over a cancellation that arrived after the child
                // completed; consume it so it cannot affect the next command.
                if let Some(handle) = &cancel {
                    handle.take_cancelled();
                }
                state = State::Exited(Instant::now());
            }
            if matches!(state, State::Running)
                && cancel.as_ref().is_some_and(|h| h.take_cancelled())
            {
                event(pid, "observe_running");
                outcome.cancelled = true;
                event(pid, "cancel");
                unsafe {
                    libc::kill(-pid, libc::SIGTERM);
                    event(pid, "term");
                    libc::kill(-pid, libc::SIGCONT);
                    event(pid, "cont");
                }
                state = State::Cancelling(Instant::now());
            }
            if let State::Cancelling(t) = state {
                if t.elapsed() >= GRACE || (exited && group_observed_quiet(pid)) {
                    unsafe {
                        libc::kill(-pid, libc::SIGKILL);
                    }
                    event(pid, "kill");
                    state = State::AfterKill(Instant::now());
                }
            }
            let (drain_expired, can_finish) = match state {
                State::Exited(t) => (
                    t.elapsed() >= GRACE,
                    !out_open && !err_open || t.elapsed() >= GRACE,
                ),
                State::AfterKill(t) => (
                    t.elapsed() >= GRACE,
                    !out_open && !err_open || t.elapsed() >= GRACE,
                ),
                _ => (false, false),
            };
            if drain_expired && (out_open || err_open) {
                outcome.drain_incomplete = true;
            }
            if can_finish {
                break;
            }
            drain(
                &mut out,
                &mut stdout,
                &mut out_open,
                &mut outcome.stdout_truncated,
                &mut outcome.drain_incomplete,
            );
            drain(
                &mut err,
                &mut stderr,
                &mut err_open,
                &mut outcome.stderr_truncated,
                &mut outcome.drain_incomplete,
            );
            if let (Some(bytes), Some(pipe)) = (input, inp.as_mut()) {
                if at < bytes.len() {
                    match pipe.write(&bytes[at..]) {
                        Ok(n) => at += n,
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                        Err(_) => {
                            outcome.stdin_error = true;
                            inp = None;
                        }
                    }
                }
                if at == bytes.len() {
                    inp = None;
                }
            }
            let mut fds = [
                libc::pollfd {
                    fd: if out_open { out.as_raw_fd() } else { -1 },
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: if err_open { err.as_raw_fd() } else { -1 },
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: inp
                        .as_ref()
                        .filter(|_| input.is_some_and(|b| at < b.len()))
                        .map_or(-1, AsRawFd::as_raw_fd),
                    events: libc::POLLOUT,
                    revents: 0,
                },
            ];
            let result = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as _, 10) };
            if result < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                cleanup(child, pid, error.raw_os_error() != Some(libc::ECHILD));
                return Err(error);
            }
        }
        if matches!(state, State::Cancelling(_)) {
            unreachable!()
        }
        // Reap is bounded. A still-live leader after group SIGKILL is handed to
        // a dedicated process-lifetime reaper instead of blocking this caller.
        let mut status = None;
        match child.try_wait() {
            Err(error) => {
                cleanup(child, pid, false);
                return Err(error);
            }
            Ok(Some(s)) => {
                event(pid, "reap");
                status = Some(s);
            }
            Ok(None) => {
                let deadline = match state {
                    State::AfterKill(t) => t + GRACE,
                    _ => Instant::now() + GRACE,
                };
                while Instant::now() < deadline {
                    match child.try_wait() {
                        Ok(Some(s)) => {
                            event(pid, "reap");
                            status = Some(s);
                            break;
                        }
                        Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                        Err(error) => {
                            cleanup(child, pid, false);
                            return Err(error);
                        }
                    }
                }
                if status.is_none() {
                    outcome.reap_incomplete = true;
                    std::thread::spawn(move || {
                        if child.wait().is_ok() {
                            event(pid, "reap");
                        }
                    });
                }
            }
        }
        Ok(OwnedOutput {
            status,
            stdout,
            stderr,
            outcome,
        })
    }

    fn event(pid: libc::pid_t, phase: &'static str) {
        super::super::trace_current(
            "owned_process",
            serde_json::json!({"pid": pid, "phase": phase}),
        );
        #[cfg(test)]
        EVENTS.with(|events| {
            events.borrow_mut().push((
                phase,
                Instant::now(),
                std::path::Path::new(&format!("/proc/{pid}")).exists(),
            ))
        });
    }

    #[cfg(test)]
    thread_local! {
        static EVENTS: std::cell::RefCell<Vec<(&'static str, Instant, bool)>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    fn cleanup(mut child: std::process::Child, pid: libc::pid_t, signal_group: bool) {
        if signal_group {
            unsafe {
                libc::kill(-pid, libc::SIGTERM);
                libc::kill(-pid, libc::SIGCONT);
            }
            let deadline = Instant::now() + GRACE;
            while Instant::now() < deadline {
                let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
                let result = unsafe {
                    libc::waitid(
                        libc::P_PID,
                        pid as libc::id_t,
                        &mut info,
                        libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                    )
                };
                if result < 0 && io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD) {
                    std::thread::spawn(move || {
                        let _ = child.wait();
                    });
                    return;
                }
                if result == 0 && unsafe { info.si_pid() != 0 } && group_observed_quiet(pid) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }

    // Only called after WNOWAIT observed the unreaped leader exit. This scan
    // is a latency hint, not a containment proof: a concurrent fork can be
    // missed. The final group KILL still precedes every reap; never signal
    // individual PIDs discovered here. Uncertain inspection keeps full grace.
    #[cfg(target_os = "linux")]
    fn group_observed_quiet(leader: libc::pid_t) -> bool {
        let inspect = || -> Option<bool> {
            let started = Instant::now();
            for entry in std::fs::read_dir("/proc").ok()? {
                if started.elapsed() >= Duration::from_millis(25) {
                    return None;
                }
                let entry = entry.ok()?;
                let name = entry.file_name();
                let Some(name) = name.to_str() else {
                    continue;
                };
                if !name.bytes().all(|b| b.is_ascii_digit()) {
                    continue;
                }
                let pid: libc::pid_t = name.parse().ok()?;
                if pid <= 0 {
                    return None;
                }
                let stat = match std::fs::read(entry.path().join("stat")) {
                    Ok(stat) => stat,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                    Err(_) => return None,
                };
                let (state, group) = proc_stat_group(&stat, pid)?;
                if pid != leader && group == leader && !matches!(state, b'Z' | b'X' | b'x') {
                    return Some(false);
                }
            }
            Some(true)
        };
        inspect().unwrap_or(false)
    }

    #[cfg(not(target_os = "linux"))]
    fn group_observed_quiet(_leader: libc::pid_t) -> bool {
        false
    }

    // comm may contain spaces, newlines, non-UTF8 and ')' characters. Only
    // numeric fields outside its final ')' are decoded as text.
    #[cfg(target_os = "linux")]
    fn proc_stat_group(stat: &[u8], expected_pid: libc::pid_t) -> Option<(u8, libc::pid_t)> {
        let open = stat.iter().position(|&b| b == b'(')?;
        let close = stat.iter().rposition(|&b| b == b')')?;
        if open == 0
            || close < open
            || stat.get(open - 1) != Some(&b' ')
            || stat.get(close + 1) != Some(&b' ')
        {
            return None;
        }
        let pid: libc::pid_t = std::str::from_utf8(&stat[..open - 1]).ok()?.parse().ok()?;
        if pid != expected_pid || pid <= 0 {
            return None;
        }
        let mut fields = stat[close + 2..]
            .split(|b| b.is_ascii_whitespace())
            .filter(|f| !f.is_empty());
        let state = fields.next()?;
        if state.len() != 1 || !b"RSDZTtXxKWPI".contains(&state[0]) {
            return None;
        }
        let ppid: libc::pid_t = std::str::from_utf8(fields.next()?).ok()?.parse().ok()?;
        let group: libc::pid_t = std::str::from_utf8(fields.next()?).ok()?.parse().ok()?;
        if ppid < 0 || group < 0 {
            return None;
        }
        Some((state[0], group))
    }

    fn drain<R: Read>(
        pipe: &mut R,
        buf: &mut Vec<u8>,
        open: &mut bool,
        truncated: &mut bool,
        incomplete: &mut bool,
    ) {
        if !*open {
            return;
        }
        let mut tmp = [0u8; 8192];
        let mut drained = 0usize;
        loop {
            if drained >= 64 * 1024 {
                break;
            }
            match pipe.read(&mut tmp) {
                Ok(0) => {
                    *open = false;
                    break;
                }
                Ok(n) => {
                    drained += n;
                    let keep = OUTPUT_LIMIT.saturating_sub(buf.len());
                    buf.extend_from_slice(&tmp[..n.min(keep)]);
                    if n > keep {
                        *truncated = true;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    *open = false;
                    *incomplete = true;
                    break;
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::runner::{with_cancel, CancelHandle};

        #[cfg(target_os = "linux")]
        #[test]
        fn exit_before_cancel_is_normal_success() {
            use std::sync::mpsc;
            use std::thread;

            let handle = CancelHandle::new();
            let dir = std::env::temp_dir().join(format!("magit-lsp-owned-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let leader_file = dir.join("leader");
            let daemon_file = dir.join("daemon");
            let script = format!(
                "echo $$ > '{}'; (exec sleep 30) & echo $! > '{}'; exit 0",
                leader_file.display(),
                daemon_file.display()
            );
            let (tx, rx) = mpsc::channel();
            let worker_cancel = handle.clone();
            let worker = thread::spawn(move || {
                let result = with_cancel(Some(worker_cancel), || {
                    run_owned(Command::new("sh").arg("-c").arg(script), None)
                });
                let events = EVENTS.with(|events| events.borrow().clone());
                tx.send((result, events)).unwrap();
            });
            let until = Instant::now() + Duration::from_secs(5);
            while !leader_file.exists() || !daemon_file.exists() {
                assert!(Instant::now() < until, "leader/daemon readiness timed out");
                thread::yield_now();
            }
            let leader: libc::pid_t = std::fs::read_to_string(&leader_file)
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            let daemon: libc::pid_t = std::fs::read_to_string(&daemon_file)
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            loop {
                let rc = unsafe {
                    libc::waitid(
                        libc::P_PID,
                        leader as _,
                        &mut info,
                        libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                    )
                };
                assert_eq!(rc, 0, "waitid leader");
                if unsafe { info.si_pid() } != 0 {
                    break;
                }
                assert!(Instant::now() < until, "leader did not become zombie");
                thread::yield_now();
            }
            handle.cancel();
            let (result, events) = rx
                .recv_timeout(Duration::from_secs(5))
                .expect("run_owned drain bound");
            let result = result.unwrap();
            worker.join().unwrap();
            assert!(result.status.unwrap().success());
            assert!(!result.outcome.cancelled);
            assert!(result.outcome.drain_incomplete);
            assert_eq!(
                events.iter().map(|e| e.0).collect::<Vec<_>>(),
                ["exit_observed", "reap"],
                "normal exit must never signal, including after reap"
            );
            assert!(events[0].2, "WNOWAIT must retain leader identity");
            assert!(!events[1].2, "final wait must release leader identity");
            assert_eq!(
                unsafe { libc::kill(daemon, 0) },
                0,
                "same-group daemon was signalled"
            );
            unsafe {
                libc::kill(daemon, libc::SIGKILL);
            }
            let _ = std::fs::remove_dir_all(dir);
        }

        #[cfg(not(target_os = "linux"))]
        #[test]
        fn exit_before_cancel_is_normal_success() {
            let handle = CancelHandle::new();
            let result = with_cancel(Some(handle.clone()), || {
                run_owned(Command::new("sh").arg("-c").arg("exit 0"), None)
            })
            .unwrap();
            handle.cancel();
            assert!(result.status.unwrap().success());
            assert!(!result.outcome.cancelled);
        }

        #[test]
        fn caps_large_stdout_and_stderr() {
            let result = run_owned(
                Command::new("sh")
                    .arg("-c")
                    .arg("head -c 5000000 /dev/zero; head -c 5000000 /dev/zero >&2"),
                None,
            )
            .unwrap();
            assert!(result.status.unwrap().success());
            assert_eq!(result.stdout.len(), OUTPUT_LIMIT);
            assert_eq!(result.stderr.len(), OUTPUT_LIMIT);
            assert!(result.outcome.stdout_truncated);
            assert!(result.outcome.stderr_truncated);
        }

        #[test]
        fn substantial_stdin_echo_roundtrip() {
            let input = vec![b'x'; 1024 * 1024];
            let result = run_owned(Command::new("sh").arg("-c").arg("cat"), Some(&input)).unwrap();
            assert!(result.status.unwrap().success());
            assert_eq!(result.stdout, input);
            assert!(!result.outcome.stdin_error);
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn child_starts_as_session_and_group_leader_without_a_tty() {
            let result = run_owned(
                Command::new("sh")
                    .arg("-c")
                    .arg("ps -o pid=,pgid=,sid=,tty= -p $$"),
                None,
            )
            .unwrap();
            assert!(result.status.unwrap().success());
            let text = String::from_utf8(result.stdout).unwrap();
            let fields: Vec<_> = text.split_whitespace().collect();
            assert_eq!(fields.len(), 4, "{text}");
            assert_eq!(fields[0], fields[1], "new process group before exec");
            assert_eq!(fields[0], fields[2], "new session before exec");
            assert_eq!(fields[3], "?", "no controlling terminal");
        }

        #[test]
        fn cancellation_escalates_after_leader_exits() {
            let handle = CancelHandle::new();
            let ready = tempfile::tempdir().unwrap();
            let marker = ready.path().join("ready");
            let script = format!(
                "trap 'exit 0' TERM; (trap '' TERM; touch '{}'; exec sleep 30) & wait",
                marker.display()
            );
            let worker_handle = handle.clone();
            let worker = std::thread::spawn(move || {
                with_cancel(Some(worker_handle), || {
                    let result = run_owned(Command::new("sh").arg("-c").arg(script), None);
                    (result, EVENTS.with(|events| events.borrow().clone()))
                })
            });
            let deadline = Instant::now() + Duration::from_secs(5);
            while !marker.exists() {
                assert!(
                    Instant::now() < deadline,
                    "resistant child readiness timed out"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            let started = Instant::now();
            handle.cancel();
            let (result, events) = worker.join().unwrap();
            let result = result.unwrap();
            assert!(result.outcome.cancelled);
            assert!(started.elapsed() >= GRACE);
            assert_eq!(
                events.iter().map(|e| e.0).collect::<Vec<_>>(),
                ["observe_running", "cancel", "term", "cont", "kill", "reap"]
            );
            assert!(
                events[4].1.duration_since(events[3].1) >= GRACE,
                "resistant member must receive full grace before KILL"
            );
            #[cfg(target_os = "linux")]
            {
                assert!(
                    events[..5].iter().all(|e| e.2),
                    "leader identity must remain pinned until final KILL"
                );
                assert!(
                    !events[5].2,
                    "reap must be final, with no subsequent signal"
                );
            }
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn proc_stat_validates_identity_state_and_group_outside_comm() {
            assert_eq!(
                proc_stat_group(b"42 (a ) tricky\n\xff) T 1 42 0", 42),
                Some((b'T', 42))
            );
            assert_eq!(
                proc_stat_group(b"42 (zombie) Z 1 42 0", 42),
                Some((b'Z', 42))
            );
            for stat in [
                &b"43 (wrong pid) S 1 42"[..],
                &b"42 (bad state) ? 1 42"[..],
                &b"42 (bad group) S 1 -1"[..],
                &b"42 (missing) S 1"[..],
                &b"42 (state) SS 1 42"[..],
                &b"42(bad delimiter) S 1 42"[..],
            ] {
                assert_eq!(proc_stat_group(stat, 42), None, "{stat:?}");
            }
        }
    }
}
