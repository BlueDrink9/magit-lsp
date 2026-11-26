//! Tests-only fake `git` executables, each scoped to a spawned child's `PATH`.
//!
//! [`GatedGit`] blocks a gated subcommand until the test explicitly releases it,
//! with an entered/release handshake, or (in kill mode) kills that first
//! invocation by signal before it can log itself. [`FakeGit`] replaces `add -u`
//! with a start marker and a long sleep so cancellation can be observed.
//!
//! Both are placed first on the *server child's* `PATH` only; the test process
//! and every other process keep the real Git, and no parent environment is ever
//! mutated. Every other command delegates to the real binary, so repository
//! behaviour stays correct.
//!
//! [`GatedGit`] is file-based and bounded: the child writes its pid to
//! `entered` when it is blocked inside the gate, and the test creates `release`
//! to let it proceed.

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

/// Set in the re-executed child so it runs the test body instead of spawning.
pub const CHILD_ENV: &str = "MAGIT_LSP_FAKE_GIT_CHILD";
/// The start marker [`FakeGit`] writes, handed to the child.
pub const MARKER_ENV: &str = "MAGIT_LSP_FAKE_GIT_MARKER";

/// A fake `git` for a child process's `PATH`; it delegates everything to the
/// real Git except `add -u`, which signals its start and then sleeps until
/// killed.
pub struct FakeGit {
    _dir: tempfile::TempDir,
    /// `PATH` value putting the fake first, passed with `Command::env`.
    pub path: OsString,
}

impl FakeGit {
    /// Install the fake in a private temp directory.
    pub fn new() -> Self {
        let real_git = real_git_path();
        let dir = tempfile::TempDir::new().expect("fake git tempdir");
        let script = dir.path().join("git");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\n\
                 if [ \"$1\" = \"add\" ] && [ \"$2\" = \"-u\" ]; then\n\
                   printf 'started\\n' > \"${{{MARKER_ENV}}}\"\n\
                   exec sleep 30\n\
                 fi\n\
                 exec '{}' \"$@\"\n",
                real_git.display()
            ),
        )
        .expect("write fake git");
        let mut permissions = std::fs::metadata(&script)
            .expect("fake git metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).expect("chmod fake git");

        let mut paths = vec![dir.path().to_path_buf()];
        if let Some(path) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&path));
        }

        Self {
            _dir: dir,
            path: std::env::join_paths(paths).expect("join PATH"),
        }
    }
}

/// How long the fake waits for the release file before giving up, so a killed
/// test can never leak a child that blocks forever.
const GATE_TIMEOUT_POLLS: u32 = 4000;
/// How long [`GatedGit::wait_entered`] waits for the handshake's entered half.
///
/// This bounds a whole server spawn + initialize + view open + Git discovery
/// chain on a loaded CI runner, so it matches the sibling server_async bounds.
const ENTERED_TIMEOUT: Duration = Duration::from_secs(60);
/// How long [`GatedGit`]'s drop waits for a released child to leave the gate.
const EXIT_TIMEOUT: Duration = Duration::from_secs(5);
/// Poll interval for every bounded wait in this fixture.
const POLL: Duration = Duration::from_millis(10);

/// A fake `git` executable that gates the first invocation of one subcommand.
///
/// The gate is single-use: the first gated invocation blocks inside the fake
/// until [`Self::release`]; every later invocation (gated or not) runs the
/// real Git immediately. In kill mode ([`Self::arm_killing`]) that first
/// invocation instead dies by signal before it records itself, reproducing a
/// Git child killed between spawn and completion.
pub struct GatedGit {
    dir: tempfile::TempDir,
    command: String,
    script: PathBuf,
    entered_path: PathBuf,
    release_path: PathBuf,
    claimed_path: PathBuf,
    invocations_path: PathBuf,
    /// Whether the first gated invocation dies by signal instead of blocking.
    kill: bool,
}

impl GatedGit {
    /// Install the fake in a private temp directory, gating `command`.
    ///
    /// Nothing is armed on the parent environment; use [`Self::child_env`] to
    /// scope the fake to a spawned server.
    pub fn arm(command: &str) -> Self {
        Self::install(command, false)
    }

    /// Install the fake so the first `command` invocation is killed by signal.
    ///
    /// The killed invocation writes [`Self::claimed`]'s marker but never reaches
    /// its invocation-log echo and never enters the gate, exactly like a Git
    /// child the host kills on spawn (OOM or code-signing termination). That is
    /// the CI signature this mode reproduces: the invocation log names only the
    /// surrounding probes and [`Self::wait_entered`] would wait forever.
    pub fn arm_killing(command: &str) -> Self {
        Self::install(command, true)
    }

