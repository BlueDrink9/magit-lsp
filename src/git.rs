use crate::patch::{self, FileLayout, Hunk, Selection, SelectionOp};
use crate::runner;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

const RECOMMENDED_GIT_VERSION: &str = include_str!("../RECOMMENDED_GIT_VERSION");

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

/// Filters and presentation options for a Git log query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct GitLogOptions {
    /// Maximum commits returned per page.
    pub page_size: usize,
    /// Include Git's graph ordering (the record identities remain full OIDs).
    pub graph: bool,
    /// Include ref decorations.
    pub decorate: bool,
    /// Restrict author name/email.
    pub author: Option<String>,
    /// Restrict commits since this Git date expression.
    pub since: Option<String>,
    /// Restrict commit messages using Git's grep expression.
    pub grep: Option<String>,
    /// Pickaxe string search (-S).
    pub pickaxe: Option<String>,
    /// Pickaxe regex search (-G).
    pub pickaxe_regex: Option<String>,
    /// Restrict changes to these literal paths.
    pub paths: Vec<String>,
    /// Starting revision; defaults to HEAD.
    pub revision: Option<String>,
}

impl Default for GitLogOptions {
    fn default() -> Self {
        Self {
            page_size: 100,
            graph: true,
            decorate: true,
            author: None,
            since: None,
            grep: None,
            pickaxe: None,
            pickaxe_regex: None,
            paths: Vec::new(),
            revision: None,
        }
    }
}

/// One commit returned by [`GitRepo::log`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommit {
    /// Full object ID.
    pub oid: String,
    /// Full parent object IDs.
    pub parents: Vec<String>,
    /// Commit subject.
    pub subject: String,
    /// Author name and email display string.
    pub author: String,
    /// Author date as an ISO-8601 string.
    pub date: String,
    /// Ref decorations, if requested.
    pub decorations: Vec<String>,
    /// Graph connector prefix emitted for this commit, empty when graph mode is off.
    pub graph_prefix: String,
}

/// A page of commits and whether another page exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitLogPage {
    /// Commits in Git's traversal order.
    pub commits: Vec<GitCommit>,
    /// True when at least one further matching commit exists.
    pub has_more: bool,
    /// Connector-only rows emitted by Git in graph mode.
    pub rows: Vec<GitLogRow>,
}

/// A recorded commit, with any diagnostic from cancellation, output or message reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitOutcome {
    /// Commit message read back from HEAD, when available.
    pub message: Option<String>,
    /// Non-fatal output or message-read diagnostic.
    pub warning: Option<String>,
}

/// An ordered row in a Git log page's graph output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitLogRow {
    /// The commit at this row, identified by its full object ID.
    Commit(String),
    /// A connector-only graph row emitted between commits.
    Connector(String),
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
    /// Best-effort compatibility advisory, separate from the original stderr.
    pub warning: Option<String>,
}

