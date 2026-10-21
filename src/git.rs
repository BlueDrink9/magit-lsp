use crate::patch::{self, FileLayout, Hunk, Selection, SelectionOp};
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
    /// Old path for a staged rename or copy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orig_path: Option<String>,
    pub hunks: Vec<Hunk>,
}

/// Repository status: branch plus the staged and unstaged file lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub repo_path: String,
    pub branch: String,
    /// Commit `HEAD` points at; absent in a repository with no commits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ahead: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behind: Option<u32>,
    pub staged: Vec<FileEntry>,
    pub unstaged: Vec<FileEntry>,
    /// Files Git does not track yet; they have no hunks.
    pub untracked: Vec<FileEntry>,
}

impl Status {
    /// An empty status, used when a path cannot be opened as a repository.
    pub fn empty(repo_path: impl Into<String>) -> Self {
        Self {
            repo_path: repo_path.into(),
            branch: "HEAD".to_string(),
            head: None,
            upstream: None,
            ahead: None,
            behind: None,
            staged: Vec::new(),
            unstaged: Vec::new(),
            untracked: Vec::new(),
        }
    }
}

/// A failed `git` invocation, kept structured so it can be explained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitFailure {
    pub exit_code: i32,
    pub stderr: String,
    pub args: Vec<String>,
    pub subcommand: String,
}

impl GitFailure {
    /// Build a failure from a finished Git invocation.
    pub fn from_output(args: &[&str], status: std::process::ExitStatus, stderr: &[u8]) -> Self {
        Self {
            exit_code: status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(stderr).trim_end().to_string(),
            args: args.iter().map(|arg| arg.to_string()).collect(),
            subcommand: subcommand_of(args).to_string(),
        }
    }
}

