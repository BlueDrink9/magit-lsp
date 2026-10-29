use crate::errors;
use crate::git::GitRepo;
use crate::patch::FileLayout;
use crate::protocol::{Effect, MessageLevel};
use crate::Result;
use crate::protocol::{Cursor, Selection};
use serde_json::Value;

/// A content-addressed action target tied to an existing server node identity.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ActionTarget {
    /// Existing path/content-derived tree node identity.
    pub node_id: String,
    /// Hash of the exact content the action is permitted to mutate.
    pub mutation_hash: String,
}

/// The displayed view location and input context bound to a dispatch request.
#[derive(Debug, Clone, PartialEq)]
pub struct DispatchTarget {
    /// Server view identity.
    pub view_id: String,
    /// Exact displayed render version, when positional input is used.
    pub version: Option<u64>,
    /// Cursor from that displayed render.
    pub cursor: Option<Cursor>,
    /// Inclusive selection from that displayed render.
    pub selection: Option<Selection>,
    /// Canonical key chord, when dispatched from a key.
    pub key: Option<String>,
}

/// A semantic command request shared by invocation and resolved key paths.
#[derive(Debug, Clone, PartialEq)]
pub struct DispatchRequest {
    /// Registered command identifier.
    pub command: String,
    /// Display-version-bound input target.
    pub target: DispatchTarget,
    /// Optional content target offered from the named displayed render.
    pub action_target: Option<ActionTarget>,
    /// Typed command arguments, including captured menu arguments.
    pub args: Option<Value>,
}

/// Whether a command mutates displayed status content (not deliberate bulk).
pub fn content_command(command: &str) -> bool {
    matches!(command, "status.stage" | "status.unstage" | "status.discard")
}

/// Whether a hunk or line in a status section may authorize this command.
pub fn hunk_section_compatible(command: &str, section: Option<&str>) -> bool {
    match command {
        // Staging a staged hunk is an intentional quiet no-op.
        "status.stage" => matches!(section, Some("unstaged" | "staged")),
        "status.unstage" => section == Some("staged"),
        "status.discard" => section == Some("unstaged"),
        _ => false,
    }
}

fn file_identity(tree: &crate::tree::Tree, node: &crate::tree::Node) -> Option<crate::git::FileMutationIdentity> {
    tree.nodes.iter().find_map(|file| {
        if file.path != node.path || file.section != node.section { return None; }
        match &file.mutation {
            Some(crate::tree::MutationContent::File(identity)) => Some(identity.clone()),
            _ => None,
        }
    })
}

fn mutation_hash(
    command: &str, mut identity: crate::git::FileMutationIdentity,
    hunk: Option<(&crate::patch::Hunk, usize)>,
) -> String {
    use sha2::{Digest, Sha256};
    fn index_only(identity: &mut crate::git::FileMutationIdentity) {
        identity.worktree.clear();
        if let Some((_, original)) = &mut identity.original { index_only(original); }
    }
    if command == "status.unstage" { index_only(&mut identity); }
    let value = if let Some((hunk, identical)) = hunk {
        // Ordinals distinguish identical siblings only. Bind ambiguous siblings
        // to the complete file state so removing one cannot retarget another.
        serde_json::json!([command, hunk.id, identical,
            hunk.lines.iter().map(|line| (line.marker, &line.text, line.no_newline)).collect::<Vec<_>>(),
            &hunk.metadata,
            identity.original.as_ref().map(|(path, _)| path),
            if identical > 1 || command == "status.discard" { Some(&identity) } else { None }])
    } else { serde_json::json!([command, identity]) };
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}

/// Offer a content target from typed metadata in the exact displayed tree.
/// Diff lines intentionally target their owning hunk; sections remain bulk.
pub fn offer_target(tree: &crate::tree::Tree, command: &str, node_id: &str) -> Option<ActionTarget> {
    use crate::tree::{MutationContent, NodeKind};
    if !content_command(command) { return None; }
    let mut node = tree.node(node_id)?;
    if node.kind == NodeKind::Line {
        node = tree.nodes.iter().find(|hunk| hunk.kind == NodeKind::Hunk
            && hunk.path == node.path && hunk.section == node.section && hunk.hunk_id == node.hunk_id)?;
    }
    if matches!(node.kind, NodeKind::Hunk | NodeKind::Line)
        && !hunk_section_compatible(command, node.section.as_deref()) { return None; }
    let identity = file_identity(tree, node)?;
    let hunk = match node.mutation.as_ref()? {
        MutationContent::File(_) => None,
        MutationContent::Hunk { hunk, identical } => Some((hunk, *identical)),
    };
    Some(ActionTarget { node_id: node.id.clone(), mutation_hash: mutation_hash(command, identity, hunk) })
}

/// Re-derive an offered target from fresh Git state without using fresh rows.
pub fn target_is_current(repo: &GitRepo, tree: &crate::tree::Tree, command: &str, target: &ActionTarget) -> Result<bool> {
    use crate::tree::MutationContent;
    let Some(node) = tree.node(&target.node_id) else { return Ok(false); };
    if offer_target(tree, command, &node.id).as_ref() != Some(target) { return Ok(false); }
    let Some(captured) = file_identity(tree, node) else { return Ok(false); };
    let Some(path) = node.path.as_deref() else { return Ok(false); };
    let identity = repo.file_mutation_identity(path, captured.original.as_ref().map(|(path, _)| path.as_str()), command != "status.unstage")?;
    let hunks;
    let hunk = match node.mutation.as_ref() {
        Some(MutationContent::Hunk { hunk, .. }) => {
            hunks = repo.file_hunks(path, node.section.as_deref() == Some("staged"))?;
            let Some(current) = hunks.iter().find(|current| current.id == hunk.id) else { return Ok(false); };
            Some((current, hunks.iter().filter(|other| other.same_content(current)).count()))
        }
        Some(MutationContent::File(_)) => None,
        _ => return Ok(false),
    };
    Ok(mutation_hash(command, identity, hunk) == target.mutation_hash)
}

