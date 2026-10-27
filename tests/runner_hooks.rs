//! Linux real-Git hook tests for owned process cancellation and pipe draining.

#![cfg(target_os = "linux")]

mod common;

use common::*;
use magit_lsp::runner::{self, CancelHandle};
use std::os::unix::fs::PermissionsExt;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

fn hook(fixture: &RepoFixture, name: &str, body: &str) {
    let path = fixture.path().join(".git/hooks").join(name);
    write(&path, &format!("#!/bin/sh\n{body}\n"));
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn wait_marker(path: &std::path::Path) {
    let until = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        assert!(Instant::now() < until, "hook readiness marker missing");
        thread::sleep(Duration::from_millis(10));
    }
}

fn process_is_live(pid: i32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|stat| stat.rsplit_once(')').map(|(_, tail)| tail.trim_start().starts_with(|c: char| c != 'Z' && c != 'X')))
        .unwrap_or(false)
}

fn proc_start_time(pid: i32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let tail = stat.rsplit_once(')')?.1;
    tail.split_whitespace().nth(19)?.parse().ok()
}

struct OwnedDaemon { pid: i32, start_time: u64 }

struct OwnedChild(Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for OwnedDaemon {
    fn drop(&mut self) {
        if proc_start_time(self.pid) == Some(self.start_time) && process_is_live(self.pid) {
            unsafe { libc::kill(self.pid, libc::SIGKILL); }
            let until = Instant::now() + Duration::from_secs(2);
            while proc_start_time(self.pid) == Some(self.start_time) && process_is_live(self.pid) && Instant::now() < until {
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

#[test]
fn cancellation_kills_real_hook_process_group_and_preserves_repository() {
    let (fixture, repo) = init_repo();
    let marker = fixture.path().join(".git/hook-ready");
    let pidfile = fixture.path().join(".git/hook-child");
    hook(&fixture, "pre-commit", &format!(
        "sh -c 'trap \"\" TERM; echo $$ > \"$1\"; touch \"$2\"; exec sleep 60' sh '{}' '{}' </dev/null >/dev/null 2>&1 &\nwait",
        pidfile.display(), marker.display()
    ));
    write(&fixture.path().join("new.txt"), "staged\n");
    fixture.git(&["add", "new.txt"]);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let staged = fixture.git(&["ls-files", "--stage", "-z"]);
    let cached_diff = fixture.git(&["diff", "--cached", "--binary"]);
    let staged_tree = fixture.git(&["write-tree"]);
    let worktree = read(&fixture.path().join("f.txt"));
    let new_worktree = read(&fixture.path().join("new.txt"));
    let sentinel = OwnedChild(Command::new("sh")
        .args(["-c", "trap '' TERM; touch \"$1\"; exec sleep 60", "sentinel"])
        .arg(fixture.path().join(".git/sentinel-ready"))
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().unwrap());
    let sentinel_pid = sentinel.0.id() as i32;
    let sentinel_start = proc_start_time(sentinel_pid).expect("sentinel process identity");
    let _sentinel_identity = OwnedDaemon { pid: sentinel_pid, start_time: sentinel_start };
    wait_marker(&fixture.path().join(".git/sentinel-ready"));
    let cancel = CancelHandle::new();
    let worker_cancel = cancel.clone();
    let root = fixture.path().to_path_buf();
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let repo = magit_lsp::git::GitRepo::open(&root).unwrap();
        tx.send(runner::with_cancel(Some(worker_cancel), || repo.commit(&["-m", "hook"], &[]))).unwrap();
    });
    wait_marker(&marker);
    let pid: i32 = read(&pidfile).trim().parse().unwrap();
    let child_start = proc_start_time(pid).expect("hook descendant process identity");
    let _child = OwnedDaemon { pid, start_time: child_start };
    let start = Instant::now();
    cancel.cancel();
    assert!(rx.recv_timeout(Duration::from_secs(8)).unwrap().is_err());
    assert!(start.elapsed() < Duration::from_secs(8));
    worker.join().unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    while process_is_live(pid) && Instant::now() < until {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!process_is_live(pid), "hook descendant survived cancellation");
    assert!(process_is_live(sentinel_pid), "cancellation signalled unrelated process");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["ls-files", "--stage", "-z"]), staged);
    assert_eq!(fixture.git(&["diff", "--cached", "--binary"]), cached_diff);
    assert_eq!(fixture.git(&["write-tree"]), staged_tree);
    assert_eq!(read(&fixture.path().join("f.txt")), worktree);
    assert_eq!(read(&fixture.path().join("new.txt")), new_worktree);
    std::fs::remove_file(fixture.path().join(".git/hooks/pre-commit")).unwrap();
    repo.commit(&["-m", "follow-up"], &[]).unwrap();
}

#[test]
fn normal_hook_exit_does_not_kill_background_writer_and_reports_drain() {
    let (fixture, repo) = init_repo();
    let marker = fixture.path().join(".git/daemon-ready");
    let pidfile = fixture.path().join(".git/daemon-pid");
    hook(&fixture, "pre-commit", &format!(
        "sh -c 'echo $$ > \"$1\"; touch \"$2\"; exec sleep 30' sh '{}' '{}' &\nchild=$!\necho $child > '{}'.parent\nexit 0",
        pidfile.display(), marker.display(), pidfile.display()
    ));
    write(&fixture.path().join("new.txt"), "staged\n");
    fixture.git(&["add", "new.txt"]);
    let start = Instant::now();
    let result = repo.commit(&["-m", "daemon"], &[]);
    assert!(start.elapsed() < Duration::from_secs(4), "inherited pipes exceeded drain bound");
    let outcome = result.expect("successful commit despite held hook pipes");
    assert!(outcome.warning.as_deref().unwrap().contains("incomplete"));
    wait_marker(&marker);
    let pid: i32 = read(&pidfile).trim().parse().unwrap();
    let start_time = proc_start_time(pid).expect("daemon proc identity");
    let _daemon = OwnedDaemon { pid, start_time };
    let background_pid: i32 = read(&fixture.path().join(".git/daemon-pid.parent")).trim().parse().unwrap();
    assert_eq!(pid, background_pid, "hook must identify the actual background child via $!");
    assert!(process_is_live(pid), "normal exit killed its daemon");
    assert_eq!(fixture.git(&["log", "-1", "--format=%s"]).trim(), "daemon");
    assert!(!fixture.git(&["rev-parse", "HEAD"]).trim().is_empty(), "commit actually reached HEAD");
}
