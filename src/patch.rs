use serde::{Deserialize, Serialize};

/// The role of a line within a hunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

/// One line of a hunk, with its marker and (internal) source line numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub marker: char,
    pub text: String,
    #[serde(skip)]
    pub old_no: Option<u32>,
    #[serde(skip)]
    pub new_no: Option<u32>,
    #[serde(skip)]
    pub no_newline: bool,
}

/// A parsed unified-diff hunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hunk {
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub lines: Vec<DiffLine>,
}

impl Hunk {
    /// Classify a line as context, added, or removed.
    pub fn kind(&self, line: &DiffLine) -> LineKind {
        match line.marker {
            '+' => LineKind::Added,
            '-' => LineKind::Removed,
            _ => LineKind::Context,
        }
    }

    /// Whether the line at `index` is an addition or removal.
    pub fn is_change(&self, index: usize) -> bool {
        self.lines
            .get(index)
            .map(|l| l.marker == '+' || l.marker == '-')
            .unwrap_or(false)
    }
}

/// A single file's parsed diff: the raw header before the first hunk plus hunks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    pub path: String,
    pub header: String,
    pub hunks: Vec<Hunk>,
}

/// A hunk's position in the rendered status document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HunkLayout {
    /// Document line of the `@@` header.
    pub header_line: u32,
    /// Document line of the first diff line (`lines[0]`).
    pub first_line: u32,
    pub hunk: Hunk,
}

/// A file's position in the rendered status document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileLayout {
    pub path: String,
    /// Raw file header, reused verbatim when building patches.
    pub header: String,
    /// Document line of the file header.
    pub header_line: u32,
    pub hunks: Vec<HunkLayout>,
}

impl FileLayout {
    /// Parse a one-file diff into a layout whose document lines are consecutive.
    pub fn from_diff(diff: &str, header_line: u32) -> Option<Self> {
        let file = parse_file_diff(diff)?;
        let mut cursor = header_line + 1;
        let hunks = file
            .hunks
            .into_iter()
            .map(|hunk| {
                let layout = HunkLayout {
                    header_line: cursor,
                    first_line: cursor + 1,
                    hunk,
                };
                cursor = layout.first_line + layout.hunk.lines.len() as u32;
                layout
            })
            .collect();
        Some(Self {
            path: file.path,
            header: file.header,
            header_line,
            hunks,
        })
    }

    /// Patch text for the whole file, using the original header.
    pub fn whole_file_patch(&self) -> String {
        let mut out = self.header.clone();
        for hunk in &self.hunks {
            out.push_str(&full_hunk_patch(&hunk.hunk));
        }
        out
    }

    /// Patch text for one whole hunk.
    pub fn whole_hunk_patch(&self, hunk_index: usize) -> Option<String> {
        let hunk = self.hunks.get(hunk_index)?;
        Some(format!("{}{}", self.header, full_hunk_patch(&hunk.hunk)))
    }

    /// Patch text for selected change lines within one hunk.
    pub fn lines_patch(
        &self,
        hunk_index: usize,
        selected: &[usize],
        reverse: bool,
    ) -> Option<String> {
        let hunk = self.hunks.get(hunk_index)?;
        let changes_file_kind =
            (reverse && is_new_file(&hunk.hunk)) || (!reverse && is_deleted_file(&hunk.hunk));
        let header = if changes_file_kind {
            // A partial apply cannot use the `/dev/null` header from a file
            // creation or deletion: git rejects it as a whole-file operation.
            synthetic_file_header(&self.path, false)
        } else {
            self.header.clone()
        };
        Some(format!(
            "{}{}",
            header,
            selected_hunk_patch(&hunk.hunk, selected, reverse)
        ))
    }
}

/// One unit of work produced by normalising a document-line selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionOp {
    WholeFile {
        path: String,
    },
    WholeHunk {
        path: String,
        hunk_index: usize,
    },
    Lines {
        path: String,
        hunk_index: usize,
        /// Indices of selected `+`/`-` lines within the hunk.
        lines: Vec<usize>,
    },
}

impl SelectionOp {
    /// The path of the file this operation belongs to.
    pub fn path(&self) -> &str {
        match self {
            SelectionOp::WholeFile { path }
            | SelectionOp::WholeHunk { path, .. }
            | SelectionOp::Lines { path, .. } => path,
        }
    }
}

