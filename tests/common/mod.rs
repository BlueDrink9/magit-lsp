//! Shared fixtures and helpers for the integration tests.
//!
//! Every git invocation made here first applies [`apply_determinism_env`], so
//! commit hashes and diff output are stable across machines.

#![allow(dead_code)]

use magit_lsp::git::GitRepo;
use std::path::Path;
use std::process::Command;
use std::sync::Once;
use tempfile::TempDir;

/// Ten numbered lines, the content most fixtures start from.
pub const TEN_LINES: &str =
    "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\n";

const FIXED_NAME: &str = "Test User";
const FIXED_EMAIL: &str = "test@example.com";
const FIXED_DATE: &str = "2000-01-01T00:00:00+0000";

static DETERMINISM: Once = Once::new();

/// Make spawned `git` deterministic: fixed identity and dates, no system
/// config, an isolated `HOME`/`XDG_CONFIG_HOME`, and `LC_ALL=C`.
///
/// The environment is process-global, so this runs once. Tests only rely on it
/// being constant, never on a particular value.
pub fn apply_determinism_env() {
    DETERMINISM.call_once(|| {
        let home = TempDir::new().expect("tempdir for HOME").keep();
        std::fs::create_dir_all(home.join("config")).expect("create XDG config dir");
        std::env::set_var("HOME", &home);
        std::env::set_var("XDG_CONFIG_HOME", home.join("config"));
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
        std::env::set_var("LC_ALL", "C");
        std::env::set_var("GIT_AUTHOR_NAME", FIXED_NAME);
        std::env::set_var("GIT_AUTHOR_EMAIL", FIXED_EMAIL);
        std::env::set_var("GIT_COMMITTER_NAME", FIXED_NAME);
        std::env::set_var("GIT_COMMITTER_EMAIL", FIXED_EMAIL);
        std::env::set_var("GIT_AUTHOR_DATE", FIXED_DATE);
        std::env::set_var("GIT_COMMITTER_DATE", FIXED_DATE);
    });
}

/// Run `git` in `dir`, panicking with its stderr when it fails.
pub fn git(dir: &Path, args: &[&str]) -> String {
    apply_determinism_env();
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run git");
    if !out.status.success() {
        panic!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Write `content` to `path`, creating any missing parent directories.
pub fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent directories");
    }
    std::fs::write(path, content).expect("write file");
}

/// Read `path` into a string.
pub fn read(path: &Path) -> String {
    std::fs::read_to_string(path).expect("read file")
}

/// `git diff --cached -U3` for `f.txt`.
pub fn cached_diff(dir: &Path) -> String {
    git(dir, &["diff", "--cached", "-U3", "--", "f.txt"])
}

/// Number of hunks in a unified diff.
pub fn hunk_count(diff: &str) -> usize {
    diff.lines().filter(|line| line.starts_with("@@")).count()
}

/// `n` numbered lines, each newline terminated.
pub fn numbered_lines(n: usize) -> String {
    (1..=n).map(|i| format!("line{i}\n")).collect()
}

/// The worktree content produced by [`make_standard_changes`].
pub fn standard_changes() -> String {
    let mut lines: Vec<String> = (1..=10).map(|i| format!("line{i}")).collect();
    lines[2] = "line3 CHANGED".into();
    lines.insert(5, "NEW line".into());
    lines.remove(8);
    lines.join("\n") + "\n"
}

/// The worktree content produced by [`make_two_hunks`].
pub fn two_hunk_changes() -> String {
    let mut lines: Vec<String> = (1..=40).map(|i| format!("line{i}")).collect();
    lines[4] = "line5 CHANGED".into();
    lines[34] = "line35 CHANGED".into();
    lines.join("\n") + "\n"
}

/// Apply three interleaved edits to `f.txt` without staging them.
pub fn make_standard_changes(tmp: &Path) {
    write(&tmp.join("f.txt"), &standard_changes());
}

/// Change lines 5 and 35 of a 40-line `f.txt`, producing two hunks.
pub fn make_two_hunks(tmp: &Path) {
    write(&tmp.join("f.txt"), &two_hunk_changes());
}

/// Builder for deterministic temporary repositories.
///
/// Files are written and committed in the order they are added; uncommitted
/// changes are applied last so they stay in the worktree.
pub struct RepoFixture {
    dir: TempDir,
    commits: Vec<(String, Vec<(String, String)>)>,
    renames: Vec<(String, String)>,
    staged: Vec<(String, String)>,
    changes: Vec<(String, String)>,
    unborn: bool,
    detached: bool,
}

impl RepoFixture {
    /// Start a fixture whose repository is created by [`Self::build`].
    pub fn new() -> Self {
        apply_determinism_env();
        Self {
            dir: TempDir::new().expect("tempdir"),
            commits: Vec::new(),
            renames: Vec::new(),
            staged: Vec::new(),
            changes: Vec::new(),
            unborn: false,
            detached: false,
        }
    }

