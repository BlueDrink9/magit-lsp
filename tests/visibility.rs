//! Server-owned visibility and lazy bodies (R-1.5).

mod common;

use common::*;
use magit_lsp::diff_text;
use magit_lsp::git::diff_invocation_count;
use magit_lsp::protocol::{Cursor, ViewInvokeParams, ViewRender};
use magit_lsp::session::Session;
use magit_lsp::tree::{Node, NodeKind, Tree};
use magit_lsp::view::{Placement, ViewKind, Viewport};
use serde_json::json;

fn open_status(session: &mut Session) -> ViewRender {
    session
        .open(
            "v1",
            ViewKind::Status,
            Placement::Main,
            Viewport { cols: 80, rows: 24 },
        )
        .expect("open status view")
        .1
}

fn tree(session: &Session) -> &Tree {
    session.views().get("v1").expect("view").tree()
}

fn acked_tree(session: &Session) -> &Tree {
    session
        .views()
        .get("v1")
        .and_then(|view| view.acked_tree())
        .expect("acked tree")
}

fn node_at<'a>(tree: &'a Tree, cursor: Cursor) -> &'a Node {
    let line = tree
        .line_nodes
        .get(cursor.line)
        .unwrap_or_else(|| panic!("cursor line {} is outside the tree", cursor.line));
    tree.node(&line.node).expect("cursor node")
}

fn line_with_marker(tree: &Tree, path: &str, marker: char) -> usize {
    tree.line_nodes
        .iter()
        .position(|line| {
            line.kind == NodeKind::Line
                && line.path.as_deref() == Some(path)
                && line.marker == Some(marker)
        })
        .unwrap_or_else(|| panic!("no {marker:?} line for {path}"))
}

fn file_line(tree: &Tree, section: &str, path: &str) -> usize {
    tree.line_nodes
        .iter()
        .position(|line| {
            line.kind == NodeKind::File
                && line.section.as_deref() == Some(section)
                && line.path.as_deref() == Some(path)
        })
        .unwrap_or_else(|| panic!("no file header for {section}/{path}"))
}

fn section_line(tree: &Tree, section: &str) -> usize {
    tree.line_nodes
        .iter()
        .position(|line| line.kind == NodeKind::Section && line.section.as_deref() == Some(section))
        .unwrap_or_else(|| panic!("no section header for {section}"))
}

fn invoke(
    session: &mut Session,
    command: &str,
    cursor: Option<Cursor>,
    recursive: Option<bool>,
) -> magit_lsp::session::InvokeResult {
    session
        .invoke(&ViewInvokeParams {
            view_id: "v1".to_string(),
            command: command.to_string(),
            args: recursive.map(|recursive| json!({ "recursive": recursive })),
            cursor,
            selection: None,
        })
        .expect("invoke")
}

fn count_kind(tree: &Tree, kind: NodeKind) -> usize {
    tree.nodes.iter().filter(|node| node.kind == kind).count()
}

#[test]
fn collapsing_a_file_hides_its_hunks_and_moves_the_cursor_to_the_header() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = Session::new(tmp.path().to_string_lossy());
    let initial = open_status(&mut session);
    let initial_text = initial.text.clone().expect("initial full text");
    session.ack("v1", initial.version).expect("ack");

    let before = acked_tree(&session);
    let line = line_with_marker(before, "f.txt", '+');
    let header = file_line(before, "unstaged", "f.txt");
    let cursor = Cursor { line, column: 0 };

    let outcome = invoke(&mut session, "common.toggle-section", Some(cursor), None);
    let render = outcome.render.expect("render notification");
    assert_eq!(render.base_version, Some(initial.version));
    assert!(render.text.is_none(), "an acked client gets line edits");
    let edits = render.edits.expect("line edits");
    let collapsed_text = diff_text::apply_edits(&initial_text, &edits);
    assert_eq!(collapsed_text, tree(&session).lines);

    let visible = tree(&session);
    assert!(
        !visible.lines.iter().any(|line| line.contains("@@")),
        "collapsed file keeps no hunk lines: {:?}",
        visible.lines
    );
    assert_eq!(count_kind(visible, NodeKind::Hunk), 0);
    assert_eq!(count_kind(visible, NodeKind::Line), 0);

    let moved = render.cursor.expect("successor cursor");
    assert_eq!(
        moved.line, header,
        "the cursor moves to the collapsed header"
    );
    let node = node_at(visible, moved);
    assert_eq!(node.kind, NodeKind::File);
    assert_eq!(node.path.as_deref(), Some("f.txt"));

    // Symbols and folds describe the visible tree only.
    let symbols = visible.document_symbols();
    let unstaged = symbols
        .iter()
        .find(|symbol| symbol.name == "Unstaged changes")
        .expect("unstaged symbol");
    let file = unstaged
        .children
        .as_ref()
        .expect("files")
        .iter()
        .find(|symbol| symbol.name == "f.txt")
        .expect("file symbol");
    assert!(file.children.is_none(), "a collapsed file has no hunks");
    assert!(
        visible
            .folding_ranges()
            .iter()
            .all(|range| range.start_line as usize != header),
        "a collapsed file has no fold range"
    );

    // Expanding restores exactly the old text.
    session.ack("v1", render.version).expect("ack collapse");
    let outcome = invoke(&mut session, "common.toggle-section", Some(moved), None);
    let render = outcome.render.expect("render notification");
    let edits = render.edits.expect("line edits");
    let restored = diff_text::apply_edits(&collapsed_text, &edits);
    assert_eq!(restored, initial_text);
}