    /// Write the fake script for `command` and place it in a private directory.
    fn install(command: &str, kill: bool) -> Self {
        let real_git = real_git_path();
        let dir = tempfile::TempDir::new().expect("fake git tempdir");
        let entered_path = dir.path().join("entered");
        let release_path = dir.path().join("release");
        let claimed_path = dir.path().join("claimed");
        let invocations_path = dir.path().join("invocations");
        let script = dir.path().join("git");
        let action = if kill {
            // Die before the log echo, but still claim the single gated slot so
            // a retry would run the real Git instead of dying again.
            format!(
                "# Tests-only kill gate: the first '{command}' invocation is killed by\n\
                 # signal before it can log itself, matching a host-killed Git child.\n\
                 if [ \"$subcommand\" = '{command}' ] && [ ! -f '{claimed}' ]; then\n\
                   : > '{claimed}'\n\
                   kill -KILL $$\n\
                 fi\n\
                 echo \"$subcommand\" >> '{invocations}'\n",
                command = command,
                claimed = claimed_path.display(),
                invocations = invocations_path.display(),
            )
        } else {
            format!(
                "echo \"$subcommand\" >> '{invocations}'\n\
                 # Tests-only gate: block the first '{command}' until the release\n\
                 # file exists, then run the real Git so the repository stays correct.\n\
                 if [ \"$subcommand\" = '{command}' ] && [ ! -f '{claimed}' ]; then\n\
                   : > '{claimed}'\n\
                   echo \"$$\" > '{entered}'\n\
                   n=0\n\
                   while [ ! -f '{release}' ]; do\n\
                     sleep 0.05\n\
                     n=$((n+1))\n\
                     [ \"$n\" -ge {timeout} ] && break\n\
                   done\n\
                   rm -f '{entered}'\n\
                 fi\n",
                command = command,
                claimed = claimed_path.display(),
                entered = entered_path.display(),
                release = release_path.display(),
                invocations = invocations_path.display(),
                timeout = GATE_TIMEOUT_POLLS,
            )
        };
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\n\
                 subcommand=\"$1\"\n\
                 if [ \"$subcommand\" = '--no-optional-locks' ]; then subcommand=\"$2\"; fi\n\
                 {action}\
                 exec '{real}' \"$@\"\n",
                real = real_git.display(),
            ),
        )
        .expect("write fake git");
        let mut permissions = std::fs::metadata(&script)
            .expect("fake git metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).expect("chmod fake git");

        Self {
            dir,
            command: command.to_string(),
            script,
            entered_path,
            release_path,
            claimed_path,
            invocations_path,
            kill,
        }
    }

    /// The `PATH` override that puts the fake first for one child process.
    ///
    /// Pass the result to [`super::stdio::TestServer::spawn`]; the parent's
    /// environment is never touched.
    pub fn child_env(&self) -> Vec<(String, String)> {
        let path = format!(
            "{}:{}",
            self.dir.path().display(),
            std::env::var("PATH").unwrap_or_default()
        );
        vec![("PATH".to_string(), path)]
    }

    /// Whether a gated invocation is currently blocked inside the gate.
    ///
    /// Unlike [`Self::wait_entered`] this neither waits nor panics, so a poll
    /// loop can notice a gate that will never be entered — a killed child, or
    /// an acquisition that failed before Git ran.
    pub fn has_entered(&self) -> bool {
        self.entered_path.exists()
    }

    /// Block until a gated invocation has entered the gate (bounded).
    ///
    /// This is the handshake's entered half: after it returns, the server's
    /// command is verifiably blocked inside the fake Git.
    pub fn wait_entered(&self) {
        self.wait_entered_within(ENTERED_TIMEOUT);
    }

    /// [`Self::wait_entered`] with an explicit bound, for tests of the timeout.
    pub fn wait_entered_within(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        while !self.entered_path.exists() {
            if Instant::now() >= deadline {
                panic!("{}", self.entered_timeout_message(timeout));
            }
            thread::sleep(POLL);
        }
    }

    /// The timeout panic text: the absent enter marker plus the invocation log's
    /// tail, so a failure shows whether Git ran at all and which subcommands.
    fn entered_timeout_message(&self, timeout: Duration) -> String {
        const TAIL_CHARS: usize = 2048;
        let log = self.invocation_log();
        let tail: String = {
            let chars: Vec<char> = log.chars().collect();
            let start = chars.len().saturating_sub(TAIL_CHARS);
            chars[start..].iter().collect()
        };
        let tail = if tail.is_empty() {
            "(no Git invocations recorded)".to_string()
        } else {
            tail.trim_end().to_string()
        };
        let mode = if self.kill {
            " (kill mode: the gated invocation dies by signal and can never enter the gate)"
        } else {
            ""
        };
        format!(
            "the gated `{}` never entered the gate within {:?}{mode}; enter marker `{}` is still absent; \
             invocation log `{}` (tail):\n{}",
            self.command,
            timeout,
            self.entered_path.display(),
            self.invocations_path.display(),
            tail
        )
    }

    /// Let the blocked invocation proceed (idempotent).
    ///
    /// This is the handshake's release half; it is also called by [`Drop`], so
    /// a failing test can never leave a child blocked in the gate.
    pub fn release(&self) {
        let _ = std::fs::write(&self.release_path, b"release\n");
    }

    /// Whether [`Self::release`] has been called at least once.
    pub fn is_released(&self) -> bool {
        self.release_path.exists()
    }

    /// Every subcommand this fixture's fake Git recorded, in order.
    ///
    /// A killed invocation dies before its log echo, so its subcommand is
    /// absent; the surviving entries are the decisive evidence of what did run.
    pub fn invocation_log(&self) -> String {
        std::fs::read_to_string(&self.invocations_path)
            .map(|log| log.replace("\r\n", "\n"))
            .unwrap_or_default()
    }

    /// Number of matching Git invocations observed by this fixture.
    ///
    /// The log records every subcommand the fake saw; only the gated one counts.
    pub fn invocation_count(&self) -> usize {
        self.invocation_log()
            .lines()
            .filter(|line| line.trim() == self.command)
            .count()
    }

    /// The pid of the child currently blocked in the gate, if any.
    fn entered_pid(&self) -> Option<u32> {
        std::fs::read_to_string(&self.entered_path)
            .ok()
            .and_then(|text| text.trim().parse().ok())
    }
}

