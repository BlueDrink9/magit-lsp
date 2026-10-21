use crate::patch::{self, Hunk};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A single changed file: its display status char and its diff hunks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub status: String,
    pub hunks: Vec<Hunk>,
}

/// Repository status: branch plus the staged and unstaged file lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub repo_path: String,
    pub branch: String,
    pub staged: Vec<FileEntry>,
    pub unstaged: Vec<FileEntry>,
}

/// A Git repository driven through the `git` CLI.
pub struct GitRepo {
    root: PathBuf,
}

impl GitRepo {
    /// Open the repository containing `path`, resolving its working-tree root.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let cwd = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent().unwrap_or(path).to_path_buf()
        };
        let out = Self::command_in(&cwd, &["rev-parse", "--show-toplevel"])?;
        let root = out.trim().to_string();
        if root.is_empty() {
            return Err(Error("not inside a git repository".into()));
        }
        Ok(Self {
            root: PathBuf::from(root),
        })
    }

    /// The repository's working-tree root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Compute the full status: branch, staged files, and unstaged files.
    pub fn status(&self) -> Result<Status> {
        let repo_path = self.root.to_string_lossy().into_owned();
        let branch = self
            .current_branch()
            .unwrap_or_else(|_| "HEAD".to_string());
        let raw = self.run(&["status", "--porcelain=v1", "-z"])?;

        let mut staged: Vec<FileEntry> = Vec::new();
        let mut unstaged: Vec<FileEntry> = Vec::new();

        let mut tokens = raw.split('\0');
        while let Some(entry) = tokens.next() {
            if entry.len() < 4 {
                continue;
            }
            let xy = &entry[..2];
            let path = entry[3..].to_string();
            if xy == "??" {
                unstaged.push(FileEntry {
                    status: "?".into(),
                    hunks: Vec::new(),
                    path,
                });
                continue;
            }
            if xy.starts_with('R') || xy.starts_with('C') {
                let _orig = tokens.next().unwrap_or("").to_string();
            }
            let x = xy.as_bytes()[0];
            let y = xy.as_bytes()[1];
            if x != b' ' {
                let hunks = self.hunks_for(path.as_str(), true)?;
                staged.push(FileEntry {
                    status: (x as char).to_string(),
                    hunks,
                    path: path.clone(),
                });
            }
            if y != b' ' {
                let hunks = self.hunks_for(path.as_str(), false)?;
                unstaged.push(FileEntry {
                    status: (y as char).to_string(),
                    hunks,
                    path,
                });
            }
        }

        Ok(Status {
            repo_path,
            branch,
            staged,
            unstaged,
        })
    }

    /// Current branch name, or `HEAD` when detached.
    pub fn current_branch(&self) -> Result<String> {
        let out = self.run(&["rev-parse", "--abbrev-ref", "HEAD"])?;
        Ok(out.trim().to_string())
    }

    /// Unified diff of the unstaged changes to `path` (index vs working tree).
    pub fn diff_unstaged(&self, path: &str) -> Result<String> {
        self.run(&["diff", "-U3", "--", path])
    }

    /// Unified diff of the staged changes to `path` (HEAD vs index).
    pub fn diff_staged(&self, path: &str) -> Result<String> {
        self.run(&["diff", "--cached", "-U3", "--", path])
    }

    /// Stage the whole unstaged hunk at `hunk_index` for `path`.
    pub fn stage_hunk(&self, path: &str, hunk_index: usize) -> Result<()> {
        let hunks = self.unstaged_hunks(path)?;
        let hunk = hunks
            .get(hunk_index)
            .ok_or_else(|| Error(format!("hunk {hunk_index} not found for {path}")))?;
        let patch = file_patch(path, &patch::full_hunk_patch(hunk), false);
        self.apply_index(&patch, false)
    }

    /// Unstage the whole staged hunk at `hunk_index` for `path`.
    pub fn unstage_hunk(&self, path: &str, hunk_index: usize) -> Result<()> {
        let hunks = self.staged_hunks(path)?;
        let hunk = hunks
            .get(hunk_index)
            .ok_or_else(|| Error(format!("hunk {hunk_index} not found for {path}")))?;
        let patch = file_patch(path, &patch::full_hunk_patch(hunk), is_new_file(hunk));
        self.apply_index(&patch, true)
    }

    /// Stage the lines in `start..=end` of the unstaged hunk at `hunk_index`.
    pub fn stage_lines(&self, path: &str, hunk_index: usize, start: usize, end: usize) -> Result<()> {
        let hunks = self.unstaged_hunks(path)?;
        let hunk = hunks.get(hunk_index).ok_or_else(|| {
            Error(format!("hunk {hunk_index} not found for {path}"))
        })?;
        let patch = file_patch(path, &patch::subset_hunk_patch(hunk, start, end), false);
        self.apply_index(&patch, false)
    }

    /// Unstage the lines in `start..=end` of the staged hunk at `hunk_index`.
    pub fn unstage_lines(
        &self,
        path: &str,
        hunk_index: usize,
        start: usize,
        end: usize,
    ) -> Result<()> {
        let hunks = self.staged_hunks(path)?;
        let hunk = hunks.get(hunk_index).ok_or_else(|| {
            Error(format!("hunk {hunk_index} not found for {path}"))
        })?;
        let patch = file_patch(path, &patch::subset_hunk_patch(hunk, start, end), false);
        self.apply_index(&patch, true)
    }

    /// Stage the entire file, including untracked files.
    pub fn stage_file(&self, path: &str) -> Result<()> {
        self.run(&["add", "--", path]).map(|_| ())
    }

    /// Unstage the entire file, leaving the working tree untouched.
    pub fn unstage_file(&self, path: &str) -> Result<()> {
        self.unstage_paths(&[path])
    }

    /// Stage all tracked modifications and deletions.
    pub fn stage_all(&self) -> Result<()> {
        self.run(&["add", "-u"]).map(|_| ())
    }

    /// Unstage everything, including repositories without a commit yet.
    pub fn unstage_all(&self) -> Result<()> {
        self.unstage_paths(&["."])
    }

    /// Unstage `paths`, leaving the working tree untouched. Falls back to
    /// `git rm --cached` because `git reset` needs a resolvable HEAD.
    fn unstage_paths(&self, paths: &[&str]) -> Result<()> {
        let mut reset = vec!["reset", "-q", "--"];
        reset.extend_from_slice(paths);
        if self.run(&reset).is_ok() {
            return Ok(());
        }

        let mut rm = vec!["rm", "--cached", "-q", "-f", "--ignore-unmatch", "--"];
        rm.extend_from_slice(paths);
        self.run(&rm).map(|_| ())
    }

    fn unstaged_hunks(&self, path: &str) -> Result<Vec<Hunk>> {
        Ok(patch::parse_unified_diff(&self.diff_unstaged(path)?))
    }

    fn staged_hunks(&self, path: &str) -> Result<Vec<Hunk>> {
        Ok(patch::parse_unified_diff(&self.diff_staged(path)?))
    }

    fn hunks_for(&self, path: &str, staged: bool) -> Result<Vec<Hunk>> {
        let diff = if staged {
            self.diff_staged(path)?
        } else {
            self.diff_unstaged(path)?
        };
        Ok(patch::parse_unified_diff(&diff))
    }

    fn apply_index(&self, patch_text: &str, reverse: bool) -> Result<()> {
        // `--unidiff-zero` lets line subsets of a new file apply, where the
        // hunk has no context lines to anchor it.
        let mut args: Vec<&str> = vec!["apply", "--cached", "--unidiff-zero"];
        if reverse {
            args.push("-R");
        }
        let mut child = Command::new("git")
            .args(&args)
            .current_dir(&self.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error(format!("failed to spawn git apply: {e}")))?;
        child
            .stdin
            .as_mut()
            .ok_or_else(|| Error("failed to open stdin".into()))?
            .write_all(patch_text.as_bytes())?;
        let out = child.wait_with_output()?;
        if !out.status.success() {
            return Err(Error(format!(
                "git apply failed: {}",
                String::from_utf8_lossy(&out.stderr)
            )));
        }
        Ok(())
    }

    fn run(&self, args: &[&str]) -> Result<String> {
        Self::command_in(&self.root, args)
    }

    fn command_in(cwd: &Path, args: &[&str]) -> Result<String> {
        let out = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .map_err(|e| Error(format!("failed to run git: {e}")))?;
        if !out.status.success() {
            return Err(Error(format!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr)
            )));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

fn file_patch(path: &str, hunk_patch: &str, new_file: bool) -> String {
    if new_file {
        // Reversing a new file must target `/dev/null`, otherwise `git apply -R`
        // leaves an empty file in the index instead of removing the entry.
        format!(
            "diff --git a/{path} b/{path}\nnew file mode 100644\n--- /dev/null\n+++ b/{path}\n{hunk_patch}"
        )
    } else {
        format!("diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n{hunk_patch}")
    }
}

fn is_new_file(h: &Hunk) -> bool {
    h.old_start == 0 && h.old_count == 0
}
