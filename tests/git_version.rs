mod common;

use common::stdio::TestServer;
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::TempDir;

struct FakeGit {
    _dir: TempDir,
    path: String,
    root: String,
    version_log: String,
}

fn fake_git(version: &str, fail_version_probe: bool, fail_git: bool) -> FakeGit {
    let dir = TempDir::new().expect("fake git directory");
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).expect("create fake bin");
    let root = dir.path().join("repo");
    std::fs::create_dir_all(&root).expect("create fake repo");
    let version_log = dir.path().join("versions");
    let fake = bin.join("git");
    let probe = if fail_version_probe {
        "exit 1".to_string()
    } else if version == "hang" {
        "echo $$ > \"$GIT_VERSION_LOG.pid\"; while :; do :; done".to_string()
    } else {
        format!("echo 'git version {version}'")
    };
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo probe >> \"$GIT_VERSION_LOG\"; {} ; exit 0; fi\n{}\n",
        probe,
        if fail_git {
            "echo 'fatal: preserved fake failure' >&2; exit 17"
        } else {
            "if [ \"$1\" = --no-optional-locks ]; then shift; fi; case \"$1\" in rev-parse) echo \"$FAKE_GIT_ROOT\";; status) printf '# branch.oid (initial)\\0# branch.head main\\0';; *) exit 0;; esac"
        }
    );
    std::fs::write(&fake, script).expect("write fake git");
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755))
        .expect("make fake git executable");
    FakeGit {
        _dir: dir,
        path: bin.to_string_lossy().into_owned(),
        root: root.to_string_lossy().into_owned(),
        version_log: version_log.to_string_lossy().into_owned(),
    }
}

fn open_status(server: &mut TestServer, root: &Path) -> Value {
    server.initialize(json!({}));
    server.send(json!({
        "jsonrpc":"2.0", "id":2, "method":"view/open",
        "params":{"kind":"status", "args":{"repo_path":root}, "viewport":{"cols":80,"rows":24}}
    }));
    server.response_with_id(2)
}

fn env(fake: &FakeGit) -> Vec<(&'static str, String)> {
    vec![
        (
            "PATH",
            format!(
                "{}:{}",
                fake.path,
                std::env::var("PATH").unwrap_or_default()
            ),
        ),
        ("FAKE_GIT_ROOT", fake.root.clone()),
        ("GIT_VERSION_LOG", fake.version_log.clone()),
    ]
}

fn finish(server: TestServer) -> String {
    let (_, stderr) = server.finish_with_stderr(std::time::Duration::from_secs(10));
    stderr
}

