use crate::patch::{self, FileLayout, Hunk, Selection, SelectionOp};
use crate::runner;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Git's supported `push.default` modes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PushDefault {
    Nothing,
    Current,
    Upstream,
    Simple,
    Matching,
}

/// Destination for an ignore rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IgnoreScope {
    /// The repository root `.gitignore`.
    Root,
    /// A `.gitignore` beside the selected repository-relative path.
    Directory,
    /// The repository's shared Git directory `info/exclude`.
    Repository,
    /// The configured (or default) user excludes file.
    User,
}

/// An ignore rule, either a literal repository path or an intentional pattern.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IgnoreRule<'a> {
    /// Ignore exactly this repository-relative path.
    LiteralPath(&'a Path),
    /// Add this explicitly supplied Git ignore pattern.
    Pattern(&'a str),
}

impl PushDefault {
    fn as_str(self) -> &'static str {
        match self {
            Self::Nothing => "nothing",
            Self::Current => "current",
            Self::Upstream => "upstream",
            Self::Simple => "simple",
            Self::Matching => "matching",
        }
    }
    fn parse(value: &str) -> Result<Self> {
        match value {
            "nothing" => Ok(Self::Nothing),
            "current" => Ok(Self::Current),
            "upstream" => Ok(Self::Upstream),
            "simple" => Ok(Self::Simple),
            "matching" => Ok(Self::Matching),
            _ => Err(Error::new(format!("Invalid push.default value: {value}"))),
        }
    }
}

const RECOMMENDED_GIT_VERSION: &str = include_str!("../RECOMMENDED_GIT_VERSION");

fn validate_repo_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(Error::new(
            "Ignore path must be a non-empty repository-relative path without traversal",
        ));
    }
    Ok(())
}

fn reject_symlink_components(root: &Path, path: &Path) -> Result<()> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| Error::new("Ignore destination escapes repository"))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        if let Ok(metadata) = std::fs::symlink_metadata(&current) {
            if metadata.file_type().is_symlink() {
                return Err(Error::new("Ignore destination must not traverse symlinks"));
            }
        }
    }
    Ok(())
}

fn reject_symlink_destination(root: &Path, path: &Path) -> Result<()> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        if let Ok(metadata) = std::fs::symlink_metadata(&current) {
            if metadata.file_type().is_symlink() {
                return Err(Error::new("Ignore destination must not traverse symlinks"));
            }
        }
    }
    Ok(())
}

fn user_excludes_path(root: &Path) -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["config", "--path", "--get", "core.excludesFile"])
        .current_dir(root)
        .output()?;
    if output.status.success() {
        return Ok(PathBuf::from(
            String::from_utf8_lossy(&output.stdout).trim(),
        ));
    }
    if output.status.code() != Some(1) {
        return Err(Error::git(GitFailure::from_output(
            &["config", "--path", "--get", "core.excludesFile"],
            output.status,
            &output.stderr,
        )));
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    base.map(|base| base.join("git/ignore")).ok_or_else(|| {
        Error::new("Cannot resolve default user excludes file without HOME or XDG_CONFIG_HOME")
    })
}

// Hash only this path's subtree. Never follow symlinks (including directory
// links), and include sorted membership so collapsed untracked directories are
// as strongly bound to their render as individual files.
fn worktree_identity(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok("missing".into()),
        Err(error) => return Err(error.into()),
    };
    let mut hash = Sha256::new();
    if metadata.file_type().is_symlink() {
        hash.update(b"link\0");
        hash.update(std::fs::read_link(path)?.as_os_str().as_encoded_bytes());
    } else {
        hash.update(format!("{:?}\0", metadata.permissions()));
        if metadata.is_file() {
            hash.update(b"file\0");
            let mut file = std::fs::File::open(path)?;
            let mut buffer = [0; 65536];
            loop {
                let count = file.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                hash.update(&buffer[..count]);
            }
        } else if metadata.is_dir() {
            hash.update(b"directory\0");
            let mut children = std::fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
            children.sort_by_key(|child| child.file_name());
            for child in children {
                let name = child.file_name();
                let bytes = name.as_encoded_bytes();
                hash.update((bytes.len() as u64).to_le_bytes());
                hash.update(bytes);
                hash.update(worktree_identity(&child.path())?);
            }
        } else {
            return Err(Error::new(format!(
                "Cannot snapshot special file {}",
                path.display()
            )));
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// A single changed file: its display status char and its diff hunks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub status: String,
    /// Old path for a staged rename or copy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orig_path: Option<String>,
    pub hunks: Vec<Hunk>,
    /// Render-time index, HEAD and worktree state used to validate file actions.
    #[serde(skip)]
    pub mutation_identity: Option<FileMutationIdentity>,
}

/// Exact repository state relevant to a rendered file row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMutationIdentity {
    pub head: String,
    pub index: String,
    pub worktree: String,
    /// Rename/copy source, including the state of that endpoint.
    pub original: Option<(String, Box<FileMutationIdentity>)>,
}

/// Destination path → origin path for the renames and copies on one side.
///
/// Built by one whole-repository scan and scoped to a single status read, so
/// every file body of that render looks its origin up instead of rescanning.
#[derive(Debug, Default)]
pub struct RenameOrigins {
    origins: HashMap<String, String>,
}

impl RenameOrigins {
    /// The old path when `path` is the new side of a rename or copy.
    pub fn origin(&self, path: &str) -> Option<&str> {
        self.origins.get(path).map(String::as_str)
    }
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
    /// Whether this listing reports an unresolved conflict, derived from the
    /// porcelain `u` records the parser surfaced as `U` entries.
    pub fn has_conflicts(&self) -> bool {
        self.unstaged.iter().any(|file| file.status == "U")
    }
}

/// Git operation state relevant to an in-progress rebase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RebaseState {
    /// Whether Git has stopped for unresolved conflicts.
    pub conflicted: bool,
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
    /// Restrict commits until this Git date expression.
    pub until: Option<String>,
    /// Include only merge commits.
    pub merges_only: bool,
    /// Exclude merge commits.
    pub no_merges: bool,
    /// Follow only first parents.
    pub first_parent: bool,
    /// Select local branches, local and remote branches, or all refs.
    pub ref_selection: Option<String>,
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
            until: None,
            merges_only: false,
            no_merges: false,
            first_parent: false,
            ref_selection: None,
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

/// Captured commit operation, independent of menu and prompt lifetime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitOperation {
    /// Create a new commit using validated menu options.
    Create(CommitOptions),
    /// Amend HEAD while editing its message.
    Amend(CommitOptions),
    /// Amend staged changes without editing the message.
    Extend(CommitOptions),
    /// Amend only the message, excluding staged changes from the resulting tree.
    Reword(CommitOptions),
}

/// A captured repository operation ready for deferred execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryOperation {
    /// Run a commit operation through the registered editor bridge.
    Commit(CommitOperation),
    /// Create a typed marker and run Git's autosquash rebase.
    InstantRebase(crate::instant_rebase::InstantRebaseAuthority),
}

/// Result from executing a deferred repository operation.
#[derive(Debug)]
pub enum RepositoryOperationOutcome {
    /// Commit-specific result.
    Commit(CommitOutcome),
    /// Instant autosquash phase result.
    InstantRebase(crate::instant_rebase::InstantRebaseOutcome),
}

/// Typed commit action selected from the commit menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitKind {
    /// Create a new commit.
    Create,
    /// Amend HEAD with an editor.
    Amend,
    /// Amend HEAD without an editor.
    Extend,
    /// Edit only HEAD's message.
    Reword,
}

impl CommitOperation {
    /// Render the captured operation as individual Git arguments.
    pub fn argv(&self) -> Vec<String> {
        let (mut args, options) = match self {
            Self::Create(options) => (vec![], options),
            Self::Amend(options) => (vec!["--amend".into()], options),
            Self::Extend(options) => (vec!["--amend".into(), "--no-edit".into()], options),
            Self::Reword(options) => (
                vec!["--amend".into(), "--only".into(), "--allow-empty".into()],
                options,
            ),
        };
        args.extend(options.argv());
        args
    }
}

/// Validated arguments for creating a commit; values are passed as argv entries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommitOptions {
    /// `git commit --all`.
    pub all: bool,
    /// `git commit --allow-empty`.
    pub allow_empty: bool,
    /// `git commit --verbose`.
    pub verbose: bool,
    /// `git commit --no-verify`.
    pub no_verify: bool,
    /// `git commit --reset-author`.
    pub reset_author: bool,
    /// `git commit --signoff`.
    pub signoff: bool,
    /// Optional explicit author identity.
    pub author: Option<String>,
    /// Optional Git date expression.
    pub date: Option<String>,
    /// Optional signing key (empty means Git's default key).
    pub signing_key: Option<String>,
    /// Optional message to reuse.
    pub reuse_message: Option<String>,
    /// Optional message to re-edit.
    pub reedit_message: Option<String>,
}

