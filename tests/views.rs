//! In-process session/view tests: rendering, ack-based edits, cursor
//! succession and stale-action rejection (R-1.2).

mod common;

use common::*;
use magit_lsp::config::Profile;
use magit_lsp::diff_text;
use magit_lsp::errors;
use magit_lsp::protocol::{Cursor, Effect, MessageLevel, Selection};

#[test]
fn staging_a_hunk_line_sends_edits_and_moves_the_cursor_to_the_staged_hunk() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, initial) = TestSession::open_status(&tmp, Profile::Vim);
    let initial_text = initial.text.clone().expect("initial full text");
    test.ack();

    let line = test.goto_marker("f.txt", '+');
    let hunk_id = test.tree().line_nodes[line]
        .hunk_id
        .clone()
        .expect("hunk id");
    let cursor = Cursor { line, column: 0 };

    let outcome = test.invoke_at("status.stage", cursor);
    assert!(outcome.effects.is_empty(), "{:?}", outcome.effects);

    let render = outcome.render.expect("render notification");
    assert_eq!(render.base_version, Some(initial.version));
    assert!(render.text.is_none(), "acked client must get line edits");
    let edits = render.edits.expect("line edits");
    assert!(!edits.is_empty(), "staging changes the rendered text");
    assert_eq!(diff_text::apply_edits(&initial_text, &edits), test.text());

    let moved = render.cursor.expect("successor cursor");
    test.set_cursor(moved);
    let moved_node = test.focused_node();
    assert_eq!(moved_node.path.as_deref(), Some("f.txt"));
    assert_eq!(moved_node.hunk_id.as_deref(), Some(hunk_id.as_str()));
    assert_eq!(moved_node.section.as_deref(), Some("staged"));
    assert_eq!(hunk_count(&cached_diff(tmp.path())), 1);
}

#[test]
fn quitting_a_nested_view_restores_the_same_parent_but_root_quit_clears_session() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, initial) = TestSession::open_status(&tmp, Profile::Vim);
    let parent_id = test.view_id().to_string();
    let parent_tree = test.tree().clone();
    test.ack();

    let (child, _) = test.open_via_command("open.process-log");
    let child_id = child.id;
    let result = test.invoke("common.quit");
    assert_eq!(
        result.effects,
        vec![
            Effect::PopView {
                view_id: child_id.clone(),
            },
            Effect::FocusView {
                view_id: parent_id.clone(),
            },
        ]
    );
    assert!(result.render.is_none());
    assert_eq!(test.view_ids(), vec![parent_id.clone()]);
    assert_eq!(test.view_id(), parent_id);
    assert_eq!(test.version(), initial.version);
    assert_eq!(test.tree(), &parent_tree);

    let root_quit = test.invoke("common.quit");
    assert_eq!(root_quit.effects, vec![Effect::Quit]);
    assert!(test.view_ids().is_empty());
}

#[test]
fn refresh_after_an_external_stage_sends_edits_and_keeps_the_cursor_on_the_hunk() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();

    let line = test.goto_marker("f.txt", '+');
    let hunk_id = test.tree().line_nodes[line]
        .hunk_id
        .clone()
        .expect("hunk id");
    let cursor = Cursor { line, column: 0 };

    tmp.git(&["add", "f.txt"]);
    let outcome = test.invoke_at("common.refresh", cursor);
    assert!(outcome.effects.is_empty(), "{:?}", outcome.effects);

    let render = outcome.render.expect("render notification");
    assert_eq!(render.base_version, Some(initial.version));
    assert!(render.text.is_none(), "acked client must get line edits");
    assert!(render.edits.as_ref().is_some_and(|edits| !edits.is_empty()));

    let moved = render.cursor.expect("successor cursor");
    test.set_cursor(moved);
    let moved_node = test.focused_node();
    assert_eq!(moved_node.path.as_deref(), Some("f.txt"));
    assert_eq!(moved_node.hunk_id.as_deref(), Some(hunk_id.as_str()));
    assert_eq!(moved_node.section.as_deref(), Some("staged"));
}

#[test]
fn an_unacked_client_gets_full_text_and_resync_matches_the_edits() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, initial) = TestSession::open_status(&tmp, Profile::Vim);
    let initial_text = initial.text.clone().expect("initial full text");
    test.ack();

    let line = test.goto_marker("f.txt", '+');
    let cursor = Cursor { line, column: 0 };

    let staged = test.invoke_at("status.stage", cursor);
    let edits = staged
        .render
        .expect("render")
        .edits
        .expect("line edits for acked client");
    let expected = diff_text::apply_edits(&initial_text, &edits);

    // The client never acked version 2, so the next render must be full text.
    let refreshed = test.invoke_at("common.refresh", cursor);
    let render = refreshed.render.expect("render");
    assert!(render.base_version.is_none());
    assert_eq!(render.text.as_deref(), Some(expected.as_slice()));

    let resync = test.session().resync("v1").expect("resync");
    assert_eq!(resync.text.as_deref(), Some(expected.as_slice()));
}

#[test]
fn an_unacked_open_gets_a_full_render_on_its_first_refresh() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    let line = test.goto_marker("f.txt", '+');

    // No `view/rendered` for version 1.
    let outcome = test.invoke_at("status.stage", Cursor { line, column: 0 });
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
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();

    let line = test.goto_marker("f.txt", '+');
    let cursor = Cursor { line, column: 0 };

    // Change the hunk content behind the rendered view.
    write(
        &tmp.path().join("f.txt"),
        &standard_changes().replace("line3 CHANGED", "line3 DIFFERENT"),
    );

    let outcome = test.invoke_at("status.stage", cursor);
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
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();

    let line = test.goto_text("f.txt", "NEW line");
    assert_eq!(test.tree().line_nodes[line].marker, Some('+'));

    let outcome = test.invoke_selection(
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
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();

    let staged = test.invoke("status.stage-all");
    assert!(staged.effects.is_empty(), "{:?}", staged.effects);
    assert_eq!(hunk_count(&cached_diff(tmp.path())), 1);

    let unstaged = test.invoke("status.unstage-all");
    assert!(unstaged.effects.is_empty(), "{:?}", unstaged.effects);
    assert_eq!(cached_diff(tmp.path()), "");
}
