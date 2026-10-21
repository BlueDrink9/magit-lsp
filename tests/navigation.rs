//! `definition` and `documentLink` navigation from the tree (R-1.4).

mod common;

use common::*;
use lsp_types::Position;
use magit_lsp::status_view;
use magit_lsp::tree::Tree;

fn line_of(tree: &Tree, text: &str) -> usize {
    tree.lines
        .iter()
        .position(|line| line == text)
        .unwrap_or_else(|| panic!("missing line {text:?} in {:?}", tree.lines))
}

fn file_uri(root: &str, path: &str) -> String {
    format!("file://{root}/{path}")
}

#[test]
fn definition_targets_the_new_side_of_the_worktree_file() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let tree = status_view::build(&repo.status().expect("status"));
    let root = tmp.path().to_string_lossy().to_string();

    let file_line = line_of(&tree, "  M f.txt");
    let location = tree
        .definition(&root, Position::new(file_line as u32, 0))
        .expect("file line visits the file");
    assert_eq!(location.uri.as_str(), file_uri(&root, "f.txt"));
    assert_eq!(location.range.start.line, 0);

    let hunk_line = tree
        .lines
        .iter()
        .position(|line| line.starts_with("    @@"))
        .expect("hunk header");
    let hunk = tree
        .definition(&root, Position::new(hunk_line as u32, 0))
        .expect("hunk header visits the hunk start");
    assert_eq!(hunk.range.start.line, 0, "hunk new_start is line 1");

    let added = tree
        .definition(
            &root,
            Position::new(line_of(&tree, "    +line3 CHANGED") as u32, 0),
        )
        .expect("added line");
    assert_eq!(added.range.start.line, 2);

    let added_second = tree
        .definition(
            &root,
            Position::new(line_of(&tree, "    +NEW line") as u32, 0),
        )
        .expect("added line");
    assert_eq!(added_second.range.start.line, 5);

    let removed = tree
        .definition(&root, Position::new(line_of(&tree, "    -line3") as u32, 0))
        .expect("removed line");
    assert_eq!(removed.range.start.line, 1, "nearest preceding new line");

    let removed_late = tree
        .definition(&root, Position::new(line_of(&tree, "    -line8") as u32, 0))
        .expect("removed line");
    assert_eq!(removed_late.range.start.line, 7);
}

#[test]
fn definition_visits_untracked_files_and_skips_deleted_ones() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("gone.txt", "a\n"), ("f.txt", TEN_LINES)])
        .build();
    std::fs::remove_file(tmp.path().join("gone.txt")).expect("delete gone.txt");
    write(&tmp.path().join("new.txt"), "new\n");

    let tree = status_view::build(&repo.status().expect("status"));
    let root = tmp.path().to_string_lossy().to_string();

    let deleted = line_of(&tree, "  D gone.txt");
    assert!(
        tree.definition(&root, Position::new(deleted as u32, 0))
            .is_none(),
        "a deleted file has no worktree file to visit"
    );

    let untracked = line_of(&tree, "  ? new.txt");
    let location = tree
        .definition(&root, Position::new(untracked as u32, 0))
        .expect("untracked file is visited");
    assert_eq!(location.uri.as_str(), file_uri(&root, "new.txt"));
    assert_eq!(location.range.start.line, 0);
}

#[test]
fn document_links_point_paths_at_worktree_files() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .rename("f.txt", "renamed file.txt")
        .modify("renamed file.txt", standard_changes())
        .build();
    write(&tmp.path().join("new.txt"), "new\n");

    let tree = status_view::build(&repo.status().expect("status"));
    let root = tmp.path().to_string_lossy().to_string();
    let links = tree.document_links(&root);
    assert_eq!(links.len(), tree.links.len());

    let renamed_line = line_of(&tree, "  R renamed file.txt");
    let link = links
        .iter()
        .find(|link| link.range.start.line == renamed_line as u32)
        .expect("a link on the renamed file line");
    assert_eq!(
        link.target.as_ref().expect("file target").as_str(),
        file_uri(&root, "renamed%20file.txt")
    );
    assert_eq!(link.range.start.character, 4);
    assert_eq!(
        link.range.end.character,
        4 + "renamed file.txt".len() as u32
    );
}
