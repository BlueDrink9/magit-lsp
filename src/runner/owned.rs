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
                if error.kind() == io::ErrorKind::Interrupted { continue; }
                cleanup(child, pid, error.raw_os_error() != Some(libc::ECHILD));
                return Err(error);
            }
            let exited = unsafe { info.si_pid() != 0 };
            if exited && matches!(state, State::Running) {
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
                outcome.cancelled = true;
                unsafe {
                    libc::kill(-pid, libc::SIGTERM);
                    libc::kill(-pid, libc::SIGCONT);
                }
                state = State::Cancelling(Instant::now());
            }
            if let State::Cancelling(t) = state {
                if t.elapsed() >= GRACE {
                    unsafe {
                        libc::kill(-pid, libc::SIGKILL);
                    }
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
                    fd: inp.as_ref().filter(|_| input.is_some_and(|b| at < b.len())).map_or(-1, AsRawFd::as_raw_fd),
                    events: libc::POLLOUT,
                    revents: 0,
                },
            ];
            let result = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as _, 10) };
            if result < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted { continue; }
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
            Ok(Some(s)) => status = Some(s),
            Ok(None) => {
                let deadline = match state {
                    State::AfterKill(t) => t + GRACE,
                    _ => Instant::now() + GRACE,
                };
                while Instant::now() < deadline {
                    match child.try_wait() {
                        Ok(Some(s)) => { status = Some(s); break; }
                        Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                        Err(error) => { cleanup(child, pid, false); return Err(error); }
                    }
                }
                if status.is_none() {
                    outcome.reap_incomplete = true;
                    std::thread::spawn(move || { let _ = child.wait(); });
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

    fn cleanup(mut child: std::process::Child, pid: libc::pid_t, signal_group: bool) {
        if signal_group {
            unsafe { libc::kill(-pid, libc::SIGTERM); libc::kill(-pid, libc::SIGCONT); }
            let deadline = Instant::now() + GRACE;
            while Instant::now() < deadline {
                let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
                let result = unsafe {
                    libc::waitid(libc::P_PID, pid as libc::id_t, &mut info,
                        libc::WEXITED | libc::WNOHANG | libc::WNOWAIT)
                };
                if result < 0 && io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD) {
                    std::thread::spawn(move || { let _ = child.wait(); });
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            unsafe { libc::kill(-pid, libc::SIGKILL); }
        }
        std::thread::spawn(move || { let _ = child.wait(); });
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
            if drained >= 64 * 1024 { break; }
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
            let script = format!("echo $$ > '{}'; (exec sleep 30) & echo $! > '{}'; exit 0", leader_file.display(), daemon_file.display());
            let (tx, rx) = mpsc::channel();
            let worker_cancel = handle.clone();
            let worker = thread::spawn(move || {
                tx.send(with_cancel(Some(worker_cancel), || run_owned(Command::new("sh").arg("-c").arg(script), None))).unwrap();
            });
            let until = Instant::now() + Duration::from_secs(5);
            while !leader_file.exists() || !daemon_file.exists() {
                assert!(Instant::now() < until, "leader/daemon readiness timed out");
                thread::yield_now();
            }
            let leader: libc::pid_t = std::fs::read_to_string(&leader_file).unwrap().trim().parse().unwrap();
            let daemon: libc::pid_t = std::fs::read_to_string(&daemon_file).unwrap().trim().parse().unwrap();
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            loop {
                let rc = unsafe { libc::waitid(libc::P_PID, leader as _, &mut info, libc::WEXITED | libc::WNOHANG | libc::WNOWAIT) };
                assert_eq!(rc, 0, "waitid leader");
                if unsafe { info.si_pid() } != 0 { break; }
                assert!(Instant::now() < until, "leader did not become zombie");
                thread::yield_now();
            }
            handle.cancel();
            let result = rx.recv_timeout(Duration::from_secs(5)).expect("run_owned drain bound").unwrap();
            worker.join().unwrap();
            assert!(result.status.unwrap().success());
            assert!(!result.outcome.cancelled);
            assert!(result.outcome.drain_incomplete);
            assert_eq!(unsafe { libc::kill(daemon, 0) }, 0, "same-group daemon was signalled");
            unsafe { libc::kill(daemon, libc::SIGKILL); }
            let _ = std::fs::remove_dir_all(dir);
        }

        #[cfg(not(target_os = "linux"))]
        #[test]
        fn exit_before_cancel_is_normal_success() {
            let handle = CancelHandle::new();
            let result = with_cancel(Some(handle.clone()), || run_owned(Command::new("sh").arg("-c").arg("exit 0"), None)).unwrap();
            handle.cancel();
            assert!(result.status.unwrap().success());
            assert!(!result.outcome.cancelled);
        }

        #[test]
        fn caps_large_stdout_and_stderr() {
            let result = run_owned(
                Command::new("sh").arg("-c").arg("head -c 5000000 /dev/zero; head -c 5000000 /dev/zero >&2"),
                None,
            ).unwrap();
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

        #[test]
        fn cancellation_escalates_after_leader_exits() {
            let handle = CancelHandle::new();
            handle.cancel();
            let started = Instant::now();
            let result = with_cancel(Some(handle), || {
                run_owned(
                    Command::new("sh")
                        .arg("-c")
                        .arg("trap 'exit 0' TERM; (trap '' TERM; exec sleep 30) & wait"),
                    None,
                )
            })
            .unwrap();
            assert!(result.outcome.cancelled);
            assert!(started.elapsed() >= GRACE);
        }
    }
}
