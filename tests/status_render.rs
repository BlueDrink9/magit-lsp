//! Status rendering: text snapshots, node identity, tokens and tree-derived
//! folding/targets (R-1.1).

mod common;

use common::*;
use expect_test::expect;
use magit_lsp::git::{FileEntry, Status};
use magit_lsp::patch::{DiffLine, Hunk};
use magit_lsp::status_view;
use magit_lsp::tree::{Document, NodeKind, Token, Tree};

fn hunk(id: &str, lines: Vec<(char, &str)>) -> Hunk {
    Hunk {
        old_start: 1,
        old_count: 3,
        new_start: 1,
        new_count: 3,
        lines: lines
            .into_iter()
            .map(|(marker, text)| DiffLine {
                marker,
                text: text.to_string(),
                old_no: None,
                new_no: None,
                no_newline: false,
            })
            .collect(),
        id: id.to_string(),
    }
}

/// The document from the old `src/document.rs` parser tests.
const OLD_PARSER_DOC: &str = "\
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

fn old_parser_status() -> Status {
    Status {
        repo_path: "/repo".to_string(),
        branch: "main".to_string(),
        head: None,
        upstream: None,
        ahead: None,
        behind: None,
        staged: Vec::new(),
        unstaged: vec![FileEntry {
            path: "f.txt".to_string(),
            status: "M".to_string(),
            orig_path: None,
            hunks: vec![hunk(
                "old",
                vec![(' ', "a"), ('-', "b"), ('+', "B"), (' ', "c")],
            )],
        }],
        untracked: vec![FileEntry {
            path: "new.txt".to_string(),
            status: "?".to_string(),
            orig_path: None,
            hunks: Vec::new(),
        }],
    }
}

fn line_of(tree: &Tree, text: &str) -> usize {
    tree.lines
        .iter()
        .position(|line| line == text)
        .unwrap_or_else(|| panic!("missing line {text:?} in {:?}", tree.lines))
}

fn has_token(tree: &Tree, line: usize, start: usize, length: usize, token_type: &str) -> bool {
    tree.tokens.iter().any(|token| {
        token.line == line
            && token.start == start
            && token.length == length
            && token.token_type == token_type
    })
}

fn token_with_type<'a>(tree: &'a Tree, line: usize, token_type: &str) -> Option<&'a Token> {
    tree.tokens
        .iter()
        .find(|token| token.line == line && token.token_type == token_type)
}

#[test]
fn snapshot_shows_staged_unstaged_untracked_and_a_rename() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .rename("f.txt", "renamed file.txt")
        .modify("renamed file.txt", standard_changes())
        .build();
    write(&tmp.path().join("new.txt"), "new\n");

    let tree = status_view::build(&repo.status().expect("status"));
    let text = tree.lines.join("\n");
    expect![[r#"
        Head: main

        Staged changes (1)
          R renamed file.txt

        Unstaged changes (1)
          M renamed file.txt
            @@ -1,10 +1,10 @@
             line1
             line2
            -line3
            +line3 CHANGED
             line4
             line5
            +NEW line
             line6
             line7
            -line8
             line9
             line10

        Untracked files (1)
          ? new.txt"#]]
    .assert_eq(&text);
}

#[test]
fn node_ids_survive_a_hunk_shifting_lines() {
    let (tmp, repo) = init_40_line_repo();
    let mut lines: Vec<String> = (1..=40).map(|i| format!("line{i}")).collect();
    lines[34] = "line35 CHANGED".into();
    write(&tmp.path().join("f.txt"), &(lines.join("\n") + "\n"));

    let before = status_view::build(&repo.status().expect("status"));
    let hunk = before
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Hunk)
        .expect("hunk before");
    let hunk_node_id = hunk.id.clone();
    let hunk_line_before = hunk.start_line;

    // A new line far above the hunk shifts it but does not change its content.
    let mut shifted = vec!["line0".to_string()];
    shifted.extend(lines);
    write(&tmp.path().join("f.txt"), &(shifted.join("\n") + "\n"));

    let after = status_view::build(&repo.status().expect("status"));
    let moved = after
        .node(&hunk_node_id)
        .expect("the same node id survives the refresh");
    assert_ne!(moved.start_line, hunk_line_before, "the hunk moved lines");
    assert_eq!(moved.kind, NodeKind::Hunk);
}

