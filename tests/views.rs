//! In-process session/view tests: rendering, ack-based edits, cursor
//! succession and stale-action rejection (R-1.2).

mod common;

use common::*;
use magit_lsp::diff_text;
use magit_lsp::errors;
use magit_lsp::protocol::{Cursor, Effect, MessageLevel, Selection, ViewInvokeParams, ViewRender};
use magit_lsp::session::Session;
use magit_lsp::tree::{Node, NodeKind, Tree};
use magit_lsp::view::{Placement, ViewDescriptor, ViewKind, Viewport};

fn open_status(session: &mut Session) -> (ViewDescriptor, ViewRender) {
    session
        .open(
            "v1",
            ViewKind::Status,
            Placement::Main,
            Viewport { cols: 80, rows: 24 },
        )
        .expect("open status view")
}

fn acked_tree<'a>(session: &'a Session) -> &'a Tree {
    session
        .views()
        .get("v1")
        .and_then(|view| view.acked_tree())
        .expect("acked tree")
}

fn tree(session: &Session) -> &Tree {
    session.views().get("v1").expect("view").tree()
}

/// The first rendered line owned by a diff line with `marker` in `path`.
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

fn node_at<'a>(tree: &'a Tree, cursor: Cursor) -> &'a Node {
    let line = tree
        .line_nodes
        .get(cursor.line)
        .unwrap_or_else(|| panic!("cursor line {} is outside the tree", cursor.line));
    tree.node(&line.node).expect("cursor node")
}

fn invoke(
    session: &mut Session,
    command: &str,
    cursor: Option<Cursor>,
) -> magit_lsp::session::InvokeResult {
    invoke_selection(session, command, cursor, None)
}

fn invoke_selection(
    session: &mut Session,
    command: &str,
    cursor: Option<Cursor>,
    selection: Option<Selection>,
) -> magit_lsp::session::InvokeResult {
    session
        .invoke(&ViewInvokeParams {
            view_id: "v1".to_string(),
            command: command.to_string(),
            args: None,
            cursor,
            selection,
        })
        .expect("invoke")
}

fn message_texts(effects: &[Effect]) -> Vec<&str> {
    effects
        .iter()
        .map(|effect| match effect {
            Effect::Message { text, .. } => text.as_str(),
        })
        .collect()
}

#[test]
fn staging_a_hunk_line_sends_edits_and_moves_the_cursor_to_the_staged_hunk() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = Session::new(tmp.path().to_string_lossy());
    let (_descriptor, initial) = open_status(&mut session);
    let initial_text = initial.text.expect("initial full text");
    session.ack("v1", initial.version).expect("ack");

    let before = acked_tree(&session);
    let line = line_with_marker(before, "f.txt", '+');
    let hunk_id = before.line_nodes[line].hunk_id.clone().expect("hunk id");
    let cursor = Cursor { line, column: 0 };

    let outcome = invoke(&mut session, "status.stage", Some(cursor));
    assert!(outcome.effects.is_empty(), "{:?}", outcome.effects);

    let render = outcome.render.expect("render notification");
    assert_eq!(render.base_version, Some(initial.version));
    assert!(render.text.is_none(), "acked client must get line edits");
    let edits = render.edits.expect("line edits");
    assert!(!edits.is_empty(), "staging changes the rendered text");
    assert_eq!(
        diff_text::apply_edits(&initial_text, &edits),
        tree(&session).lines
    );

    let moved = node_at(tree(&session), render.cursor.expect("successor cursor"));
    assert_eq!(moved.path.as_deref(), Some("f.txt"));
    assert_eq!(moved.hunk_id.as_deref(), Some(hunk_id.as_str()));
    assert_eq!(moved.section.as_deref(), Some("staged"));
    assert_eq!(hunk_count(&cached_diff(tmp.path())), 1);
}

#[test]
fn refresh_after_an_external_stage_sends_edits_and_keeps_the_cursor_on_the_hunk() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = Session::new(tmp.path().to_string_lossy());
    let (_descriptor, initial) = open_status(&mut session);
    session.ack("v1", initial.version).expect("ack");

    let before = acked_tree(&session);
    let line = line_with_marker(before, "f.txt", '+');
    let hunk_id = before.line_nodes[line].hunk_id.clone().expect("hunk id");
    let cursor = Cursor { line, column: 0 };

    tmp.git(&["add", "f.txt"]);
    let outcome = invoke(&mut session, "common.refresh", Some(cursor));
    assert!(outcome.effects.is_empty(), "{:?}", outcome.effects);

    let render = outcome.render.expect("render notification");
    assert_eq!(render.base_version, Some(initial.version));
    assert!(render.text.is_none(), "acked client must get line edits");
    assert!(render.edits.as_ref().is_some_and(|edits| !edits.is_empty()));

    let moved = node_at(tree(&session), render.cursor.expect("successor cursor"));
    assert_eq!(moved.path.as_deref(), Some("f.txt"));
    assert_eq!(moved.hunk_id.as_deref(), Some(hunk_id.as_str()));
    assert_eq!(moved.section.as_deref(), Some("staged"));
}

