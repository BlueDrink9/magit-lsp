//! Server-owned visibility and lazy bodies (R-1.5).

mod common;

use common::*;
use magit_lsp::config::Profile;
use magit_lsp::diff_text;
use magit_lsp::git::diff_invocation_count;
use magit_lsp::protocol::Cursor;
use magit_lsp::tree::{NodeKind, Tree};
use serde_json::json;

fn count_kind(tree: &Tree, kind: NodeKind) -> usize {
    tree.nodes.iter().filter(|node| node.kind == kind).count()
}

#[test]
fn cycle_visibility_wraps_all_four_levels_and_survives_empty_refreshes() {
    for profile in [Profile::Vim, Profile::Standard] {
        let (tmp, _repo) = RepoFixture::new().commit("init", [("f.txt", TEN_LINES)])
            .modify_standard().build();
        let (mut test, _) = TestSession::open_status(&tmp, profile);
        test.ack();
        test.goto_marker("f.txt", '+');
        for level in [1, 2, 3, 4, 1] {
            let result = test.press("shift+Tab");
            test.set_cursor(result.render.unwrap().cursor.unwrap());
            test.ack();
            assert_eq!(count_kind(test.tree(), NodeKind::File) > 0, level >= 2);
            assert_eq!(count_kind(test.tree(), NodeKind::Hunk) > 0, level >= 3);
            assert_eq!(count_kind(test.tree(), NodeKind::Line) > 0, level >= 4);
            if level == 1 { assert_eq!(test.focused_node().kind, NodeKind::Section); }
            test.refresh();
            test.ack();
        }
        // An empty level must not stall cycling or lose the level on refresh.
        tmp.git(&["checkout", "--", "f.txt"]);
        test.refresh();
        test.ack();
        test.invoke("common.cycle-visibility"); // level 2, no children
        test.ack();
        test.refresh();
        test.ack();
        write(&tmp.path().join("f.txt"), &standard_changes());
        test.refresh();
        test.ack();
        test.invoke("common.cycle-visibility"); // level 3
        assert!(count_kind(test.tree(), NodeKind::Hunk) > 0);
        assert_eq!(count_kind(test.tree(), NodeKind::Line), 0);
    }
}

#[test]
fn cycle_visibility_preserves_the_retained_cursor_identity_and_refuses_stale_input() {
    let (tmp, _repo) = RepoFixture::new().commit("init", [("base.txt", "base\n")]).build();
    write(&tmp.path().join("b.txt"), "b\n");
    let (mut test, initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let cursor = Cursor { line: test.goto_file("untracked", "b.txt"), column: 0 };
    write(&tmp.path().join("a.txt"), "a\n");
    test.refresh();
    let result = test.invoke_at_version("common.cycle-visibility", initial.version, Some(cursor), None, None);
    test.set_cursor(result.render.unwrap().cursor.unwrap());
    assert_eq!(test.focused_node().id, "section:untracked");
    test.ack();
    let result = test.invoke_at_version("common.cycle-visibility", initial.version, Some(cursor), None, None);
    assert!(result.render.is_none());
    assert_eq!(message_texts(&result.effects), vec![magit_lsp::errors::STALE_VIEW]);
    assert_eq!(count_kind(test.tree(), NodeKind::File), 0);
}

/// Run one of the recursive visibility commands.
fn toggle(
    test: &mut TestSession,
    command: &str,
    cursor: Option<Cursor>,
    recursive: Option<bool>,
) -> magit_lsp::session::InvokeResult {
    let args = recursive.map(|recursive| json!({ "recursive": recursive }));
    match (cursor, args) {
        (Some(cursor), Some(args)) => test.invoke_at_args(command, Some(cursor), args),
        (Some(cursor), None) => test.invoke_at(command, cursor),
        (None, Some(args)) => test.invoke_args(command, args),
        (None, None) => test.invoke(command),
    }
}

#[test]
fn collapsing_a_file_hides_its_hunks_and_moves_the_cursor_to_the_header() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, initial) = TestSession::open_status(&tmp, Profile::Vim);
    let initial_text = initial.text.clone().expect("initial full text");
    test.ack();

    let header = test.goto_file("unstaged", "f.txt");
    let cursor = Cursor { line: header, column: 0 };

    let outcome = toggle(&mut test, "common.toggle-section", Some(cursor), None);
    let render = outcome.render.expect("render notification");
    assert_eq!(render.base_version, Some(initial.version));
    assert!(render.text.is_none(), "an acked client gets line edits");
    let edits = render.edits.expect("line edits");
    let collapsed_text = diff_text::apply_edits(&initial_text, &edits);
    assert_eq!(collapsed_text, test.text());

    let visible = test.tree();
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
    test.set_cursor(moved);
    let node = test.focused_node();
    assert_eq!(node.kind, NodeKind::File);
    assert_eq!(node.path.as_deref(), Some("f.txt"));

    // Symbols and folds describe the visible tree only.
    let visible = test.tree();
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
    test.ack();
    let outcome = toggle(&mut test, "common.toggle-section", Some(moved), None);
    let render = outcome.render.expect("render notification");
    let edits = render.edits.expect("line edits");
    let restored = diff_text::apply_edits(&collapsed_text, &edits);
    assert_eq!(restored, initial_text);
}

