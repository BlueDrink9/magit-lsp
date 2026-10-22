use crate::errors;
use crate::git::GitRepo;
use crate::patch::{self, FileLayout};
use crate::protocol::{Effect, MessageLevel};
use crate::Result;

/// The command the user invoked on the row under the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Stage,
    Unstage,
    Discard,
}

/// The kind of status row the cursor is on.
///
/// `DiffLine` covers both `+`/`-` lines and context lines; `changed` records
/// which one it is even though both resolve to the whole hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    SectionUnstaged,
    SectionStaged,
    SectionUntracked,
    FileHeader { path: String },
    HunkHeader { path: String, hunk_id: String },
    DiffLine {
        changed: bool,
        path: String,
        hunk_id: String,
    },
    Other,
}

/// An inclusive document-line selection, as produced by visual mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

/// The operation a command resolves to for a given row or selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    StageAllUnstaged,
    UnstageAllStaged,
    StageAllUntracked,
    StageFile {
        path: String,
    },
    UnstageFile {
        path: String,
    },
    StageHunk {
        path: String,
        hunk_id: String,
    },
    UnstageHunk {
        path: String,
        hunk_id: String,
    },
    /// Delegate to IMM-2's `normalise_selection`; `reverse` selects unstage.
    Selection {
        start: u32,
        end: u32,
        reverse: bool,
    },
    DiscardAllUnstaged {
        needs_confirm: bool,
    },
    DiscardAllUntracked {
        needs_confirm: bool,
    },
    DiscardFile {
        path: String,
        needs_confirm: bool,
    },
    DiscardHunk {
        path: String,
        hunk_id: String,
        needs_confirm: bool,
    },
    DiscardSelection {
        start: u32,
        end: u32,
        needs_confirm: bool,
    },
    /// Nothing to do, with a sentence to show the user.
    Message(Effect),
}

/// Message for a row or selection that has nothing to stage.
pub const NOTHING_TO_STAGE: &str = "Nothing to stage here";
/// Message for a row or selection that has nothing to unstage.
pub const NOTHING_TO_UNSTAGE: &str = "Nothing to unstage here";
/// Message for a row or selection that has nothing to discard.
pub const NOTHING_TO_DISCARD: &str = "Nothing to discard here";

fn info(text: &str) -> Resolution {
    Resolution::Message(Effect::message(MessageLevel::Info, text))
}

/// Map a command plus the smallest node under the cursor (or the selection) to
/// an operation. Pure: no repository access, so every table row is testable.
pub fn resolve(command: Command, target: NodeKind, selection: Option<LineRange>) -> Resolution {
    // A selection always wins over the node under the cursor (IMM-2 rules).
    if let Some(range) = selection {
        return match command {
            Command::Stage => Resolution::Selection {
                start: range.start,
                end: range.end,
                reverse: false,
            },
            Command::Unstage => Resolution::Selection {
                start: range.start,
                end: range.end,
                reverse: true,
            },
            Command::Discard => Resolution::DiscardSelection {
                start: range.start,
                end: range.end,
                needs_confirm: true,
            },
        };
    }

    match command {
        Command::Stage => match target {
            NodeKind::SectionUnstaged => Resolution::StageAllUnstaged,
            NodeKind::SectionStaged => info(NOTHING_TO_STAGE),
            NodeKind::SectionUntracked => Resolution::StageAllUntracked,
            NodeKind::FileHeader { path } => Resolution::StageFile { path },
            NodeKind::HunkHeader { path, hunk_id }
            | NodeKind::DiffLine {
                path, hunk_id, ..
            } => Resolution::StageHunk { path, hunk_id },
            NodeKind::Other => info(NOTHING_TO_STAGE),
        },
        Command::Unstage => match target {
            NodeKind::SectionStaged => Resolution::UnstageAllStaged,
            NodeKind::SectionUnstaged | NodeKind::SectionUntracked => info(NOTHING_TO_UNSTAGE),
            NodeKind::FileHeader { path } => Resolution::UnstageFile { path },
            NodeKind::HunkHeader { path, hunk_id }
            | NodeKind::DiffLine {
                path, hunk_id, ..
            } => Resolution::UnstageHunk { path, hunk_id },
            NodeKind::Other => info(NOTHING_TO_UNSTAGE),
        },
        Command::Discard => match target {
            NodeKind::SectionUnstaged => Resolution::DiscardAllUnstaged {
                needs_confirm: true,
            },
            NodeKind::SectionStaged => info(NOTHING_TO_DISCARD),
            NodeKind::SectionUntracked => Resolution::DiscardAllUntracked {
                needs_confirm: true,
            },
            NodeKind::FileHeader { path } => Resolution::DiscardFile {
                path,
                needs_confirm: true,
            },
            NodeKind::HunkHeader { path, hunk_id }
            | NodeKind::DiffLine {
                path, hunk_id, ..
            } => Resolution::DiscardHunk {
                path,
                hunk_id,
                needs_confirm: true,
            },
            NodeKind::Other => info(NOTHING_TO_DISCARD),
        },
    }
}

