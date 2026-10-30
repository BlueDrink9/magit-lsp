use std::path::{Path, PathBuf};
use std::process::Command;

fn git(path: &Path, args: &[&str]) {
    std::fs::create_dir_all(path).unwrap();
    let status = Command::new("git").args(args).current_dir(path).status().unwrap();
    assert!(status.success(), "git {args:?}");
}

fn make_repo(path: &Path) {
    git(path, &["init", "-q"]);
    git(path, &["-c", "user.name=test", "-c", "user.email=test@invalid", "commit", "--allow-empty", "-qm", "initial"]);
}

fn wrapper(dir: &Path, real_git: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join("git");
    std::fs::write(&path, format!("#!/bin/sh\nfor arg in \"$@\"; do [ \"$arg\" = --path-format=absolute ] && {{ echo 'unsupported --path-format' >&2; exit 129; }}; done\nif [ \"$GIT_DISCOVERY_MODE\" = failure ] && [ \"$1\" = rev-parse ]; then echo 'simulated git execution failure' >&2; exit 23; fi\nexec '{}' \"$@\"\n", real_git.display())).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn child(path: &Path, wrapper_dir: &Path, mode: &str) {
    let old_path = std::env::var_os("PATH").unwrap();
    let mut paths = vec![wrapper_dir.to_path_buf()];
    paths.extend(std::env::split_paths(&old_path));
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_checks_discovery", "--nocapture"])
        .env("PATH", std::env::join_paths(paths).unwrap())
        .env("GIT_DISCOVERY_MODE", mode)
        .env("GIT_DISCOVERY_PATH", path)
        .output().unwrap();
    assert!(output.status.success(), "child failed: {}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn recommended_git_queries_discover_and_open_worktrees_and_propagate_failures() {
    let temp = tempfile::tempdir().unwrap();
    let normal = temp.path().join("normal");
    make_repo(&normal);
    let nested = normal.join("nested");
    std::fs::create_dir(&nested).unwrap();
    let linked = temp.path().join("linked");
    git(&normal, &["worktree", "add", "-q", "-b", "linked", linked.to_str().unwrap()]);
    let real_git = std::fs::canonicalize("/usr/bin/git").unwrap();
    let wrapper_dir = temp.path().join("wrapper");
    wrapper(&wrapper_dir, &real_git);
    child(&normal, &wrapper_dir, "compatible");
    child(&linked, &wrapper_dir, "compatible");
    child(&temp.path().join("not-a-repo"), &wrapper_dir, "failure");
}

#[test]
fn child_checks_discovery() {
    let Ok(raw) = std::env::var("GIT_DISCOVERY_PATH") else { return };
    let path = PathBuf::from(raw);
    if std::env::var("GIT_DISCOVERY_MODE").unwrap() == "failure" {
        let error = match magit_lsp::git::GitRepo::discover(&path) { Err(error) => error, Ok(_) => panic!("simulated Git failure was swallowed") };
        assert!(error.message.contains("failed") || error.git.is_some(), "{}", error.message);
        assert!(magit_lsp::git::GitRepo::open(&path).is_err());
        return;
    }
    let expected_root = std::fs::canonicalize(&path).unwrap();
    let git_marker = expected_root.join(".git");
    let expected_git = if git_marker.is_file() {
        let value = std::fs::read_to_string(&git_marker).unwrap();
        std::fs::canonicalize(expected_root.join(value.trim().strip_prefix("gitdir: ").unwrap())).unwrap()
    } else { git_marker };
    let expected_common = if expected_git.join("commondir").exists() {
        let value = std::fs::read_to_string(expected_git.join("commondir")).unwrap();
        std::fs::canonicalize(expected_git.join(value.trim())).unwrap()
    } else { expected_git.clone() };
    let discovered = magit_lsp::git::GitRepo::discover(&path).unwrap().unwrap();
    let opened = magit_lsp::git::GitRepo::open(&path).unwrap();
    assert_eq!(discovered.root(), expected_root);
    assert_eq!(opened.root(), expected_root);
    assert_eq!(discovered.git_dir(), expected_git);
    assert_eq!(opened.git_dir(), discovered.git_dir());
    assert_eq!(discovered.common_dir(), expected_common);
    assert_eq!(opened.common_dir(), discovered.common_dir());
    let nested = expected_root.join("nested");
    if nested.exists() {
        let nested_repo = magit_lsp::git::GitRepo::discover(&nested).unwrap().unwrap();
        assert_eq!(nested_repo.root(), expected_root);
    }
}
