use crate::git::GitFailure;
use crate::protocol::{Effect, MessageLevel};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

/// A Git failure explained in terms a user can act on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserError {
    /// One plain-language sentence describing what went wrong.
    pub summary: String,
    /// What the user can do next, when there is a useful answer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Raw command output, for the error-detail view.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// The command that failed, e.g. `git apply --cached`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Reserved link to a captured process log.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_log_id: Option<u64>,
}

/// Map a failed Git command to a user-facing error.
///
/// This is the single place that knows Git's common failure messages. The
/// exit code is accepted for future distinctions; current rules match stderr.
pub fn map_git_failure(_exit_code: i32, stderr: &str, subcommand: &str) -> UserError {
    let text = stderr.to_lowercase();
    let command = Some(format!("git {subcommand}"));

    let (summary, hint) = if text.contains("could not resolve head")
        || text.contains("unborn")
        // Git versions word the same condition differently.
        || text.contains("does not have any commits yet")
        || text.contains("ambiguous argument 'head'")
        || text.contains("failed to resolve 'head'")
    {
        ("This repository has no commits yet.".to_string(), None)
    } else if text.contains("corrupt patch") {
        (
            "Git could not apply that selection. Try selecting whole lines that are changes."
                .to_string(),
            None,
        )
    } else if text.contains("index.lock") {
        (
            "Another Git process is running in this repository.".to_string(),
            Some(
                "If no Git process is running, remove the stale .git/index.lock file and try again."
                    .to_string(),
            ),
        )
    } else if text.contains("not a git repository") {
        ("This folder is not inside a Git repository.".to_string(), None)
    } else if text.contains("would be overwritten") {
        (
            "Your local changes would be overwritten. Commit, stash or discard them first."
                .to_string(),
            None,
        )
    } else if text.contains("non-fast-forward") || text.contains("rejected") {
        (
            "The remote has commits you do not have. Fetch or pull first.".to_string(),
            None,
        )
    } else if text.contains("authentication failed") || text.contains("permission denied") {
        ("Git could not authenticate with the remote.".to_string(), None)
    } else if text.contains("conflict") {
        (
            "The operation stopped because of conflicts.".to_string(),
            Some("Resolve the conflicts, then continue the operation.".to_string()),
        )
    } else {
        return UserError {
            summary: format!("`git {subcommand}` failed."),
            hint: None,
            detail: Some(stderr.trim_end().to_string()),
            command,
            process_log_id: None,
        };
    };

    UserError {
        summary,
        hint,
        detail: None,
        command,
        process_log_id: None,
    }
}

impl Effect {
    /// Build an error notification from a structured Git failure.
    pub fn from_git_failure(failure: &GitFailure) -> Effect {
        let user = failure.user_error();
        Effect::Message {
            level: MessageLevel::Error,
            text: user.summary,
            hint: user.hint,
            detail: user.detail,
            process_log_id: user.process_log_id,
        }
    }

    /// The error effect for `err`, or `None` when it is an internal fault.
    pub fn from_error(err: &Error) -> Option<Effect> {
        err.git.as_ref().map(Effect::from_git_failure)
    }
}

impl GitFailure {
    /// Explain this failure to the user, including the full command line.
    pub fn user_error(&self) -> UserError {
        let mut user = map_git_failure(self.exit_code, &self.stderr, &self.subcommand);
        user.command = Some(format!("git {}", self.args.join(" ")));
        user
    }
}

/// The effect list for a mutation outcome.
///
/// A Git failure is not a request failure: the caller still gets a status.
pub fn effects_for_result(result: Result<bool>) -> Result<Vec<Effect>> {
    match result {
        Ok(true) => Ok(Vec::new()),
        Ok(false) => Ok(vec![Effect::message(
            MessageLevel::Info,
            crate::patch::NO_CHANGED_LINES,
        )]),
        Err(err) => match Effect::from_error(&err) {
            Some(effect) => Ok(vec![effect]),
            None => Err(err),
        },
    }
}