impl GitFailure {
    /// Build a failure from a finished Git invocation.
    pub fn from_output(args: &[&str], status: std::process::ExitStatus, stderr: &[u8]) -> Self {
        Self {
            exit_code: status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(stderr).trim_end().to_string(),
            args: args.iter().map(|arg| arg.to_string()).collect(),
            subcommand: subcommand_of(args).to_string(),
            warning: None,
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

    /// Resolve COMMIT_EDITMSG in this worktree's actual, canonical Git directory.
    /// Linked worktrees must not use the common repository's message file.
    pub fn commit_message_path(&self) -> Result<PathBuf> {
        let args = ["rev-parse", "--absolute-git-dir"];
        let mut command = git_command(&self.root, &args, Stdio::null(), &[]);
        let out = runner::run_owned(&mut command, None)
            .map_err(|error| Error::new(format!("Cannot resolve editor directory: {error}")))?;
        let status = out.status.ok_or_else(|| Error::new("Editor directory lookup was cancelled"))?;
        if !status.success() {
            return Err(Error::git(GitFailure::from_output(&args, status, &out.stderr)));
        }
        if out.outcome.stdout_truncated || out.outcome.stderr_truncated || out.outcome.drain_incomplete {
            return Err(Error::new("Editor directory lookup returned incomplete output"));
        }
        let output = std::str::from_utf8(&out.stdout)
            .map_err(|_| Error::new("Git editor directory is not UTF-8"))?;
        let path = output
            .strip_suffix('\n')
            .filter(|path| !path.is_empty() && !path.contains(['\n', '\r', '\0']))
            .ok_or_else(|| Error::new("Git returned an invalid editor directory"))?;
        let path = Path::new(path);
        if !path.is_absolute() {
            return Err(Error::new("Git returned a relative editor directory"));
        }
        Ok(std::fs::canonicalize(path)?.join("COMMIT_EDITMSG"))
    }

    /// Return a coherent page of log records; filtering values are individual
    /// argv entries and paths are separated from revisions by `--`.
    pub fn log(&self, options: &GitLogOptions) -> Result<GitLogPage> {
        let size = if options.page_size == 0 {
            100
        } else {
            options.page_size
        };
        let query_limit = size
            .checked_add(1)
            .ok_or_else(|| Error::new("log page size is too large"))?;
        let mut args: Vec<String> = vec!["log".into(), "--topo-order".into(), "--no-color".into()];
        if options.graph {
            args.push("--graph".into());
        }
        if options.decorate {
            args.push("--decorate=short".into());
        } else {
            args.push("--no-decorate".into());
        }
        args.extend(["--format=%H%x00%P%x00%s%x00%an <%ae>%x00%aI%x00%D".into()]);
        args.extend(["-n".into(), query_limit.to_string()]);
        for (flag, value) in [
            ("--author", options.author.as_ref()),
            ("--since", options.since.as_ref()),
            ("--grep", options.grep.as_ref()),
            ("-S", options.pickaxe.as_ref()),
            ("-G", options.pickaxe_regex.as_ref()),
        ] {
            if let Some(value) = value {
                args.push(if flag == "-S" || flag == "-G" {
                    format!("{flag}{value}")
                } else {
                    format!("{flag}={value}")
                });
            }
        }
        let revision = options.revision.as_deref().unwrap_or("HEAD");
        if options.revision.is_none()
            && self
                .run(&["rev-parse", "--verify", "HEAD^{commit}"])
                .is_err()
        {
            let missing_head =
                self.run(&["symbolic-ref", "-q", "HEAD"])
                    .ok()
                    .is_some_and(|reference| {
                        self.run(&["show-ref", "--verify", "--quiet", reference.trim()])
                            .is_err()
                    });
            if !missing_head {
                return Err(self
                    .run(&["rev-parse", "--verify", "HEAD^{commit}"])
                    .unwrap_err());
            }
            return Ok(GitLogPage {
                commits: Vec::new(),
                has_more: false,
                rows: Vec::new(),
            });
        }
        args.push("--end-of-options".into());
        args.push(revision.to_string());
        if !options.paths.is_empty() {
            args.push("--".into());
            args.extend(options.paths.iter().map(|path| format!(":(literal){path}")));
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let raw = self.run(&refs)?;
        let mut commits = Vec::new();
        let mut rows = Vec::new();
        for line in raw.lines() {
            let oid_start = line
                .find(|c: char| c.is_ascii_hexdigit())
                .unwrap_or(line.len());
            let graph_prefix = &line[..oid_start];
            let fields: Vec<_> = line[oid_start..].split('\0').collect();
            if fields.len() < 6
                || fields[0].len() < 40
                || !fields[0].bytes().all(|b| b.is_ascii_hexdigit())
            {
                if options.graph && !line.trim().is_empty() {
                    rows.push(GitLogRow::Connector(line.to_string()));
                }
                continue;
            }
            commits.push(GitCommit {
                oid: fields[0].to_string(),
                parents: fields[1].split_whitespace().map(str::to_string).collect(),
                subject: fields[2].to_string(),
                author: fields[3].to_string(),
                date: fields[4].to_string(),
                decorations: if options.decorate {
                    fields[5]
                        .split(", ")
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect()
                } else {
                    Vec::new()
                },
                graph_prefix: graph_prefix.to_string(),
            });
            rows.push(GitLogRow::Commit(fields[0].to_string()));
        }
        let mut seen = std::collections::HashSet::new();
        commits.retain(|commit| seen.insert(commit.oid.clone()));
        let has_more = commits.len() > size;
        let retained: std::collections::HashSet<_> =
            commits.iter().take(size).map(|c| c.oid.clone()).collect();
        commits.truncate(size);
        rows.retain(|row| match row {
            GitLogRow::Commit(oid) => retained.contains(oid),
            GitLogRow::Connector(_) => true,
        });
        if let Some(last) = rows
            .iter()
            .rposition(|row| matches!(row, GitLogRow::Commit(_)))
        {
            rows.truncate(last + 1);
        }
        Ok(GitLogPage {
            commits,
            has_more,
            rows,
        })
    }

    /// Resolve a revision expression to its full commit object ID.
    pub fn resolve_commit(&self, revision: &str) -> Result<String> {
        let value = self.run(&[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{revision}^{{commit}}"),
        ])?;
        Ok(value.trim().to_string())
    }

    /// Compute the full status with every file's diff hunks.
    ///
    /// Views that hide bodies use [`Self::status_listing`] and load hunks only
    /// for visible files with [`Self::file_hunks`].
    pub fn status(&self) -> Result<Status> {
        let mut status = self.status_listing()?;
        for file in &mut status.staged {
            file.hunks = self.file_hunks(&file.path, true)?;
        }
        for file in &mut status.unstaged {
            file.hunks = self.file_hunks(&file.path, false)?;
        }
        Ok(status)
    }

    /// Compute the status without reading any file's diff.
    pub fn status_listing(&self) -> Result<Status> {
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
            status.staged.push(FileEntry {
                path: path.to_string(),
                status: staged,
                orig_path: orig_path.clone(),
                hunks: Vec::new(),
            });
        }
        if let Some(unstaged) = side_status(bytes.get(1).copied()) {
            status.unstaged.push(FileEntry {
                path: path.to_string(),
                status: unstaged,
                orig_path,
                hunks: Vec::new(),
            });
        }
        Ok(())
    }

    /// Unified diff of the unstaged changes to `path` (index vs working tree).
    pub fn diff_unstaged(&self, path: &str) -> Result<String> {
        self.diff_side(path, false)
    }

    /// Unified diff of the staged changes to `path` (HEAD vs index).
    pub fn diff_staged(&self, path: &str) -> Result<String> {
        self.diff_side(path, true)
    }

    /// Diff one side of `path`, including the old path of a rename.
    ///
    /// Path limiting disables Git's rename detection, so a rename pair has to
    /// be passed to `git diff` together or it looks like a new file.
    fn diff_side(&self, path: &str, staged: bool) -> Result<String> {
        record_diff_call(&self.root.join(path));
        let mut args = vec!["-c", "core.quotepath=false", "diff"];
        if staged {
            args.push("--cached");
        }
        args.extend(["-M", "-U3", "--"]);
        let origin = self.rename_origin(path, staged)?;
        match &origin {
            Some(origin) => {
                args.push(origin);
                args.push(path);
            }
            None => args.push(path),
        }
        self.run(&args)
    }

    /// Stage the whole unstaged hunk with `hunk_id` for `path`.
    ///
    /// The diff is re-read first, so the hunk is located by content even when
    /// lines above it shifted since the status was rendered.
    pub fn stage_hunk(&self, path: &str, hunk_id: &str) -> Result<()> {
        let layout = self.file_layout(path, false)?;
        let hunk = layout
            .hunks
            .iter()
            .find(|h| h.hunk.id == hunk_id)
            .ok_or_else(|| Error::hunk_not_found(path))?;
        self.apply_index(
            &format!("{}{}", layout.header, patch::full_hunk_patch(&hunk.hunk)),
            false,
        )
    }

    /// Unstage the whole staged hunk with `hunk_id` for `path`.
    pub fn unstage_hunk(&self, path: &str, hunk_id: &str) -> Result<()> {
        let layout = self.file_layout(path, true)?;
        let hunk = layout
            .hunks
            .iter()
            .find(|h| h.hunk.id == hunk_id)
            .ok_or_else(|| Error::hunk_not_found(path))?;
        self.apply_index(
            &format!("{}{}", layout.header, patch::full_hunk_patch(&hunk.hunk)),
            true,
        )
    }

    /// Discard an unstaged hunk by its content-based identity.
    pub fn discard_hunk(&self, path: &str, hunk_id: &str) -> Result<()> {
        let layout = self.file_layout(path, false)?;
        let hunk = layout
            .hunks
            .iter()
            .find(|h| h.hunk.id == hunk_id)
            .ok_or_else(|| Error::hunk_not_found(path))?;
        self.apply_worktree(
            &format!("{}{}", layout.header, patch::full_hunk_patch(&hunk.hunk)),
            true,
        )
    }

    /// Discard a document-line selection from the worktree without changing the index.
    pub fn discard_selection(&self, files: &[FileLayout], start: u32, end: u32) -> Result<bool> {
        let Selection::Ops(ops) = patch::normalise_selection(files, start, end) else {
            return Ok(false);
        };
        self.discard_selection_ops(files, &ops)
    }

    /// Discard identity-resolved operations using verified fresh diffs.
    pub fn discard_selection_ops(&self, files: &[FileLayout], ops: &[SelectionOp]) -> Result<bool> {
        if ops.is_empty() {
            return Ok(false);
        }
        for file in files {
            let file_ops: Vec<&SelectionOp> =
                ops.iter().filter(|op| op.path() == file.path).collect();
            if file_ops.is_empty() {
                continue;
            }
            if file_ops
                .iter()
                .any(|op| matches!(op, SelectionOp::WholeFile { .. }))
            {
                self.discard_file(&file.path)?;
                continue;
            }
            let mut body = String::new();
            for op in file_ops {
                let (hunk_id, lines) = match op {
                    SelectionOp::WholeHunk { hunk_id, .. } => {
                        let h = file
                            .hunks
                            .iter()
                            .find(|h| &h.hunk.id == hunk_id)
                            .ok_or_else(|| Error::hunk_not_found(&file.path))?;
                        body.push_str(&patch::full_hunk_patch(&h.hunk));
                        continue;
                    }
                    SelectionOp::Lines { hunk_id, lines, .. } => (hunk_id, lines),
                    SelectionOp::WholeFile { .. } => unreachable!(),
                };
                let hunk = file
                    .hunks
                    .iter()
                    .find(|h| &h.hunk.id == hunk_id)
                    .ok_or_else(|| Error::hunk_not_found(&file.path))?;
                body.push_str(&patch::selected_hunk_patch(&hunk.hunk, lines, true));
            }
            self.apply_worktree(&format!("{}{}", file.header, body), true)?;
        }
        Ok(true)
    }

    /// Discard all worktree changes for a path, preserving its index contents.
    pub fn discard_file(&self, path: &str) -> Result<()> {
        let status = self.status_listing()?;
        if status.untracked.iter().any(|file| file.path == path) {
            let full = self.root.join(path);
            if full.is_dir() {
                std::fs::remove_dir_all(full)?;
            } else if full.exists() {
                std::fs::remove_file(full)?;
            }
            return Ok(());
        }
        self.run(&["restore", "--worktree", "--", path]).map(|_| ())
    }

    /// Discard all tracked worktree changes, leaving untracked files untouched.
    pub fn discard_all_unstaged(&self) -> Result<()> {
        self.run(&["restore", "--worktree", "--", "."])?;
        Ok(())
    }

    /// Remove every untracked path without changing tracked files.
    pub fn discard_all_untracked(&self) -> Result<()> {
        for file in self.status_listing()?.untracked {
            self.discard_file(&file.path)?;
        }
        Ok(())
    }

    /// Stage the lines in `start..=end` of the unstaged hunk with `hunk_id`.
    /// Returns `false` when the range contains no changed lines.
    pub fn stage_lines(&self, path: &str, hunk_id: &str, start: usize, end: usize) -> Result<bool> {
        self.apply_hunk_lines(path, hunk_id, start, end, false)
    }

    /// Unstage the lines in `start..=end` of the staged hunk with `hunk_id`.
    /// Returns `false` when the range contains no changed lines.
    pub fn unstage_lines(
        &self,
        path: &str,
        hunk_id: &str,
        start: usize,
        end: usize,
    ) -> Result<bool> {
        self.apply_hunk_lines(path, hunk_id, start, end, true)
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

        self.apply_selection_ops(files, &ops, reverse)
    }

    /// Apply identity-resolved operations using verified fresh diffs.
    pub fn apply_selection_ops(
        &self,
        files: &[FileLayout],
        ops: &[SelectionOp],
        reverse: bool,
    ) -> Result<bool> {
        if ops.is_empty() {
            return Ok(false);
        }
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
                    SelectionOp::WholeHunk { hunk_id, .. } => {
                        let hunk = file
                            .hunks
                            .iter()
                            .find(|h| &h.hunk.id == hunk_id)
                            .ok_or_else(|| Error::hunk_not_found(&file.path))?;
                        body.push_str(&patch::full_hunk_patch(&hunk.hunk));
                    }
                    SelectionOp::Lines { hunk_id, lines, .. } => {
                        let hunk = file
                            .hunks
                            .iter()
                            .find(|h| &h.hunk.id == hunk_id)
                            .ok_or_else(|| Error::hunk_not_found(&file.path))?;
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
        hunk_id: &str,
        start: usize,
        end: usize,
        reverse: bool,
    ) -> Result<bool> {
        let layout = self.file_layout(path, reverse)?;
        let hunk = layout
            .hunks
            .iter()
            .find(|h| h.hunk.id == hunk_id)
            .ok_or_else(|| Error::hunk_not_found(path))?;
        let first_line = hunk.first_line;
        let len = hunk.hunk.lines.len();
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

    /// Stage the entire file, including untracked files and rename pairs.
    pub fn stage_file(&self, path: &str) -> Result<()> {
        match self.rename_origin(path, false)? {
            Some(origin) => self.run(&["add", "--", origin.as_str(), path]).map(|_| ()),
            None => self.run(&["add", "--", path]).map(|_| ()),
        }
    }

    /// Unstage the entire file, leaving the working tree untouched.
    ///
    /// A staged rename is reset at both its old and new path so the pair leaves
    /// the index together.
    pub fn unstage_file(&self, path: &str) -> Result<()> {
        match self.rename_origin(path, true)? {
            Some(origin) => self.unstage_paths(&[origin.as_str(), path]),
            None => self.unstage_paths(&[path]),
        }
    }

    /// The old path when `path` is the new side of a rename or copy.
    fn rename_origin(&self, path: &str, staged: bool) -> Result<Option<String>> {
        let mut args = vec!["diff"];
        if staged {
            args.push("--cached");
        }
        args.extend(["--name-status", "-z", "--find-renames"]);
        let raw = self.run(&args)?;
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
        let status = self.status_listing()?;
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

    /// Unstage `paths`, leaving the working tree untouched, even without HEAD.
    fn unstage_paths(&self, paths: &[&str]) -> Result<()> {
        let mut reset = vec!["reset", "-q", "--"];
        reset.extend_from_slice(paths);
        self.run(&reset).map(|_| ())
    }

    fn file_layout(&self, path: &str, staged: bool) -> Result<FileLayout> {
        let diff = if staged {
            self.diff_staged(path)?
        } else {
            self.diff_unstaged(path)?
        };
        FileLayout::from_diff(&diff, 0).ok_or_else(|| Error::new(format!("no diff for {path}")))
    }

    /// The diff hunks for one file, reading its diff from Git.
    ///
    /// A type change renders as two file sections and has no single hunk list;
    /// treat it as whole-file only so the status view stays honest.
    pub fn file_hunks(&self, path: &str, staged: bool) -> Result<Vec<Hunk>> {
        let diff = if staged {
            self.diff_staged(path)?
        } else {
            self.diff_unstaged(path)?
        };
        Ok(patch::parse_file_diff(&diff)
            .map(|file| file.hunks)
            .unwrap_or_default())
    }

    fn apply_index(&self, patch_text: &str, reverse: bool) -> Result<()> {
        self.apply_patch(patch_text, true, reverse)
    }

    fn apply_patch(&self, patch_text: &str, cached: bool, reverse: bool) -> Result<()> {
        // `--unidiff-zero` lets line subsets of a new file apply, where the
        // hunk has no context lines to anchor it. `--recount` lets us keep the
        // original hunk header counts after neutralising unselected lines.
        let mut args: Vec<&str> = vec!["apply"];
        if cached {
            args.push("--cached");
        }
        args.extend(["--unidiff-zero", "--recount"]);
        if reverse {
            args.push("-R");
        }
        let started = Instant::now();
        tracing::debug!(cwd = %self.root.display(), args = ?args, "git");
        // The patch body is user content: only ever visible at trace level.
        tracing::trace!(patch = %patch_text, "git stdin");

        let mut command = git_command(&self.root, &args, Stdio::null(), &[]);
        let out = runner::run_owned(&mut command, Some(patch_text.as_bytes()))
            .map_err(|e| Error::new(format!("failed to run git apply: {e}")))?;
        let Some(status) = out.status else {
            return Err(Error::new("git apply was cancelled; process reap is incomplete"));
        };
        log_finished(&self.root, &args, status, started.elapsed(), &out.stdout, &out.stderr);
        if !status.success() {
            let mut failure = GitFailure::from_output(&args, status, &out.stderr);
            failure.warning = warn_if_git_is_below_recommendation(&self.root, &[], &status);
            return Err(Error::git(failure));
        }
        if out.outcome.stdout_truncated || out.outcome.stderr_truncated || out.outcome.drain_incomplete {
            return Err(Error::new("git apply completed with incomplete process output"));
        }
        if out.outcome.stdin_error {
            return Err(Error::new("failed to write patch to git apply stdin"));
        }
        Ok(())
    }

    fn apply_worktree(&self, patch_text: &str, reverse: bool) -> Result<()> {
        self.apply_patch(patch_text, false, reverse)
    }

    /// Run `git commit` with extra environment variables and return the
    /// committed message as read back by `git log -1 --format=%B`. Only the
    /// mutation inherits cancellation, not the subsequent message read.
    pub fn commit(&self, args: &[&str], env: &[(&str, &str)]) -> Result<CommitOutcome> {
        let mut commit_args = vec!["commit"];
        commit_args.extend_from_slice(args);
        let started = Instant::now();
        let refs: Vec<_> = commit_args.iter().copied().collect();
        // Git updates HEAD before post-commit runs. A killed post-commit must
        // not turn an already recorded commit into a reported failure.
        let before = commit_head(&self.root);
        let mut command = git_command(&self.root, &refs, Stdio::null(), env);
        let out = runner::run_owned(&mut command, None).map_err(|e| Error::new(format!("failed to run git: {e}")))?;
        let Some(status) = out.status else { return Err(Error::new("git commit cancellation is incomplete; process ownership remains with the reaper. Git lock files may remain; no locks were removed.")); };
        log_finished(&self.root, &refs, status, started.elapsed(), &out.stdout, &out.stderr);
        let recorded = out.outcome.cancelled && before.as_ref().is_some_and(|before| {
            commit_head(&self.root).is_some_and(|after| !after.is_empty() && &after != before)
        });
        let cancellation_warning = out.outcome.cancelled.then(|| cancellation_lock_warning(&self.root));
        if !status.success() && !recorded {
            let mut failure = GitFailure::from_output(&refs, status, &out.stderr);
            failure.warning = cancellation_warning.or_else(|| warn_if_git_is_below_recommendation(&self.root, env, &status));
            return Err(Error::git(failure));
        }
        let mut warning = (out.outcome.stdout_truncated || out.outcome.stderr_truncated || out.outcome.drain_incomplete)
            .then(|| "git commit completed, but process output was incomplete".to_string());
        if let Some(locks) = cancellation_warning {
            warning = Some(format!("Git recorded a commit before cancellation; a hook may have been interrupted. {locks}"));
        }
        let read = runner::with_cancel(None, || self.run(&["log", "-1", "--format=%B"]));
        let message = match read {
            Ok(message) => Some(message.trim_end().to_string()),
            Err(error) => { warning = Some(format!("{}; commit message could not be read: {error}", warning.unwrap_or_else(|| "commit succeeded".into()))); None }
        };
        Ok(CommitOutcome { message, warning })
    }

    fn run(&self, args: &[&str]) -> Result<String> {
        Self::command_in(&self.root, args)
    }

    fn command_in(cwd: &Path, args: &[&str]) -> Result<String> {
        Self::command_in_with_env(cwd, args, &[])
    }

    fn command_in_with_env(cwd: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<String> {
        let started = Instant::now();
        tracing::debug!(cwd = %cwd.display(), args = ?args, "git");
        let mut command = git_command(cwd, args, Stdio::null(), env);
        let out = runner::run_owned(&mut command, None)
            .map_err(|e| Error::new(format!("failed to run git: {e}")))?;
        let Some(status) = out.status else {
            return Err(Error::new("git command was cancelled; process reap is incomplete"));
        };
        log_finished(cwd, args, status, started.elapsed(), &out.stdout, &out.stderr);
        if !status.success() {
            let mut failure = GitFailure::from_output(args, status, &out.stderr);
            failure.warning = warn_if_git_is_below_recommendation(cwd, env, &status);
            return Err(Error::git(failure));
        }
        if out.outcome.stdout_truncated || out.outcome.stderr_truncated || out.outcome.drain_incomplete {
            return Err(Error::new("git command completed with incomplete output"));
        }
        if out.outcome.stdin_error {
            return Err(Error::new("git command stdin write failed"));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

// Diagnostics have their own cancellation deadline, never consume mutation
// intent, and use the same sole-Child owner as every other Git invocation.
// The 500ms probe budget is followed by the owner's bounded TERM/KILL/reap.
fn readonly_probe(root: &Path, args: &[&str]) -> Option<runner::OwnedOutput> {
    let cancel = runner::CancelHandle::new();
    let timer_cancel = cancel.clone();
    let (done, waiting) = std::sync::mpsc::channel();
    let timer = std::thread::spawn(move || {
        if waiting.recv_timeout(std::time::Duration::from_millis(500)).is_err() {
            timer_cancel.cancel();
        }
    });
    let mut args_with_flags = vec!["--no-optional-locks"];
    args_with_flags.extend_from_slice(args);
    let result = runner::with_cancel(Some(cancel), || {
        runner::run_owned(&mut git_command(root, &args_with_flags, Stdio::null(), &[]), None)
    });
    let _ = done.send(());
    let _ = timer.join();
    let out = result.ok()?;
    (out.status.is_some() && !out.outcome.cancelled && !out.outcome.stdout_truncated
        && !out.outcome.stderr_truncated && !out.outcome.drain_incomplete).then_some(out)
}

// An empty, successfully observed state denotes unborn HEAD; None is unknown.
fn commit_head(root: &Path) -> Option<String> {
    let out = readonly_probe(root, &["rev-parse", "--verify", "--quiet", "HEAD"])?;
    if out.status?.code() == Some(1) && out.stdout.is_empty() && out.stderr.is_empty() {
        return Some(String::new());
    }
    if !out.status?.success() { return None; }
    let oid = std::str::from_utf8(&out.stdout).ok()?.trim();
    (matches!(oid.len(), 40 | 64) && oid.bytes().all(|b| b.is_ascii_hexdigit())).then(|| oid.to_string())
}

fn cancellation_lock_warning(root: &Path) -> String {
    let mut locks = Vec::new();
    if let Some(out) = readonly_probe(root, &["rev-parse", "--absolute-git-dir", "--git-common-dir"]) {
        if out.status.is_some_and(|s| s.success()) {
            if let Ok(paths) = std::str::from_utf8(&out.stdout) {
                // Exactly two paths are expected. Newlines in Git paths or
                // malformed diagnostic output must not expand the scan budget.
                let paths: Vec<_> = paths.lines().take(3).collect();
                if paths.len() == 2 && paths.iter().all(|p| !p.is_empty() && !p.contains('\0')) {
                let deadline = Instant::now() + std::time::Duration::from_millis(25);
                let mut remaining = 128usize;
                let mut dirs: Vec<_> = paths.into_iter().map(|p| root.join(p)).collect();
                dirs.sort();
                dirs.dedup();
                for dir in dirs {
                    for name in ["index.lock", "HEAD.lock", "packed-refs.lock", "config.lock", "shallow.lock"] {
                        let path = dir.join(name);
                        if path.symlink_metadata().is_ok() { locks.push(path); }
                    }
                    let mut pending = vec![dir.join("refs")];
                    while let Some(dir) = pending.pop() {
                        if remaining == 0 || Instant::now() >= deadline { break; }
                        let Ok(entries) = std::fs::read_dir(dir) else { continue; };
                        for entry in entries {
                            if remaining == 0 || Instant::now() >= deadline { break; }
                            remaining -= 1;
                            let Ok(entry) = entry else { continue; };
                            if entry.file_type().is_ok_and(|t| t.is_dir()) { pending.push(entry.path()); }
                            else if entry.path().extension().is_some_and(|e| e == "lock") { locks.push(entry.path()); }
                        }
                    }
                }
                }
            }
        }
    }
    let mut warning = "Cancellation may leave Git lock files; no locks were removed. Check that no Git process is still using them before manual recovery.".to_string();
    if !locks.is_empty() {
        warning.push_str(" Observed: ");
        warning.push_str(&locks.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", "));
    }
    warning
}

/// Best-effort warning for a completed Git failure on an older-than-recommended Git.
/// This separate `--version` probe cannot alter the original invocation result.
fn warn_if_git_is_below_recommendation(
    cwd: &Path,
    env: &[(&str, &str)],
    status: &std::process::ExitStatus,
) -> Option<String> {
    // Do not register the diagnostic as the operation's cancellation target.
    // Cap both output and elapsed time, including inherited pipe handles.
    let deadline = Instant::now() + std::time::Duration::from_millis(500);
    let Ok(mut child) = git_command(cwd, &["--version"], Stdio::null(), env)
        .stderr(Stdio::null())
        .spawn()
    else {
        return None;
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::Read;
        let mut bytes = Vec::new();
        let result = stdout.take(1024).read_to_end(&mut bytes);
        let _ = tx.send(result.map(|_| bytes));
    });
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) => return None,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let Ok(Ok(bytes)) = rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) else {
        return None;
    };
    let installed = String::from_utf8_lossy(&bytes);
    let Some(version) = installed.trim().strip_prefix("git version ") else {
        return None;
    };
    let Some(installed_version) = parse_git_version(version) else {
        return None;
    };
    let Some(recommended_version) = parse_git_version(RECOMMENDED_GIT_VERSION) else {
        return None;
    };
    if compare_versions(&installed_version, &recommended_version).is_lt() {
        let warning = format!(
            "Git {version} is older than the recommended version {}. Upgrade Git to at least {}.",
            RECOMMENDED_GIT_VERSION.trim(),
            RECOMMENDED_GIT_VERSION.trim()
        );
        tracing::warn!(
            installed = %installed.trim(),
            recommended = RECOMMENDED_GIT_VERSION.trim(),
            exit_code = status.code().unwrap_or(-1),
            "Git command failed with an older-than-recommended Git; upgrade Git or report compatibility issues"
        );
        return Some(warning);
    }
    None
}

fn parse_git_version(text: &str) -> Option<Vec<u64>> {
    let digits = text
        .trim()
        .split(|ch: char| !ch.is_ascii_digit() && ch != '.')
        .next()?;
    let components = digits
        .trim_end_matches('.')
        .split('.')
        .map(str::parse::<u64>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .ok()?;
    (!components.is_empty()).then_some(components)
}

fn compare_versions(left: &[u64], right: &[u64]) -> std::cmp::Ordering {
    let count = left.len().max(right.len());
    (0..count)
        .map(|index| {
            left.get(index)
                .copied()
                .unwrap_or(0)
                .cmp(&right.get(index).copied().unwrap_or(0))
        })
        .find(|ordering| !ordering.is_eq())
        .unwrap_or(std::cmp::Ordering::Equal)
}

/// Per-file `git diff` counters, used by tests to prove lazy bodies.
fn diff_calls() -> &'static Mutex<HashMap<PathBuf, usize>> {
    static CALLS: OnceLock<Mutex<HashMap<PathBuf, usize>>> = OnceLock::new();
    CALLS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// How many `git diff` invocations this process made for `path`.
///
/// Keyed by absolute path, so concurrent tests with separate repositories do
/// not observe each other.
pub fn diff_invocation_count(path: &Path) -> usize {
    diff_calls()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(path)
        .copied()
        .unwrap_or(0)
}

fn record_diff_call(path: &Path) {
    *diff_calls()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .entry(path.to_path_buf())
        .or_insert(0) += 1;
}

/// Build a `git` child that can never write to the server's own stdout and
/// never prompts on a terminal nobody is reading.
fn git_command(cwd: &Path, args: &[&str], stdin: Stdio, env: &[(&str, &str)]) -> Command {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(cwd)
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("GIT_TERMINAL_PROMPT", "0");
    // Git may need a password or passphrase; route it through the editor
    // bridge so the client can show a masked prompt. Without a running server
    // (unit tests, a bare `git` backend) leave the normal Git behaviour.
    if let Some(socket) = crate::editor::socket_path() {
        command.env(crate::editor::SOCKET_ENV, socket);
        if let Some(askpass) = crate::editor::askpass_invocation() {
            command.env("GIT_ASKPASS", askpass);
            command.env("SSH_ASKPASS", askpass);
        }
    }
    for (key, value) in env {
        command.env(key, value);
    }
    command
}

/// Record the outcome of one Git invocation.
fn log_finished(
    cwd: &Path,
    args: &[&str],
    status: std::process::ExitStatus,
    elapsed: std::time::Duration,
    stdout: &[u8],
    stderr: &[u8],
) {
    let exit_code = status.code().unwrap_or(-1);
    let elapsed_ms = elapsed.as_millis();
    let record_id = runner::global_log().record(cwd, args, exit_code, elapsed, stdout, stderr);
    runner::trace_current(
        "git_call",
        serde_json::json!({
            "cmd": format!("git {}", args.join(" ")),
            "duration_ms": u64::try_from(elapsed_ms).unwrap_or(u64::MAX),
            "exit_code": exit_code,
        }),
    );
    if status.success() {
        tracing::debug!(cwd = %cwd.display(), args = ?args, exit_code, elapsed_ms, record_id, "git ok");
    } else {
        tracing::warn!(
            cwd = %cwd.display(),
            args = ?args,
            exit_code,
            elapsed_ms,
            record_id,
            stderr = %String::from_utf8_lossy(stderr).trim_end(),
            "git failed"
        );
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