/// Message to show when a selection contains no changed lines.
pub const NO_CHANGED_LINES: &str = "No changed lines in the selection";

/// The result of normalising a document-line selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    NoChanges,
    Ops(Vec<SelectionOp>),
}

/// Decode a path as Git writes it in a diff header.
///
/// Git C-quotes a path that contains whitespace, quotes or backslashes, and
/// leaves a trailing tab after an unquoted name that contains a space.
fn decode_header_path(raw: &str) -> String {
    let raw = raw.strip_suffix('\t').unwrap_or(raw);
    match raw.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        Some(quoted) => unquote_c(quoted),
        None => raw.to_string(),
    }
}

/// Undo Git's C-style quoting of a path.
fn unquote_c(quoted: &str) -> String {
    let bytes = quoted.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' || i + 1 == bytes.len() {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let escape = bytes[i + 1];
        let literal = match escape {
            b'a' => Some(7),
            b'b' => Some(8),
            b'f' => Some(12),
            b'n' => Some(b'\n'),
            b'r' => Some(b'\r'),
            b't' => Some(b'\t'),
            b'v' => Some(11),
            b'\\' => Some(b'\\'),
            b'"' => Some(b'"'),
            _ => None,
        };
        if let Some(byte) = literal {
            out.push(byte);
            i += 2;
        } else if (b'0'..=b'7').contains(&escape) {
            let mut value = 0u32;
            let mut digits = 0;
            while digits < 3 && i + 1 + digits < bytes.len() {
                let digit = bytes[i + 1 + digits];
                if !(b'0'..=b'7').contains(&digit) {
                    break;
                }
                value = value * 8 + (digit - b'0') as u32;
                digits += 1;
            }
            out.push(value as u8);
            i += 1 + digits;
        } else {
            out.push(escape);
            i += 2;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The file path from a one-file diff, preferring the new side.
///
/// Only header lines are inspected: a hunk body can start with `+++ `.
/// Header-only diffs (mode changes, binary files, pure renames) have no
/// `---`/`+++` lines, so the `rename to` and `diff --git` lines are fallbacks.
fn diff_path(diff: &str) -> Option<String> {
    let mut old_side = None;
    let mut rename_to = None;
    let mut diff_git = None;
    for line in diff.lines() {
        if line.starts_with("@@") {
            break;
        }
        if let Some(raw) = line.strip_prefix("+++ ") {
            let path = decode_header_path(raw);
            if let Some(path) = path.strip_prefix("b/") {
                return Some(path.to_string());
            }
        }
        if let Some(raw) = line.strip_prefix("--- ") {
            let path = decode_header_path(raw);
            if let Some(path) = path.strip_prefix("a/") {
                old_side = Some(path.to_string());
            }
        }
        if let Some(raw) = line.strip_prefix("rename to ") {
            rename_to = Some(decode_header_path(raw));
        }
        if line.starts_with("diff --git ") {
            diff_git = split_diff_git_paths(line).map(|(_, new)| new);
        }
    }
    old_side.or(rename_to).or(diff_git)
}

/// Split the two paths on a `diff --git` line.
///
/// Git C-quotes a path that needs it; otherwise the sides are separated by the
/// last ` b/` (or ` a/`) boundary, which tolerates spaces in either name.
fn split_diff_git_paths(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("diff --git ")?;
    if rest.starts_with('"') {
        let (old, rest) = take_quoted(rest)?;
        let rest = rest.strip_prefix(' ')?;
        let (new, _) = take_quoted(rest)?;
        return Some((strip_prefix_side(&old, 'a'), strip_prefix_side(&new, 'b')));
    }
    let index = rest.rfind(" b/")?;
    let old = decode_header_path(&rest[..index]);
    let new = decode_header_path(&rest[index + 1..]);
    Some((strip_prefix_side(&old, 'a'), strip_prefix_side(&new, 'b')))
}

/// Remove Git's `a/` or `b/` prefix from a diff path.
fn strip_prefix_side(path: &str, side: char) -> String {
    path.strip_prefix(&format!("{side}/"))
        .unwrap_or(path)
        .to_string()
}

/// Read one C-quoted token, returning it unquoted plus the remainder.
fn take_quoted(input: &str) -> Option<(String, &str)> {
    let quoted = input.strip_prefix('"')?;
    let bytes = quoted.as_bytes();
    let mut end = 0;
    while end < bytes.len() {
        match bytes[end] {
            b'\\' => end += 2,
            b'"' => break,
            _ => end += 1,
        }
    }
    if end >= bytes.len() {
        return None;
    }
    Some((unquote_c(&quoted[..end]), &quoted[end + 1..]))
}

/// Parse a one-file unified diff, keeping its header lines verbatim.
///
/// A header-only diff (mode change, binary file, pure rename) yields a file
/// with no hunks. A path that Git rendered as several file sections, such as a
/// type change, is refused: no single header describes it.
pub fn parse_file_diff(diff: &str) -> Option<FileDiff> {
    let sections = diff
        .lines()
        .filter(|line| line.starts_with("diff --git "))
        .count();
    if sections != 1 {
        return None;
    }
    let hunks = parse_unified_diff(diff);
    let mut header = String::new();
    for line in diff.lines() {
        if line.starts_with("@@") {
            break;
        }
        header.push_str(line);
        header.push('\n');
    }
    Some(FileDiff {
        path: diff_path(diff)?,
        header,
        hunks,
    })
}

/// Normalise an inclusive document-line selection into per-file operations.
///
/// The file header selects the whole file; otherwise a hunk header selects the
/// whole hunk; otherwise only `+`/`-` lines inside the range are selected.
pub fn normalise_selection(files: &[FileLayout], start: u32, end: u32) -> Selection {
    let (start, end) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    let mut ops = Vec::new();

    for file in files {
        let file_end = match file.hunks.last() {
            Some(last) => last.first_line + last.hunk.lines.len() as u32,
            None => file.header_line,
        };
        if end < file.header_line || start > file_end {
            continue;
        }
        if start <= file.header_line && file.header_line <= end {
            ops.push(SelectionOp::WholeFile {
                path: file.path.clone(),
            });
            continue;
        }

        for (hunk_index, hunk) in file.hunks.iter().enumerate() {
            let hunk_end = hunk.first_line + hunk.hunk.lines.len() as u32;
            if end < hunk.header_line || start > hunk_end {
                continue;
            }
            if start <= hunk.header_line && hunk.header_line <= end {
                ops.push(SelectionOp::WholeHunk {
                    path: file.path.clone(),
                    hunk_index,
                });
                continue;
            }
            let lines: Vec<usize> = hunk
                .hunk
                .lines
                .iter()
                .enumerate()
                .filter_map(|(index, line)| {
                    let document_line = hunk.first_line + index as u32;
                    let is_change = line.marker == '+' || line.marker == '-';
                    (is_change && document_line >= start && document_line <= end).then_some(index)
                })
                .collect();
            if !lines.is_empty() {
                ops.push(SelectionOp::Lines {
                    path: file.path.clone(),
                    hunk_index,
                    lines,
                });
            }
        }
    }

    if ops.is_empty() {
        Selection::NoChanges
    } else {
        Selection::Ops(ops)
    }
}

/// Whether a hunk creates a new file (it has no old side).
pub fn is_new_file(h: &Hunk) -> bool {
    h.old_start == 0 && h.old_count == 0
}

/// Whether a hunk deletes a file (it has no new side).
pub fn is_deleted_file(h: &Hunk) -> bool {
    h.new_start == 0 && h.new_count == 0
}

/// A minimal file header for patches that cannot reuse the original one.
pub fn synthetic_file_header(path: &str, new_file: bool) -> String {
    let old = quote_header_path(&format!("a/{path}"));
    let new = quote_header_path(&format!("b/{path}"));
    if new_file {
        format!("diff --git {old} {new}\nnew file mode 100644\n--- /dev/null\n+++ {new}\n")
    } else {
        format!("diff --git {old} {new}\n--- {old}\n+++ {new}\n")
    }
}

/// Quote `path` the way Git does when a diff header cannot hold it verbatim.
fn quote_header_path(path: &str) -> String {
    if !path
        .chars()
        .any(|c| c.is_whitespace() || c == '"' || c == '\\' || c.is_control())
    {
        return path.to_string();
    }
    let mut out = String::from("\"");
    for c in path.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => out.push_str(&format!("\\{:03o}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn parse_hunk_header(header: &str) -> Option<(u32, u32, u32, u32)> {
    let s = header.trim_start_matches('@').trim();
    let s = match s.find(" @@") {
        Some(i) => &s[..i],
        None => s,
    };
    let s = s.trim();
    let mut parts = s.split_whitespace();
    let old = parts.next()?;
    let new = parts.next().unwrap_or("+0,0");
    let parse_side = |p: &str| -> Option<(u32, u32)> {
        let p = p.trim_start_matches(['-', '+']);
        let mut it = p.split(',');
        let start = it.next()?.parse::<u32>().ok()?;
        let count = match it.next() {
            Some(c) => c.parse::<u32>().ok()?,
            None => 1,
        };
        Some((start, count))
    };
    let (old_start, old_count) = parse_side(old)?;
    let (new_start, new_count) = parse_side(new)?;
    Some((old_start, old_count, new_start, new_count))
}

/// Parse a unified diff (as produced by `git diff`) into hunks.
pub fn parse_unified_diff(diff: &str) -> Vec<Hunk> {
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut cur: Option<Hunk> = None;
    let mut old_pos: u32 = 0;
    let mut new_pos: u32 = 0;

    for raw in diff.split('\n') {
        if raw.starts_with("@@") {
            if let Some(h) = cur.take() {
                hunks.push(h);
            }
            if let Some((old_start, old_count, new_start, new_count)) = parse_hunk_header(raw) {
                old_pos = old_start;
                new_pos = new_start;
                cur = Some(Hunk {
                    old_start,
                    old_count,
                    new_start,
                    new_count,
                    lines: Vec::new(),
                });
            }
            continue;
        }
        let Some(h) = cur.as_mut() else {
            continue;
        };
        if raw.is_empty() {
            continue;
        }
        if raw == "\\ No newline at end of file" {
            if let Some(last) = h.lines.last_mut() {
                last.no_newline = true;
            }
            continue;
        }

        let marker = raw.chars().next().unwrap_or(' ');
        let text = raw.get(1..).unwrap_or("").to_string();
        let (marker, text) = if matches!(marker, ' ' | '+' | '-') {
            (marker, text)
        } else {
            (' ', raw.to_string())
        };

        let mut line = DiffLine {
            marker,
            text,
            old_no: None,
            new_no: None,
            no_newline: false,
        };
        match marker {
            ' ' => {
                line.old_no = Some(old_pos);
                line.new_no = Some(new_pos);
                old_pos += 1;
                new_pos += 1;
            }
            '-' => {
                line.old_no = Some(old_pos);
                old_pos += 1;
            }
            '+' => {
                line.new_no = Some(new_pos);
                new_pos += 1;
            }
            _ => {}
        }
        h.lines.push(line);
    }

    if let Some(h) = cur.take() {
        hunks.push(h);
    }
    hunks
}

fn push_hunk_line(out: &mut String, marker: char, text: &str) {
    out.push(marker);
    out.push_str(text);
    out.push('\n');
}

/// Render a hunk header line (`@@ -a,b +c,d @@`).
pub fn hunk_header(h: &Hunk) -> String {
    format!(
        "@@ -{},{} +{},{} @@\n",
        h.old_start, h.old_count, h.new_start, h.new_count
    )
}

/// Reconstruct the full hunk as patch text (header + every line).
pub fn full_hunk_patch(h: &Hunk) -> String {
    let mut out = hunk_header(h);
    for l in &h.lines {
        push_hunk_line(&mut out, l.marker, &l.text);
        if l.no_newline {
            out.push_str("\\ No newline at end of file\n");
        }
    }
    out
}

/// Build a hunk patch containing only the selected change lines.
///
/// Unselected change lines are neutralised: a `-` becomes context when staging
/// (the line stays in the old file) and a `+` is dropped; for a reverse apply
/// the roles swap. Counts are left stale because `git apply --recount` infers
/// them from the body.
pub fn selected_hunk_patch(h: &Hunk, selected: &[usize], reverse: bool) -> String {
    let mut out = hunk_header(h);
    for (index, line) in h.lines.iter().enumerate() {
        let chosen = selected.contains(&index);
        let marker = match line.marker {
            '+' if chosen => Some('+'),
            '-' if chosen => Some('-'),
            '+' if !chosen => reverse.then_some(' '),
            '-' if !chosen => (!reverse).then_some(' '),
            _ => Some(' '),
        };
        if let Some(marker) = marker {
            push_hunk_line(&mut out, marker, &line.text);
            if line.no_newline {
                out.push_str("\\ No newline at end of file\n");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "diff --git a/f.txt b/f.txt\n\
index 4083766..31cf455 100644\n\
--- a/f.txt\n\
+++ b/f.txt\n\
@@ -1,10 +1,10 @@\n\
 line1\n\
 line2\n\
-line3\n\
+line3 CHANGED\n\
 line4\n\
 line5\n\
+NEW line\n\
 line6\n\
 line7\n\
-line8\n\
 line9\n\
 line10\n";

    #[test]
    fn parses_single_hunk() {
        let hunks = parse_unified_diff(SAMPLE);
        assert_eq!(hunks.len(), 1);
        let h = &hunks[0];
        assert_eq!((h.old_start, h.old_count), (1, 10));
        assert_eq!((h.new_start, h.new_count), (1, 10));
        assert_eq!(h.lines.len(), 12);
        assert_eq!(h.lines[0].text, "line1");
        assert_eq!(h.lines[2].marker, '-');
        assert_eq!(h.lines[2].text, "line3");
        assert_eq!(h.lines[2].old_no, Some(3));
        assert_eq!(h.lines[3].marker, '+');
        assert_eq!(h.lines[3].new_no, Some(3));
        assert_eq!(h.lines[6].text, "NEW line");
        assert_eq!(h.lines[6].new_no, Some(6));
    }

    fn sample_hunk() -> Hunk {
        parse_unified_diff(SAMPLE).remove(0)
    }

    #[test]
    fn full_hunk_reconstruction_is_valid() {
        let h = sample_hunk();
        let p = full_hunk_patch(&h);
        assert!(p.starts_with("@@ -1,10 +1,10 @@"));
        assert!(p.contains("-line3\n"));
        assert!(p.contains("+line3 CHANGED\n"));
        assert!(p.contains("-line8\n"));
    }

    const TWO_HUNKS: &str = "diff --git a/f.txt b/f.txt\n\
--- a/f.txt\n\
+++ b/f.txt\n\
@@ -1,3 +1,3 @@\n\
 a\n\
-b\n\
+B\n\
 c\n\
@@ -10,3 +10,3 @@\n\
 j\n\
-k\n\
+K\n\
 l\n";

    fn sample_layout() -> FileLayout {
        FileLayout::from_diff(SAMPLE, 0).expect("layout")
    }

    #[test]
    fn normalise_context_only_is_no_changes() {
        // Document line 2 is the first context line of the hunk.
        assert_eq!(
            normalise_selection(&[sample_layout()], 2, 2),
            Selection::NoChanges
        );
    }

    #[test]
    fn normalise_file_header_selects_whole_file() {
        assert_eq!(
            normalise_selection(&[sample_layout()], 0, 0),
            Selection::Ops(vec![SelectionOp::WholeFile {
                path: "f.txt".to_string()
            }])
        );
    }

    #[test]
    fn normalise_hunk_header_selects_whole_hunk() {
        assert_eq!(
            normalise_selection(&[sample_layout()], 1, 1),
            Selection::Ops(vec![SelectionOp::WholeHunk {
                path: "f.txt".to_string(),
                hunk_index: 0
            }])
        );
    }

    #[test]
    fn normalise_context_and_change_selects_only_changes() {
        // Context line1 (doc 2) through the line3 replacement (doc 4..=5).
        assert_eq!(
            normalise_selection(&[sample_layout()], 2, 5),
            Selection::Ops(vec![SelectionOp::Lines {
                path: "f.txt".to_string(),
                hunk_index: 0,
                lines: vec![2, 3]
            }])
        );
    }

    #[test]
    fn normalise_range_crossing_two_hunks() {
        let layout = FileLayout::from_diff(TWO_HUNKS, 0).expect("layout");
        // First hunk's change is at doc 3..=4; the second hunk header is doc 6.
        match normalise_selection(&[layout], 3, 8) {
            Selection::Ops(ops) => {
                assert_eq!(
                    ops[0],
                    SelectionOp::Lines {
                        path: "f.txt".to_string(),
                        hunk_index: 0,
                        lines: vec![1, 2]
                    }
                );
                assert_eq!(
                    ops[1],
                    SelectionOp::WholeHunk {
                        path: "f.txt".to_string(),
                        hunk_index: 1
                    }
                );
            }
            other => panic!("unexpected selection: {other:?}"),
        }
    }

    #[test]
    fn normalise_range_crossing_two_files() {
        let first = FileLayout::from_diff(SAMPLE, 0).expect("first");
        let second = FileLayout::from_diff(TWO_HUNKS, 100).expect("second");
        match normalise_selection(&[first, second], 4, 103) {
            Selection::Ops(ops) => {
                assert_eq!(
                    ops[0],
                    SelectionOp::Lines {
                        path: "f.txt".to_string(),
                        hunk_index: 0,
                        lines: vec![2, 3, 6, 9]
                    }
                );
                // The range covers the second file's header line (100).
                assert_eq!(
                    ops[1],
                    SelectionOp::WholeFile {
                        path: "f.txt".to_string()
                    }
                );
            }
            other => panic!("unexpected selection: {other:?}"),
        }
    }

    #[test]
    fn selected_lines_stage_patch_keeps_only_selection() {
        let p = selected_hunk_patch(&sample_hunk(), &[2, 3], false);
        assert!(p.contains("-line3\n"), "got: {p}");
        assert!(p.contains("+line3 CHANGED\n"), "got: {p}");
        // The unselected deletion stays in the old file as context.
        assert!(p.contains(" line8\n"), "got: {p}");
        assert!(!p.contains("-line8"), "got: {p}");
        // The unselected addition is dropped entirely.
        assert!(!p.contains("NEW line"), "got: {p}");
    }

    #[test]
    fn selected_lines_unstage_patch_swaps_roles() {
        let p = selected_hunk_patch(&sample_hunk(), &[2, 3], true);
        assert!(p.contains("-line3\n"), "got: {p}");
        assert!(p.contains("+line3 CHANGED\n"), "got: {p}");
        // An unselected addition must survive the reverse apply as context.
        assert!(p.contains(" NEW line\n"), "got: {p}");
        // An unselected deletion is dropped so it is not restored.
        assert!(!p.contains("line8"), "got: {p}");
    }

    #[test]
    fn diff_path_decodes_git_header_quoting() {
        let spaced = "diff --git a/a b.txt b/a b.txt\n\
--- a/a b.txt\t\n\
+++ b/a b.txt\t\n\
@@ -1 +1 @@\n\
-a\n\
+b\n";
        assert_eq!(
            parse_file_diff(spaced).expect("spaced diff").path,
            "a b.txt"
        );

        let quoted = "diff --git \"a/a\\\"b\\\\c.txt\" \"b/a\\\"b\\\\c.txt\"\n\
--- \"a/a\\\"b\\\\c.txt\"\n\
+++ \"b/a\\\"b\\\\c.txt\"\n\
@@ -1 +1 @@\n\
-a\n\
+b\n";
        assert_eq!(
            parse_file_diff(quoted).expect("quoted diff").path,
            "a\"b\\c.txt"
        );
    }

    #[test]
    fn diff_path_ignores_hunk_body_lines() {
        let diff = "diff --git a/f.txt b/f.txt\n\
--- a/f.txt\n\
+++ b/f.txt\n\
@@ -1 +1,2 @@\n\
 x\n\
+++ b/not-a-path\n";
        assert_eq!(parse_file_diff(diff).expect("diff").path, "f.txt");
    }

    #[test]
    fn synthetic_header_quotes_paths_git_cannot_take_verbatim() {
        assert!(synthetic_file_header("f.txt", false).contains("--- a/f.txt\n"));
        let header = synthetic_file_header("a b.txt", false);
        assert!(header.contains("--- \"a/a b.txt\"\n"), "got: {header}");
        assert!(header.contains("+++ \"b/a b.txt\"\n"), "got: {header}");
    }
}
