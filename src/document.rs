use crate::protocol::Target;
use lsp_types::{FoldingRange, FoldingRangeKind};

/// A section header in the rendered status document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub key: String,
    pub title: String,
    pub line: usize,
}

/// A file headline in the rendered status document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileItem {
    pub path: String,
    pub section: String,
    pub line: usize,
}

/// A hunk header in the rendered status document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HunkItem {
    pub section: String,
    pub path: Option<String>,
    pub line: usize,
}

/// Structural elements parsed out of the rendered status document.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub sections: Vec<Section>,
    pub files: Vec<FileItem>,
    pub hunks: Vec<HunkItem>,
}

fn section_key(title: &str) -> Option<&'static str> {
    match title {
        "Staged changes" => Some("staged"),
        "Unstaged changes" => Some("unstaged"),
        "Untracked files" => Some("untracked"),
        _ => None,
    }
}

/// Parse the rendered status buffer text into its structural elements.
/// Recognised line shapes (must match the client renderer):
/// - section header: `Staged changes (1)` at column 0
/// - file headline:  `  M path/to/file` (2 spaces, one status char, one space)
/// - hunk header:    `    @@ -a,b +c,d @@` (4 spaces, then `@@`)
pub fn parse(text: &str) -> Parsed {
    let mut parsed = Parsed::default();
    let mut current_section: Option<String> = None;
    let mut current_path: Option<String> = None;

    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let bytes = raw.as_bytes();

        if !raw.starts_with(' ') {
            if let Some(idx) = raw.rfind(" (") {
                let title = &raw[..idx];
                if raw.ends_with(')') {
                    if let Some(key) = section_key(title) {
                        parsed.sections.push(Section {
                            key: key.to_string(),
                            title: title.to_string(),
                            line,
                        });
                        current_section = Some(key.to_string());
                        current_path = None;
                        continue;
                    }
                }
            }
        }

        // File headline: exactly two leading spaces, a non-space status char, a space.
        if bytes.len() > 4
            && bytes[0] == b' '
            && bytes[1] == b' '
            && bytes[2] != b' '
            && bytes[3] == b' '
        {
            let path = raw[4..].to_string();
            if let Some(section) = current_section.clone() {
                parsed.files.push(FileItem {
                    path: path.clone(),
                    section,
                    line,
                });
                current_path = Some(path);
                continue;
            }
        }

        if raw.starts_with("    @@") {
            parsed.hunks.push(HunkItem {
                section: current_section.clone().unwrap_or_default(),
                path: current_path.clone(),
                line,
            });
        }
    }

    parsed
}

/// Navigation targets (sections, files, hunks) with 1-based line numbers.
pub fn targets(text: &str) -> Vec<Target> {
    let parsed = parse(text);
    let mut out = Vec::new();
    for s in &parsed.sections {
        out.push(Target {
            kind: "section".to_string(),
            label: s.title.clone(),
            line: s.line as u32,
            section: Some(s.key.clone()),
            path: None,
        });
    }
    for f in &parsed.files {
        out.push(Target {
            kind: "file".to_string(),
            label: f.path.clone(),
            line: f.line as u32,
            section: Some(f.section.clone()),
            path: Some(f.path.clone()),
        });
    }
    for h in &parsed.hunks {
        out.push(Target {
            kind: "hunk".to_string(),
            label: "@@".to_string(),
            line: h.line as u32,
            section: Some(h.section.clone()),
            path: h.path.clone(),
        });
    }
    out.sort_by_key(|t| t.line);
    out
}

fn last_nonblank(lines: &[&str], start: usize, end_exclusive: usize) -> Option<usize> {
    (start..end_exclusive.min(lines.len()))
        .rev()
        .find(|&i| !lines[i].trim().is_empty())
}

