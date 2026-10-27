//! Bounded server-owned commit drafts used by the production editor registry.
//!
//! The caller must serialize this object with the run's sole lifecycle owner,
//! derive `owner`/`repository` from the authenticated connection, and forward a
//! returned terminal reply to the helper. Text travels only through full-sync
//! didOpen/didChange; save and finish carry versions, not text. Errors never
//! include message bodies/tokens.

use super::{Reply, COMMIT_MESSAGE_KIND};
use crate::git::GitRepo;
use crate::protocol::DraftFinishReason;
use crate::{Error, Result};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// Independent UTF-8 byte limit for the initial, live and saved snapshots.
pub const MAX_DRAFT_BYTES: usize = 1024 * 1024;

/// Trusted dispatch identity; the owner/repository must not come from client JSON.
#[derive(Clone, Copy)]
pub struct DraftContext<'a> {
    /// Authenticated connection/session identity owning this run.
    pub owner: &'a str,
    /// Server-issued opaque run identity.
    pub run_id: &'a str,
    /// Exact server-issued draft URI.
    pub uri: &'a str,
    /// Canonical repository root selected by the owning session.
    pub repository: &'a Path,
}

/// An accepted full-sync snapshot. Deliberately not Debug: text is private data.
pub struct DraftSnapshot {
    version: i32,
    text: String,
}

impl DraftSnapshot {
    /// The exact full-sync version represented by this snapshot.
    pub fn version(&self) -> i32 {
        self.version
    }

    /// Exact UTF-8 text, including comments, whitespace and final-newline choice.
    pub fn text(&self) -> &str {
        &self.text
    }
}

struct Contents {
    target: Target,
    initial: String,
    live: DraftSnapshot,
    saved: DraftSnapshot,
    attached: bool,
}

/// One pending helper/run, bound permanently to its owner, URI and repository.
/// Keep terminal objects until registry cleanup so late operations are rejected.
/// No Debug implementation exposes tokens or message text.
pub struct Draft {
    owner: String,
    repository: PathBuf,
    run_id: String,
    uri: String,
    helper_token: String,
    expected: PathBuf,
    contents: Option<Contents>,
    terminal: bool,
}

impl Draft {
    /// Allocate opaque run/URI and one-shot helper credentials before spawning Git.
    /// Reading the initial template waits until the validated helper joins.
    pub fn new(repo: &GitRepo, owner: impl Into<String>) -> Result<Self> {
        let run_id = opaque_id()?;
        Ok(Self {
            owner: owner.into(),
            repository: std::fs::canonicalize(repo.root())?,
            uri: format!("magit-commit://{run_id}/message"),
            run_id,
            helper_token: opaque_id()?,
            expected: repo.commit_message_path()?,
            contents: None,
            terminal: false,
        })
    }

    /// Opaque identity to bind all subsequent synchronization and intent.
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// Sole full-sync document URI for this run; never a Git filesystem URI.
    pub fn uri(&self) -> &str {
        &self.uri
    }

    /// Credential for the spawned helper only; never log or send to the client.
    pub fn helper_token(&self) -> &str {
        &self.helper_token
    }

    /// Canonical repository identity, including a linked worktree's distinct root.
    pub fn repository(&self) -> &Path {
        &self.repository
    }

