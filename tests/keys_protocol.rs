//! `view/key`, chords, capture and `view/keymap` (R-2.3), in process.

mod common;

use common::*;
use magit_lsp::config::{BindingSpec, Config, Profile};
use magit_lsp::errors;
use magit_lsp::keyhandler::{self, MenuResolution, PendingChord, Resolution};
use magit_lsp::keymap::EditorMode;
use magit_lsp::keys::KeyChord;
use magit_lsp::protocol::{Cursor, Effect, MessageLevel, Selection};

#[test]
fn key_s_stages_the_hunk_like_invoke() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let line = test.goto_marker("f.txt", '+');

    let outcome = test.press("s");
    assert!(outcome.effects.is_empty(), "{:?}", outcome.effects);
    let render = outcome.render.expect("view/key renders like view/invoke");
    assert_eq!(render.version, 2);
    assert!(
        render.edits.as_ref().is_some_and(|edits| !edits.is_empty()),
        "staging changes the rendered text"
    );
    assert_eq!(hunk_count(&cached_diff(tmp.path())), 1);

    // `view/invoke` reaches the same code path.
    let invoked = test.invoke_at("status.unstage", Cursor { line, column: 0 });
    assert!(invoked.effects.is_empty(), "{:?}", invoked.effects);
    assert_eq!(cached_diff(tmp.path()), "");
}

#[test]
fn chords_keep_pending_until_a_complete_match() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    test.set_cursor(Cursor { line: 0, column: 0 });

    // `g` opens a chord: no effects, no render.
    let pending = test.press("g");
    assert!(pending.effects.is_empty(), "{:?}", pending.effects);
    assert!(pending.render.is_none());

    // `r` completes `g r`, which is `common.refresh` in the vim profile.
    let refreshed = test.press("r");
    assert!(refreshed.effects.is_empty(), "{:?}", refreshed.effects);
    assert!(refreshed.render.is_some(), "refresh re-renders");

    // `g` then an unbound key passes the whole chord through.
    test.press("g");
    let unbound = test.press("Q");
    assert_eq!(
        unbound.effects,
        vec![Effect::PassThrough {
            key: "g Q".to_string()
        }]
    );
}

#[test]
fn unbound_key_in_a_non_capture_view_passes_through() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    test.set_cursor(Cursor { line: 0, column: 0 });

    let outcome = test.press("Q");
    assert_eq!(
        outcome.effects,
        vec![Effect::PassThrough {
            key: "Q".to_string()
        }]
    );
    assert!(outcome.render.is_none());
}

#[test]
fn menu_pending_steps_survive_non_matches_and_pop_on_backspace() {
    let keys = vec![
        "c".to_string(),
        "-a".to_string(),
        "=A".to_string(),
        "g l".to_string(),
    ];
    let mut pending = Vec::new();

    assert_eq!(
        keyhandler::resolve_menu(&mut pending, "-", &keys),
        MenuResolution::Pending
    );
    assert_eq!(pending, vec!["-".to_string()]);

    // A key that cannot follow `-` is swallowed and leaves the prefix alone.
    assert_eq!(
        keyhandler::resolve_menu(&mut pending, "c", &keys),
        MenuResolution::Unbound {
            chord: "-c".to_string()
        }
    );
    assert_eq!(pending, vec!["-".to_string()]);

    assert_eq!(
        keyhandler::resolve_menu(&mut pending, "a", &keys),
        MenuResolution::Complete("-a".to_string())
    );
    assert!(pending.is_empty());

    // Backspace with nothing pending is a no-op, not a close.
    assert_eq!(
        keyhandler::resolve_menu(&mut pending, "Backspace", &keys),
        MenuResolution::Ignored
    );

    // A chord prefix collects, then completes.
    assert_eq!(
        keyhandler::resolve_menu(&mut pending, "g", &keys),
        MenuResolution::Pending
    );
    assert_eq!(pending, vec!["g".to_string()]);
    assert_eq!(
        keyhandler::resolve_menu(&mut pending, "l", &keys),
        MenuResolution::Complete("g l".to_string())
    );
    assert!(pending.is_empty());
}

#[test]
fn capture_mode_swallows_an_unbound_key_with_a_hint() {
    let config = Config {
        profile: Profile::Vim,
        ..Default::default()
    };
    let maps = TestSession::validate_keymaps(&config);
    let map = maps.get("status").expect("status keymap");

    let mut pending = PendingChord::default();
    assert_eq!(
        keyhandler::resolve(Some(map), &mut pending, "g", EditorMode::Normal, true).unwrap(),
        Resolution::Pending
    );
    assert_eq!(
        keyhandler::resolve(Some(map), &mut pending, "Q", EditorMode::Normal, true).unwrap(),
        Resolution::Swallow {
            chord: "g Q".to_string()
        }
    );
}