/// Run a resolution against the repository and return the effects to show.
///
/// `layouts` are the parsed file diffs for the section the selection was made
/// in; they are only consulted for [`Resolution::Selection`].
pub fn execute(
    repo: &GitRepo,
    resolution: &Resolution,
    layouts: &[FileLayout],
) -> Result<Vec<Effect>> {
    match resolution {
        Resolution::StageAllUnstaged => {
            errors::effects_for_result(repo.stage_all().map(|_| true))
        }
        Resolution::UnstageAllStaged => {
            errors::effects_for_result(repo.unstage_all().map(|_| true))
        }
        Resolution::StageAllUntracked => {
            errors::effects_for_result(repo.stage_untracked().map(|_| true))
        }
        Resolution::StageFile { path } => {
            errors::effects_for_result(repo.stage_file(path).map(|_| true))
        }
        Resolution::UnstageFile { path } => {
            errors::effects_for_result(repo.unstage_file(path).map(|_| true))
        }
        Resolution::StageHunk { path, hunk_id } => {
            errors::effects_for_result(repo.stage_hunk(path, hunk_id).map(|_| true))
        }
        Resolution::UnstageHunk { path, hunk_id } => {
            errors::effects_for_result(repo.unstage_hunk(path, hunk_id).map(|_| true))
        }
        Resolution::Selection {
            start,
            end,
            reverse,
        } => {
            if repo.apply_selection(layouts, *start, *end, *reverse)? {
                Ok(Vec::new())
            } else {
                Ok(vec![Effect::message(
                    MessageLevel::Info,
                    patch::NO_CHANGED_LINES,
                )])
            }
        }
        Resolution::Message(effect) => Ok(vec![effect.clone()]),
        // Discard has no backend yet (it lands in a later roadmap item), so it
        // reports the gap instead of touching the repository.
        discard @ (Resolution::DiscardAllUnstaged { .. }
        | Resolution::DiscardAllUntracked { .. }
        | Resolution::DiscardFile { .. }
        | Resolution::DiscardHunk { .. }
        | Resolution::DiscardSelection { .. }) => Ok(vec![Effect::message(
            MessageLevel::Info,
            deferred_text(discard),
        )]),
    }
}

fn deferred_text(resolution: &Resolution) -> String {
    let what = match resolution {
        Resolution::DiscardAllUnstaged { .. } => "discard all unstaged changes".to_string(),
        Resolution::DiscardAllUntracked { .. } => "discard all untracked files".to_string(),
        Resolution::DiscardFile { path, .. } => format!("discard {path}"),
        Resolution::DiscardHunk { path, .. } => format!("discard a hunk of {path}"),
        Resolution::DiscardSelection { .. } => "discard the selection".to_string(),
        _ => "discard".to_string(),
    };
    format!("Not implemented yet: {what}")
}
