//! Captured, read-only authority for a future instant autosquash operation.

use crate::git::GitRepo;
use crate::rebase_sequence_editor::SequenceEditorPolicy;
use crate::runner;
use crate::{Error, Result};
use std::path::Path;

/// Phase reached while executing a captured instant autosquash.
#[derive(Debug, PartialEq, Eq)]
pub enum InstantRebaseOutcome {
    /// Captured authority became stale before any mutation.
    Refused(String),
    /// Git failed to create the marker commit.
    MarkerFailed(String),
    /// Marker exists, but autosquash launch or completion failed.
    RebaseStopped {
        marker: String,
        phase: String,
        error: String,
    },
    /// Autosquash completed; Git's commit machinery may report a warning.
    Completed {
        marker: String,
        warning: Option<String>,
    },
}

/// Autosquash operation selected before repository execution begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstantAutosquashKind {
    Fixup,
    Squash,
}

/// Exact repository authority captured for one future instant autosquash operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstantRebaseAuthority {
    kind: InstantAutosquashKind,
    target: String,
    base: String,
    head: String,
    head_reference: Option<String>,
    membership: Vec<String>,
    repository: String,
    index: String,
    staged_patch: String,
}

impl InstantRebaseAuthority {
    /// Execute a captured autosquash, retaining every mutation after execution starts.
    pub fn execute(
        &self,
        repo: &GitRepo,
        executable: &Path,
        editor_env: &[(&str, &str)],
    ) -> InstantRebaseOutcome {
        let cancel = runner::current_cancel();
        let stopped = |marker: String| InstantRebaseOutcome::RebaseStopped {
            marker,
            phase: "cancelled".into(),
            error: "operation cancelled".into(),
        };
        if cancel
            .as_ref()
            .is_some_and(runner::CancelHandle::was_requested)
        {
            return stopped(String::new());
        }
        if let Err(error) = self.validate(repo) {
            return InstantRebaseOutcome::Refused(error.to_string());
        }
        if cancel
            .as_ref()
            .is_some_and(runner::CancelHandle::was_requested)
        {
            return stopped(String::new());
        }
        let target = self.target.clone();
        let marker_prefix = match self.kind {
            InstantAutosquashKind::Fixup => "fixup!",
            InstantAutosquashKind::Squash => "squash!",
        };
        let marker_message = format!("{marker_prefix} {target}");
        let marker_arg = format!("--message={marker_message}");
        let commit = match repo.commit(&[&marker_arg], &[]) {
            Ok(outcome) => outcome,
            Err(error) => return InstantRebaseOutcome::MarkerFailed(error.to_string()),
        };
        let marker = match repo.head_identity() {
            Ok((head, _)) => head,
            Err(error) => {
                return InstantRebaseOutcome::RebaseStopped {
                    marker: String::new(),
                    phase: "marker verification".into(),
                    error: error.to_string(),
                }
            }
        };
        if let Err(error) = repo.verify_autosquash_marker(&marker, &self.head, &marker_message) {
            return InstantRebaseOutcome::RebaseStopped {
                marker,
                phase: "marker verification".into(),
                error: error.to_string(),
            };
        }
        if cancel
            .as_ref()
            .is_some_and(runner::CancelHandle::was_requested)
        {
            return stopped(marker);
        }
        match repo.launch_interactive_rebase_with_env(
            &self.base,
            None,
            SequenceEditorPolicy::PreserveAutosquash,
            executable,
            editor_env,
        ) {
            Ok(()) => InstantRebaseOutcome::Completed {
                marker,
                warning: commit.warning,
            },
            Err(error) => InstantRebaseOutcome::RebaseStopped {
                marker,
                phase: "autosquash".into(),
                error: error.to_string(),
            },
        }
    }

    /// Capture a full-OID target and exact staged repository state without mutation.
    pub fn prepare(repo: &GitRepo, target: &str, kind: InstantAutosquashKind) -> Result<Self> {
        let repository = repo.repository_identity()?;
        let (head, head_reference) = repo.head_identity()?;
        let target = repo.resolve_commit(target)?;
        let base = repo
            .resolve_commit(&format!("{target}^"))
            .map_err(|_| Error::new("instant autosquash target must have a parent"))?;
        let state = repo.instant_rebase_preconditions()?;
        repo.linear_replay_membership(&target, &head)?;
        let membership = repo.linear_replay_membership(&base, &head)?;
        if membership.first() != Some(&target) {
            return Err(Error::new(
                "selected target is not in the captured linear history",
            ));
        }
        Ok(Self {
            kind,
            target,
            base,
            head,
            head_reference,
            membership,
            repository,
            index: state.index,
            staged_patch: state.staged_patch,
        })
    }

    /// Revalidate every captured authority input without mutating the repository.
    pub fn validate(&self, repo: &GitRepo) -> Result<()> {
        if repo.repository_identity()? != self.repository {
            return Err(Error::new(
                "repository identity changed since instant autosquash preparation",
            ));
        }
        let (head, reference) = repo.head_identity()?;
        if head != self.head {
            return Err(Error::new(
                "HEAD changed since instant autosquash preparation",
            ));
        }
        if reference != self.head_reference {
            return Err(Error::new(
                "HEAD attachment changed since instant autosquash preparation",
            ));
        }
        let state = repo.instant_rebase_preconditions()?;
        if state.index != self.index || state.staged_patch != self.staged_patch {
            return Err(Error::new(
                "index changed since instant autosquash preparation",
            ));
        }
        if repo.linear_replay_membership(&self.base, &self.head)? != self.membership {
            return Err(Error::new(
                "instant autosquash history changed since preparation",
            ));
        }
        Ok(())
    }

    /// Consume this unlaunched authority without changing repository state.
    pub fn discard(self) {}

    /// Selected operation kind.
    pub fn kind(&self) -> InstantAutosquashKind {
        self.kind
    }

    /// Full object ID of the selected target.
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Full object ID of the target's parent, used as the exclusive replay base.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// Inclusive full-OID replay membership from target through captured HEAD.
    pub fn membership(&self) -> &[String] {
        &self.membership
    }
}