impl CommitOptions {
    /// Render trusted values to argv without shell parsing.
    pub fn argv(&self) -> Vec<String> {
        let mut args = Vec::new();
        for (enabled, flag) in [
            (self.all, "--all"),
            (self.allow_empty, "--allow-empty"),
            (self.verbose, "--verbose"),
            (self.no_verify, "--no-verify"),
            (self.reset_author, "--reset-author"),
            (self.signoff, "--signoff"),
        ] {
            if enabled {
                args.push(flag.to_string());
            }
        }
        for (flag, value) in [
            ("--author", self.author.as_deref()),
            ("--date", self.date.as_deref()),
            ("--gpg-sign", self.signing_key.as_deref()),
            ("--reuse-message", self.reuse_message.as_deref()),
            ("--reedit-message", self.reedit_message.as_deref()),
        ] {
            if let Some(value) = value {
                args.push(format!("{flag}={value}"));
            }
        }
        args
    }
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RebaseTodoOwner {
    git_dir: PathBuf,
    metadata: Vec<(String, Vec<u8>)>,
    merge_identity: std::sync::Arc<same_file::Handle>,
}

/// A coherent todo source and owning rebase operation snapshot.
pub(crate) struct RebaseTodoSnapshot {
    pub source: Vec<u8>,
    pub owner: RebaseTodoOwner,
}

/// A Git repository handle, cheap to clone once discovery resolved its identities.
#[derive(Debug, Clone)]
pub struct GitRepo {
    root: PathBuf,
    git_dir: PathBuf,
    common_dir: PathBuf,
}

/// Exact staged state owned by a prepared instant autosquash operation.
pub(crate) struct InstantRebaseState {
    pub index: String,
    pub staged_patch: String,
}

/// Own and remove one create-new rebase todo lock.
struct RebaseTodoLock {
    path: PathBuf,
    file: std::fs::File,
}

impl Drop for RebaseTodoLock {
    fn drop(&mut self) {
        let _ = &self.file;
        let _ = std::fs::remove_file(&self.path);
    }
}

impl GitRepo {
    /// Read the note attached to an explicit commit, or `None` when it has no note.
    pub fn note(&self, commit: &str) -> Result<Option<String>> {
        let commit = self.validate_note_target(commit)?;
        let listed = match self.run(&["notes", "list", &commit]) {
            Ok(listed) => listed,
            Err(error)
                if error.git.as_ref().is_some_and(|failure| {
                    failure.args == ["notes", "list", commit.as_str()]
                        && failure.exit_code == 1
                        && failure.stderr == format!("error: no note found for object {commit}.")
                }) =>
            {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        if listed.trim().is_empty() {
            return Ok(None);
        }
        self.run(&["notes", "show", &commit]).map(Some)
    }

    /// Add a note to an explicit commit, failing if it already has one.
    pub fn add_note(&self, commit: &str, note: &str) -> Result<()> {
        let commit = self.validate_note_target(commit)?;
        self.command_with_input(&["notes", "add", "-F", "-", &commit], note.as_bytes())?;
        Ok(())
    }

    /// Explicitly replace the note attached to an explicit commit.
    pub fn replace_note(&self, commit: &str, note: &str) -> Result<()> {
        let commit = self.validate_note_target(commit)?;
        self.command_with_input(&["notes", "add", "-f", "-F", "-", &commit], note.as_bytes())?;
        Ok(())
    }

    /// Remove the note attached to an explicit commit.
    pub fn remove_note(&self, commit: &str) -> Result<()> {
        let commit = self.validate_note_target(commit)?;
        self.run(&["notes", "remove", &commit])?;
        Ok(())
    }

    fn validate_note_target(&self, target: &str) -> Result<String> {
        Self::reject_option_like("commit", target)?;
        self.run(&[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{}^{{commit}}", target.trim()),
        ])
        .map(|commit| commit.trim().to_string())
    }

    /// Detect an active merge/apply rebase in this worktree, including conflict state.
    pub fn rebase_state(&self) -> Result<Option<RebaseState>> {
        if !self.rebase_in_progress() {
            return Ok(None);
        }
        let status = self.run(&["status", "--porcelain"])?;
        let conflicted = status.lines().any(|line| {
            matches!(
                line.get(..2),
                Some("UU" | "AA" | "DD" | "AU" | "UA" | "DU" | "UD")
            )
        });
        Ok(Some(RebaseState { conflicted }))
    }

    /// Detect an active rebase, reusing the conflict state of a render-time status listing.
    ///
    /// The rebase directories are still inspected on disk; only the duplicate
    /// `status --porcelain` read is avoided by deriving conflicts from the
    /// parsed porcelain `u` records of `status`.
    pub fn rebase_state_from_status(&self, status: &Status) -> Result<Option<RebaseState>> {
        if !self.rebase_in_progress() {
            return Ok(None);
        }
        Ok(Some(RebaseState {
            conflicted: status.has_conflicts(),
        }))
    }

    /// Whether a merge- or apply-backend rebase currently owns this worktree.
    fn rebase_in_progress(&self) -> bool {
        let merge = self.git_dir.join("rebase-merge").is_dir();
        let apply_dir = self.git_dir.join("rebase-apply");
        let apply = apply_dir.is_dir()
            && (apply_dir.join("rebasing").exists() || apply_dir.join("applying").exists())
            && apply_dir.join("rebasing").exists();
        merge || apply
    }

    /// Continue an active rebase without opening an editor for its unchanged message.
    pub fn rebase_continue(&self) -> Result<()> {
        self.rebase_operation("--continue", true)
    }

    /// Skip the current commit in an active rebase.
    pub fn rebase_skip(&self) -> Result<()> {
        self.rebase_operation("--skip", false)
    }

    /// Abort an active rebase.
    pub fn rebase_abort(&self) -> Result<()> {
        self.rebase_operation("--abort", false)
    }

    fn rebase_operation(&self, operation: &str, noninteractive: bool) -> Result<()> {
        if self.rebase_state()?.is_none() {
            return Err(Error::new("No rebase operation is in progress."));
        }
        let args = ["rebase", operation];
        if noninteractive {
            Self::command_in_with_env(&self.root, &args, &[("GIT_EDITOR", "true")])?;
        } else {
            self.run(&args)?;
        }
        Ok(())
    }

    /// Validate a branch name at the Git boundary without changing repository state.
    pub fn validate_branch_name(&self, name: &str) -> Result<String> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::new("A branch name is required."));
        }
        self.run(&["check-ref-format", "--branch", name])?;
        Ok(name.to_owned())
    }

    /// Name of the branch HEAD points at, or `None` when HEAD is detached.
    pub fn current_branch(&self) -> Option<String> {
        self.run(&["symbolic-ref", "--short", "--quiet", "HEAD"])
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    }

    /// Whether `name` is an existing local branch.
    pub fn is_local_branch(&self, name: &str) -> bool {
        if Self::reject_option_like("branch name", name).is_err() || name.is_empty() {
            return false;
        }
        self.run(&[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{name}"),
        ])
        .is_ok()
    }

    /// Whether `name` is an existing remote-tracking ref such as `origin/main`.
    pub fn is_remote_ref(&self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || name.starts_with('-') {
            return false;
        }
        self.run(&[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/remotes/{name}"),
        ])
        .is_ok()
    }

    /// Every local branch name.
    pub fn local_branch_names(&self) -> Result<Vec<String>> {
        let output = self.run(&["for-each-ref", "--format=%(refname:short)", "refs/heads"])?;
        Ok(output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect())
    }