#[test]
fn cancel_passes_through_a_selection_and_is_a_noop_without_one() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let line = test.goto_marker("f.txt", '+');
    let cursor = Cursor { line, column: 0 };

    let with_selection = test.press_full(
        "Escape",
        EditorMode::Normal,
        cursor,
        Some(Selection {
            start_line: cursor.line,
            end_line: cursor.line,
        }),
    );
    assert_eq!(
        with_selection.effects,
        vec![Effect::PassThrough {
            key: "Escape".to_string()
        }]
    );

    let without = test.press("Escape");
    assert!(without.effects.is_empty(), "{:?}", without.effects);
}

#[test]
fn visual_only_bindings_are_ignored_in_normal_mode_and_get_the_selection() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut config = Config {
        profile: Profile::Vim,
        ..Default::default()
    };
    config
        .keys
        .scopes
        .entry("status".to_string())
        .or_default()
        .commands
        .insert(
            "stage".to_string(),
            BindingSpec {
                keys: vec![KeyChord::parse("v").unwrap()],
                modes: vec![EditorMode::Visual],
            },
        );

    let mut test = TestSession::with_config(tmp.path().to_string_lossy(), config);
    test.open_status_view(STATUS_VIEWPORT);
    test.ack();

    let line = test.goto_text("f.txt", "NEW line");
    assert_eq!(test.tree().line_nodes[line].marker, Some('+'));
    let normal = test.press("v");
    assert_eq!(
        normal.effects,
        vec![Effect::PassThrough {
            key: "v".to_string()
        }],
        "a visual-only binding must be ignored in normal mode"
    );
    assert_eq!(cached_diff(tmp.path()), "", "nothing may be staged");

    let visual = test.press_visual(line, line, "v");
    assert!(visual.effects.is_empty(), "{:?}", visual.effects);
    let cached = cached_diff(tmp.path());
    assert!(cached.contains("+NEW line"), "got: {cached}");
    assert!(!cached.contains("line3 CHANGED"), "got: {cached}");
}

#[test]
fn a_stale_version_resolves_by_identity_not_by_line_index() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    test.goto_marker("f.txt", '+');

    // The hunk behind the cursor changes after the view was acked.
    write(
        &tmp.path().join("f.txt"),
        &standard_changes().replace("line3 CHANGED", "line3 DIFFERENT"),
    );

    let outcome = test.press("s");
    assert_eq!(
        outcome.effects,
        vec![Effect::message(MessageLevel::Warn, errors::HUNK_CHANGED)]
    );
    assert_eq!(cached_diff(tmp.path()), "", "nothing may be staged");
}

#[test]
fn view_keymap_notices_follow_the_profile_and_config_reloads() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();

    let (mut vim, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    vim.ack();
    let notice = vim.keymap_notice().expect("keymap for the status view");
    assert_eq!(notice.scope, "status");
    assert_eq!(notice.version, 1);
    assert!(!notice.capture);
    let log = notice
        .bindings
        .iter()
        .find(|binding| binding.command == "open.log")
        .expect("open.log binding");
    assert_eq!(log.keys, vec!["g l"]);

    let (mut standard, _initial) = TestSession::open_status(&tmp, Profile::Standard);
    standard.ack();
    let notice = standard
        .keymap_notice()
        .expect("keymap for the status view");
    let log = notice
        .bindings
        .iter()
        .find(|binding| binding.command == "open.log")
        .expect("open.log binding");
    assert_eq!(log.keys, vec!["l"]);

    // A hot reload changes the resolved keys and the notice with them.
    let mut reloaded = Config {
        profile: Profile::Vim,
        ..Default::default()
    };
    reloaded
        .keys
        .scopes
        .entry("status".to_string())
        .or_default()
        .commands
        .insert(
            "stage".to_string(),
            BindingSpec {
                keys: vec![KeyChord::parse("q").unwrap()],
                modes: vec![EditorMode::Normal, EditorMode::Visual],
            },
        );
    vim.set_keymaps(&reloaded);
    let notice = vim.keymap_notice().expect("keymap after reload");
    let stage = notice
        .bindings
        .iter()
        .find(|binding| binding.command == "status.stage")
        .expect("status.stage binding");
    assert_eq!(stage.keys, vec!["q"]);
}

#[test]
fn the_harness_sends_the_tracked_render_version_and_records_acks() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, initial) = TestSession::open_status(&tmp, Profile::Vim);
    assert_eq!(test.version(), initial.version);
    assert_eq!(test.acked_version(), None, "opening does not ack");

    test.ack();
    assert_eq!(test.acked_version(), Some(initial.version));
    test.goto_marker("f.txt", '+');

    let staged = test.press("s");
    let render = staged.render.expect("staging renders");
    assert_eq!(
        test.last_key_version(),
        initial.version,
        "the key request carried the tracked version, not a literal 1"
    );
    assert_eq!(test.version(), render.version);
    assert_eq!(
        test.acked_version(),
        Some(initial.version),
        "the new render is not acked until the client applies it"
    );

    test.ack();
    assert_eq!(test.acked_version(), Some(render.version));
    let refreshed = test.press("g r");
    assert_eq!(
        test.last_key_version(),
        render.version,
        "the next key request carries the newly acked version"
    );
    let refresh_render = refreshed.render.expect("refresh renders");
    assert_eq!(test.version(), refresh_render.version);
}
