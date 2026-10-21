pub mod diff_text;
pub mod dispatch;
pub mod errors;
pub mod git;
pub mod logging;
pub mod patch;
pub mod protocol;
pub mod semantic_tokens;
pub mod server;
pub mod session;
pub mod status_view;
pub mod transport;
pub mod tree;
pub mod view;

use std::fmt;

/// Why an error happened, when callers need to tell cases apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Internal,
    /// The action addressed a hunk that is no longer in the current diff.
    HunkNotFound,
}

/// A crate-wide error, optionally carrying a structured Git failure.
#[derive(Debug, Clone)]
pub struct Error {
    pub message: String,
    pub git: Option<git::GitFailure>,
    pub kind: ErrorKind,
}

impl Error {
    /// Create an internal error from a plain message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            git: None,
            kind: ErrorKind::Internal,
        }
    }

    /// Create an error that wraps a failed Git command.
    pub fn git(failure: git::GitFailure) -> Self {
        Self {
            message: failure.to_string(),
            git: Some(failure),
            kind: ErrorKind::Internal,
        }
    }

    /// The addressed hunk is absent from the current diff for `path`.
    pub fn hunk_not_found(path: &str) -> Self {
        Self {
            message: format!("hunk not found for {path}"),
            git: None,
            kind: ErrorKind::HunkNotFound,
        }
    }

    /// The user-facing explanation, when this was a Git failure.
    pub fn user_error(&self) -> Option<errors::UserError> {
        self.git.as_ref().map(git::GitFailure::user_error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::new(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::new(e.to_string())
    }
}

/// Crate-wide result type.
pub type Result<T> = std::result::Result<T, Error>;