impl Drop for GatedGit {
    fn drop(&mut self) {
        // Release first: a blocked child must be allowed to finish so the
        // server can complete and be reaped, even when the test failed.
        self.release();
        if let Some(pid) = self.entered_pid() {
            let deadline = Instant::now() + EXIT_TIMEOUT;
            while process_exists(pid) && Instant::now() < deadline {
                thread::sleep(POLL);
            }
            if process_exists(pid) {
                // Last resort so the gate can never leak a waiting child; the
                // identity check makes sure a recycled pid is never signalled.
                // The evidence goes out first: a killed gate child otherwise
                // only shows up as a later client-side timeout.
                let gate_child = self.is_gate_child(pid);
                eprintln!("{}", self.kill_forensics(pid, gate_child));
                if gate_child {
                    let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
                }
            }
        }
    }
}

impl GatedGit {
    /// The stderr evidence emitted before the last-resort `kill -9`.
    ///
    /// It names the pid, its `ps -o pid=,ppid=,args=` line and the identity
    /// guard's verdict, so a kill (or a live pid the guard refused to signal)
    /// is explainable from the test log. Every step is failure-tolerant: this
    /// runs from [`Drop`], which must never panic.
    fn kill_forensics(&self, pid: u32, gate_child: bool) -> String {
        let verdict = if gate_child {
            "confirmed this fixture's gate child; signalling kill -9"
        } else {
            "failed (pid is not this fixture's gate child); not signalled"
        };
        format!(
            "gated git last-resort kill forensics: pid {pid}; identity guard: {verdict}; \
             `ps -o pid=,ppid=,args=`:\n{}",
            process_table_entry(pid)
        )
    }

    /// Whether `pid` is a process still running this fixture's gate script.
    ///
    /// The script path is unique per fixture, so a recycled pid can never be
    /// mistaken for one of this gate's blocked children.
    fn is_gate_child(&self, pid: u32) -> bool {
        let output = Command::new("ps")
            .args(["-o", "args=", "-p", &pid.to_string()])
            .output()
            .expect("run ps");
        String::from_utf8_lossy(&output.stdout).contains(self.script.to_string_lossy().as_ref())
    }
}

/// One bounded `ps -o pid=,ppid=,args=` line for `pid`, for kill diagnostics.
///
/// A failed or absent `ps` becomes readable text instead of a panic, because
/// the caller is a [`Drop`] implementation.
fn process_table_entry(pid: u32) -> String {
    match Command::new("ps")
        .args(["-o", "pid=,ppid=,args=", "-p", &pid.to_string()])
        .output()
    {
        Ok(output) => {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if text.is_empty() {
                "(no process table entry)".to_string()
            } else {
                text
            }
        }
        Err(error) => format!("(ps failed: {error})"),
    }
}

/// Whether a process with `pid` is still running.
fn process_exists(pid: u32) -> bool {
    let output = Command::new("ps")
        .args(["-o", "pid=", "-p", &pid.to_string()])
        .output()
        .expect("run ps");
    !String::from_utf8_lossy(&output.stdout).trim().is_empty()
}