    /// Every remote-tracking ref name, excluding each remote's symbolic `HEAD`.
    pub fn remote_ref_names(&self) -> Result<Vec<String>> {
        let output = self.run(&["for-each-ref", "--format=%(refname:short)", "refs/remotes"])?;
        Ok(output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.ends_with("/HEAD"))
            .map(str::to_string)
            .collect())
    }

    /// Create branch `new` at `startpoint` without switching to it.
    ///
    /// A startpoint that names a local branch becomes the new branch's
    /// upstream via explicit tracking; any other startpoint creates the branch
    /// at that revision. Neither answer can select a different `git branch`
    /// operation: the new name is validated and an option-like startpoint is
    /// refused here.
    pub fn create_branch_at(&self, new: &str, startpoint: &str) -> Result<()> {
        let new = self.validate_branch_name(new)?;
        Self::reject_option_like("start point", startpoint)?;
        if self.is_local_branch(startpoint) {
            return self.create_tracking_branch(&new, startpoint);
        }
        self.run(&["branch", new.as_str(), "--", startpoint])
            .map(|_| ())
    }

    /// Create branch `new` at `startpoint` and switch to it.
    ///
    /// A local-branch startpoint becomes the new branch's upstream, matching
    /// [`Self::create_branch_at`]. The ref creation and worktree switch happen
    /// in one `git switch`, so a checkout Git refuses (for example because it
    /// would overwrite local changes) leaves no new ref behind.
    pub fn create_and_checkout_branch(&self, new: &str, startpoint: &str) -> Result<()> {
        let new = self.validate_branch_name(new)?;
        Self::reject_option_like("start point", startpoint)?;
        if self.is_local_branch(startpoint) {
            return self
                .run(&["switch", "-c", new.as_str(), "--track", startpoint])
                .map(|_| ());
        }
        self.run(&["switch", "-c", new.as_str(), startpoint])
            .map(|_| ())
    }

    /// Create branch `new` at `startpoint`, setting `startpoint` as its
    /// upstream.
    ///
    /// Both answers are guarded here: the new name is validated and a
    /// startpoint Git would parse as an option is refused, so an answer can
    /// never select a different `git branch` operation.
    pub fn create_tracking_branch(&self, new: &str, startpoint: &str) -> Result<()> {
        let new = self.validate_branch_name(new)?;
        Self::reject_option_like("start point", startpoint)?;
        self.run(&["branch", "--track", new.as_str(), "--", startpoint])
            .map(|_| ())
    }

    /// Switch the worktree to `name` without changing the checked-out commit.
    ///
    /// Uses `git switch` rather than `git checkout`, so the validated branch
    /// name can never be read as a path.
    pub fn checkout_branch(&self, name: &str) -> Result<()> {
        let name = self.validate_branch_name(name)?;
        self.run(&["switch", name.as_str()]).map(|_| ())
    }

    /// Check out `revision` with a detached HEAD at its commit.
    ///
    /// The revision is resolved to a commit first, so a name that only points
    /// at a tree or blob is refused, and an option-like answer is refused here.
    pub fn checkout_detached(&self, revision: &str) -> Result<()> {
        Self::reject_option_like("revision", revision)?;
        let commit = self.resolve_commit(revision)?;
        self.run(&["switch", "--detach", commit.as_str()])
            .map(|_| ())
    }

    /// Create a local branch tracking `remote_ref` and switch to it.
    ///
    /// The intended local name is the remote-tracking ref without its
    /// configured remote prefix. An existing local branch is never moved or
    /// reset: the operation is refused before any ref changes. Git creates the
    /// branch and updates the worktree in one checkout, so a refused checkout
    /// never force-deletes a ref another actor could own.
    pub fn checkout_remote_ref(&self, remote_ref: &str) -> Result<String> {
        Self::reject_option_like("remote ref", remote_ref)?;
        if !self.is_remote_ref(remote_ref) {
            return Err(Error::new(format!(
                "{remote_ref} is not a remote-tracking ref."
            )));
        }
        let local = self.local_name_for_remote_ref(remote_ref).ok_or_else(|| {
            Error::new(format!(
                "Cannot derive a local branch name from {remote_ref}."
            ))
        })?;
        let local = self.validate_branch_name(&local)?;
        if self.is_local_branch(&local) {
            return Err(Error::new(format!(
                "A local branch {local} already exists; use local branch checkout to switch to it."
            )));
        }
        self.run(&["switch", "-c", local.as_str(), "--track", remote_ref])?;
        Ok(local)
    }

    /// Local branch name implied by `remote_ref`, using the longest configured
    /// remote prefix so remote names containing slashes still resolve.
    fn local_name_for_remote_ref(&self, remote_ref: &str) -> Option<String> {
        let remote = self
            .remote_names()
            .ok()?
            .into_iter()
            .filter(|remote| {
                remote_ref
                    .strip_prefix(remote)
                    .is_some_and(|rest| rest.starts_with('/') && rest.len() > 1)
            })
            .max_by_key(String::len)?;
        Some(remote_ref[remote.len() + 1..].to_string())
    }

    /// Full ref name of the configured upstream of `branch`, or `None` when it
    /// has no upstream.
    pub fn upstream_ref(&self, branch: &str) -> Option<String> {
        let branch = self.validate_branch_name(branch).ok()?;
        let spec = format!("{branch}@{{upstream}}");
        self.run(&["rev-parse", "--symbolic-full-name", &spec])
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    }

    /// Most recent commit shared by `left` and `right`, or `None` when Git
    /// finds no merge base.
    pub fn merge_base(&self, left: &str, right: &str) -> Option<String> {
        Self::reject_option_like("revision", left).ok()?;
        Self::reject_option_like("revision", right).ok()?;
        self.run(&["merge-base", left, right])
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    }

    /// Rename `source` to `target` after validating both names.
    pub fn rename_branch(&self, source: &str, target: &str) -> Result<()> {
        let source = self.validate_branch_name(source)?;
        let target = self.validate_branch_name(target)?;
        self.run(&["branch", "-m", source.as_str(), target.as_str()])
            .map(|_| ())
    }

    /// Delete `name`, leaving Git's refusal of the current or an unmerged
    /// branch to surface as its own error.
    pub fn delete_branch(&self, name: &str) -> Result<()> {
        let name = self.validate_branch_name(name)?;
        self.run(&["branch", "-d", name.as_str()]).map(|_| ())
    }

    /// Force `name` to point at `target` without checking it out or touching
    /// the working tree.
    ///
    /// The branch name is validated with [`Self::validate_branch_name`] and a
    /// target Git would parse as an option is refused here, so an answer can
    /// never select a different `git branch` operation. Git reports its own
    /// refusal of invalid revisions.
    pub fn reset_branch(&self, name: &str, target: &str) -> Result<()> {
        let name = self.validate_branch_name(name)?;
        Self::reject_option_like("revision", target)?;
        self.run(&["branch", "-f", name.as_str(), "--", target])
            .map(|_| ())
    }

    /// Whether the index or worktree has any uncommitted change, including
    /// untracked files.
    pub fn is_dirty(&self) -> Result<bool> {
        Ok(!self.run(&["status", "--porcelain"])?.trim().is_empty())
    }

    /// Reset the current branch and index to `revision`, leaving the worktree
    /// untouched.
    ///
    /// `revision` is refused here when it would parse as an option; Git
    /// reports its own refusal of invalid revisions. The revision follows the
    /// mode directly because `git reset` reads an operand after `--` as a
    /// path.
    pub fn reset_mixed(&self, revision: &str) -> Result<()> {
        Self::reject_option_like("revision", revision)?;
        self.run(&["reset", "--mixed", revision]).map(|_| ())
    }

    /// Move the current branch to `revision`, leaving the index and worktree
    /// untouched.
    pub fn reset_soft(&self, revision: &str) -> Result<()> {
        Self::reject_option_like("revision", revision)?;
        self.run(&["reset", "--soft", revision]).map(|_| ())
    }

    /// Reset the current branch, index and worktree to `revision`.
    ///
    /// Git discards staged and unstaged changes to tracked files here; it does
    /// not refuse the operation when those changes would be lost.
    pub fn reset_hard(&self, revision: &str) -> Result<()> {
        Self::reject_option_like("revision", revision)?;
        self.run(&["reset", "--hard", revision]).map(|_| ())
    }

    /// Reset the current branch and index to `revision`, keeping local
    /// changes that do not conflict with it.
    ///
    /// Git refuses the operation itself when local changes conflict.
    pub fn reset_keep(&self, revision: &str) -> Result<()> {
        Self::reject_option_like("revision", revision)?;
        self.run(&["reset", "--keep", revision]).map(|_| ())
    }

    /// Refuse a free-text operand that Git would read as an option.
    ///
    /// Operands are forwarded after a `--` terminator as well; this check
    /// exists so the user sees a readable refusal instead of Git's or a
    /// different operation's output.
    fn reject_option_like(kind: &str, value: &str) -> Result<()> {
        if value.starts_with('-') {
            return Err(Error::new(format!(
                "A {kind} cannot begin with '-'. Enter a plain name without leading dashes."
            )));
        }
        Ok(())
    }

    /// Create a lightweight tag at HEAD. Tag signing is disabled for this
    /// invocation only, so a `tag.gpgSign` default cannot turn it into an
    /// annotated, signed tag; the repository configuration is left untouched.
    /// An option-like name is refused at this boundary so it cannot select a
    /// different `git tag` mode; Git refuses invalid or duplicate names itself.
    pub fn create_tag(&self, name: &str) -> Result<()> {
        self.create_lightweight_tag(name, "HEAD")
    }

    /// Create a lightweight tag at a selected commit, ignoring signing config.
    pub fn create_lightweight_tag(&self, name: &str, revision: &str) -> Result<()> {
        Self::reject_option_like("tag name", name)?;
        Self::reject_option_like("revision", revision)?;
        let commit = self.resolve_commit(revision)?;
        self.run(&["-c", "tag.gpgSign=false", "tag", "--", name, &commit])
            .map(|_| ())
    }

    /// Delete `name`, leaving Git's refusal of a missing tag to surface as its
    /// own error. An option-like name is refused at this boundary.
    pub fn delete_tag(&self, name: &str) -> Result<()> {
        Self::reject_option_like("tag name", name)?;
        self.run(&["tag", "-d", "--", name]).map(|_| ())
    }

    /// Add remote `name` pointing at `url`. Option-like names and URLs are
    /// refused at this boundary; Git refuses invalid or duplicate names itself.
    pub fn add_remote(&self, name: &str, url: &str) -> Result<()> {
        Self::reject_option_like("remote name", name)?;
        Self::reject_option_like("remote URL", url)?;
        self.run(&["remote", "add", "--", name, url]).map(|_| ())
    }

    /// Replace an existing remote's fetch and push URLs, preserving other config.
    pub fn remote_configure(&self, name: &str, fetch_url: &str, push_url: &str) -> Result<()> {
        Self::reject_option_like("remote name", name)?;
        for url in [fetch_url, push_url] {
            Self::reject_option_like("remote URL", url)?;
            if url.is_empty() || url.contains(['\n', '\r', '\0']) {
                return Err(Error::new(
                    "Remote URL must be non-empty and contain no line breaks",
                ));
            }
        }
        if !self.remote_names()?.iter().any(|remote| remote == name) {
            return Err(Error::new(format!("Remote '{name}' does not exist")));
        }
        self.run(&[
            "config",
            "--replace-all",
            &format!("remote.{name}.url"),
            fetch_url,
        ])?;
        self.run(&[
            "config",
            "--replace-all",
            &format!("remote.{name}.pushurl"),
            push_url,
        ])?;
        Ok(())
    }

    /// Remove remote `name`, leaving Git's refusal of a missing remote to
    /// surface as its own error. An option-like name is refused at this
    /// boundary.
    pub fn remove_remote(&self, name: &str) -> Result<()> {
        Self::reject_option_like("remote name", name)?;
        self.run(&["remote", "remove", "--", name]).map(|_| ())
    }

    /// Rename remote `old` to `new`, leaving Git's refusals (missing source,
    /// duplicate target) to surface as their own errors. Option-like names are
    /// refused at this boundary.
    pub fn rename_remote(&self, old: &str, new: &str) -> Result<()> {
        Self::reject_option_like("remote name", old)?;
        Self::reject_option_like("remote name", new)?;
        self.run(&["remote", "rename", "--", old, new]).map(|_| ())
    }

    /// Delete local tracking refs of remote `name` that no longer exist on it.
    /// Git contacts the remote to discover its current refs and then deletes
    /// the stale local tracking refs without fetching new objects. Git's
    /// refusal of a missing remote surfaces as its own error; an option-like
    /// name is refused at this boundary.
    pub fn prune_remote(&self, name: &str) -> Result<()> {
        Self::reject_option_like("remote name", name)?;
        self.run(&["remote", "prune", "--", name]).map(|_| ())
    }

    /// Store `description` in Git's configuration for the named branch.
    /// The `--` terminator keeps dash-leading prose a stored value instead of
    /// letting it select a `git config` operation.
    pub fn set_branch_description(&self, name: &str, description: &str) -> Result<()> {
        let name = self.validate_branch_name(name)?;
        let key = format!("branch.{name}.description");
        self.run(&["config", "--", key.as_str(), description])
            .map(|_| ())
    }

    fn existing_local_branch(&self, name: &str) -> Result<String> {
        let name = self.validate_branch_name(name)?;
        self.run(&[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{name}"),
        ])?;
        Ok(name)
    }

    /// Read a branch's Git-resolved upstream, or `None` when unset.
    pub fn branch_upstream(&self, branch: &str) -> Result<Option<String>> {
        let branch = self.existing_local_branch(branch)?;
        let value = self.run(&[
            "for-each-ref",
            "--format=%(upstream:short)",
            &format!("refs/heads/{branch}"),
        ])?;
        Ok((!value.trim().is_empty()).then(|| value.trim().to_owned()))
    }

    /// Set or clear the upstream of an existing local branch using Git's upstream semantics.
    pub fn set_branch_upstream(&self, branch: &str, upstream: Option<&str>) -> Result<()> {
        let branch = self.existing_local_branch(branch)?;
        match upstream {
            Some(upstream) => {
                if self
                    .run(&[
                        "rev-parse",
                        "--verify",
                        "--quiet",
                        &format!("{upstream}^{{commit}}"),
                    ])
                    .is_err()
                {
                    return Err(Error::new(format!(
                        "Upstream ref does not exist: {upstream}"
                    )));
                }
                self.run(&[
                    "branch",
                    &format!("--set-upstream-to={upstream}"),
                    "--",
                    &branch,
                ])?;
            }
            None => {
                self.run(&["branch", "--unset-upstream", "--", &branch])?;
            }
        }
        Ok(())
    }

    /// Read branch-specific rebase policy, distinguishing unset from false.
    pub fn branch_rebase(&self, branch: &str) -> Result<Option<bool>> {
        self.branch_bool_config(branch, "rebase")
    }
    /// Set or clear branch-specific rebase policy on an existing local branch.
    pub fn set_branch_rebase(&self, branch: &str, value: Option<bool>) -> Result<()> {
        self.set_branch_value(
            branch,
            "rebase",
            value.map(|v| if v { "true" } else { "false" }),
        )
    }
    /// Read branch-specific push remote, or `None` when unset.
    pub fn branch_push_remote(&self, branch: &str) -> Result<Option<String>> {
        self.branch_string_config(branch, "pushRemote")
    }
    /// Set or clear branch-specific push remote after validating it exists.
    pub fn set_branch_push_remote(&self, branch: &str, remote: Option<&str>) -> Result<()> {
        if let Some(remote) = remote {
            Self::reject_option_like("remote name", remote)?;
            self.run(&["remote", "get-url", remote])?;
        }
        self.set_branch_value(branch, "pushRemote", remote)
    }

    fn branch_bool_config(&self, branch: &str, key: &str) -> Result<Option<bool>> {
        let Some(value) = self.branch_string_config(branch, key)? else {
            return Ok(None);
        };
        match value.as_str() {
            "true" | "yes" | "on" | "1" => Ok(Some(true)),
            "false" | "no" | "off" | "0" => Ok(Some(false)),
            _ => Err(Error::new(format!(
                "Invalid branch.{branch}.{key} value: {value}"
            ))),
        }
    }
    fn branch_string_config(&self, branch: &str, key: &str) -> Result<Option<String>> {
        let branch = self.existing_local_branch(branch)?;
        match self.run(&[
            "config",
            "--local",
            "--get",
            &format!("branch.{branch}.{key}"),
        ]) {
            Ok(value) => Ok(Some(value.trim().to_owned())),
            Err(error) if is_missing_config_key(&error) => Ok(None),
            Err(error) => Err(error),
        }
    }
    fn set_branch_value(&self, branch: &str, key: &str, value: Option<&str>) -> Result<()> {
        let branch = self.existing_local_branch(branch)?;
        let key = format!("branch.{branch}.{key}");
        match value {
            Some(value) => {
                self.run(&["config", "--local", "--", &key, value])?;
            }
            None => {
                if let Err(error) = self.run(&["config", "--local", "--unset-all", &key]) {
                    if !is_missing_config_clear(&error) {
                        return Err(error);
                    }
                }
            }
        }
        Ok(())
    }

    /// Read repository-local pull.rebase, or `None` when unset.
    pub fn pull_rebase(&self) -> Result<Option<bool>> {
        self.repo_bool_config("pull.rebase")
    }
    /// Set or clear repository-local pull.rebase.
    pub fn set_pull_rebase(&self, value: Option<bool>) -> Result<()> {
        self.set_repo_value(
            "pull.rebase",
            value.map(|v| if v { "true" } else { "false" }),
        )
    }
    /// Read repository-local push.default, or `None` when unset.
    pub fn push_default(&self) -> Result<Option<PushDefault>> {
        self.repo_string_config("push.default")?
            .map(|v| PushDefault::parse(&v))
            .transpose()
    }
    /// Set or clear repository-local push.default.
    pub fn set_push_default(&self, value: Option<PushDefault>) -> Result<()> {
        self.set_repo_value("push.default", value.map(PushDefault::as_str))
    }
    fn repo_string_config(&self, key: &str) -> Result<Option<String>> {
        match self.run(&["config", "--local", "--get", key]) {
            Ok(value) => Ok(Some(value.trim().to_owned())),
            Err(error) if is_missing_config_key(&error) => Ok(None),
            Err(error) => Err(error),
        }
    }
    fn repo_bool_config(&self, key: &str) -> Result<Option<bool>> {
        let Some(value) = self.repo_string_config(key)? else {
            return Ok(None);
        };
        match value.as_str() {
            "true" | "yes" | "on" | "1" => Ok(Some(true)),
            "false" | "no" | "off" | "0" => Ok(Some(false)),
            _ => Err(Error::new(format!("Invalid {key} value: {value}"))),
        }
    }
    fn set_repo_value(&self, key: &str, value: Option<&str>) -> Result<()> {
        match value {
            Some(value) => {
                self.run(&["config", "--local", key, value])?;
            }
            None => {
                if let Err(error) = self.run(&["config", "--local", "--unset-all", key]) {
                    if !is_missing_config_clear(&error) {
                        return Err(error);
                    }
                }
            }
        }
        Ok(())
    }

    /// Check an external author-date expression without changing repository state.
    pub fn validate_author_date(&self, date: &str) -> Result<()> {
        Self::command_in_with_env(
            &self.root,
            &["var", "GIT_AUTHOR_IDENT"],
            &[
                ("GIT_AUTHOR_NAME", "Date validation"),
                ("GIT_AUTHOR_EMAIL", "date@example.invalid"),
                ("GIT_AUTHOR_DATE", date),
            ],
        )?;
        Ok(())
    }

    /// Open the repository containing `path`, resolving its working-tree root.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::discover(path)?
            .ok_or_else(|| Error::new("This folder is not inside a Git repository."))
    }

    /// Discover a repository and its canonical worktree/Git identities.
    pub fn discover<P: AsRef<Path>>(path: P) -> Result<Option<Self>> {
        let path = path.as_ref();
        let cwd = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent().unwrap_or(path).to_path_buf()
        };
        let out = match Self::command_in(&cwd, &["rev-parse", "--show-toplevel"]) {
            Ok(out) => out,
            Err(error)
                if error.git.as_ref().is_some_and(|failure| {
                    failure
                        .stderr
                        .to_lowercase()
                        .contains("not a git repository")
                }) =>
            {
                return Ok(None)
            }
            Err(error) => return Err(error),
        };
        let root = out.trim();
        if root.is_empty() {
            return Ok(None);
        }
        let root = std::fs::canonicalize(root)?;
        let git_dir = Self::command_in(&root, &["rev-parse", "--git-dir"])?;
        let common_dir = Self::command_in(&root, &["rev-parse", "--git-common-dir"])?;
        let resolve_dir = |value: &str| {
            let path = Path::new(value.trim());
            std::fs::canonicalize(if path.is_absolute() {
                path.to_path_buf()
            } else {
                root.join(path)
            })
        };
        let git_dir = resolve_dir(&git_dir)?;
        let common_dir = resolve_dir(&common_dir)?;
        Ok(Some(Self {
            root,
            git_dir,
            common_dir,
        }))
    }

    /// The repository's working-tree root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Append an ignore rule to the explicitly selected scope without replacing existing bytes.
    pub fn add_ignore_rule(&self, scope: IgnoreScope, rule: IgnoreRule<'_>) -> Result<()> {
        use std::io::Write;
        let (path, directory_scope) = match scope {
            IgnoreScope::Root => (self.root.join(".gitignore"), false),
            IgnoreScope::Directory => {
                let IgnoreRule::LiteralPath(selected) = rule else {
                    return Err(Error::new("Directory ignore scope requires a literal path"));
                };
                validate_repo_path(selected)?;
                let parent = selected.parent().unwrap_or(Path::new(""));
                (self.root.join(parent).join(".gitignore"), true)
            }
            IgnoreScope::Repository => (self.common_dir.join("info/exclude"), false),
            IgnoreScope::User => (user_excludes_path(&self.root)?, false),
        };
        let line = match rule {
            IgnoreRule::LiteralPath(selected) => {
                validate_repo_path(selected)?;
                let selected = if directory_scope {
                    selected
                        .file_name()
                        .ok_or_else(|| Error::new("Ignore path must name a file"))?
                } else {
                    selected.as_os_str()
                };
                let mut escaped = String::new();
                for ch in selected.to_string_lossy().chars() {
                    if matches!(ch, '\\' | '*' | '?' | '[' | ']' | '#' | '!' | ' ') {
                        escaped.push('\\');
                    }
                    escaped.push(ch);
                }
                format!("/{escaped}")
            }
            IgnoreRule::Pattern(pattern)
                if !pattern.is_empty() && !pattern.contains('\n') && !pattern.contains('\r') =>
            {
                pattern.to_owned()
            }
            IgnoreRule::Pattern(_) => {
                return Err(Error::new("Invalid empty or multiline ignore pattern"))
            }
        };
        if path.starts_with(&self.root) {
            reject_symlink_components(&self.root, &path)?;
        } else if scope == IgnoreScope::User {
            reject_symlink_destination(&self.root, &path)?;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let len = file.metadata()?.len();
        if len > 0 {
            let bytes = std::fs::read(&path)?;
            if !bytes.ends_with(b"\n") {
                file.write_all(b"\n")?;
            }
        }
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
        Ok(())
    }

    /// The per-worktree Git directory.
    pub fn git_dir(&self) -> &Path {
        &self.git_dir
    }

    /// The shared Git directory, identical across linked worktrees.
    pub fn common_dir(&self) -> &Path {
        &self.common_dir
    }

    /// Resolve COMMIT_EDITMSG in this worktree's actual, canonical Git directory.
    /// Linked worktrees must not use the common repository's message file.
    pub fn commit_message_path(&self) -> Result<PathBuf> {
        let args = ["rev-parse", "--absolute-git-dir"];
        let mut command = git_command(&self.root, &args, Stdio::null(), &[]);
        let out = runner::run_owned(&mut command, None)
            .map_err(|error| Error::new(format!("Cannot resolve editor directory: {error}")))?;
        let status = out
            .status
            .ok_or_else(|| Error::new("Editor directory lookup was cancelled"))?;
        if !status.success() {
            return Err(Error::git(GitFailure::from_output(
                &args,
                status,
                &out.stderr,
            )));
        }
        if out.outcome.stdout_truncated
            || out.outcome.stderr_truncated
            || out.outcome.drain_incomplete
        {
            return Err(Error::new(
                "Editor directory lookup returned incomplete output",
            ));
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

    /// Resolve Git's worktree-specific interactive rebase todo path.
    pub fn rebase_todo_path(&self) -> Result<PathBuf> {
        let args = [
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "rebase-merge/git-rebase-todo",
        ];
        let mut command = git_command(&self.root, &args, Stdio::null(), &[]);
        let out = runner::run_owned(&mut command, None)
            .map_err(|error| Error::new(format!("Cannot resolve rebase todo: {error}")))?;
        let status = out
            .status
            .ok_or_else(|| Error::new("Rebase todo lookup was cancelled"))?;
        if !status.success() {
            return Err(Error::git(GitFailure::from_output(
                &args,
                status,
                &out.stderr,
            )));
        }
        if out.outcome.stdout_truncated
            || out.outcome.stderr_truncated
            || out.outcome.drain_incomplete
        {
            return Err(Error::new("Rebase todo lookup returned incomplete output"));
        }
        let output = std::str::from_utf8(&out.stdout)
            .map_err(|_| Error::new("Git rebase todo path is not UTF-8"))?;
        let path = output
            .strip_suffix('\n')
            .filter(|path| !path.is_empty() && !path.contains(['\n', '\r', '\0']))
            .ok_or_else(|| Error::new("Git returned an invalid rebase todo path"))?;
        let path = Path::new(path);
        if !path.is_absolute() {
            return Err(Error::new("Git returned a relative rebase todo path"));
        }
        Ok(path.to_path_buf())
    }

    /// Read the todo owned by an active paused merge-backend rebase.
    /// Capture todo bytes, metadata, and directory identity as one validated snapshot.
    pub(crate) fn read_rebase_todo_snapshot(&self) -> Result<RebaseTodoSnapshot> {
        if self.rebase_state()?.is_none() || !self.git_dir.join("rebase-merge").is_dir() {
            return Err(Error::new("no paused merge-backend rebase owns the todo"));
        }
        let git_dir = std::fs::canonicalize(&self.git_dir)?;
        let merge = git_dir.join("rebase-merge");
        let merge_identity = std::sync::Arc::new(same_file::Handle::from_path(&merge)?);
        let mut metadata = Vec::new();
        for name in ["orig-head", "onto", "head-name"] {
            metadata.push((name.to_owned(), std::fs::read(merge.join(name))?));
        }
        let path = self.rebase_todo_path()?;
        if path
            .parent()
            .and_then(|parent| std::fs::canonicalize(parent).ok())
            .as_deref()
            != Some(merge.as_path())
        {
            return Err(Error::new(
                "rebase todo path is outside the paused operation",
            ));
        }
        let source = std::fs::read(path)?;
        let owner = RebaseTodoOwner {
            git_dir,
            metadata,
            merge_identity,
        };
        if self.rebase_todo_identity()? != owner
            || std::fs::read(self.rebase_todo_path()?)? != source
        {
            return Err(Error::new(
                "rebase todo operation changed while it was loaded",
            ));
        }
        Ok(RebaseTodoSnapshot { source, owner })
    }

    /// Capture the active operation identity, including the merge directory instance.
    pub(crate) fn rebase_todo_identity(&self) -> Result<RebaseTodoOwner> {
        if self.rebase_state()?.is_none() || !self.git_dir.join("rebase-merge").is_dir() {
            return Err(Error::new("no paused merge-backend rebase owns the todo"));
        }
        let git_dir = std::fs::canonicalize(&self.git_dir)?;
        let merge = git_dir.join("rebase-merge");
        let merge_identity = std::sync::Arc::new(same_file::Handle::from_path(&merge)?);
        let mut metadata = Vec::new();
        for name in ["orig-head", "onto", "head-name"] {
            metadata.push((name.to_owned(), std::fs::read(merge.join(name))?));
        }
        Ok(RebaseTodoOwner {
            git_dir,
            metadata,
            merge_identity,
        })
    }

    /// Atomically replace the todo of this repository's paused merge-backend rebase.
    pub(crate) fn write_rebase_todo(
        &self,
        identity: &RebaseTodoOwner,
        expected: &[u8],
        contents: &[u8],
    ) -> Result<()> {
        if !self.rebase_state()?.is_some() || !self.git_dir.join("rebase-merge").is_dir() {
            return Err(Error::new("no paused merge-backend rebase owns the todo"));
        }
        let path = self.rebase_todo_path()?;
        let merge_dir = std::fs::canonicalize(self.git_dir.join("rebase-merge"))?;
        if path
            .parent()
            .and_then(|parent| std::fs::canonicalize(parent).ok())
            .as_deref()
            != Some(merge_dir.as_path())
        {
            return Err(Error::new(
                "rebase todo path is outside the paused operation",
            ));
        }
        let lock_path = merge_dir.join("git-rebase-todo.lock");
        let lock_file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    Error::new("rebase todo is locked by another process")
                } else {
                    Error::from(error)
                }
            })?;
        let _lock = RebaseTodoLock {
            path: lock_path,
            file: lock_file,
        };
        if &self.rebase_todo_identity()? != identity {
            return Err(Error::new(
                "rebase todo operation changed since it was loaded",
            ));
        }
        if std::fs::read(&path)? != expected {
            return Err(Error::new("rebase todo changed since it was loaded"));
        }
        let mut temporary = tempfile::NamedTempFile::new_in(&merge_dir)?;
        std::io::Write::write_all(&mut temporary, contents)?;
        std::io::Write::flush(&mut temporary)?;
        if &self.rebase_todo_identity()? != identity || std::fs::read(&path)? != expected {
            return Err(Error::new("rebase todo changed since it was loaded"));
        }
        temporary.persist(&path).map_err(|error| error.error)?;
        Ok(())
    }

    /// Read HEAD's committer timestamp for Magit-style metadata-preserving amendments.
    pub fn head_committer_date(&self) -> Result<String> {
        let date = self.run(&["show", "-s", "--format=%cI", "HEAD"])?;
        let date = date.trim();
        if date.is_empty() {
            return Err(Error::new("Cannot read HEAD committer date"));
        }
        Ok(date.to_string())
    }

    /// Whether any fetched remote-tracking branch contains the current HEAD.
    pub fn head_is_published(&self) -> Result<bool> {
        Ok(!self
            .run(&[
                "for-each-ref",
                "--format=%(refname)",
                "--contains=HEAD",
                "refs/remotes/",
            ])?
            .trim()
            .is_empty())
    }

    /// Return a coherent page of log records; filtering values are individual
    /// argv entries and paths are separated from revisions by `--`.
    pub fn log(&self, options: &GitLogOptions) -> Result<GitLogPage> {
        if options.merges_only && options.no_merges {
            return Err(Error::new(
                "Log filters cannot include and exclude merges at the same time.",
            ));
        }
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
            ("--until", options.until.as_ref()),
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
        if options.merges_only {
            args.push("--merges".into());
        }
        if options.no_merges {
            args.push("--no-merges".into());
        }
        if options.first_parent {
            args.push("--first-parent".into());
        }
        let revision = options.revision.as_deref().unwrap_or("HEAD");
        if let Some(selection) = &options.ref_selection {
            if selection != "local" && selection != "branches" && selection != "all" {
                return Err(Error::new("Unsupported log ref selection."));
            }
        }
        if options.revision.is_none()
            && options.ref_selection.is_none()
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
        if let Some(selection) = &options.ref_selection {
            if selection == "all" {
                args.push("--all".into());
            } else {
                args.push("--branches".into());
                if selection == "branches" {
                    args.push("--remotes".into());
                }
            }
        }
        args.push("--end-of-options".into());
        if options.ref_selection.is_none() {
            args.push(revision.to_string());
        }
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

    /// Stable identity for the selected worktree and Git directory.
    pub fn repository_identity(&self) -> Result<String> {
        Ok(format!(
            "{}\0{}",
            std::fs::canonicalize(&self.root)?.display(),
            std::fs::canonicalize(&self.git_dir)?.display()
        ))
    }

    /// Identify HEAD by its resolved commit and exact symbolic ref, or detached state.
    pub fn head_identity(&self) -> Result<(String, Option<String>)> {
        let commit = self.resolve_commit("HEAD")?;
        let reference = self
            .run(&["symbolic-ref", "--quiet", "HEAD"])
            .ok()
            .map(|value| value.trim().to_owned());
        Ok((commit, reference))
    }

    /// Capture exact index bytes and require a clean, operation-free worktree.
    pub fn rebase_launch_preconditions(&self) -> Result<(String, String)> {
        if self.rebase_state()?.is_some() {
            return Err(Error::new(
                "cannot prepare rebase while a Git operation is in progress",
            ));
        }
        let status = self.run(&["status", "--porcelain", "--untracked-files=all"])?;
        if !status.is_empty() {
            return Err(Error::new(
                "cannot prepare rebase with staged, unstaged, or untracked changes",
            ));
        }
        let index = self.run(&["rev-parse", "--git-path", "index"])?;
        let path = PathBuf::from(index.trim());
        let path = if path.is_absolute() {
            path
        } else {
            self.root.join(path)
        };
        let bytes = std::fs::read(path)?;
        use sha2::{Digest, Sha256};
        let index = format!("{:x}", Sha256::digest(&bytes));
        let worktree = self.run(&["diff", "--binary", "HEAD", "--"])?;
        Ok((index, worktree))
    }

    /// Capture exact staged state after refusing repository state an instant rebase cannot own.
    pub(crate) fn instant_rebase_preconditions(&self) -> Result<InstantRebaseState> {
        if self.git_dir.join("rebase-merge").exists()
            || self.git_dir.join("rebase-apply").exists()
            || self.git_dir.join("sequencer").exists()
            || [
                "MERGE_HEAD",
                "CHERRY_PICK_HEAD",
                "REVERT_HEAD",
                "BISECT_LOG",
            ]
            .iter()
            .any(|name| self.git_dir.join(name).exists())
        {
            return Err(Error::new(
                "cannot prepare instant autosquash while a Git operation is in progress",
            ));
        }
        let status = self.status_listing()?;
        if !status.untracked.is_empty() {
            return Err(Error::new(
                "cannot prepare instant autosquash with untracked files",
            ));
        }
        if !status.unstaged.is_empty() {
            return Err(Error::new(
                "cannot prepare instant autosquash with tracked unstaged changes",
            ));
        }
        if status.staged.is_empty() {
            return Err(Error::new("instant autosquash requires a staged change"));
        }
        let staged_patch = self.run(&["diff", "--cached", "--binary", "HEAD", "--"])?;
        if staged_patch.is_empty() {
            return Err(Error::new("instant autosquash requires a staged change"));
        }
        let index = self.run(&["rev-parse", "--git-path", "index"])?;
        let path = PathBuf::from(index.trim());
        let path = if path.is_absolute() {
            path
        } else {
            self.root.join(path)
        };
        use sha2::{Digest, Sha256};
        let index = format!("{:x}", Sha256::digest(std::fs::read(path)?));
        Ok(InstantRebaseState {
            index,
            staged_patch,
        })
    }

    /// Launch Git's interactive rebase using a validated external todo plan.
    /// Launch interactive rebase with either an exact prepared todo or Git-preserved autosquash todo.
    pub fn launch_interactive_rebase(
        &self,
        base: &str,
        plan: Option<&[u8]>,
        policy: crate::rebase_sequence_editor::SequenceEditorPolicy,
        executable: &Path,
    ) -> Result<()> {
        self.launch_interactive_rebase_with_env(base, plan, policy, executable, &[])
    }

    /// Launch interactive rebase with editor variables inherited by Git's replay callbacks.
    pub fn launch_interactive_rebase_with_env(
        &self,
        base: &str,
        plan: Option<&[u8]>,
        policy: crate::rebase_sequence_editor::SequenceEditorPolicy,
        executable: &Path,
        editor_env: &[(&str, &str)],
    ) -> Result<()> {
        let directory = tempfile::Builder::new()
            .prefix("magit-lsp-rebase-")
            .tempdir()?;
        let plan_path = if let Some(plan) = plan {
            let path = directory.path().join("todo plan");
            std::fs::write(&path, plan)?;
            Some(path)
        } else {
            None
        };
        let editor = crate::rebase_sequence_editor::command_for(executable, policy);
        let plan_path = plan_path.map(|path| path.to_string_lossy().into_owned());
        let mut command_args = vec![
            "-c",
            if policy == crate::rebase_sequence_editor::SequenceEditorPolicy::PreserveAutosquash {
                "rebase.autoSquash=true"
            } else {
                "rebase.autoSquash=false"
            },
            "rebase",
            "--interactive",
        ];
        if policy == crate::rebase_sequence_editor::SequenceEditorPolicy::PreserveAutosquash {
            command_args.push("--autosquash");
        } else {
            command_args.push("--no-autosquash");
        }
        command_args.push(base);
        let args = command_args;
        let mut env = vec![("GIT_SEQUENCE_EDITOR", editor.as_str())];
        env.extend_from_slice(editor_env);
        if let Some(path) = plan_path.as_deref() {
            env.push((crate::rebase_sequence_editor::PLAN_FILE_ENV, path));
        }
        let started = Instant::now();
        let mut command = git_command(&self.root, &args, Stdio::null(), &env);
        let out = runner::run_owned(&mut command, None)
            .map_err(|error| Error::new(format!("failed to run git: {error}")))?;
        let status = out
            .status
            .ok_or_else(|| Error::new("git command was cancelled; process reap is incomplete"))?;
        log_finished(
            &self.root,
            &args,
            status,
            started.elapsed(),
            &out.stdout,
            &out.stderr,
        );
        if !status.success() {
            return Err(Error::git(GitFailure::from_output(
                &args,
                status,
                &out.stderr,
            )));
        }
        Ok(())
    }

    /// Verify exact autosquash marker subject, identity, and expected captured HEAD parent.
    pub fn verify_autosquash_marker(
        &self,
        marker: &str,
        expected_parent: &str,
        expected_subject: &str,
    ) -> Result<()> {
        let head = self.run(&["rev-parse", "HEAD"])?;
        let parent = self.run(&["rev-parse", &format!("{marker}^")])?;
        let subject = self.run(&["show", "-s", "--format=%s", marker])?;
        let subject = subject.strip_suffix('\n').unwrap_or(&subject);
        if head.trim() != marker || parent.trim() != expected_parent || subject != expected_subject
        {
            return Err(Error::new(
                "created autosquash marker does not match captured target and HEAD",
            ));
        }
        Ok(())
    }

    /// Return full commit IDs after an exclusive base, rejecting non-linear history.
    pub fn linear_replay_membership(&self, base: &str, head: &str) -> Result<Vec<String>> {
        let ancestry = self.run(&["merge-base", "--is-ancestor", base, head]);
        if ancestry.is_err() {
            return Err(Error::new("selected base is not an ancestor of HEAD"));
        }
        let output = self.run(&[
            "rev-list",
            "--reverse",
            "--topo-order",
            "--parents",
            &format!("{base}..{head}"),
        ])?;
        let mut result = Vec::new();
        let mut expected_parent = base;
        for line in output.lines() {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() != 2 || fields[1] != expected_parent {
                return Err(Error::new(
                    "selected range is not a supported linear history",
                ));
            }
            result.push(fields[0].to_owned());
            expected_parent = fields[0];
        }
        if expected_parent != head {
            return Err(Error::new("selected range does not end at captured HEAD"));
        }
        Ok(result)
    }

    /// Resolve ordered hexadecimal todo tokens in one batch to per-token outcomes.
    pub fn resolve_todo_commits(&self, tokens: &[String]) -> Result<Vec<Result<String>>> {
        for (index, token) in tokens.iter().enumerate() {
            if token.len() < 4 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(Error::new(format!(
                    "todo batch row {} identity must be at least four hexadecimal characters",
                    index + 1
                )));
            }
        }
        let input = tokens
            .iter()
            .map(|token| format!("{token}\n"))
            .collect::<String>();
        let output = self.command_with_input(
            &["cat-file", "--batch-check=%(objectname) %(objecttype)"],
            input.as_bytes(),
        )?;
        decode_todo_commit_batch(&output, tokens.len())
    }

    /// Read the subject belonging to a validated commit object ID.
    pub fn commit_subject(&self, oid: &str) -> Result<String> {
        self.run(&["show", "-s", "--format=%s", oid])
            .map(|subject| subject.trim_end().to_string())
    }

    /// Whether `value` resolves to a revision in this repository.
    pub fn verify_revision(&self, value: &str) -> bool {
        self.run(&["rev-parse", "--verify", "--end-of-options", value])
            .is_ok()
    }

    /// Whether `value` names a configured remote.
    pub fn verify_remote(&self, value: &str) -> bool {
        self.run(&["remote", "get-url", value]).is_ok()
    }

    /// Whether `value` resolves as a revision or names a configured remote.
    pub fn verify_ref_or_remote(&self, value: &str) -> bool {
        self.verify_revision(value) || self.verify_remote(value)
    }

    /// Every local branch, remote-tracking branch and tag name.
    pub fn ref_names(&self) -> Result<Vec<String>> {
        let output = self.run(&[
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ])?;
        Ok(output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect())
    }

    /// Every configured remote name.
    pub fn remote_names(&self) -> Result<Vec<String>> {
        let output = self.run(&["remote"])?;
        Ok(output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect())
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
        let options = crate::config::LoadOptions {
            repo_path: Some(self.root.clone()),
            ..Default::default()
        };
        let policy = crate::config::load_config(&options)
            .map(|config| config.presentation_files)
            .unwrap_or_else(|_| vec![".magit".to_owned()]);
        self.status_listing_excluding(&policy)
    }

    /// Set or clear Git's skip-worktree bit for explicitly selected tracked paths.
    pub fn set_skip_worktree(&self, paths: &[&str], enabled: bool) -> Result<()> {
        self.set_index_flag(paths, enabled, "--skip-worktree", "--no-skip-worktree")
    }

    /// Set or clear Git's assume-unchanged bit for explicitly selected tracked paths.
    pub fn set_assume_unchanged(&self, paths: &[&str], enabled: bool) -> Result<()> {
        self.set_index_flag(
            paths,
            enabled,
            "--assume-unchanged",
            "--no-assume-unchanged",
        )
    }

    fn set_index_flag(&self, paths: &[&str], enabled: bool, set: &str, clear: &str) -> Result<()> {
        if paths.is_empty() {
            return Err(Error::new("at least one tracked path is required"));
        }
        let mut literals = Vec::with_capacity(paths.len());
        for path in paths {
            let p = Path::new(path);
            if path.is_empty()
                || p.is_absolute()
                || p.components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(Error::new(format!("invalid tracked path: {path}")));
            }
            if self
                .run(&[
                    "--literal-pathspecs",
                    "ls-files",
                    "--error-unmatch",
                    "-z",
                    "--",
                    path,
                ])
                .is_err()
            {
                return Err(Error::new(format!("path is not tracked: {path}")));
            }
            literals.push(path.to_string());
        }
        let mut owned = vec![
            "--literal-pathspecs".to_string(),
            "update-index".to_string(),
            if enabled { set } else { clear }.to_string(),
            "--".to_string(),
        ];
        owned.extend(literals);
        let args = owned.iter().map(String::as_str).collect::<Vec<_>>();
        self.run(&args)?;
        Ok(())
    }

    /// Read status while excluding configured presentation-document basenames.
    pub fn status_listing_excluding(&self, presentation_files: &[String]) -> Result<Status> {
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
        let mut status = self.parse_status(&raw, repo_path, presentation_files)?;
        // HEAD presence answers the same for every entry of this read, and the
        // porcelain branch header already carries it, so no per-file
        // `rev-parse --verify HEAD` spawn is needed.
        let head_present = status.head.is_some();
        for file in status
            .staged
            .iter_mut()
            .chain(&mut status.unstaged)
            .chain(&mut status.untracked)
        {
            file.mutation_identity = Some(self.file_mutation_identity_in(
                &file.path,
                file.orig_path.as_deref(),
                true,
                head_present,
            )?);
        }
        Ok(status)
    }

    /// Capture blobs/modes for both endpoints, optionally including non-following
    /// worktree content. Unstage authorization does not inspect the worktree.
    pub fn file_mutation_identity(
        &self,
        path: &str,
        original: Option<&str>,
        worktree: bool,
    ) -> Result<FileMutationIdentity> {
        let head_present = self.head_present()?;
        self.file_mutation_identity_in(path, original, worktree, head_present)
    }

    /// Whether `HEAD` resolves to a commit, probed once per caller.
    ///
    /// Absence is the quiet exit-1 failure of `rev-parse --verify --quiet`; any
    /// other failure propagates.
    fn head_present(&self) -> Result<bool> {
        match self.run(&["rev-parse", "--verify", "--quiet", "HEAD"]) {
            Ok(_) => Ok(true),
            Err(error)
                if error
                    .git
                    .as_ref()
                    .is_some_and(|failure| failure.exit_code == 1 && failure.stderr.is_empty()) =>
            {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    /// Capture identities for one endpoint with a HEAD-presence answer in hand.
    ///
    /// `head_present` is repository-global, so [`Self::status_listing_excluding`]
    /// answers it once from the porcelain branch header and reuses it for every
    /// entry and rename origin. The `ls-tree`/`ls-files` lookups stay per path:
    /// both are pathspec lookups whose directory-descendant and per-path
    /// semantics a batched query would change.
    fn file_mutation_identity_in(
        &self,
        path: &str,
        original: Option<&str>,
        worktree: bool,
        head_present: bool,
    ) -> Result<FileMutationIdentity> {
        let head = if head_present {
            self.run(&["ls-tree", "-r", "-z", "HEAD", "--", path])?
        } else {
            String::new()
        };
        let literal = format!(":(literal){path}");
        let index = self.run(&["ls-files", "--stage", "-z", "--", &literal])?;
        let original = original
            .map(|origin| {
                self.file_mutation_identity_in(origin, None, worktree, head_present)
                    .map(|identity| (origin.to_string(), Box::new(identity)))
            })
            .transpose()?;
        let worktree = if worktree {
            worktree_identity(&self.root.join(path))?
        } else {
            String::new()
        };
        Ok(FileMutationIdentity {
            head,
            index,
            worktree,
            original,
        })
    }

    fn parse_status(
        &self,
        raw: &str,
        repo_path: String,
        presentation_files: &[String],
    ) -> Result<Status> {
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
                        mutation_identity: None,
                    });
                }
                b'?' => {
                    if let Some(path) = token.strip_prefix("? ") {
                        // Plain presentation documents are editor state, not repository content.
                        if crate::config::is_presentation_file(Path::new(path), presentation_files)
                        {
                            continue;
                        }
                        status.untracked.push(FileEntry {
                            path: path.to_string(),
                            status: "?".to_string(),
                            orig_path: None,
                            hunks: Vec::new(),
                            mutation_identity: None,
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
                mutation_identity: None,
            });
        }
        if let Some(unstaged) = side_status(bytes.get(1).copied()) {
            status.unstaged.push(FileEntry {
                path: path.to_string(),
                status: unstaged,
                orig_path,
                hunks: Vec::new(),
                mutation_identity: None,
            });
        }
        Ok(())
    }

    /// Unified diff of the unstaged changes to `path` (index vs working tree).
    pub fn diff_unstaged(&self, path: &str) -> Result<String> {
        let origin = self.rename_origin(path, false)?;
        self.diff_side(path, false, origin.as_deref())
    }

    /// Unified diff of the staged changes to `path` (HEAD vs index).
    pub fn diff_staged(&self, path: &str) -> Result<String> {
        let origin = self.rename_origin(path, true)?;
        self.diff_side(path, true, origin.as_deref())
    }

    /// Diff one side of `path`, including `origin` when it is a rename side.
    ///
    /// Path limiting disables Git's rename detection, so a rename pair has to
    /// be passed to `git diff` together or it looks like a new file. Callers
    /// pass the origin from [`Self::rename_origin`] or a snapshot-scoped
    /// [`RenameOrigins`] lookup.
    fn diff_side(&self, path: &str, staged: bool, origin: Option<&str>) -> Result<String> {
        record_diff_call(&self.root.join(path));
        let mut args = vec!["-c", "core.quotepath=false", "diff"];
        if staged {
            args.push("--cached");
        }
        args.extend(["-M", "-U3", "--"]);
        match origin {
            Some(origin) => {
                args.push(origin);
                args.push(path);
            }
            None => args.push(path),
        }
        self.run(&args)
    }

    /// Apply the whole hunk with `hunk_id` for `path`, targeting the index or
    /// the worktree.
    ///
    /// The diff is re-read first, so the hunk is located by content even when
    /// lines above it shifted since the status was rendered.
    fn hunk_op(&self, path: &str, hunk_id: &str, staged: bool, worktree: bool) -> Result<()> {
        let layout = self.file_layout(path, staged)?;
        let hunk = layout
            .hunks
            .iter()
            .find(|h| h.hunk.id == hunk_id)
            .ok_or_else(|| Error::hunk_not_found(path))?;
        let patch = format!("{}{}", layout.header, patch::full_hunk_patch(&hunk.hunk));
        if worktree {
            self.apply_worktree(&patch, true)
        } else {
            self.apply_index(&patch, staged)
        }
    }

    /// Stage the whole unstaged hunk with `hunk_id` for `path`.
    ///
    /// The diff is re-read first, so the hunk is located by content even when
    /// lines above it shifted since the status was rendered.
    pub fn stage_hunk(&self, path: &str, hunk_id: &str) -> Result<()> {
        self.hunk_op(path, hunk_id, false, false)
    }

    /// Unstage the whole staged hunk with `hunk_id` for `path`.
    pub fn unstage_hunk(&self, path: &str, hunk_id: &str) -> Result<()> {
        self.hunk_op(path, hunk_id, true, false)
    }

    /// Discard an unstaged hunk by its content-based identity.
    pub fn discard_hunk(&self, path: &str, hunk_id: &str) -> Result<()> {
        self.hunk_op(path, hunk_id, false, true)
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
            let metadata = std::fs::symlink_metadata(&full)?;
            if metadata.is_dir() {
                std::fs::remove_dir_all(full)?;
            } else {
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

    /// Rename and copy origins for one side of the working tree.
    ///
    /// `staged` compares HEAD against the index; the other side compares the
    /// index against the working tree. One whole-repository scan serves every
    /// file body of a render, which then looks its origin up in the returned
    /// map instead of rescanning; one-shot dispatch-time actions keep using
    /// [`Self::rename_origin`].
    pub fn rename_origins(&self, staged: bool) -> Result<RenameOrigins> {
        let mut args = vec!["diff"];
        if staged {
            args.push("--cached");
        }
        args.extend(["--name-status", "-z", "--find-renames"]);
        let raw = self.run(&args)?;
        let mut origins = HashMap::new();
        let mut tokens = raw.split('\0');
        while let Some(status) = tokens.next() {
            if status.is_empty() {
                continue;
            }
            let first = tokens.next().unwrap_or("");
            if status.starts_with('R') || status.starts_with('C') {
                let second = tokens.next().unwrap_or("");
                origins
                    .entry(second.to_string())
                    .or_insert_with(|| first.to_string());
            }
        }
        Ok(RenameOrigins { origins })
    }

    /// The old path when `path` is the new side of a rename or copy.
    fn rename_origin(&self, path: &str, staged: bool) -> Result<Option<String>> {
        Ok(self
            .rename_origins(staged)?
            .origin(path)
            .map(str::to_string))
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

    /// The diff hunks for one file, reusing a snapshot-scoped origin map.
    ///
    /// Same hunks as [`Self::file_hunks`] without the per-file whole-repository
    /// rename scan; `origins` must be the map for the same side of one status
    /// read.
    pub fn file_hunks_with_origins(
        &self,
        path: &str,
        staged: bool,
        origins: &RenameOrigins,
    ) -> Result<Vec<Hunk>> {
        let diff = self.diff_side(path, staged, origins.origin(path))?;
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
            return Err(Error::new(
                "git apply was cancelled; process reap is incomplete",
            ));
        };
        log_finished(
            &self.root,
            &args,
            status,
            started.elapsed(),
            &out.stdout,
            &out.stderr,
        );
        if !status.success() {
            let mut failure = GitFailure::from_output(&args, status, &out.stderr);
            failure.warning = warn_if_git_is_below_recommendation(&self.root, &[], &status);
            return Err(Error::git(failure));
        }
        if out.outcome.stdout_truncated
            || out.outcome.stderr_truncated
            || out.outcome.drain_incomplete
        {
            return Err(Error::new(
                "git apply completed with incomplete process output",
            ));
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
        let out = runner::run_owned(&mut command, None)
            .map_err(|e| Error::new(format!("failed to run git: {e}")))?;
        let Some(status) = out.status else {
            return Err(Error::new("git commit cancellation is incomplete; process ownership remains with the reaper. Git lock files may remain; no locks were removed."));
        };
        log_finished(
            &self.root,
            &refs,
            status,
            started.elapsed(),
            &out.stdout,
            &out.stderr,
        );
        let recorded = out.outcome.cancelled
            && before.as_ref().is_some_and(|before| {
                commit_head(&self.root).is_some_and(|after| !after.is_empty() && &after != before)
            });
        let cancellation_warning = out
            .outcome
            .cancelled
            .then(|| cancellation_lock_warning(&self.root));
        if !status.success() && !recorded {
            let mut failure = GitFailure::from_output(&refs, status, &out.stderr);
            failure.warning = cancellation_warning
                .or_else(|| warn_if_git_is_below_recommendation(&self.root, env, &status));
            return Err(Error::git(failure));
        }
        let mut warning = (out.outcome.stdout_truncated
            || out.outcome.stderr_truncated
            || out.outcome.drain_incomplete)
            .then(|| "git commit completed, but process output was incomplete".to_string());
        if let Some(locks) = cancellation_warning {
            warning = Some(format!("Git recorded a commit before cancellation; a hook may have been interrupted. {locks}"));
        }
        let read = runner::with_cancel(None, || self.run(&["log", "-1", "--format=%B"]));
        let message = match read {
            Ok(message) => Some(message.trim_end().to_string()),
            Err(error) => {
                warning = Some(format!(
                    "{}; commit message could not be read: {error}",
                    warning.unwrap_or_else(|| "commit succeeded".into())
                ));
                None
            }
        };
        Ok(CommitOutcome { message, warning })
    }

    fn run(&self, args: &[&str]) -> Result<String> {
        Self::command_in(&self.root, args)
    }

    fn command_with_input(&self, args: &[&str], input: &[u8]) -> Result<String> {
        let started = Instant::now();
        tracing::debug!(cwd = %self.root.display(), args = ?args, "git");
        let mut command = git_command(&self.root, args, Stdio::piped(), &[]);
        let out = runner::run_owned(&mut command, Some(input))
            .map_err(|e| Error::new(format!("failed to run git: {e}")))?;
        let Some(status) = out.status else {
            return Err(Error::new(
                "git command was cancelled; process reap is incomplete",
            ));
        };
        log_finished(
            &self.root,
            args,
            status,
            started.elapsed(),
            &out.stdout,
            &out.stderr,
        );
        if !status.success() {
            return Err(Error::git(GitFailure::from_output(
                args,
                status,
                &out.stderr,
            )));
        }
        if out.outcome.stdout_truncated
            || out.outcome.stderr_truncated
            || out.outcome.drain_incomplete
        {
            return Err(Error::new("git command completed with incomplete output"));
        }
        if out.outcome.stdin_error {
            return Err(Error::new("git command stdin write failed"));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
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
            return Err(Error::new(
                "git command was cancelled; process reap is incomplete",
            ));
        };
        log_finished(
            cwd,
            args,
            status,
            started.elapsed(),
            &out.stdout,
            &out.stderr,
        );
        if !status.success() {
            let mut failure = GitFailure::from_output(args, status, &out.stderr);
            failure.warning = warn_if_git_is_below_recommendation(cwd, env, &status);
            return Err(Error::git(failure));
        }
        if out.outcome.stdout_truncated
            || out.outcome.stderr_truncated
            || out.outcome.drain_incomplete
        {
            return Err(Error::new("git command completed with incomplete output"));
        }
        if out.outcome.stdin_error {
            return Err(Error::new("git command stdin write failed"));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

fn is_missing_config_key(error: &crate::Error) -> bool {
    error
        .git
        .as_ref()
        .is_some_and(|failure| failure.exit_code == 1)
}

fn is_missing_config_clear(error: &crate::Error) -> bool {
    error
        .git
        .as_ref()
        .is_some_and(|failure| failure.exit_code == 5)
}

// Diagnostics have their own cancellation deadline, never consume mutation
// intent, and use the same sole-Child owner as every other Git invocation.
// The 500ms probe budget is followed by the owner's bounded TERM/KILL/reap.
fn readonly_probe(root: &Path, args: &[&str]) -> Option<runner::OwnedOutput> {
    let cancel = runner::CancelHandle::new();
    let timer_cancel = cancel.clone();
    let (done, waiting) = std::sync::mpsc::channel();
    let timer = std::thread::spawn(move || {
        if waiting
            .recv_timeout(std::time::Duration::from_millis(500))
            .is_err()
        {
            timer_cancel.cancel();
        }
    });
    let mut args_with_flags = vec!["--no-optional-locks"];
    args_with_flags.extend_from_slice(args);
    let result = runner::with_cancel(Some(cancel), || {
        runner::run_owned(
            &mut git_command(root, &args_with_flags, Stdio::null(), &[]),
            None,
        )
    });
    let _ = done.send(());
    let _ = timer.join();
    let out = result.ok()?;
    (out.status.is_some()
        && !out.outcome.cancelled
        && !out.outcome.stdout_truncated
        && !out.outcome.stderr_truncated
        && !out.outcome.drain_incomplete)
        .then_some(out)
}

// An empty, successfully observed state denotes unborn HEAD; None is unknown.
fn commit_head(root: &Path) -> Option<String> {
    let out = readonly_probe(root, &["rev-parse", "--verify", "--quiet", "HEAD"])?;
    if out.status?.code() == Some(1) && out.stdout.is_empty() && out.stderr.is_empty() {
        return Some(String::new());
    }
    if !out.status?.success() {
        return None;
    }
    let oid = std::str::from_utf8(&out.stdout).ok()?.trim();
    (matches!(oid.len(), 40 | 64) && oid.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| oid.to_string())
}

fn cancellation_lock_warning(root: &Path) -> String {
    let mut locks = Vec::new();
    if let Some(out) = readonly_probe(
        root,
        &["rev-parse", "--absolute-git-dir", "--git-common-dir"],
    ) {
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
                        for name in [
                            "index.lock",
                            "HEAD.lock",
                            "packed-refs.lock",
                            "config.lock",
                            "shallow.lock",
                        ] {
                            let path = dir.join(name);
                            if path.symlink_metadata().is_ok() {
                                locks.push(path);
                            }
                        }
                        let mut pending = vec![dir.join("refs")];
                        while let Some(dir) = pending.pop() {
                            if remaining == 0 || Instant::now() >= deadline {
                                break;
                            }
                            let Ok(entries) = std::fs::read_dir(dir) else {
                                continue;
                            };
                            for entry in entries {
                                if remaining == 0 || Instant::now() >= deadline {
                                    break;
                                }
                                remaining -= 1;
                                let Ok(entry) = entry else {
                                    continue;
                                };
                                if entry.file_type().is_ok_and(|t| t.is_dir()) {
                                    pending.push(entry.path());
                                } else if entry.path().extension().is_some_and(|e| e == "lock") {
                                    locks.push(entry.path());
                                }
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
        warning.push_str(
            &locks
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
        );
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

fn decode_todo_commit_batch(output: &str, expected: usize) -> Result<Vec<Result<String>>> {
    let lines = output.lines().collect::<Vec<_>>();
    if lines.len() != expected {
        return Err(Error::new(format!(
            "malformed todo object batch response: expected {expected} records, received {}",
            lines.len()
        )));
    }
    Ok(lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() != 2 {
                return Err(Error::new(format!(
                    "malformed todo object response on batch row {}",
                    index + 1
                )));
            }
            match fields[1] {
                "missing" => Err(Error::new("missing todo commit object")),
                "ambiguous" => Err(Error::new("ambiguous todo commit identity")),
                "commit"
                    if matches!(fields[0].len(), 40 | 64)
                        && fields[0].bytes().all(|byte| byte.is_ascii_hexdigit()) =>
                {
                    Ok(fields[0].to_owned())
                }
                "commit" => Err(Error::new(format!(
                    "malformed todo object response on batch row {}",
                    index + 1
                ))),
                "blob" | "tree" | "tag" => Err(Error::new("todo identity is not a commit")),
                _ => Err(Error::new(format!(
                    "malformed todo object response on batch row {}",
                    index + 1
                ))),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A repo whose root is never touched: these cases need no diffs.
    fn header_repo() -> GitRepo {
        GitRepo {
            root: PathBuf::from("/nonexistent"),
            git_dir: PathBuf::new(),
            common_dir: PathBuf::new(),
        }
    }

    fn parse(raw: &str) -> Status {
        header_repo()
            .parse_status(raw, "/repo".to_string(), &[".magit".to_owned()])
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

    #[test]
    fn todo_batch_decoder_rejects_malformed_incomplete_and_extra_records() {
        let oid = "0123456789abcdef0123456789abcdef01234567";
        let malformed = decode_todo_commit_batch(&format!("{oid} commit\nbroken\n"), 2).unwrap();
        assert!(malformed[1]
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("malformed"));
        assert!(malformed[1]
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("row 2"));

        for output in [
            format!("{oid} commit\n"),
            format!("{oid} commit\n{oid} commit\n{oid} commit\n"),
        ] {
            let message = decode_todo_commit_batch(&output, 2)
                .unwrap_err()
                .to_string();
            assert!(message.contains("malformed"), "unreadable error: {message}");
            assert!(
                message.contains("expected 2 records"),
                "missing cardinality: {message}"
            );
        }
    }
}