/// The Git subcommand in `args`, skipping global options such as `-c key=value`.
fn subcommand_of<'a>(args: &'a [&'a str]) -> &'a str {
    let mut skip_value = false;
    for arg in args {
        if skip_value {
            skip_value = false;
            continue;
        }
        if *arg == "-c" || *arg == "-C" {
            skip_value = true;
            continue;
        }
        if !arg.starts_with('-') {
            return arg;
        }
    }
    "git"
}

/// The status letter for one side of a porcelain-v2 `XY` pair, if it changed.
fn side_status(flag: Option<u8>) -> Option<String> {
    match flag {
        Some(b'.') | Some(b' ') | None => None,
        Some(flag) => Some((flag as char).to_string()),
    }
}

impl std::fmt::Display for GitFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "git {} failed: {}", self.args.join(" "), self.stderr)
    }
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
            return Err(Error::new("not inside a git repository"));
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
        // `-z` keeps paths verbatim; `--no-optional-locks` keeps a read from
        // taking the index lock away from a concurrent mutation.
        let raw = self.run(&[
            "--no-optional-locks",
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
        ])?;
        self.parse_status(&raw, repo_path)
    }

    fn parse_status(&self, raw: &str, repo_path: String) -> Result<Status> {
        let mut status = Status::empty(repo_path);

        let mut tokens = raw.split('\0');
        while let Some(token) = tokens.next() {
            if token.is_empty() {
                continue;
            }
            if let Some(header) = token.strip_prefix("# ") {
                self.read_branch_header(&mut status, header);
                continue;
            }
            match token.as_bytes()[0] {
                b'1' | b'2' => {
                    let renamed = token.as_bytes()[0] == b'2';
                    // A rename record ends with a second NUL-separated path.
                    let fields = token
                        .splitn(if renamed { 10 } else { 9 }, ' ')
                        .collect::<Vec<_>>();
                    let path = fields.last().copied().unwrap_or("");
                    let xy = fields.get(1).copied().unwrap_or("..");
                    let orig_path = renamed.then(|| tokens.next().unwrap_or("").to_string());
                    self.push_entry(&mut status, xy, path, orig_path)?;
                }
                b'u' => {
                    let fields = token.splitn(11, ' ').collect::<Vec<_>>();
                    let path = fields.last().copied().unwrap_or("");
                    // Unmerged paths need conflict resolution, not staging; the
                    // section is surfaced as `U` without hunks for now.
                    status.unstaged.push(FileEntry {
                        path: path.to_string(),
                        status: "U".to_string(),
                        orig_path: None,
                        hunks: Vec::new(),
                    });
                }
                b'?' => {
                    if let Some(path) = token.strip_prefix("? ") {
                        status.untracked.push(FileEntry {
                            path: path.to_string(),
                            status: "?".to_string(),
                            orig_path: None,
                            hunks: Vec::new(),
                        });
                    }
                }
                // Ignored entries are not part of the status view.
                _ => {}
            }
        }

        Ok(status)
    }

    fn read_branch_header(&self, status: &mut Status, header: &str) {
        if let Some(oid) = header.strip_prefix("branch.oid ") {
            if oid != "(initial)" {
                status.head = Some(oid.to_string());
            }
        } else if let Some(head) = header.strip_prefix("branch.head ") {
            status.branch = if head == "(detached)" {
                "HEAD".to_string()
            } else {
                head.to_string()
            };
        } else if let Some(upstream) = header.strip_prefix("branch.upstream ") {
            status.upstream = Some(upstream.to_string());
        } else if let Some(counts) = header.strip_prefix("branch.ab ") {
            for count in counts.split_whitespace() {
                if let Some(ahead) = count.strip_prefix('+') {
                    status.ahead = ahead.parse().ok();
                } else if let Some(behind) = count.strip_prefix('-') {
                    status.behind = behind.parse().ok();
                }
            }
        }
    }

    fn push_entry(
        &self,
        status: &mut Status,
        xy: &str,
        path: &str,
        orig_path: Option<String>,
    ) -> Result<()> {
        let bytes = xy.as_bytes();
        if let Some(staged) = side_status(bytes.first().copied()) {
            let hunks = self.hunks_for(path, true)?;
            status.staged.push(FileEntry {
                path: path.to_string(),
                status: staged,
                orig_path: orig_path.clone(),
                hunks,
            });
        }
        if let Some(unstaged) = side_status(bytes.get(1).copied()) {
            let hunks = self.hunks_for(path, false)?;
            status.unstaged.push(FileEntry {
                path: path.to_string(),
                status: unstaged,
                orig_path,
                hunks,
            });
        }
        Ok(())
    }

    /// Unified diff of the unstaged changes to `path` (index vs working tree).
    pub fn diff_unstaged(&self, path: &str) -> Result<String> {
        self.run(&["-c", "core.quotepath=false", "diff", "-U3", "--", path])
    }

    /// Unified diff of the staged changes to `path` (HEAD vs index).
    pub fn diff_staged(&self, path: &str) -> Result<String> {
        self.run(&[
            "-c",
            "core.quotepath=false",
            "diff",
            "--cached",
            "-U3",
            "--",
            path,
        ])
    }

    /// Stage the whole unstaged hunk at `hunk_index` for `path`.
    pub fn stage_hunk(&self, path: &str, hunk_index: usize) -> Result<()> {
        let layout = self.file_layout(path, false)?;
        let patch = layout
            .whole_hunk_patch(hunk_index)
            .ok_or_else(|| Error::new(format!("hunk {hunk_index} not found for {path}")))?;
        self.apply_index(&patch, false)
    }

    /// Unstage the whole staged hunk at `hunk_index` for `path`.
    pub fn unstage_hunk(&self, path: &str, hunk_index: usize) -> Result<()> {
        let layout = self.file_layout(path, true)?;
        let patch = layout
            .whole_hunk_patch(hunk_index)
            .ok_or_else(|| Error::new(format!("hunk {hunk_index} not found for {path}")))?;
        self.apply_index(&patch, true)
    }

    /// Stage the lines in `start..=end` of the unstaged hunk at `hunk_index`.
    /// Returns `false` when the range contains no changed lines.
    pub fn stage_lines(
        &self,
        path: &str,
        hunk_index: usize,
        start: usize,
        end: usize,
    ) -> Result<bool> {
        self.apply_hunk_lines(path, hunk_index, start, end, false)
    }

    /// Unstage the lines in `start..=end` of the staged hunk at `hunk_index`.
    /// Returns `false` when the range contains no changed lines.
    pub fn unstage_lines(
        &self,
        path: &str,
        hunk_index: usize,
        start: usize,
        end: usize,
    ) -> Result<bool> {
        self.apply_hunk_lines(path, hunk_index, start, end, true)
    }

    /// Apply a document-line selection, one `git apply` per affected file.
    ///
    /// Returns `false` when the selection contains no changed lines.
    pub fn apply_selection(
        &self,
        files: &[FileLayout],
        start: u32,
        end: u32,
        reverse: bool,
    ) -> Result<bool> {
        let Selection::Ops(ops) = patch::normalise_selection(files, start, end) else {
            return Ok(false);
        };

        for file in files {
            let file_ops: Vec<&SelectionOp> = ops
                .iter()
                .filter(|op| op.path() == file.path.as_str())
                .collect();
            if file_ops.is_empty() {
                continue;
            }
            if file_ops
                .iter()
                .any(|op| matches!(op, SelectionOp::WholeFile { .. }))
            {
                if reverse {
                    self.unstage_file(&file.path)?;
                } else {
                    self.stage_file(&file.path)?;
                }
                continue;
            }

            let mut body = String::new();
            let mut synthetic_header = false;
            for op in file_ops {
                match op {
                    SelectionOp::WholeHunk { hunk_index, .. } => {
                        let hunk = file.hunks.get(*hunk_index).ok_or_else(|| {
                            Error::new(format!("hunk {hunk_index} not found for {}", file.path))
                        })?;
                        body.push_str(&patch::full_hunk_patch(&hunk.hunk));
                    }
                    SelectionOp::Lines {
                        hunk_index, lines, ..
                    } => {
                        let hunk = file.hunks.get(*hunk_index).ok_or_else(|| {
                            Error::new(format!("hunk {hunk_index} not found for {}", file.path))
                        })?;
                        // A partial apply of a file creation or deletion needs
                        // a normal header instead of the original `/dev/null`.
                        if (reverse && patch::is_new_file(&hunk.hunk))
                            || (!reverse && patch::is_deleted_file(&hunk.hunk))
                        {
                            synthetic_header = true;
                        }
                        body.push_str(&patch::selected_hunk_patch(&hunk.hunk, lines, reverse));
                    }
                    SelectionOp::WholeFile { .. } => unreachable!(),
                }
            }
            let header = if synthetic_header {
                patch::synthetic_file_header(&file.path, false)
            } else {
                file.header.clone()
            };
            self.apply_index(&format!("{header}{body}"), reverse)?;
        }
        Ok(true)
    }

    fn apply_hunk_lines(
        &self,
        path: &str,
        hunk_index: usize,
        start: usize,
        end: usize,
        reverse: bool,
    ) -> Result<bool> {
        let layout = self.file_layout(path, reverse)?;
        let (first_line, len) = {
            let hunk = layout
                .hunks
                .get(hunk_index)
                .ok_or_else(|| Error::new(format!("hunk {hunk_index} not found for {path}")))?;
            (hunk.first_line, hunk.hunk.lines.len())
        };
        if len == 0 {
            return Ok(false);
        }
        let start = start.min(len - 1) as u32;
        let end = end.min(len - 1) as u32;
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        self.apply_selection(&[layout], first_line + start, first_line + end, reverse)
    }

    /// Stage the entire file, including untracked files.
    pub fn stage_file(&self, path: &str) -> Result<()> {
        self.run(&["add", "--", path]).map(|_| ())
    }

    /// Unstage the entire file, leaving the working tree untouched.
    ///
    /// A staged rename is reset at both its old and new path so the pair leaves
    /// the index together.
    pub fn unstage_file(&self, path: &str) -> Result<()> {
        match self.rename_origin(path)? {
            Some(origin) => self.unstage_paths(&[origin.as_str(), path]),
            None => self.unstage_paths(&[path]),
        }
    }

    /// The old path when `path` is staged as a rename or copy.
    fn rename_origin(&self, path: &str) -> Result<Option<String>> {
        let raw = self.run(&["diff", "--cached", "--name-status", "-z", "--find-renames"])?;
        let mut tokens = raw.split('\0');
        while let Some(status) = tokens.next() {
            if status.is_empty() {
                continue;
            }
            let first = tokens.next().unwrap_or("");
            if status.starts_with('R') || status.starts_with('C') {
                let second = tokens.next().unwrap_or("");
                if second == path {
                    return Ok(Some(first.to_string()));
                }
            }
        }
        Ok(None)
    }

    /// Stage all tracked modifications and deletions.
    pub fn stage_all(&self) -> Result<()> {
        self.run(&["add", "-u"]).map(|_| ())
    }

    /// Stage every untracked file, leaving tracked changes alone.
    ///
    /// Paths come from the status so spaces and non-ASCII names stay intact;
    /// `git add -A` would also stage tracked modifications.
    pub fn stage_untracked(&self) -> Result<()> {
        let status = self.status()?;
        let mut args = vec!["add", "--"];
        args.extend(status.untracked.iter().map(|file| file.path.as_str()));
        if args.len() == 2 {
            return Ok(());
        }
        self.run(&args).map(|_| ())
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

    fn file_layout(&self, path: &str, staged: bool) -> Result<FileLayout> {
        let diff = if staged {
            self.diff_staged(path)?
        } else {
            self.diff_unstaged(path)?
        };
        FileLayout::from_diff(&diff, 0).ok_or_else(|| Error::new(format!("no diff for {path}")))
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
        // hunk has no context lines to anchor it. `--recount` lets us keep the
        // original hunk header counts after neutralising unselected lines.
        let mut args: Vec<&str> = vec!["apply", "--cached", "--unidiff-zero", "--recount"];
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
            .map_err(|e| Error::new(format!("failed to spawn git apply: {e}")))?;
        child
            .stdin
            .as_mut()
            .ok_or_else(|| Error::new("failed to open stdin"))?
            .write_all(patch_text.as_bytes())?;
        let out = child.wait_with_output()?;
        if !out.status.success() {
            return Err(Error::git(GitFailure::from_output(
                &args,
                out.status,
                &out.stderr,
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
            .map_err(|e| Error::new(format!("failed to run git: {e}")))?;
        if !out.status.success() {
            return Err(Error::git(GitFailure::from_output(
                args,
                out.status,
                &out.stderr,
            )));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A repo whose root is never touched: these cases need no diffs.
    fn header_repo() -> GitRepo {
        GitRepo {
            root: PathBuf::from("/nonexistent"),
        }
    }

    fn parse(raw: &str) -> Status {
        header_repo()
            .parse_status(raw, "/repo".to_string())
            .expect("parse")
    }

    #[test]
    fn parses_initial_branch_header() {
        let status = parse("# branch.oid (initial)\0# branch.head main\0");
        assert_eq!(status.branch, "main");
        assert!(status.head.is_none());
        assert!(status.staged.is_empty());
    }

    #[test]
    fn parses_detached_branch_and_upstream_counts() {
        let raw = "# branch.oid abc123\0# branch.head (detached)\0\
                   # branch.upstream origin/main\0# branch.ab +2 -3\0";
        let status = parse(raw);
        assert_eq!(status.branch, "HEAD");
        assert_eq!(status.head.as_deref(), Some("abc123"));
        assert_eq!(status.upstream.as_deref(), Some("origin/main"));
        assert_eq!((status.ahead, status.behind), (Some(2), Some(3)));
    }

    #[test]
    fn parses_unmerged_and_skips_ignored() {
        let raw = "u UU N... 100644 100644 100644 100644 a b c conflict.txt\0\
                   ! build/\0? new file.txt\0";
        let status = parse(raw);
        assert_eq!(status.unstaged.len(), 1);
        assert_eq!(status.unstaged[0].status, "U");
        assert_eq!(status.unstaged[0].path, "conflict.txt");
        assert_eq!(status.untracked.len(), 1);
        assert_eq!(status.untracked[0].status, "?");
        assert_eq!(status.untracked[0].path, "new file.txt");
    }
}