#[test]
fn tokens_cover_sections_files_hunks_and_diff_lines() {
    let (_tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .staged_file("g.txt", "a\nb\n")
        .modify_standard()
        .build();
    let tree = status_view::build(&repo.status().expect("status"));

    let section_line = line_of(&tree, "Unstaged changes (1)");
    assert!(has_token(
        &tree,
        section_line,
        0,
        "Unstaged changes".len(),
        "magit.section"
    ));
    assert!(has_token(
        &tree,
        section_line,
        "Unstaged changes".len() + 1,
        "(1)".len(),
        "magit.count"
    ));

    let file_line = line_of(&tree, "  M f.txt");
    assert!(has_token(&tree, file_line, 2, 1, "magit.file.modified"));
    assert!(has_token(&tree, file_line, 4, "f.txt".len(), "magit.file"));

    let hunk_line = tree
        .lines
        .iter()
        .position(|line| line.starts_with("    @@"))
        .expect("hunk header");
    assert!(has_token(
        &tree,
        hunk_line,
        0,
        tree.lines[hunk_line].len(),
        "magit.hunk.header"
    ));

    let added = line_of(&tree, "    +line3 CHANGED");
    let removed = line_of(&tree, "    -line3");
    assert!(has_token(
        &tree,
        added,
        0,
        tree.lines[added].len(),
        "magit.diff.added"
    ));
    assert!(has_token(
        &tree,
        removed,
        0,
        tree.lines[removed].len(),
        "magit.diff.removed"
    ));

    let staged_file = line_of(&tree, "  A g.txt");
    let staged_token =
        token_with_type(&tree, staged_file, "magit.file.added").expect("staged file status token");
    assert_eq!(staged_token.modifiers, vec!["staged".to_string()]);
    let staged_path = token_with_type(&tree, staged_file, "magit.file").expect("staged path token");
    assert_eq!(staged_path.modifiers, vec!["staged".to_string()]);
}

#[test]
fn symbols_nest_section_file_and_hunk() {
    let tree = status_view::build(&old_parser_status());
    let unstaged = tree
        .symbols
        .iter()
        .find(|symbol| symbol.name == "Unstaged changes")
        .expect("unstaged section symbol");
    assert_eq!(unstaged.kind, "section");

    let file = unstaged
        .children
        .iter()
        .find(|symbol| symbol.name == "f.txt")
        .expect("file symbol");
    assert_eq!(file.kind, "file");

    let hunk = file.children.first().expect("hunk symbol");
    assert_eq!(hunk.kind, "hunk");
    assert_eq!(hunk.name, "@@");

    let staged = tree
        .symbols
        .iter()
        .find(|symbol| symbol.name == "Staged changes")
        .expect("staged section symbol");
    assert!(staged.children.is_empty());
}

#[test]
fn line_map_resolves_every_rendered_line() {
    let tree = status_view::build(&old_parser_status());
    assert_eq!(tree.line_nodes.len(), tree.lines.len());

    for (index, line_node) in tree.line_nodes.iter().enumerate() {
        assert!(
            tree.node(&line_node.node).is_some(),
            "line {index} ({:?}) points at unknown node {}",
            tree.lines[index],
            line_node.node
        );
    }

    let removed = line_of(&tree, "    -b");
    let entry = &tree.line_nodes[removed];
    assert_eq!(entry.kind, NodeKind::Line);
    assert_eq!(entry.marker, Some('-'));
    assert_eq!(entry.text.as_deref(), Some("b"));
    assert_eq!(entry.hunk_id.as_deref(), Some("old"));

    let document = Document::from(&tree);
    assert_eq!(document.text, tree.lines);
}

#[test]
fn targets_and_folding_match_the_old_parser() {
    let tree = status_view::build(&old_parser_status());
    assert_eq!(
        tree.lines,
        OLD_PARSER_DOC
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>()
    );

    let targets: Vec<(String, String, u32, Option<String>, Option<String>)> = tree
        .targets()
        .into_iter()
        .map(|target| {
            (
                target.kind,
                target.label,
                target.line,
                target.section,
                target.path,
            )
        })
        .collect();
    assert_eq!(
        targets,
        vec![
            (
                "section".to_string(),
                "Staged changes".to_string(),
                3,
                Some("staged".to_string()),
                None
            ),
            (
                "section".to_string(),
                "Unstaged changes".to_string(),
                5,
                Some("unstaged".to_string()),
                None
            ),
            (
                "file".to_string(),
                "f.txt".to_string(),
                6,
                Some("unstaged".to_string()),
                Some("f.txt".to_string())
            ),
            (
                "hunk".to_string(),
                "@@".to_string(),
                7,
                Some("unstaged".to_string()),
                Some("f.txt".to_string())
            ),
            (
                "section".to_string(),
                "Untracked files".to_string(),
                13,
                Some("untracked".to_string()),
                None
            ),
            (
                "file".to_string(),
                "new.txt".to_string(),
                14,
                Some("untracked".to_string()),
                Some("new.txt".to_string())
            ),
        ]
    );

    let ranges: Vec<(u32, u32)> = tree
        .folding_ranges()
        .into_iter()
        .map(|range| (range.start_line, range.end_line))
        .collect();
    assert_eq!(ranges, vec![(4, 10), (12, 13), (5, 10)]);
}