/// Capture cursor/selection targets from displayed structure, not rendered text.
pub fn displayed_targets(tree: &crate::tree::Tree, command: &str, cursor: Option<Cursor>, selection: Option<Selection>) -> Vec<ActionTarget> {
    use crate::tree::NodeKind;
    let mut targets = Vec::new();
    if let Some(selection) = selection {
        let start = selection.start_line.min(selection.end_line);
        let end = selection.start_line.max(selection.end_line);
        let section = if command == "status.unstage" { "staged" } else { "unstaged" };
        if tree.line_nodes.get(start).and_then(|line| line.section.as_deref()) != Some(section) { return targets; }
        for node in &tree.nodes {
            if node.section.as_deref() != Some(section) { continue; }
            let selected = match node.kind {
                NodeKind::File => start <= node.start_line && node.start_line <= end,
                NodeKind::Hunk => start <= node.end_line && node.start_line <= end,
                _ => false,
            };
            if selected {
                if let Some(target) = offer_target(tree, command, &node.id) { targets.push(target); }
            }
        }
    } else if let Some(node) = cursor.and_then(|cursor| tree.line_nodes.get(cursor.line)).and_then(|line| tree.node(&line.node)) {
        if let Some(target) = offer_target(tree, command, &node.id) { targets.push(target); }
    }
    targets
}

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
    FileHeader {
        path: String,
    },
    HunkHeader {
        path: String,
        hunk_id: String,
    },
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
    DiscardAllUnstaged,
    DiscardAllUntracked,
    DiscardFile {
        path: String,
    },
    DiscardHunk {
        path: String,
        hunk_id: String,
    },
    DiscardSelection {
        start: u32,
        end: u32,
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
            },
        };
    }

    match command {
        Command::Stage => match target {
            NodeKind::SectionUnstaged => Resolution::StageAllUnstaged,
            NodeKind::SectionStaged => info(NOTHING_TO_STAGE),
            NodeKind::SectionUntracked => Resolution::StageAllUntracked,
            NodeKind::FileHeader { path } => Resolution::StageFile { path },
            NodeKind::HunkHeader { path, hunk_id } | NodeKind::DiffLine { path, hunk_id, .. } => {
                Resolution::StageHunk { path, hunk_id }
            }
            NodeKind::Other => info(NOTHING_TO_STAGE),
        },
        Command::Unstage => match target {
            NodeKind::SectionStaged => Resolution::UnstageAllStaged,
            NodeKind::SectionUnstaged | NodeKind::SectionUntracked => info(NOTHING_TO_UNSTAGE),
            NodeKind::FileHeader { path } => Resolution::UnstageFile { path },
            NodeKind::HunkHeader { path, hunk_id } | NodeKind::DiffLine { path, hunk_id, .. } => {
                Resolution::UnstageHunk { path, hunk_id }
            }
            NodeKind::Other => info(NOTHING_TO_UNSTAGE),
        },
        Command::Discard => match target {
            NodeKind::SectionUnstaged => Resolution::DiscardAllUnstaged,
            NodeKind::SectionStaged => info(NOTHING_TO_DISCARD),
            NodeKind::SectionUntracked => Resolution::DiscardAllUntracked,
            NodeKind::FileHeader { path } => Resolution::DiscardFile { path },
            NodeKind::HunkHeader { path, hunk_id } | NodeKind::DiffLine { path, hunk_id, .. } => {
                Resolution::DiscardHunk { path, hunk_id }
            }
            NodeKind::Other => info(NOTHING_TO_DISCARD),
        },
    }
}

/// Run a resolution against the repository and return the effects to show.
///
/// `layouts` are the parsed file diffs for the section the selection was made
/// in; they are consulted only for selection resolutions.
pub fn execute(
    repo: &GitRepo,
    resolution: &Resolution,
    layouts: &[FileLayout],
) -> Result<Vec<Effect>> {
    match resolution {
        Resolution::StageAllUnstaged => errors::effects_for_result(repo.stage_all().map(|_| true)),
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
        } => errors::effects_for_result(repo.apply_selection(layouts, *start, *end, *reverse)),
        Resolution::Message(effect) => Ok(vec![effect.clone()]),
        Resolution::DiscardAllUnstaged => {
            errors::effects_for_result(repo.discard_all_unstaged().map(|_| true))
        }
        Resolution::DiscardAllUntracked => {
            errors::effects_for_result(repo.discard_all_untracked().map(|_| true))
        }
        Resolution::DiscardFile { path } => {
            errors::effects_for_result(repo.discard_file(path).map(|_| true))
        }
        Resolution::DiscardHunk { path, hunk_id } => {
            errors::effects_for_result(repo.discard_hunk(path, hunk_id).map(|_| true))
        }
        Resolution::DiscardSelection { start, end } => {
            errors::effects_for_result(repo.discard_selection(layouts, *start, *end))
        }
    }
}