#[test]
fn visibility_survives_a_refresh_and_a_mutation_of_another_file() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES), ("g.txt", TEN_LINES)])
        .modify("f.txt", standard_changes())
        .modify("g.txt", standard_changes())
        .build();
    let mut session = Session::new(tmp.path().to_string_lossy());
    let initial = open_status(&mut session);
    session.ack("v1", initial.version).expect("ack");

    let line = line_with_marker(acked_tree(&session), "f.txt", '+');
    let collapse = invoke(
        &mut session,
        "common.toggle-section",
        Some(Cursor { line, column: 0 }),
        None,
    );
    session
        .ack("v1", collapse.render.expect("render").version)
        .expect("ack collapse");

    // A plain refresh keeps the per-node state.
    let refreshed = invoke(&mut session, "common.refresh", None, None);
    session
        .ack("v1", refreshed.render.expect("render").version)
        .expect("ack refresh");
    assert!(
        tree(&session).node("unstaged/f.txt").is_some(),
        "the collapsed file node is still rendered"
    );
    assert!(
        !tree(&session)
            .nodes
            .iter()
            .any(|node| node.kind == NodeKind::Hunk && node.path.as_deref() == Some("f.txt")),
        "f.txt stays collapsed after a refresh"
    );

    // Staging g.txt refreshes everything; f.txt stays collapsed by node id.
    let g_line = line_with_marker(acked_tree(&session), "g.txt", '+');
    let staged = invoke(
        &mut session,
        "status.stage",
        Some(Cursor {
            line: g_line,
            column: 0,
        }),
        None,
    );
    let render = staged.render.expect("render");
    session.ack("v1", render.version).expect("ack stage");
    assert!(tree(&session).node("staged/g.txt").is_some());
    assert!(
        !tree(&session)
            .nodes
            .iter()
            .any(|node| node.kind == NodeKind::Hunk && node.path.as_deref() == Some("f.txt")),
        "f.txt stays collapsed after another file is staged"
    );
}

#[test]
fn detail_levels_and_recursive_toggles_control_the_visible_tree() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = Session::new(tmp.path().to_string_lossy());
    let initial = open_status(&mut session);
    session.ack("v1", initial.version).expect("ack");

    // Level 1: sections only, so no file nodes at all.
    let hidden = invoke(&mut session, "common.hide-all", None, None);
    session
        .ack("v1", hidden.render.expect("render").version)
        .expect("ack hide-all");
    assert_eq!(count_kind(tree(&session), NodeKind::File), 0);
    assert_eq!(count_kind(tree(&session), NodeKind::Hunk), 0);

    // Level 2: files, but no hunks.
    let files = invoke(&mut session, "common.more-detail", None, None);
    session
        .ack("v1", files.render.expect("render").version)
        .expect("ack more-detail");
    assert!(count_kind(tree(&session), NodeKind::File) > 0);
    assert_eq!(count_kind(tree(&session), NodeKind::Hunk), 0);

    // Level 3: hunks and diff lines.
    let hunks = invoke(&mut session, "common.more-detail", None, None);
    session
        .ack("v1", hunks.render.expect("render").version)
        .expect("ack more-detail");
    assert!(count_kind(tree(&session), NodeKind::Hunk) > 0);
    assert!(count_kind(tree(&session), NodeKind::Line) > 0);

    // One level down hides hunks again.
    let files = invoke(&mut session, "common.less-detail", None, None);
    session
        .ack("v1", files.render.expect("render").version)
        .expect("ack less-detail");
    assert!(count_kind(tree(&session), NodeKind::File) > 0);
    assert_eq!(count_kind(tree(&session), NodeKind::Hunk), 0);

    // show-all returns to the default full body.
    let shown = invoke(&mut session, "common.show-all", None, None);
    session
        .ack("v1", shown.render.expect("render").version)
        .expect("ack show-all");
    assert!(count_kind(tree(&session), NodeKind::Hunk) > 0);

    // Recursive hide collapses the section and its files.
    let section = section_line(tree(&session), "unstaged");
    let hidden = invoke(
        &mut session,
        "common.hide-section",
        Some(Cursor {
            line: section,
            column: 0,
        }),
        Some(true),
    );
    session
        .ack("v1", hidden.render.expect("render").version)
        .expect("ack recursive hide");
    assert_eq!(count_kind(tree(&session), NodeKind::File), 0);

    // A non-recursive show reveals the section but leaves its files hidden.
    let section = section_line(tree(&session), "unstaged");
    let shown = invoke(
        &mut session,
        "common.show-section",
        Some(Cursor {
            line: section,
            column: 0,
        }),
        Some(false),
    );
    session
        .ack("v1", shown.render.expect("render").version)
        .expect("ack show-section");
    assert!(count_kind(tree(&session), NodeKind::File) > 0);
    assert_eq!(
        count_kind(tree(&session), NodeKind::Hunk),
        0,
        "the recursive hide also collapsed the files"
    );

    // A recursive show expands the files too.
    let section = section_line(tree(&session), "unstaged");
    let shown = invoke(
        &mut session,
        "common.show-section",
        Some(Cursor {
            line: section,
            column: 0,
        }),
        Some(true),
    );
    let render = shown.render.expect("render");
    assert!(count_kind(tree(&session), NodeKind::Hunk) > 0);
    assert!(render.cursor.is_some());
}

