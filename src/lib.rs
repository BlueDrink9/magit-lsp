pub mod document;
pub mod errors;
pub mod git;
pub mod patch;
pub mod protocol;
pub mod server;

use std::fmt;

/// A crate-wide error, optionally carrying a structured Git failure.
#[derive(Debug, Clone)]
pub struct Error {
    pub message: String,
    pub git: Option<git::GitFailure>,
}

impl Error {
    /// Create an internal error from a plain message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            git: None,
        }
    }

    /// Create an error that wraps a failed Git command.
    pub fn git(failure: git::GitFailure) -> Self {
        Self {
            message: failure.to_string(),
            git: Some(failure),
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