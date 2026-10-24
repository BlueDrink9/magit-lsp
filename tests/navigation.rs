//! `definition` and `documentLink` navigation from the tree (R-1.4).

mod common;

use common::stdio::TestServer;
use common::*;
use lsp_types::Position;
use magit_lsp::status_view;
use magit_lsp::tree::{NodeKind, Tree};
use serde_json::{json, Value};

fn line_of(
    tree: &Tree,
    path: &str,
    kind: NodeKind,
    marker: Option<char>,
    text: Option<&str>,
) -> usize {
    tree.lines
        .iter()
        .enumerate()
        .find(|(index, _)| {
            let line = &tree.line_nodes[*index];
            let node = tree.node(&line.node).expect("line node");
            line.path.as_deref() == Some(path)
                && line.kind == kind
                && marker.is_none_or(|marker| line.marker == Some(marker))
                && text.is_none_or(|text| line.text.as_deref() == Some(text))
                && (kind != NodeKind::File || node.path.as_deref() == Some(path))
        })
        .map(|(index, _)| index)
        .unwrap_or_else(|| panic!("missing {kind:?} line for {path:?}"))
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

    let file_line = line_of(&tree, "f.txt", NodeKind::File, None, None);
    let location = tree
        .definition(&root, Position::new(file_line as u32, 0))
        .expect("file line visits the file");
    assert_eq!(location.uri.as_str(), file_uri(&root, "f.txt"));
    assert_eq!(location.range.start.line, 0);

    let hunk_line = line_of(&tree, "f.txt", NodeKind::Hunk, None, None);
    let hunk = tree
        .definition(&root, Position::new(hunk_line as u32, 0))
        .expect("hunk header visits the hunk start");
    assert_eq!(hunk.range.start.line, 0, "hunk new_start is line 1");

    let added = tree
        .definition(
            &root,
            Position::new(
                line_of(
                    &tree,
                    "f.txt",
                    NodeKind::Line,
                    Some('+'),
                    Some("line3 CHANGED"),
                ) as u32,
                0,
            ),
        )
        .expect("added line");
    assert_eq!(added.range.start.line, 2);

    let added_second = tree
        .definition(
            &root,
            Position::new(
                line_of(&tree, "f.txt", NodeKind::Line, Some('+'), Some("NEW line")) as u32,
                0,
            ),
        )
        .expect("added line");
    assert_eq!(added_second.range.start.line, 5);

    let removed = tree
        .definition(
            &root,
            Position::new(
                line_of(&tree, "f.txt", NodeKind::Line, Some('-'), Some("line3")) as u32,
                0,
            ),
        )
        .expect("removed line");
    assert_eq!(removed.range.start.line, 1, "nearest preceding new line");

    let removed_late = tree
        .definition(
            &root,
            Position::new(
                line_of(&tree, "f.txt", NodeKind::Line, Some('-'), Some("line8")) as u32,
                0,
            ),
        )
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

    let deleted = line_of(&tree, "gone.txt", NodeKind::File, None, None);
    assert!(
        tree.definition(&root, Position::new(deleted as u32, 0))
            .is_none(),
        "a deleted file has no worktree file to visit"
    );

    let untracked = line_of(&tree, "new.txt", NodeKind::File, None, None);
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

    let renamed_line = line_of(&tree, "renamed file.txt", NodeKind::File, None, None);
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

#[test]
fn positional_lsp_requests_use_only_the_acknowledged_render() {
    let (tmp, repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let mut server = TestServer::spawn(&[]);
    let initialized = server.initialize(json!({}));
    assert!(initialized["error"].is_null(), "{initialized}");
    server.send(json!({
        "jsonrpc": "2.0", "id": 2, "method": "view/open",
        "params": {"kind": "status", "args": {"repo_path": tmp.path().to_string_lossy()},
            "viewport": {"cols": 80, "rows": 24}},
    }));
    let opened = server.response_with_id(2);
    let view_id = opened["result"]["id"]
        .as_str()
        .expect("view id")
        .to_string();
    let uri = opened["result"]["uri"]
        .as_str()
        .expect("view URI")
        .to_string();
    let v1_symbols = status_view::build(&repo.status().expect("initial status")).document_symbols();
    let initial_render = loop {
        let message = server.recv();
        if message["method"] == json!("view/render") {
            break message;
        }
    };
    let v1 = initial_render["params"]["version"]
        .as_u64()
        .expect("v1 version");
    server.send(json!({"jsonrpc":"2.0", "method":"view/rendered", "params":{"viewId":view_id,"version":v1}}));

    write(&tmp.path().join("new.txt"), "new file\n");
    server.send(json!({"jsonrpc":"2.0", "id":3, "method":"view/invoke",
        "params":{"viewId":view_id,"command":"common.refresh"}}));
    let refreshed = server.response_with_id(3);
    assert_eq!(refreshed["result"]["effects"], json!([]), "{refreshed}");
    let v2_render = server
        .notifications
        .iter()
        .find(|message| {
            message["method"] == json!("view/render") && message["params"]["version"] != json!(v1)
        })
        .cloned()
        .unwrap_or_else(|| loop {
            let message = server.recv();
            if message["method"] == json!("view/render") {
                break message;
            }
        });
    let v2 = v2_render["params"]["version"].as_u64().expect("v2 version");
    assert_ne!(v1, v2);
    let v2_tree = status_view::build(&repo.status().expect("updated status"));
    let new_file_line = line_of(&v2_tree, "new.txt", NodeKind::File, None, None);
    let v2_symbols = v2_tree.document_symbols();
    assert!(!v2_symbols.is_empty());

    let definition_request = |id| {
        json!({"jsonrpc":"2.0","id":id,"method":"textDocument/definition",
        "params":{"textDocument":{"uri":uri},"position":{"line":new_file_line,"character":0}}})
    };
    let symbols_request = |id| {
        json!({"jsonrpc":"2.0","id":id,"method":"textDocument/documentSymbol",
        "params":{"textDocument":{"uri":uri}}})
    };
    server.send(definition_request(4));
    let def_v1 = server.response_with_id(4);
    assert_eq!(
        def_v1["result"],
        Value::Null,
        "unacked v2 must not affect definition: {def_v1}"
    );
    server.send(symbols_request(5));
    let symbols_v1 = server.response_with_id(5);
    assert_eq!(
        symbols_v1["result"],
        json!(v1_symbols),
        "unacked v2 must not affect symbols: {symbols_v1}"
    );

    server.send(
        json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view_id,"version":v2}}),
    );
    server.send(definition_request(6));
    let def_v2 = server.response_with_id(6);
    assert_eq!(
        def_v2["result"]["uri"],
        json!(file_uri(&tmp.path().to_string_lossy(), "new.txt")),
        "{def_v2}"
    );
    server.send(symbols_request(7));
    let symbols_v2 = server.response_with_id(7);
    assert_eq!(symbols_v2["result"], json!(v2_symbols));
    assert_ne!(symbols_v1["result"], symbols_v2["result"]);

    server.shutdown_and_exit();
    let status = server.finish(std::time::Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
}
