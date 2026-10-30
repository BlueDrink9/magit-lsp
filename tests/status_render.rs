//! Status rendering: text snapshots, node identity, tokens and tree-derived
//! folding/targets (R-1.1).

mod common;

use common::*;
use expect_test::expect;
use lsp_types::{Position, SymbolKind};
use magit_lsp::git::{FileEntry, Status};
use magit_lsp::patch::{DiffLine, Hunk};
use magit_lsp::status_view;
use magit_lsp::tree::{NodeKind, Token, Tree, Visibility, VisibilityState};

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
        metadata: Default::default(),
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
            mutation_identity: None,
        }],
        untracked: vec![FileEntry {
            path: "new.txt".to_string(),
            status: "?".to_string(),
            orig_path: None,
            hunks: Vec::new(),
            mutation_identity: None,
        }],
    }
}

#[test]
fn file_identity_is_retained_in_the_server_tree_but_never_sent_or_copied() {
    let (tmp, repo) = RepoFixture::new().commit("init", [("f.txt", "base\0bytes")]).build();
    write(&tmp.path().join("f.txt"), "shown\0bytes");
    let status = repo.status_listing().unwrap();
    let tree = status_view::build(&status);
    let file = tree.nodes.iter().find(|node| node.kind == NodeKind::File).unwrap();
    let captured = file.mutation.clone().expect("server-owned identity");
    assert!(matches!(captured, magit_lsp::tree::MutationContent::File(_)));
    assert!(file.copy.is_none());
    assert_eq!(tree.copy_value(Some(file.start_line), None).as_deref(), Some("f.txt"));
    let wire = serde_json::to_string(&tree).unwrap();
    assert!(!wire.contains("file-identity:"));
    assert!(!wire.contains("worktree"));
    write(&tmp.path().join("f.txt"), "later\0bytes");
    let fresh = status_view::build(&repo.status_listing().unwrap());
    let new_file = fresh.nodes.iter().find(|node| node.kind == NodeKind::File).unwrap();
    assert_ne!(new_file.mutation.as_ref(), Some(&captured));
    assert_eq!(tree.clone().node(&file.id).unwrap().mutation.as_ref(), Some(&captured));
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

/// The clean-tree message the Lua suite used to assert: even an empty tree
/// renders a non-blank buffer ending in the clean message.
#[test]
fn a_clean_tree_renders_a_non_blank_working_tree_clean_message() {
    let (_tmp, repo) = init_repo();
    let tree = status_view::build(&repo.status().expect("status"));
    assert_eq!(
        tree.lines,
        vec![
            "Head: main",
            "",
            "Staged changes (0)",
            "",
            "Unstaged changes (0)",
            "",
            "Untracked files (0)",
            "",
            "Working tree clean",
        ]
    );
}

#[test]
fn node_ids_survive_a_hunk_shifting_lines() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", lines(40).to_string())])
        .build();
    let changed = lines(40).with(35, "line35 CHANGED").to_string();
    write(&tmp.path().join("f.txt"), &changed);

    let before = status_view::build(&repo.status().expect("status"));
    let hunk = before
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Hunk)
        .expect("hunk before");
    let hunk_node_id = hunk.id.clone();
    let hunk_line_before = hunk.start_line;

    // A new line far above the hunk shifts it but does not change its content.
    write(&tmp.path().join("f.txt"), &format!("line0\n{changed}"));

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

}

#[test]
fn document_symbols_nest_sections_files_and_hunks() {
    let tree = status_view::build(&old_parser_status());
    assert_eq!(
        tree.lines,
        OLD_PARSER_DOC
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>()
    );

    let symbols = tree.document_symbols();
    let names: Vec<&str> = symbols.iter().map(|symbol| symbol.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["Staged changes", "Unstaged changes", "Untracked files"]
    );
    assert!(symbols
        .iter()
        .all(|symbol| symbol.kind == SymbolKind::NAMESPACE));

    let staged = &symbols[0];
    assert_eq!(staged.detail.as_deref(), Some("0"));
    assert!(
        staged.children.is_none(),
        "an empty section has no children"
    );

    let unstaged = &symbols[1];
    assert_eq!(unstaged.detail.as_deref(), Some("1"));
    assert_eq!(unstaged.range.start, Position::new(4, 0));
    assert_eq!(unstaged.range.end, Position::new(10, 6));
    assert_eq!(unstaged.selection_range.start, Position::new(4, 0));
    assert_eq!(unstaged.selection_range.end, Position::new(4, 16));

    let children = unstaged.children.as_ref().expect("unstaged files");
    let file = &children[0];
    assert_eq!(file.name, "f.txt");
    assert_eq!(file.kind, SymbolKind::FILE);
    assert_eq!(file.detail.as_deref(), Some("M"));
    assert_eq!(file.selection_range.start, Position::new(5, 4));
    assert_eq!(file.selection_range.end, Position::new(5, 9));

    let hunk = &file.children.as_ref().expect("hunks")[0];
    assert_eq!(hunk.name, "@@");
    assert_eq!(hunk.kind, SymbolKind::METHOD);
    assert_eq!(hunk.range.start, Position::new(6, 0));
    assert_eq!(hunk.range.end, Position::new(10, 6));
}

