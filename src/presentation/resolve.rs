//! Repository candidates discovered for Plain document initialization.

use std::path::{Path, PathBuf};

/// Canonical identity and worktree paths reported by Git.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repository {
    /// Canonical worktree root, used for session state.
    pub root: PathBuf,
    /// Per-worktree Git directory (distinct for linked worktrees).
    pub git_dir: PathBuf,
    /// Shared Git directory (common to linked worktrees).
    pub common_dir: PathBuf,
}

/// Resolution never chooses between multiple repositories implicitly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// One repository was unambiguously selected.
    Resolved(Repository),
    /// No repository applies; candidates can later be presented to the user.
    Choose(Vec<Repository>),
    /// No repository candidate was found.
    None,
}

/// Resolve a document using explicit path, workspace roots, then document ancestry.
pub fn resolve(explicit: Option<&Path>, workspaces: &[PathBuf], document: Option<&Path>) -> Resolution {
    if let Some(path) = explicit {
        return GitRepository::discover(path).map_or(Resolution::None, Resolution::Resolved);
    }
    let workspace_candidates = unique(workspaces.iter().filter_map(|p| GitRepository::discover(p)));
    if workspace_candidates.len() == 1 {
        return Resolution::Resolved(workspace_candidates[0].clone());
    }
    if workspace_candidates.len() > 1 {
        return Resolution::Choose(workspace_candidates);
    }
    if let Some(path) = document {
        if let Some(repo) = GitRepository::discover(path) {
            return Resolution::Resolved(repo);
        }
    }
    Resolution::None
}

/// Git-owned repository discovery, including linked-worktree identity.
pub struct GitRepository;

impl GitRepository {
    /// Discover the repository containing a directory or file path.
    pub fn discover(path: &Path) -> Option<Repository> {
        let cwd = if path.is_dir() { path } else { path.parent()? };
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--path-format=absolute", "--show-toplevel", "--absolute-git-dir", "--git-common-dir"])
            .current_dir(cwd).output().ok()?;
        if !output.status.success() { return None; }
        let text = String::from_utf8(output.stdout).ok()?;
        let mut lines = text.lines();
        let root = canonical(lines.next()?)?;
        let git_dir = canonical(lines.next()?)?;
        let common_dir = canonical(lines.next()?)?;
        Some(Repository { root, git_dir, common_dir })
    }
}

fn canonical(path: &str) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

fn unique(repositories: impl Iterator<Item = Repository>) -> Vec<Repository> {
    let mut result = Vec::new();
    for repo in repositories {
        if !result.iter().any(|existing: &Repository| existing.root == repo.root) {
            result.push(repo);
        }
    }
    result
}