#[test]
fn a_collapsed_file_is_not_diffed_until_it_is_expanded() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let path = tmp.path().join("f.txt");
    let mut session = Session::new(tmp.path().to_string_lossy());
    let initial = open_status(&mut session);
    session.ack("v1", initial.version).expect("ack");
    assert!(
        diff_invocation_count(&path) >= 1,
        "the default expanded view loads the body"
    );

    let line = line_with_marker(acked_tree(&session), "f.txt", '+');
    let collapse = invoke(
        &mut session,
        "common.toggle-section",
        Some(Cursor { line, column: 0 }),
        None,
    );
    session
        .ack("v1", collapse.render.expect("render").version)
        .expect("ack collapse");

    // A refresh clears the hunk cache; a collapsed file must not be diffed.
    let before = diff_invocation_count(&path);
    let refreshed = invoke(&mut session, "common.refresh", None, None);
    session
        .ack("v1", refreshed.render.expect("render").version)
        .expect("ack refresh");
    assert_eq!(
        diff_invocation_count(&path),
        before,
        "a collapsed file makes no git diff call"
    );

    // Expanding loads its body exactly once for this refresh.
    let header = file_line(tree(&session), "unstaged", "f.txt");
    let expand = invoke(
        &mut session,
        "common.toggle-section",
        Some(Cursor {
            line: header,
            column: 0,
        }),
        None,
    );
    session
        .ack("v1", expand.render.expect("render").version)
        .expect("ack expand");
    assert_eq!(
        diff_invocation_count(&path),
        before + 1,
        "expanding loads the body exactly once"
    );

    // Re-rendering without a refresh reuses the loaded body.
    invoke(&mut session, "common.more-detail", None, None);
    assert_eq!(
        diff_invocation_count(&path),
        before + 1,
        "a later render reuses the hunk cache"
    );
}

#[test]
fn next_and_prev_section_move_the_cursor_between_headers() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = Session::new(tmp.path().to_string_lossy());
    let initial = open_status(&mut session);
    session.ack("v1", initial.version).expect("ack");

    let next = invoke(
        &mut session,
        "common.next-section",
        Some(Cursor { line: 0, column: 0 }),
        None,
    );
    let render = next.render.expect("render");
    let cursor = render.cursor.expect("cursor");
    assert_eq!(
        node_at(tree(&session), cursor).section.as_deref(),
        Some("staged")
    );
    session.ack("v1", render.version).expect("ack");

    let next = invoke(&mut session, "common.next-section", Some(cursor), None);
    let render = next.render.expect("render");
    let cursor = render.cursor.expect("cursor");
    assert_eq!(
        node_at(tree(&session), cursor).section.as_deref(),
        Some("unstaged")
    );
    session.ack("v1", render.version).expect("ack");

    let prev = invoke(&mut session, "common.prev-section", Some(cursor), None);
    let render = prev.render.expect("render");
    let cursor = render.cursor.expect("cursor");
    assert_eq!(
        node_at(tree(&session), cursor).section.as_deref(),
        Some("staged")
    );
}