    /// Leave the repository without commits; [`Self::staged_file`] then populates
    /// the index.
    pub fn unborn(mut self) -> Self {
        self.unborn = true;
        self
    }

    /// Detach `HEAD` after the commits are made.
    pub fn detached(mut self) -> Self {
        self.detached = true;
        self
    }

    /// `git mv` `from` to `to` after the commits, staging the rename.
    pub fn rename(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        assert!(!self.unborn, "an unborn fixture cannot have renames");
        self.renames.push((from.into(), to.into()));
        self
    }

    /// Commit `files` (path, content) with `message`.
    pub fn commit(
        mut self,
        message: &str,
        files: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        assert!(!self.unborn, "an unborn fixture cannot have commits");
        self.commits.push((
            message.to_string(),
            files
                .into_iter()
                .map(|(path, content)| (path.into(), content.into()))
                .collect(),
        ));
        self
    }

    /// Write `content` to `path` and stage it without committing.
    pub fn staged_file(mut self, path: impl Into<String>, content: impl Into<String>) -> Self {
        self.staged.push((path.into(), content.into()));
        self
    }

    /// Write `content` to `path` in the worktree without staging it.
    pub fn modify(mut self, path: impl Into<String>, content: impl Into<String>) -> Self {
        self.changes.push((path.into(), content.into()));
        self
    }

    /// Apply [`make_standard_changes`] to `f.txt` without staging it.
    pub fn modify_standard(self) -> Self {
        self.modify("f.txt", standard_changes())
    }

    /// Apply [`make_two_hunks`] to a 40-line `f.txt` without staging it.
    pub fn modify_two_hunks(self) -> Self {
        self.modify("f.txt", two_hunk_changes())
    }

    /// Create the repository on disk and open it.
    pub fn build(self) -> (RepoFixture, GitRepo) {
        apply_determinism_env();
        let path = self.dir.path().to_path_buf();
        git(&path, &["init", "-q", "-b", "main"]);
        git(&path, &["config", "user.email", FIXED_EMAIL]);
        git(&path, &["config", "user.name", FIXED_NAME]);
        for (message, files) in &self.commits {
            for (name, content) in files {
                write(&path.join(name), content);
            }
            git(&path, &["add", "."]);
            git(&path, &["commit", "-q", "-m", message]);
        }
        for (from, to) in &self.renames {
            if let Some(parent) = path.join(to).parent() {
                std::fs::create_dir_all(parent).expect("create rename parent");
            }
            git(&path, &["mv", from.as_str(), to.as_str()]);
        }
        if self.detached {
            git(&path, &["checkout", "-q", "--detach"]);
        }
        for (name, content) in &self.staged {
            write(&path.join(name), content);
            git(&path, &["add", name.as_str()]);
        }
        for (name, content) in &self.changes {
            write(&path.join(name), content);
        }
        let repo = GitRepo::open(&path).expect("open repo");
        (self, repo)
    }

    /// Root directory of the temporary repository.
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Run `git` inside the repository.
    pub fn git(&self, args: &[&str]) -> String {
        git(self.path(), args)
    }
}

/// A repository with a ten-line tracked `f.txt`.
pub fn init_repo() -> (RepoFixture, GitRepo) {
    RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build()
}

/// A repository whose only content is a staged new file (no commits yet).
pub fn init_unborn_repo() -> (RepoFixture, GitRepo) {
    RepoFixture::new()
        .unborn()
        .staged_file("f.txt", "a\nb\nc\n")
        .build()
}

/// A repo with a 30-line file, so two distant edits produce two hunks.
pub fn init_long_repo() -> (RepoFixture, GitRepo) {
    RepoFixture::new()
        .commit("init", [("f.txt", numbered_lines(30))])
        .build()
}

/// A repo with two tracked files, both ten lines long.
pub fn init_two_file_repo() -> (RepoFixture, GitRepo) {
    let body = numbered_lines(10);
    RepoFixture::new()
        .commit("init", [("f.txt", body.clone()), ("g.txt", body)])
        .build()
}

/// A repo with a 40-line tracked file.
pub fn init_40_line_repo() -> (RepoFixture, GitRepo) {
    RepoFixture::new()
        .commit("init", [("f.txt", numbered_lines(40))])
        .build()
}

/// Like [`init_repo`], with a second commit replacing `f.txt` with 40 lines.
pub fn init_repo_40() -> (RepoFixture, GitRepo) {
    RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .commit("forty", [("f.txt", numbered_lines(40))])
        .build()
}

/// Real-server stdio transport shared by integration suites.
pub mod stdio;

/// In-process session harness shared by the session suites.
pub mod session;

#[allow(unused_imports)]
pub use session::{keymaps, message_texts, TestSession, STATUS_VIEWPORT, WIDE_VIEWPORT};
