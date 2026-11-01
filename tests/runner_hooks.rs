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
        .and_then(|stat| {
            stat.rsplit_once(')').map(|(_, tail)| {
                tail.trim_start()
                    .starts_with(|c: char| c != 'Z' && c != 'X')
            })
        })
        .unwrap_or(false)
}

fn proc_start_time(pid: i32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let tail = stat.rsplit_once(')')?.1;
    tail.split_whitespace().nth(19)?.parse().ok()
}

struct OwnedDaemon {
    pid: i32,
    start_time: u64,
}

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
            unsafe {
                libc::kill(self.pid, libc::SIGKILL);
            }
            let until = Instant::now() + Duration::from_secs(2);
            while proc_start_time(self.pid) == Some(self.start_time)
                && process_is_live(self.pid)
                && Instant::now() < until
            {
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

#[test]
fn cancellation_kills_real_hook_process_group_and_preserves_repository() {
    cancel_hook_and_preserve_repository("resistant");
}

#[test]
fn cooperative_real_hook_cancellation_finishes_below_grace() {
    cancel_hook_and_preserve_repository("cooperative");
}

#[test]
fn stopped_real_hook_is_continued_and_terminates() {
    cancel_hook_and_preserve_repository("stopped");
}

fn cancel_hook_and_preserve_repository(mode: &str) {
    let (fixture, repo) = init_repo();
    let marker = fixture.path().join(".git/hook-ready");
    let pidfile = fixture.path().join(".git/hook-child");
    if mode == "resistant" {
        hook(&fixture, "pre-commit", &format!(
        "sh -c 'trap \"\" TERM; echo $$ > \"$1\"; touch \"$2\"; exec sleep 60' sh '{}' '{}' </dev/null >/dev/null 2>&1 &\nwait",
        pidfile.display(), marker.display()
        ));
    } else {
        hook(&fixture, "pre-commit", &format!(
            "trap 'echo terminated > .git/hook-terminated; exit 1' TERM\necho $$ > '{}'\ntouch '{}'\n{}\nwhile :; do sleep 0.02; done",
            pidfile.display(), marker.display(), if mode == "stopped" { "kill -STOP $$" } else { ":" }
        ));
    }
    write(&fixture.path().join("new.txt"), "staged\n");
    fixture.git(&["add", "new.txt"]);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let staged = fixture.git(&["ls-files", "--stage", "-z"]);
    let cached_diff = fixture.git(&["diff", "--cached", "--binary"]);
    let staged_tree = fixture.git(&["write-tree"]);
    let worktree = read(&fixture.path().join("f.txt"));
    let new_worktree = read(&fixture.path().join("new.txt"));
    let sentinel = OwnedChild(
        Command::new("sh")
            .args([
                "-c",
                "trap '' TERM; touch \"$1\"; exec sleep 60",
                "sentinel",
            ])
            .arg(fixture.path().join(".git/sentinel-ready"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let sentinel_pid = sentinel.0.id() as i32;
    let sentinel_start = proc_start_time(sentinel_pid).expect("sentinel process identity");
    let _sentinel_identity = OwnedDaemon {
        pid: sentinel_pid,
        start_time: sentinel_start,
    };
    wait_marker(&fixture.path().join(".git/sentinel-ready"));
    let cancel = CancelHandle::new();
    let worker_cancel = cancel.clone();
    let root = fixture.path().to_path_buf();
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let repo = magit_lsp::git::GitRepo::open(&root).unwrap();
        tx.send(runner::with_cancel(Some(worker_cancel), || {
            repo.commit(&["-m", "hook"], &[])
        }))
        .unwrap();
    });
    wait_marker(&marker);
    let pid: i32 = read(&pidfile).trim().parse().unwrap();
    let child_start = proc_start_time(pid).expect("hook descendant process identity");
    let _child = OwnedDaemon {
        pid,
        start_time: child_start,
    };
    if mode == "stopped" {
        let until = Instant::now() + Duration::from_secs(5);
        while !read(&std::path::PathBuf::from(format!("/proc/{pid}/stat")))
            .rsplit_once(')')
            .unwrap()
            .1
            .trim_start()
            .starts_with('T')
        {
            assert!(Instant::now() < until, "hook did not stop");
            thread::sleep(Duration::from_millis(10));
        }
    }
    let start = Instant::now();
    cancel.cancel();
    assert!(rx.recv_timeout(Duration::from_secs(8)).unwrap().is_err());
    assert!(start.elapsed() < Duration::from_secs(8));
    worker.join().unwrap();
    if mode == "resistant" {
        assert!(
            start.elapsed() >= Duration::from_secs(2),
            "live resistant member lost TERM grace"
        );
    } else {
        assert!(
            start.elapsed() < Duration::from_millis(1200),
            "cooperative cleanup consumed mandatory grace: {:?}",
            start.elapsed()
        );
        assert!(
            fixture.path().join(".git/hook-terminated").exists(),
            "TERM handler did not run after CONT"
        );
    }
    let until = Instant::now() + Duration::from_secs(3);
    while process_is_live(pid) && Instant::now() < until {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !process_is_live(pid),
        "hook descendant survived cancellation"
    );
    assert!(
        process_is_live(sentinel_pid),
        "cancellation signalled unrelated process"
    );
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
    assert!(
        start.elapsed() < Duration::from_secs(4),
        "inherited pipes exceeded drain bound"
    );
    let outcome = result.expect("successful commit despite held hook pipes");
    assert!(outcome.warning.as_deref().unwrap().contains("incomplete"));
    wait_marker(&marker);
    let pid: i32 = read(&pidfile).trim().parse().unwrap();
    let start_time = proc_start_time(pid).expect("daemon proc identity");
    let _daemon = OwnedDaemon { pid, start_time };
    let background_pid: i32 = read(&fixture.path().join(".git/daemon-pid.parent"))
        .trim()
        .parse()
        .unwrap();
    assert_eq!(
        pid, background_pid,
        "hook must identify the actual background child via $!"
    );
    assert!(process_is_live(pid), "normal exit killed its daemon");
    assert_eq!(fixture.git(&["log", "-1", "--format=%s"]).trim(), "daemon");
    assert!(
        !fixture.git(&["rev-parse", "HEAD"]).trim().is_empty(),
        "commit actually reached HEAD"
    );
}

#[test]
fn cancelling_post_commit_reports_the_commit_that_git_already_recorded() {
    let (fixture, repo) = init_repo();
    let marker = fixture.path().join(".git/post-ready");
    hook(
        &fixture,
        "post-commit",
        &format!("touch '{}'\nwhile :; do sleep 0.02; done", marker.display()),
    );
    write(&fixture.path().join("new.txt"), "staged\n");
    fixture.git(&["add", "new.txt"]);
    let before = fixture.git(&["rev-parse", "HEAD"]);
    let tree = fixture.git(&["write-tree"]);
    let worktree = read(&fixture.path().join("f.txt"));
    let cancel = CancelHandle::new();
    let worker_cancel = cancel.clone();
    let root = fixture.path().to_path_buf();
    let worker = thread::spawn(move || {
        runner::with_cancel(Some(worker_cancel), || {
            magit_lsp::git::GitRepo::open(root)
                .unwrap()
                .commit(&["-m", "recorded"], &[])
        })
    });
    wait_marker(&marker);
    assert_ne!(
        fixture.git(&["rev-parse", "HEAD"]),
        before,
        "post-commit starts after the ref update"
    );
    cancel.cancel();
    let outcome = worker
        .join()
        .unwrap()
        .expect("cancellation cannot undo a recorded commit");
    assert_eq!(outcome.message.as_deref(), Some("recorded"));
    assert!(outcome
        .warning
        .as_deref()
        .unwrap()
        .contains("hook may have been interrupted"));
    assert_eq!(fixture.git(&["rev-parse", "HEAD^"]), before);
    assert_eq!(fixture.git(&["rev-parse", "HEAD^{tree}"]), tree);
    assert_eq!(fixture.git(&["write-tree"]), tree);
    assert!(fixture.git(&["diff", "--cached", "--binary"]).is_empty());
    assert_eq!(read(&fixture.path().join("f.txt")), worktree);
    assert_eq!(read(&fixture.path().join("new.txt")), "staged\n");
    std::fs::remove_file(fixture.path().join(".git/hooks/post-commit")).unwrap();
    write(&fixture.path().join("new.txt"), "next\n");
    fixture.git(&["add", "new.txt"]);
    repo.commit(&["-m", "follow-up"], &[]).unwrap();
}

#[test]
fn cancellation_warns_without_removing_gitdir_or_common_dir_locks() {
    for linked in [false, true] {
        let (fixture, _) = init_repo();
        let linked_dir = tempfile::tempdir().unwrap();
        let root = if linked {
            fixture.git(&[
                "worktree",
                "add",
                "-b",
                "linked",
                linked_dir.path().to_str().unwrap(),
            ]);
            linked_dir.path()
        } else {
            fixture.path()
        };
        let git = |args: &[&str]| {
            let out = Command::new("git")
                .current_dir(root)
                .args(args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8(out.stdout).unwrap()
        };
        let gitdir = std::path::PathBuf::from(git(&["rev-parse", "--absolute-git-dir"]).trim());
        let common = root.join(git(&["rev-parse", "--git-common-dir"]).trim());
        let local_lock = gitdir.join("HEAD.lock");
        let common_lock = common.join("packed-refs.lock");
        let ref_lock = common.join("refs/heads/advisory.lock");
        let marker = gitdir.join("ready");
        hook(&fixture, "pre-commit", &format!(
            "printf local > '{}'\nprintf common > '{}'\nprintf ref > '{}'\ntouch '{}'\nwhile :; do sleep 0.02; done",
            local_lock.display(), common_lock.display(), ref_lock.display(), marker.display()
        ));
        write(&root.join("new.txt"), "staged\n");
        git(&["add", "new.txt"]);
        let head = git(&["rev-parse", "HEAD"]);
        let index = git(&["ls-files", "--stage", "-z"]);
        let tree = git(&["write-tree"]);
        let cancel = CancelHandle::new();
        let worker_cancel = cancel.clone();
        let worker_root = root.to_path_buf();
        let worker = thread::spawn(move || {
            runner::with_cancel(Some(worker_cancel), || {
                magit_lsp::git::GitRepo::open(worker_root)
                    .unwrap()
                    .commit(&["-m", "blocked"], &[])
            })
        });
        wait_marker(&marker);
        cancel.cancel();
        let error = worker
            .join()
            .unwrap()
            .expect_err("pre-commit cancellation must not commit");
        let effect = magit_lsp::protocol::Effect::from_error(&error).expect("readable Git failure");
        let rendered = serde_json::to_value(effect).unwrap();
        let warning = rendered["hint"].as_str().expect("production lock warning");
        assert!(warning.contains("no locks were removed"), "{warning}");
        for (path, bytes) in [
            (&local_lock, "local"),
            (&common_lock, "common"),
            (&ref_lock, "ref"),
        ] {
            assert!(
                warning.contains(path.to_str().unwrap()),
                "missing {}: {warning}",
                path.display()
            );
            assert_eq!(
                read(path),
                bytes,
                "advisory must leave foreign locks untouched"
            );
        }
        assert_eq!(git(&["rev-parse", "HEAD"]), head);
        assert_eq!(git(&["ls-files", "--stage", "-z"]), index);
        assert_eq!(git(&["write-tree"]), tree);
        assert_eq!(read(&root.join("new.txt")), "staged\n");
        // These locks were deliberately supplied by the test, not left by Git.
        for path in [&local_lock, &common_lock, &ref_lock] {
            std::fs::remove_file(path).unwrap();
        }
        std::fs::remove_file(fixture.path().join(".git/hooks/pre-commit")).unwrap();
        magit_lsp::git::GitRepo::open(root)
            .unwrap()
            .commit(&["-m", "follow-up"], &[])
            .unwrap();
    }
}

#[test]
fn lock_warning_probe_has_its_own_bounded_readonly_deadline() {
    const CHILD: &str = "MAGIT_TEST_LOCK_PROBE";
    if std::env::var_os(CHILD).is_none() {
        let dir = tempfile::tempdir().unwrap();
        let real = Command::new("sh")
            .args(["-c", "command -v git"])
            .output()
            .unwrap();
        assert!(real.status.success());
        let real = String::from_utf8(real.stdout).unwrap();
        let wrapper = dir.path().join("git");
        write(&wrapper, &format!("#!/bin/sh\nif [ \"$*\" = '--no-optional-locks rev-parse --absolute-git-dir --git-common-dir' ]; then\n trap '' TERM\n echo $$ > \"$MAGIT_TEST_PROBE_PID\"\n exec sleep 60\nfi\nexec '{}' \"$@\"\n", real.trim()));
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "lock_warning_probe_has_its_own_bounded_readonly_deadline",
            ])
            .env(CHILD, "1")
            .env("MAGIT_TEST_PROBE_PID", dir.path().join("probe-pid"))
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    dir.path().display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let (fixture, _) = init_repo();
    let ready = fixture.path().join(".git/hook-ready");
    hook(
        &fixture,
        "pre-commit",
        &format!("touch '{}'\nwhile :; do sleep 0.02; done", ready.display()),
    );
    write(&fixture.path().join("new.txt"), "staged\n");
    fixture.git(&["add", "new.txt"]);
    let cancel = CancelHandle::new();
    let worker_cancel = cancel.clone();
    let root = fixture.path().to_path_buf();
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let result = runner::with_cancel(Some(worker_cancel), || {
            magit_lsp::git::GitRepo::open(root)
                .unwrap()
                .commit(&["-m", "cancel"], &[])
        });
        tx.send(result).unwrap();
    });
    wait_marker(&ready);
    let started = Instant::now();
    cancel.cancel();
    let error = rx
        .recv_timeout(Duration::from_secs(8))
        .expect("bounded probe plus owner cleanup")
        .unwrap_err();
    worker.join().unwrap();
    assert!(
        started.elapsed() >= Duration::from_millis(2500),
        "diagnostic must use its own 500ms deadline and full resistant-member grace"
    );
    assert!(error
        .git
        .unwrap()
        .warning
        .unwrap()
        .contains("no locks were removed"));
    let pid: i32 = read(&std::path::PathBuf::from(
        std::env::var_os("MAGIT_TEST_PROBE_PID").unwrap(),
    ))
    .trim()
    .parse()
    .unwrap();
    assert!(
        !process_is_live(pid),
        "diagnostic child survived its own deadline"
    );
}