/// Fold ranges for each section and file block (0-based, LSP convention).
pub fn folding_ranges(text: &str) -> Vec<FoldingRange> {
    let lines: Vec<&str> = text.lines().collect();
    let parsed = parse(text);
    let mut ranges = Vec::new();

    // Sections fold up to the next section.
    for (i, s) in parsed.sections.iter().enumerate() {
        let start = s.line - 1;
        let next = parsed
            .sections
            .get(i + 1)
            .map(|n| n.line - 1)
            .unwrap_or(lines.len());
        if let Some(end) = last_nonblank(&lines, start, next) {
            if end > start {
                ranges.push(fold(start, end));
            }
        }
    }

    // Files fold up to the next file or the next section.
    for (i, f) in parsed.files.iter().enumerate() {
        let start = f.line - 1;
        let next_file = parsed.files.get(i + 1).map(|n| n.line - 1);
        let next_section = parsed
            .sections
            .iter()
            .find(|s| s.line - 1 > start)
            .map(|s| s.line - 1);
        let next = match (next_file, next_section) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => lines.len(),
        };
        if let Some(end) = last_nonblank(&lines, start, next) {
            if end > start {
                ranges.push(fold(start, end));
            }
        }
    }

    ranges
}

fn fold(start: usize, end: usize) -> FoldingRange {
    FoldingRange {
        start_line: start as u32,
        end_line: end as u32,
        start_character: None,
        end_character: None,
        kind: Some(FoldingRangeKind::Region),
        collapsed_text: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "\
Head: main

Staged changes (0)

Unstaged changes (1)
  M f.txt
    @@ -1,3 +1,3 @@
     a
    -b
    +B
     c

Untracked files (1)
  ? new.txt
";

    #[test]
    fn parses_sections_files_hunks() {
        let p = parse(DOC);
        assert_eq!(p.sections.len(), 3);
        assert_eq!(p.sections[0].key, "staged");
        assert_eq!(p.sections[0].line, 3);
        assert_eq!(p.sections[1].key, "unstaged");
        assert_eq!(p.sections[1].line, 5);
        assert_eq!(p.sections[2].key, "untracked");
        assert_eq!(p.sections[2].line, 13);

        assert_eq!(p.files.len(), 2);
        assert_eq!(p.files[0].path, "f.txt");
        assert_eq!(p.files[0].section, "unstaged");
        assert_eq!(p.files[0].line, 6);
        assert_eq!(p.files[1].path, "new.txt");
        assert_eq!(p.files[1].section, "untracked");

        assert_eq!(p.hunks.len(), 1);
        assert_eq!(p.hunks[0].line, 7);
    }

    #[test]
    fn diff_lines_are_not_files() {
        let p = parse(DOC);
        // The '+B' line starts with 4 spaces and must not be seen as a file.
        assert!(p.files.iter().all(|f| f.path != "B"));
    }

    #[test]
    fn targets_include_sections_files_hunks() {
        let t = targets(DOC);
        assert!(t.iter().any(|x| x.kind == "section" && x.section.as_deref() == Some("unstaged")));
        assert!(t.iter().any(|x| x.kind == "file" && x.path.as_deref() == Some("f.txt")));
        assert!(t.iter().any(|x| x.kind == "hunk"));
        // sorted by line
        let lines: Vec<u32> = t.iter().map(|x| x.line).collect();
        let mut sorted = lines.clone();
        sorted.sort();
        assert_eq!(lines, sorted);
    }

    #[test]
    fn folding_ranges_cover_sections_and_files() {
        let ranges = folding_ranges(DOC);
        // section "Unstaged changes" starts at 0-based line 4, ends at 0-based 10
        assert!(ranges.iter().any(|r| r.start_line == 4 && r.end_line == 10));
        // file "f.txt" starts at 0-based 5, ends at 0-based 10
        assert!(ranges.iter().any(|r| r.start_line == 5 && r.end_line == 10));
    }

    #[test]
    fn empty_document_has_no_ranges() {
        assert!(folding_ranges("Head: main\n\nWorking tree clean\n").is_empty());
    }
}