    /// Accept exactly one valid commit-message helper and its bounded UTF-8 template.
    /// Failed validation consumes nothing, allowing recovery with the same helper.
    pub fn accept_helper(
        &mut self,
        context: DraftContext<'_>,
        token: &str,
        kind: &str,
        path: &Path,
    ) -> Result<()> {
        self.validate(context)?;
        if token != self.helper_token || kind != COMMIT_MESSAGE_KIND {
            return Err(Error::new(
                "Commit draft helper credentials or kind do not match",
            ));
        }
        if self.contents.is_some() {
            return Err(Error::new("Commit draft helper has already joined"));
        }
        let mut target = Target::open(&self.expected, path)?;
        let mut bytes = Vec::new();
        (&mut target.file)
            .take(MAX_DRAFT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        let initial = checked_text(&bytes)?.to_string();
        self.contents = Some(Contents {
            target,
            live: DraftSnapshot {
                version: 1,
                text: initial.clone(),
            },
            saved: DraftSnapshot {
                version: 1,
                text: initial.clone(),
            },
            initial,
            attached: false,
        });
        Ok(())
    }

    /// Attach with version 1 and exact initial text on first open. Reattachment
    /// must preserve the current live version/text; it never resets synchronization.
    pub fn did_open(&mut self, context: DraftContext<'_>, version: i32, text: &[u8]) -> Result<()> {
        let contents = self.pending(context)?;
        let text = checked_text(text)?;
        if contents.attached || version != contents.live.version || text != contents.live.text {
            return Err(Error::new(
                "Open the commit draft with its current version and exact text",
            ));
        }
        contents.attached = true;
        Ok(())
    }

    /// Apply one full-sync change at a strictly greater LSP integer version.
    /// Skipped versions are valid; invalid input leaves every snapshot unchanged.
    pub fn did_change(
        &mut self,
        context: DraftContext<'_>,
        version: i32,
        text: &[u8],
    ) -> Result<()> {
        let contents = self.pending(context)?;
        if !contents.attached || version <= contents.live.version {
            return Err(Error::new(
                "Commit draft change is detached, stale or duplicated",
            ));
        }
        let text = checked_text(text)?.to_string();
        contents.live = DraftSnapshot { version, text };
        Ok(())
    }

    /// Detach transport only; retain snapshots and keep the helper pending.
    pub fn did_close(&mut self, context: DraftContext<'_>) -> Result<()> {
        let contents = self.pending(context)?;
        if !contents.attached {
            return Err(Error::new("Commit draft is already detached"));
        }
        contents.attached = false;
        Ok(())
    }

    /// Checkpoint exactly the applied live version, without writing or resolving Git.
    /// Return the version only after copying it, for the caller's correlated ack.
    pub fn save(&mut self, context: DraftContext<'_>, version: i32) -> Result<i32> {
        let contents = self.pending(context)?;
        if version != contents.live.version {
            return Err(Error::new(
                "Wait for the exact commit draft version before saving",
            ));
        }
        contents.saved = DraftSnapshot {
            version,
            text: contents.live.text.clone(),
        };
        Ok(version)
    }

    /// Write the named saved checkpoint before returning helper success. Confirm
    /// additionally requires current live == saved version; normal-close can discard
    /// newer live changes. Write failure retains all draft/helper state for retry.
    pub fn finish(
        &mut self,
        context: DraftContext<'_>,
        version: i32,
        reason: DraftFinishReason,
    ) -> Result<Reply> {
        let contents = self.pending(context)?;
        if version != contents.saved.version
            || (reason == DraftFinishReason::Confirm && version != contents.live.version)
        {
            return Err(Error::new(
                "Finish must name the selected saved commit draft checkpoint",
            ));
        }
        contents.target.write(contents.saved.text.as_bytes())?;
        self.terminal = true;
        Ok(Reply::Ok)
    }

    /// Cancel without writing; the lifecycle owner forwards Abort to the helper.
    /// This also handles disconnect/cancellation before any helper has joined.
    pub fn abort(&mut self, context: DraftContext<'_>) -> Result<Reply> {
        self.validate(context)?;
        self.terminal = true;
        Ok(Reply::Abort)
    }

    /// Exact initial template for later server-owned history eligibility checks.
    pub fn initial(&self) -> Option<&str> {
        self.contents
            .as_ref()
            .map(|contents| contents.initial.as_str())
    }

    /// Current accepted text/version, independently bounded from the checkpoint.
    pub fn live(&self) -> Option<&DraftSnapshot> {
        self.contents.as_ref().map(|contents| &contents.live)
    }

    /// Last acknowledged saved text/version (initially the exact Git template).
    pub fn saved(&self) -> Option<&DraftSnapshot> {
        self.contents.as_ref().map(|contents| &contents.saved)
    }

    /// Whether finish/abort has made this run reject all further operations.
    pub fn is_terminal(&self) -> bool {
        self.terminal
    }

    fn validate(&self, context: DraftContext<'_>) -> Result<()> {
        if self.terminal {
            return Err(Error::new("Commit draft run has already finished"));
        }
        if context.owner != self.owner
            || context.run_id != self.run_id
            || context.uri != self.uri
            || context.repository != self.repository
        {
            return Err(Error::new(
                "Commit draft owner, run, URI or repository does not match",
            ));
        }
        Ok(())
    }

    fn pending(&mut self, context: DraftContext<'_>) -> Result<&mut Contents> {
        self.validate(context)?;
        self.contents
            .as_mut()
            .ok_or_else(|| Error::new("Commit draft helper has not joined"))
    }
}

fn checked_text(bytes: &[u8]) -> Result<&str> {
    if bytes.len() > MAX_DRAFT_BYTES {
        return Err(Error::new(
            "Commit draft exceeds the 1 MiB UTF-8 limit; previous text retained",
        ));
    }
    std::str::from_utf8(bytes).map_err(|_| Error::new("Commit draft must contain valid UTF-8"))
}

fn opaque_id() -> Result<String> {
    let mut bytes = [0u8; 32];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

// Pin the validated directory and inode. openat + NOFOLLOW avoid final-component
// symlink races; NONBLOCK prevents a substituted FIFO from blocking the owner.
// Hardlinks are rejected too, so a neighboring file cannot alias our write target.
struct Target {
    path: PathBuf,
    directory: File,
    file: File,
}

impl Target {
    fn open(expected: &Path, supplied: &Path) -> Result<Self> {
        if supplied != expected || std::fs::canonicalize(supplied)? != expected {
            return Err(Error::new(
                "Helper target is not this repository's COMMIT_EDITMSG",
            ));
        }
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(expected.parent().unwrap())?;
        let file = open_message(&directory, false)?;
        regular_unlinked(&file)?;
        Ok(Self {
            path: expected.to_path_buf(),
            directory,
            file,
        })
    }

    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        let parent = self.path.parent().unwrap();
        if std::fs::canonicalize(parent)? != parent
            || !same_file(&std::fs::metadata(parent)?, &self.directory.metadata()?)
        {
            return Err(Error::new(
                "Commit draft Git directory changed; retry after recovery",
            ));
        }
        let mut file = open_message(&self.directory, true)?;
        regular_unlinked(&file)?;
        if !same_file(&file.metadata()?, &self.file.metadata()?) {
            return Err(Error::new(
                "Commit draft target changed; retry after recovery",
            ));
        }
        file.seek(SeekFrom::Start(0))?;
        file.write_all(bytes)?;
        file.set_len(bytes.len() as u64)?;
        file.sync_all()?;
        Ok(())
    }
}

fn open_message(directory: &File, write: bool) -> Result<File> {
    let flags = if write {
        libc::O_WRONLY
    } else {
        libc::O_RDONLY
    };
    // SAFETY: the directory is owned/live, and the fixed filename is NUL-terminated.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            c"COMMIT_EDITMSG".as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(Error::new(format!(
            "Cannot open commit draft target: {}",
            std::io::Error::last_os_error()
        )));
    }
    // SAFETY: openat returned a new owned descriptor.
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn regular_unlinked(file: &File) -> Result<()> {
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.nlink() != 1 {
        return Err(Error::new(
            "Commit draft target must be a regular file without hardlinks",
        ));
    }
    Ok(())
}

fn same_file(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    left.dev() == right.dev() && left.ino() == right.ino()
}