#[test]
fn folding_a_hunk_body_and_title_only_hides_that_stable_hunk() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", common::lines(40).to_string())])
        .modify_two_hunks()
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();

    let body = test.goto_marker("f.txt", '+');
    let body_id = test.tree().line_nodes[body].node.clone();
    let target_id = test.tree().collapsible_ancestor(&body_id).expect("body's hunk").id.clone();
    let sibling_id = test.tree().nodes.iter().find(|node|
        node.kind == NodeKind::Hunk && node.id != target_id
    ).expect("distinct sibling hunk").id.clone();
    let target_body = test.tree().node(&target_id).unwrap().children.clone();
    let sibling_body = test.tree().node(&sibling_id).unwrap().children.clone();
    assert!(!target_body.is_empty());
    assert!(!sibling_body.is_empty());
    let file_id = test.tree().parent(&target_id).unwrap().id.clone();
    let outcome = toggle(&mut test, "common.toggle-section", Some(Cursor { line: body, column: 0 }), None);
    let cursor = outcome.render.expect("body fold renders").cursor.expect("hunk title cursor");
    test.ack();
    assert_eq!(cursor.line, test.tree().node(&target_id).unwrap().start_line);

    // Collapsing retains both titles and the file, not the target's body.
    // A repository refresh must preserve the same content-owned identities.
    for refresh in [false, true] {
        if refresh {
            test.refresh();
            test.ack();
        }
        assert_eq!(count_kind(test.tree(), NodeKind::Hunk), 2);
        assert_eq!(test.tree().node(&file_id).unwrap().children, vec![target_id.clone(), sibling_id.clone()]);
        assert!(test.tree().node(&target_id).unwrap().children.is_empty());
        assert!(target_body.iter().all(|id| test.tree().node(id).is_none()));
        assert_eq!(test.tree().node(&sibling_id).unwrap().children, sibling_body);
        assert!(sibling_body.iter().all(|id| test.tree().node(id).is_some()));
    }

    // Unfold, fold, and unfold from the containing title, leaving its sibling alone.
    for expanded in [true, false, true] {
        let title = test.tree().node(&target_id).unwrap().start_line;
        let result = toggle(&mut test, "common.toggle-section", Some(Cursor { line: title, column: 0 }), None);
        assert!(result.render.is_some());
        test.ack();
        let target = test.tree().node(&target_id).expect("stable hunk title");
        assert_eq!(target.children, if expanded { target_body.clone() } else { vec![] });
        assert_eq!(test.tree().node(&sibling_id).unwrap().children, sibling_body);
        assert_eq!(test.tree().node(&file_id).unwrap().children.len(), 2);
    }
}

