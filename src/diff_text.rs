//! Line-range edits for incremental view updates.
//!
//! The server re-renders a view completely but sends the client only the lines
//! that changed. Common prefix/suffix trimming is enough for the status view
//! and keeps the client's cursor, marks and folds stable.

use crate::protocol::LineEdit;

/// Diff two rendered documents into one replacement range.
///
/// `end_line` is exclusive, so applying an edit replaces `start_line..end_line`
/// with `lines`. Equal documents produce no edits.
pub fn line_edits(old: &[String], new: &[String]) -> Vec<LineEdit> {
    if old == new {
        return Vec::new();
    }

    let mut prefix = 0;
    while prefix < old.len() && prefix < new.len() && old[prefix] == new[prefix] {
        prefix += 1;
    }

    let mut suffix = 0;
    while suffix < old.len() - prefix
        && suffix < new.len() - prefix
        && old[old.len() - 1 - suffix] == new[new.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let old_end = old.len() - suffix;
    let new_end = new.len() - suffix;
    vec![LineEdit {
        start_line: prefix,
        end_line: old_end,
        lines: new[prefix..new_end].to_vec(),
    }]
}

/// Apply edits to `lines`, in document order, as a client would.
pub fn apply_edits(lines: &[String], edits: &[LineEdit]) -> Vec<String> {
    let mut out = lines.to_vec();
    for edit in edits {
        let start = edit.start_line.min(out.len());
        let end = edit.end_line.clamp(start, out.len());
        out.splice(start..end, edit.lines.iter().cloned());
    }
    out
}