#[test]
fn older_git_failure_warns_and_preserves_original_error() {
    let fake = fake_git("2.19.6 (vendor)", false, true);
    let env = env(&fake);
    let refs = env
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect::<Vec<_>>();
    let mut server = TestServer::spawn(&refs);
    let opened = open_status(&mut server, Path::new(&fake.root));
    assert!(opened["error"]["data"]["detail"]
        .as_str()
        .unwrap_or("")
        .contains("preserved fake failure"));
    server.send(json!({"jsonrpc":"2.0", "id":99, "method":"shutdown", "params":null}));
    server.response_with_id(99);
    server.send(json!({"jsonrpc":"2.0", "method":"exit", "params":null}));
    let stderr = finish(server);
    assert!(
        stderr.contains("installed=git version 2.19.6 (vendor)"),
        "{stderr}"
    );
    assert!(stderr.contains("recommended=\"2.30.8\""), "{stderr}");
    assert_eq!(
        std::fs::read_to_string(&fake.version_log)
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[test]
fn equal_or_newer_git_failure_does_not_warn() {
    for version in ["2.30.8", "2.30.8.windows.1", "2.55.0", "2.100.0"] {
        let fake = fake_git(version, false, true);
        let env = env(&fake);
        let refs = env
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
            .collect::<Vec<_>>();
        let mut server = TestServer::spawn(&refs);
        let opened = open_status(&mut server, Path::new(&fake.root));
        assert!(opened["error"]["data"]["detail"]
            .as_str()
            .unwrap_or("")
            .contains("preserved fake failure"));
        server.send(json!({"jsonrpc":"2.0", "id":99, "method":"shutdown", "params":null}));
        server.response_with_id(99);
        server.send(json!({"jsonrpc":"2.0", "method":"exit", "params":null}));
        assert!(!finish(server).contains("older-than-recommended"));
    }
}

#[test]
fn older_git_success_does_not_probe_or_warn() {
    let fake = fake_git("2.19.6", false, false);
    let env = env(&fake);
    let refs = env
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect::<Vec<_>>();
    let mut server = TestServer::spawn(&refs);
    let opened = open_status(&mut server, Path::new(&fake.root));
    assert!(
        opened["result"]["kind"].as_str() == Some("status"),
        "{opened}"
    );
    server.send(json!({"jsonrpc":"2.0", "id":99, "method":"shutdown", "params":null}));
    server.response_with_id(99);
    server.send(json!({"jsonrpc":"2.0", "method":"exit", "params":null}));
    assert!(!finish(server).contains("older-than-recommended"));
    assert!(!Path::new(&fake.version_log).exists());
}

#[test]
fn unavailable_or_unparseable_probe_preserves_failure_without_recursion() {
    for (version, fail_probe) in [("", true), ("not a git version", false), ("hang", false)] {
        let fake = fake_git(version, fail_probe, true);
        let env = env(&fake);
        let refs = env
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
            .collect::<Vec<_>>();
        let mut server = TestServer::spawn(&refs);
        let started = std::time::Instant::now();
        let opened = open_status(&mut server, Path::new(&fake.root));
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
        assert!(opened["error"]["data"]["detail"]
            .as_str()
            .unwrap_or("")
            .contains("preserved fake failure"));
        server.send(json!({"jsonrpc":"2.0", "id":99, "method":"shutdown", "params":null}));
        server.response_with_id(99);
        server.send(json!({"jsonrpc":"2.0", "method":"exit", "params":null}));
        assert!(!finish(server).contains("older-than-recommended"));
        assert_eq!(
            std::fs::read_to_string(&fake.version_log)
                .unwrap()
                .lines()
                .count(),
            1
        );
        if version == "hang" {
            let pid = std::fs::read_to_string(format!("{}.pid", fake.version_log)).unwrap();
            assert!(
                !Path::new(&format!("/proc/{}", pid.trim())).exists(),
                "probe not reaped"
            );
        }
    }
}

#[test]
fn backend_failure_child() {
    let Ok(root) = std::env::var("VERSION_TEST_BACKEND_ROOT") else {
        return;
    };
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_writer(std::io::stderr)
        .init();
    let repo = magit_lsp::git::GitRepo::open(root).unwrap();
    let error = if std::env::var("VERSION_TEST_APPLY").unwrap() != "no" {
        let hunks = repo.file_hunks("f.txt", false).unwrap();
        repo.stage_hunk("f.txt", &hunks[0].id).unwrap_err()
    } else {
        repo.stage_file("f.txt").unwrap_err()
    };
    let failure = error.git.as_ref().expect("structured original failure");
    assert_eq!(failure.exit_code, 17);
    assert_eq!(failure.stderr, "fatal: preserved fake failure");
    let expected = if failure.subcommand == "apply" {
        vec!["apply", "--cached", "--unidiff-zero", "--recount"]
    } else {
        vec!["add", "--", "f.txt"]
    };
    assert_eq!(failure.args, expected);
    assert_eq!(
        error.to_string(),
        format!(
            "git {} failed: fatal: preserved fake failure",
            expected.join(" ")
        )
    );
}

#[test]
fn normal_and_piped_apply_failures_keep_original_details() {
    let real_git = std::process::Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    let real_git = String::from_utf8(real_git.stdout).unwrap();
    for apply in ["no", "yes", "early"] {
        let fake = fake_git("2.19.6.windows.1", false, false);
        let root = Path::new(&fake.root);
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.email", "t@example.com"],
            vec!["config", "user.name", "t"],
        ] {
            assert!(std::process::Command::new(real_git.trim())
                .args(args)
                .current_dir(root)
                .status()
                .unwrap()
                .success());
        }
        std::fs::write(root.join("f.txt"), "before\n").unwrap();
        for args in [
            ["add", "f.txt"].as_slice(),
            ["commit", "-qm", "initial"].as_slice(),
        ] {
            assert!(std::process::Command::new(real_git.trim())
                .args(args)
                .current_dir(root)
                .status()
                .unwrap()
                .success());
        }
        std::fs::write(
            root.join("f.txt"),
            if apply == "early" {
                "after\n".repeat(20000)
            } else {
                "after\n".to_string()
            },
        )
        .unwrap();
        std::fs::write(Path::new(&fake.path).join("git"), "#!/bin/sh\nif [ \"$1\" = --version ]; then echo probe >> \"$GIT_VERSION_LOG\"; echo 'git version 2.19.6.windows.1'; exit 0; fi\ncase \"$1\" in add|apply) if [ \"$1\" = apply ] && [ \"$VERSION_TEST_APPLY\" != early ]; then cat >/dev/null; fi; echo 'fatal: preserved fake failure' >&2; exit 17;; esac\nexec \"$REAL_GIT\" \"$@\"\n").unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "backend_failure_child", "--nocapture"])
            .envs(env(&fake))
            .env("REAL_GIT", real_git.trim())
            .env("VERSION_TEST_BACKEND_ROOT", root)
            .env("VERSION_TEST_APPLY", apply)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "{stderr}\n{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(stderr.contains("older-than-recommended"), "{stderr}");
        assert!(
            stderr.contains("2.19.6.windows.1") && stderr.contains("2.30.8"),
            "{stderr}"
        );
        assert_eq!(
            std::fs::read_to_string(&fake.version_log)
                .unwrap()
                .lines()
                .count(),
            1
        );
    }
}
