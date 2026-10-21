use serde::{Deserialize, Serialize};

/// Number of context lines kept around a line selection when rebuilding a hunk.
pub const CONTEXT: isize = 3;

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

    for raw in diff.lines() {
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

/// Build a renumbered patch for the lines in `start..=end`, with context.
pub fn subset_hunk_patch(h: &Hunk, start: usize, end: usize) -> String {
    let n = h.lines.len();
    if n == 0 {
        return String::new();
    }
    let start = start.min(n - 1);
    let end = end.max(start).min(n - 1);

    let lo = start as isize;
    let hi = end as isize;
    let mut body: Vec<usize> = Vec::new();
    for (i, l) in h.lines.iter().enumerate() {
        let ii = i as isize;
        if ii >= lo && ii <= hi {
            body.push(i);
        } else if l.marker == ' ' && ii >= lo - CONTEXT && ii <= hi + CONTEXT {
            body.push(i);
        }
    }

    rebuild_subset(h, &body)
}

/// Rebuild a hunk from the given line indices, recomputing the header counts
/// and start positions so the patch applies cleanly.
pub fn rebuild_subset(h: &Hunk, body: &[usize]) -> String {
    let mut old_start: Option<u32> = None;
    for &i in body {
        let l = &h.lines[i];
        if l.marker != '+' {
            if let Some(o) = l.old_no {
                old_start = Some(o);
                break;
            }
        }
    }
    let old_start = match old_start {
        Some(v) => v,
        None => match body.iter().find_map(|&i| h.lines[i].new_no) {
            Some(v) => v.saturating_sub(1),
            None => 0,
        },
    };

    let mut new_cur = old_start;
    let mut new_start: Option<u32> = None;
    let mut old_count: u32 = 0;
    let mut new_count: u32 = 0;
    let mut out = String::new();

    for &i in body {
        let l = &h.lines[i];
        match l.marker {
            ' ' => {
                if new_start.is_none() {
                    new_start = Some(new_cur);
                }
                old_count += 1;
                new_count += 1;
                push_hunk_line(&mut out, ' ', &l.text);
                if l.no_newline {
                    out.push_str("\\ No newline at end of file\n");
                }
                new_cur += 1;
            }
            '-' => {
                old_count += 1;
                push_hunk_line(&mut out, '-', &l.text);
                if l.no_newline {
                    out.push_str("\\ No newline at end of file\n");
                }
            }
            '+' => {
                if new_start.is_none() {
                    new_start = Some(new_cur);
                }
                new_count += 1;
                push_hunk_line(&mut out, '+', &l.text);
                if l.no_newline {
                    out.push_str("\\ No newline at end of file\n");
                }
                new_cur += 1;
            }
            _ => {}
        }
    }

    let new_start = new_start.unwrap_or(old_start);
    let mut header = format!(
        "@@ -{},{} +{},{} @@\n",
        old_start, old_count, new_start, new_count
    );
    header.push_str(&out);
    header
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

    #[test]
    fn subset_single_modification() {
        let h = sample_hunk();
        // change is indices 2..=3 (modify line3). Select that range.
        let p = subset_hunk_patch(&h, 2, 3);
        assert!(p.starts_with("@@ -1,5 +1,5 @@"), "got: {p}");
        assert!(p.contains("-line3\n"));
        assert!(p.contains("+line3 CHANGED\n"));
        assert!(!p.contains("NEW line"));
        assert!(!p.contains("-line8"));
    }

    #[test]
    fn subset_single_addition() {
        let h = sample_hunk();
        let p = subset_hunk_patch(&h, 6, 6);
        assert!(p.starts_with("@@ -4,4 +4,5 @@"), "got: {p}");
        assert!(p.contains("+NEW line\n"));
        assert!(!p.contains("line3 CHANGED"));
        assert!(!p.contains("-line8"));
    }

    #[test]
    fn subset_single_deletion() {
        let h = sample_hunk();
        let p = subset_hunk_patch(&h, 9, 9);
        assert!(p.starts_with("@@ -6,5 +6,4 @@"), "got: {p}");
        assert!(p.contains("-line8\n"));
        assert!(!p.contains("line3 CHANGED"));
        assert!(!p.contains("NEW line"));
    }
}