#[test]
fn limited_sections_emit_a_structured_exact_overflow_row_at_the_boundary() {
    let status = old_parser_status();
    let full = status_view::build(&status);
    let section = full.nodes.iter().find(|node| node.id == "section:unstaged").unwrap();
    let body_lines = section.end_line - section.start_line;
    let render = |limit| status_view::build_with_limit(&status, &Visibility::new(), Some(limit), |_, file| file.hunks.clone());

    let below = render(body_lines - 1);
    let collapsed = below.node("section:unstaged").unwrap();
    let overflow_id = "section:unstaged:overflow";
    assert_eq!(below.lines[collapsed.start_line + 1], format!("{body_lines} more lines: expand"));
    assert_eq!(collapsed.children, vec![overflow_id.to_string()]);
    assert_eq!(below.node(overflow_id).unwrap().kind, NodeKind::Message);
    assert_eq!(below.line_nodes.len(), below.lines.len());
    assert!(below.line_nodes.iter().all(|entry| below.node(&entry.node).is_some()));
    assert!(below.tokens.iter().all(|token| token.line < below.lines.len()));
    assert!(below.links.iter().all(|link| link.line < below.lines.len()));

    let at = render(body_lines);
    assert!(at.node("unstaged/f.txt").is_some());
    assert!(at.node(overflow_id).is_none());
    let above = render(body_lines + 1);
    assert!(above.node("unstaged/f.txt").is_some());

    let mut explicitly_expanded = Visibility::new();
    explicitly_expanded.set("section:unstaged", VisibilityState::Expanded);
    let expanded = status_view::build_with_limit(&status, &explicitly_expanded, Some(1), |_, file| file.hunks.clone());
    assert!(expanded.node("unstaged/f.txt").is_some());
    let mut explicitly_collapsed = Visibility::new();
    explicitly_collapsed.set("section:unstaged", VisibilityState::Collapsed);
    let collapsed = status_view::build_with_limit(&status, &explicitly_collapsed, Some(1), |_, file| file.hunks.clone());
    assert!(collapsed.node("section:unstaged:overflow").is_none());
    assert!(collapsed.node("unstaged/f.txt").is_none());

    // Ordinary/rich rendering remains byte-for-byte the original full tree.
    assert_eq!(full, status_view::build(&status));
}

#[test]
fn one_oversized_file_is_summarized_without_partial_hunk_structure() {
    let mut status = old_parser_status();
    let lines = vec![('+', "large added line"); 1200];
    status.unstaged[0].hunks = vec![hunk("large", lines)];
    let full = status_view::build(&status);
    let expected_hidden = {
        let section = full.node("section:unstaged").unwrap();
        section.end_line - section.start_line
    };
    let tree = status_view::build_with_limit(&status, &Visibility::new(), Some(100), |_, file| file.hunks.clone());
    let row = tree.node("section:unstaged:overflow").unwrap();
    assert_eq!(tree.lines[row.start_line], format!("{expected_hidden} more lines: expand"));
    assert!(tree.node("unstaged/f.txt").is_none());
    assert!(tree.nodes.iter().all(|node| node.kind != NodeKind::Hunk));
    assert_eq!(tree.line_nodes.len(), tree.lines.len());
    assert!(tree.symbols.iter().any(|symbol| symbol.node == "section:unstaged" && symbol.children.is_empty()));
    assert!(tree.tokens.iter().all(|token| token.line < tree.lines.len()));
    assert!(tree.links.iter().all(|link| link.line < tree.lines.len()));
}

#[test]
fn document_symbols_use_utf16_columns_for_a_unicode_path() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .rename("f.txt", "é😀.txt")
        .modify("é😀.txt", standard_changes())
        .build();
    write(&tmp.path().join("new.txt"), "new\n");

    let tree = status_view::build(&repo.status().expect("status"));
    let symbols = tree.document_symbols();
    let staged_file = symbols[0].children.as_ref().expect("staged")[0].clone();
    assert_eq!(staged_file.name, "é😀.txt");
    assert_eq!(staged_file.kind, SymbolKind::FILE);
    assert_eq!(staged_file.detail.as_deref(), Some("R"));

    // "é" is two UTF-8 bytes but one UTF-16 code unit; "😀" is four bytes
    // and two code units. The prefix "  R " is ASCII, so the path starts at 4.
    assert_eq!(staged_file.selection_range.start.character, 4);
    assert_eq!(staged_file.selection_range.end.character, 11);

    let unstaged_file = symbols[1].children.as_ref().expect("unstaged")[0].clone();
    let hunk = &unstaged_file.children.as_ref().expect("hunks")[0];
    assert_eq!(hunk.kind, SymbolKind::METHOD);
    assert_eq!(
        symbols[2].children.as_ref().expect("untracked")[0].name,
        "new.txt"
    );
}

#[test]
fn folding_matches_the_old_parser() {
    let tree = status_view::build(&old_parser_status());
    let ranges: Vec<(u32, u32)> = tree
        .folding_ranges()
        .into_iter()
        .map(|range| (range.start_line, range.end_line))
        .collect();
    assert_eq!(ranges, vec![(4, 10), (12, 13), (5, 10)]);
}
