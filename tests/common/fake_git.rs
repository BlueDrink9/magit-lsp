//! Tests-only fake `git` executables, each scoped to a spawned child's `PATH`.
//!
//! [`GatedGit`] blocks a gated subcommand until the test explicitly releases it,
//! with an entered/release handshake. [`FakeGit`] replaces `add -u` with a
//! start marker and a long sleep so cancellation can be observed.
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
const ENTERED_TIMEOUT: Duration = Duration::from_secs(10);
/// How long [`GatedGit`]'s drop waits for a released child to leave the gate.
const EXIT_TIMEOUT: Duration = Duration::from_secs(5);
/// Poll interval for every bounded wait in this fixture.
const POLL: Duration = Duration::from_millis(10);

/// A fake `git` executable that gates the first invocation of one subcommand.
///
/// The gate is single-use: the first gated invocation blocks inside the fake
/// until [`Self::release`]; every later invocation (gated or not) runs the
/// real Git immediately.
pub struct GatedGit {
    dir: tempfile::TempDir,
    command: String,
    script: PathBuf,
    entered_path: PathBuf,
    release_path: PathBuf,
}

impl GatedGit {
    /// Install the fake in a private temp directory, gating `command`.
    ///
    /// Nothing is armed on the parent environment; use [`Self::child_env`] to
    /// scope the fake to a spawned server.
    pub fn arm(command: &str) -> Self {
        let real_git = real_git_path();
        let dir = tempfile::TempDir::new().expect("fake git tempdir");
        let entered_path = dir.path().join("entered");
        let release_path = dir.path().join("release");
        let script = dir.path().join("git");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\n\
                 # Tests-only gate: block the first '{command}' until the release\n\
                 # file exists, then run the real Git so the repository stays correct.\n\
                 if [ \"$1\" = '{command}' ] && [ ! -f '{release}' ]; then\n\
                   echo \"$$\" > '{entered}'\n\
                   n=0\n\
                   while [ ! -f '{release}' ]; do\n\
                     sleep 0.05\n\
                     n=$((n+1))\n\
                     [ \"$n\" -ge {timeout} ] && break\n\
                   done\n\
                   rm -f '{entered}'\n\
                 fi\n\
                 exec '{real}' \"$@\"\n",
                command = command,
                release = release_path.display(),
                entered = entered_path.display(),
                timeout = GATE_TIMEOUT_POLLS,
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

    /// Block until a gated invocation has entered the gate (bounded).
    ///
    /// This is the handshake's entered half: after it returns, the server's
    /// command is verifiably blocked inside the fake Git.
    pub fn wait_entered(&self) {
        let deadline = Instant::now() + ENTERED_TIMEOUT;
        while !self.entered_path.exists() {
            assert!(
                Instant::now() < deadline,
                "the gated `{}` never entered the gate",
                self.command
            );
            thread::sleep(POLL);
        }
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
            if process_exists(pid) && self.is_gate_child(pid) {
                // Last resort so the gate can never leak a waiting child; the
                // identity check makes sure a recycled pid is never signalled.
                let _ = Command::new("kill")
                    .args(["-9", &pid.to_string()])
                    .status();
            }
        }
    }
}

impl GatedGit {
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