#[test]
fn an_unacked_client_gets_full_text_and_resync_matches_the_edits() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = Session::new(tmp.path().to_string_lossy());
    let (_descriptor, initial) = open_status(&mut session);
    let initial_text = initial.text.clone().expect("initial full text");
    session.ack("v1", initial.version).expect("ack");

    let before = acked_tree(&session);
    let line = line_with_marker(before, "f.txt", '+');
    let cursor = Cursor { line, column: 0 };

    let staged = invoke(&mut session, "status.stage", Some(cursor));
    let edits = staged
        .render
        .expect("render")
        .edits
        .expect("line edits for acked client");
    let expected = diff_text::apply_edits(&initial_text, &edits);

    // The client never acked version 2, so the next render must be full text.
    let refreshed = invoke(&mut session, "common.refresh", Some(cursor));
    let render = refreshed.render.expect("render");
    assert!(render.base_version.is_none());
    assert_eq!(render.text.as_deref(), Some(expected.as_slice()));

    let resync = session.resync("v1").expect("resync");
    assert_eq!(resync.text.as_deref(), Some(expected.as_slice()));
}

#[test]
fn an_unacked_open_gets_a_full_render_on_its_first_refresh() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = Session::new(tmp.path().to_string_lossy());
    let (_descriptor, _initial) = open_status(&mut session);
    let before = tree(&session);
    let line = line_with_marker(before, "f.txt", '+');

    // No `view/rendered` for version 1.
    let outcome = invoke(
        &mut session,
        "status.stage",
        Some(Cursor { line, column: 0 }),
    );
    let render = outcome.render.expect("render");
    assert!(render.base_version.is_none());
    assert!(render.text.is_some(), "unacked client must get full text");
    assert!(render.edits.is_none());
}

#[test]
fn a_stale_hunk_action_reports_a_message_and_changes_no_repo_state() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = Session::new(tmp.path().to_string_lossy());
    let (_descriptor, initial) = open_status(&mut session);
    session.ack("v1", initial.version).expect("ack");

    let before = acked_tree(&session);
    let line = line_with_marker(before, "f.txt", '+');
    let cursor = Cursor { line, column: 0 };

    // Change the hunk content behind the rendered view.
    write(
        &tmp.path().join("f.txt"),
        &standard_changes().replace("line3 CHANGED", "line3 DIFFERENT"),
    );

    let outcome = invoke(&mut session, "status.stage", Some(cursor));
    assert_eq!(message_texts(&outcome.effects), vec![errors::HUNK_CHANGED]);
    assert_eq!(
        outcome.effects[0],
        Effect::message(MessageLevel::Warn, errors::HUNK_CHANGED)
    );
    assert_eq!(cached_diff(tmp.path()), "", "nothing may be staged");
}

#[test]
fn a_visual_selection_stages_only_the_lines_it_covers() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = Session::new(tmp.path().to_string_lossy());
    let (_descriptor, initial) = open_status(&mut session);
    session.ack("v1", initial.version).expect("ack");

    let before = acked_tree(&session);
    let line = before
        .line_nodes
        .iter()
        .position(|line| {
            line.kind == NodeKind::Line
                && line.path.as_deref() == Some("f.txt")
                && line.marker == Some('+')
                && line.text.as_deref() == Some("NEW line")
        })
        .expect("the +NEW line");

    let outcome = invoke_selection(
        &mut session,
        "status.stage",
        Some(Cursor { line, column: 0 }),
        Some(Selection {
            start_line: line,
            end_line: line,
        }),
    );
    assert!(outcome.effects.is_empty(), "{:?}", outcome.effects);

    let cached = cached_diff(tmp.path());
    assert!(cached.contains("+NEW line"), "got: {cached}");
    assert!(!cached.contains("line3 CHANGED"), "got: {cached}");
}

#[test]
fn stage_all_and_unstage_all_run_through_the_view() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = Session::new(tmp.path().to_string_lossy());
    let (_descriptor, initial) = open_status(&mut session);
    session.ack("v1", initial.version).expect("ack");

    let staged = invoke(&mut session, "status.stage-all", None);
    assert!(staged.effects.is_empty(), "{:?}", staged.effects);
    assert_eq!(hunk_count(&cached_diff(tmp.path())), 1);

    let unstaged = invoke(&mut session, "status.unstage-all", None);
    assert!(unstaged.effects.is_empty(), "{:?}", unstaged.effects);
    assert_eq!(cached_diff(tmp.path()), "");
}