#[test]
fn visibility_survives_a_refresh_and_a_mutation_of_another_file() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES), ("g.txt", TEN_LINES)])
        .modify("f.txt", standard_changes())
        .modify("g.txt", standard_changes())
        .build();
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();

    let line = test.goto_file("unstaged", "f.txt");
    let collapse = toggle(
        &mut test,
        "common.toggle-section",
        Some(Cursor { line, column: 0 }),
        None,
    );
    assert!(collapse.render.is_some(), "collapsing renders");
    test.ack();

    // A plain refresh keeps the per-node state.
    let refreshed = toggle(&mut test, "common.refresh", None, None);
    assert!(refreshed.render.is_some(), "refreshing renders");
    test.ack();
    assert!(
        test.tree().node("unstaged/f.txt").is_some(),
        "the collapsed file node is still rendered"
    );
    assert!(
        !test
            .tree()
            .nodes
            .iter()
            .any(|node| node.kind == NodeKind::Hunk && node.path.as_deref() == Some("f.txt")),
        "f.txt stays collapsed after a refresh"
    );

    // Staging g.txt refreshes everything; f.txt stays collapsed by node id.
    let g_line = test.goto_marker("g.txt", '+');
    let staged = toggle(
        &mut test,
        "status.stage",
        Some(Cursor {
            line: g_line,
            column: 0,
        }),
        None,
    );
    assert!(staged.render.is_some(), "staging renders");
    test.ack();
    assert!(test.tree().node("staged/g.txt").is_some());
    assert!(
        !test
            .tree()
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
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();

    // Level 1: sections only, so no file nodes at all.
    let hidden = toggle(&mut test, "common.hide-all", None, None);
    assert!(hidden.render.is_some(), "hide-all renders");
    test.ack();
    assert_eq!(count_kind(test.tree(), NodeKind::File), 0);
    assert_eq!(count_kind(test.tree(), NodeKind::Hunk), 0);

    // Level 2: files, but no hunks.
    let files = toggle(&mut test, "common.more-detail", None, None);
    assert!(files.render.is_some(), "more-detail renders");
    test.ack();
    assert!(count_kind(test.tree(), NodeKind::File) > 0);
    assert_eq!(count_kind(test.tree(), NodeKind::Hunk), 0);

    // Level 3: hunk headers, but no diff lines.
    let hunks = toggle(&mut test, "common.more-detail", None, None);
    assert!(hunks.render.is_some(), "more-detail renders");
    test.ack();
    assert!(count_kind(test.tree(), NodeKind::Hunk) > 0);
    assert_eq!(count_kind(test.tree(), NodeKind::Line), 0);

    // Level 4: individually expanded hunk bodies.
    let bodies = toggle(&mut test, "common.more-detail", None, None);
    assert!(bodies.render.is_some(), "more-detail renders");
    test.ack();
    assert!(count_kind(test.tree(), NodeKind::Line) > 0);
    toggle(&mut test, "common.more-detail", None, None);
    test.ack();
    assert!(
        count_kind(test.tree(), NodeKind::Line) > 0,
        "level 4 clamps upward"
    );

    // One level down hides only hunk bodies.
    let hunks = toggle(&mut test, "common.less-detail", None, None);
    assert!(hunks.render.is_some(), "less-detail renders");
    test.ack();
    assert!(count_kind(test.tree(), NodeKind::Hunk) > 0);
    assert_eq!(count_kind(test.tree(), NodeKind::Line), 0);

    toggle(&mut test, "common.less-detail", None, None);
    test.ack();
    assert!(count_kind(test.tree(), NodeKind::File) > 0);
    assert_eq!(count_kind(test.tree(), NodeKind::Hunk), 0);
    toggle(&mut test, "common.less-detail", None, None);
    test.ack();
    toggle(&mut test, "common.less-detail", None, None);
    test.ack();
    assert_eq!(
        count_kind(test.tree(), NodeKind::File),
        0,
        "level 1 clamps downward"
    );

    // show-all returns to the default full body.
    let shown = toggle(&mut test, "common.show-all", None, None);
    assert!(shown.render.is_some(), "show-all renders");
    test.ack();
    assert!(count_kind(test.tree(), NodeKind::Hunk) > 0);

    // Recursive hide collapses the section and its files.
    let section = test.goto_section("unstaged");
    let hidden = toggle(
        &mut test,
        "common.hide-section",
        Some(Cursor {
            line: section,
            column: 0,
        }),
        Some(true),
    );
    assert!(hidden.render.is_some(), "recursive hide renders");
    test.ack();
    assert_eq!(count_kind(test.tree(), NodeKind::File), 0);

    // A non-recursive show reveals the section but leaves its files hidden.
    let section = test.goto_section("unstaged");
    let shown = toggle(
        &mut test,
        "common.show-section",
        Some(Cursor {
            line: section,
            column: 0,
        }),
        Some(false),
    );
    assert!(shown.render.is_some(), "show-section renders");
    test.ack();
    assert!(count_kind(test.tree(), NodeKind::File) > 0);
    assert_eq!(
        count_kind(test.tree(), NodeKind::Hunk),
        0,
        "the recursive hide also collapsed the files"
    );

    // A recursive show expands the files too.
    let section = test.goto_section("unstaged");
    let shown = toggle(
        &mut test,
        "common.show-section",
        Some(Cursor {
            line: section,
            column: 0,
        }),
        Some(true),
    );
    let render = shown.render.expect("render");
    assert!(count_kind(test.tree(), NodeKind::Hunk) > 0);
    assert!(render.cursor.is_some());
}

#[test]
fn standard_absolute_detail_levels_override_any_prior_level() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Standard);
    test.ack();

    for (start, target) in [(4, 1), (1, 2), (2, 3), (3, 4), (4, 2), (2, 1)] {
        toggle(
            &mut test,
            &format!("common.detail-level-{start}"),
            None,
            None,
        );
        test.ack();
        let outcome = toggle(
            &mut test,
            &format!("common.detail-level-{target}"),
            None,
            None,
        );
        assert!(outcome.render.is_some());
        test.ack();
        let tree = test.tree();
        assert_eq!(count_kind(tree, NodeKind::File) > 0, target >= 2);
        assert_eq!(count_kind(tree, NodeKind::Hunk) > 0, target >= 3);
        assert_eq!(count_kind(tree, NodeKind::Line) > 0, target >= 4);
    }
}

