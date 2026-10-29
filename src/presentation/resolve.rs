//! Repository candidates discovered for Plain document initialization.

use std::path::{Path, PathBuf};
use crate::git::GitRepo;

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
        return GitRepo::discover(path).ok().flatten().map(repository).map_or(Resolution::None, Resolution::Resolved);
    }
    let workspace_candidates = unique(workspaces.iter().filter_map(|p| GitRepo::discover(p).ok().flatten().map(repository)));
    if workspace_candidates.len() == 1 {
        return Resolution::Resolved(workspace_candidates[0].clone());
    }
    if workspace_candidates.len() > 1 {
        return Resolution::Choose(workspace_candidates);
    }
    if let Some(path) = document {
        if let Some(repo) = GitRepo::discover(path).ok().flatten().map(repository) {
            return Resolution::Resolved(repo);
        }
    }
    Resolution::None
}

fn repository(repo: GitRepo) -> Repository {
    Repository { root: repo.root().to_owned(), git_dir: repo.git_dir().to_owned(), common_dir: repo.common_dir().to_owned() }
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