/// The absolute path of the real `git` on `PATH`.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_entered_timeout_names_absent_marker_and_invocation_log_tail() {
        let gate = GatedGit::arm("add");
        let path = gate
            .child_env()
            .into_iter()
            .find_map(|(key, value)| (key == "PATH").then_some(value))
            .expect("PATH override");
        // A non-gated subcommand: it reaches the fake but never the gate.
        // Route through the shell so the script is read as data: a direct exec
        // can race a sibling thread's still-open write fd and fail ETXTBSY.
        let output = Command::new("sh")
            .arg(&gate.script)
            .arg("status")
            .env("PATH", path)
            .output()
            .expect("run fake git");
        assert!(
            output.status.success(),
            "fake git status failed: {output:?}"
        );

        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            gate.wait_entered_within(Duration::from_millis(50));
        }))
        .expect_err("an unentered gate must panic");
        let message = panic
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string()))
            .expect("panic carries a message");
        assert!(message.contains("never entered the gate"), "{message}");
        assert!(message.contains("still absent"), "{message}");
        assert!(
            message.contains("status"),
            "invocation log tail missing: {message}"
        );
    }

    #[test]
    fn last_resort_kill_forensics_names_pid_table_and_guard_verdict() {
        let gate = GatedGit::arm("add");
        // This test process is alive but can never be this fixture's gate
        // child, so the builder reports the refusal and signals nothing.
        let pid = std::process::id();
        let refused = gate.kill_forensics(pid, false);
        assert!(refused.contains(&format!("pid {pid};")), "{refused}");
        assert!(
            refused.contains("failed (pid is not this fixture's gate child); not signalled"),
            "{refused}"
        );
        assert!(refused.contains("ps -o pid=,ppid=,args="), "{refused}");
        let entry = refused.split("`ps -o pid=,ppid=,args=`:\n").nth(1);
        let entry = entry.expect("process table entry section");
        assert!(
            entry.contains(&pid.to_string()),
            "ps entry missing the pid: {entry}"
        );

        let confirmed = gate.kill_forensics(pid, true);
        assert!(
            confirmed.contains("confirmed this fixture's gate child; signalling kill -9"),
            "{confirmed}"
        );
    }

    /// The `PATH` override of `gate`, for running the fake script directly.
    fn gate_path(gate: &GatedGit) -> String {
        gate.child_env()
            .into_iter()
            .find_map(|(key, value)| (key == "PATH").then_some(value))
            .expect("PATH override")
    }

    #[test]
    fn kill_mode_child_dies_by_signal_before_its_invocation_log_echo() {
        use std::os::unix::process::ExitStatusExt as _;
        let gate = GatedGit::arm_killing("status");
        let path = gate_path(&gate);
        // Route through the shell so the script is read as data (see the
        // timeout test above for the direct-exec ETXTBSY race).
        let killed = Command::new("sh")
            .arg(&gate.script)
            .arg("status")
            .env("PATH", &path)
            .output()
            .expect("run fake git");
        assert_eq!(
            killed.status.signal(),
            Some(9),
            "the kill-mode invocation must die by signal: {killed:?}"
        );
        assert!(
            !gate.has_entered(),
            "a killed invocation can never enter the gate"
        );
        assert_eq!(gate.invocation_count(), 0);
        assert!(
            !gate
                .invocation_log()
                .lines()
                .any(|line| line.trim() == "status"),
            "the kill happens before the log echo: {:?}",
            gate.invocation_log()
        );

        // The single gated slot stays spent: the next command runs the real Git.
        let next = Command::new("sh")
            .arg(&gate.script)
            .arg("--version")
            .env("PATH", &path)
            .output()
            .expect("run fake git");
        assert!(next.status.success(), "{next:?}");
        assert!(
            gate.invocation_log().contains("--version"),
            "{:?}",
            gate.invocation_log()
        );
    }

    #[test]
    fn kill_mode_timeout_names_the_mode_and_the_surviving_invocations() {
        let gate = GatedGit::arm_killing("status");
        let path = gate_path(&gate);
        let survivor = Command::new("sh")
            .arg(&gate.script)
            .arg("--version")
            .env("PATH", &path)
            .output()
            .expect("run fake git");
        assert!(survivor.status.success(), "{survivor:?}");

        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            gate.wait_entered_within(Duration::from_millis(50));
        }))
        .expect_err("an unentered kill-mode gate must panic");
        let message = panic
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string()))
            .expect("panic carries a message");
        assert!(message.contains("kill mode"), "{message}");
        assert!(message.contains("never entered the gate"), "{message}");
        assert!(
            message.contains("--version"),
            "invocation log tail missing: {message}"
        );
    }
}