#[test]
fn a_collapsed_file_is_not_diffed_until_it_is_expanded() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let path = tmp.path().join("f.txt");
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    assert!(
        diff_invocation_count(&path) >= 1,
        "the default expanded view loads the body"
    );

    let line = test.goto_file("unstaged", "f.txt");
    let collapse = toggle(
        &mut test,
        "common.toggle-section",
        Some(Cursor { line, column: 0 }),
        None,
    );
    assert!(collapse.render.is_some(), "collapsing renders");
    test.ack();

    // A refresh clears the hunk cache; a collapsed file must not be diffed.
    let before = diff_invocation_count(&path);
    let refreshed = toggle(&mut test, "common.refresh", None, None);
    assert!(refreshed.render.is_some(), "refreshing renders");
    test.ack();
    assert_eq!(
        diff_invocation_count(&path),
        before,
        "a collapsed file makes no git diff call"
    );

    // Expanding loads its body exactly once for this refresh.
    let header = test.goto_file("unstaged", "f.txt");
    let expand = toggle(
        &mut test,
        "common.toggle-section",
        Some(Cursor {
            line: header,
            column: 0,
        }),
        None,
    );
    assert!(expand.render.is_some(), "expanding renders");
    test.ack();
    assert_eq!(
        diff_invocation_count(&path),
        before + 1,
        "expanding loads the body exactly once"
    );

    // Re-rendering without a refresh reuses the loaded body.
    toggle(&mut test, "common.more-detail", None, None);
    assert_eq!(
        diff_invocation_count(&path),
        before + 1,
        "a later render reuses the hunk cache"
    );
}

#[test]
fn collapsed_section_does_not_diff_expanded_child_files_until_reopened() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES), ("g.txt", TEN_LINES)])
        .modify("f.txt", standard_changes())
        .modify("g.txt", standard_changes())
        .build();
    let f_path = tmp.path().join("f.txt");
    let g_path = tmp.path().join("g.txt");
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();

    let section = test.goto_section("unstaged");
    let collapsed = toggle(
        &mut test,
        "common.toggle-section",
        Some(Cursor { line: section, column: 0 }),
        None,
    );
    assert!(collapsed.render.is_some());
    test.ack();
    assert!(test.tree().node("unstaged/f.txt").is_none());
    let before_f = diff_invocation_count(&f_path);
    let before_g = diff_invocation_count(&g_path);

    let refreshed = toggle(&mut test, "common.refresh", None, None);
    assert!(refreshed.render.is_some());
    test.ack();
    assert_eq!(diff_invocation_count(&f_path), before_f);
    assert_eq!(diff_invocation_count(&g_path), before_g);

    let section = test.goto_section("unstaged");
    let opened = toggle(
        &mut test,
        "common.show-section",
        Some(Cursor { line: section, column: 0 }),
        Some(false),
    );
    assert!(opened.render.is_some());
    test.ack();
    assert_eq!(diff_invocation_count(&f_path), before_f + 1);
    assert_eq!(diff_invocation_count(&g_path), before_g + 1);

    toggle(&mut test, "common.more-detail", None, None);
    assert_eq!(diff_invocation_count(&f_path), before_f + 1);
    assert_eq!(diff_invocation_count(&g_path), before_g + 1);
}

#[test]
fn next_and_prev_section_move_the_cursor_between_headers() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();

    let next = toggle(
        &mut test,
        "common.next-section",
        Some(Cursor { line: 0, column: 0 }),
        None,
    );
    let render = next.render.expect("render");
    let cursor = render.cursor.expect("cursor");
    test.set_cursor(cursor);
    assert_eq!(test.focused_node().section.as_deref(), Some("staged"));
    test.ack();

    let next = toggle(&mut test, "common.next-section", Some(cursor), None);
    let render = next.render.expect("render");
    let cursor = render.cursor.expect("cursor");
    test.set_cursor(cursor);
    assert_eq!(test.focused_node().section.as_deref(), Some("unstaged"));
    test.ack();

    let prev = toggle(&mut test, "common.prev-section", Some(cursor), None);
    let render = prev.render.expect("render");
    let cursor = render.cursor.expect("cursor");
    test.set_cursor(cursor);
    assert_eq!(test.focused_node().section.as_deref(), Some("staged"));
}
