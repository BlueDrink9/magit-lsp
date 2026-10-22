//! `view/key`, chords, capture and `view/keymap` (R-2.3), in process.

mod common;

use common::*;
use magit_lsp::commands::registry;
use magit_lsp::config::{BindingSpec, Config, Profile};
use magit_lsp::errors;
use magit_lsp::keyhandler::{self, PendingChord, Resolution};
use magit_lsp::keymap::{self, EditorMode, Keymap};
use magit_lsp::keys::KeyChord;
use magit_lsp::protocol::{Cursor, Effect, MessageLevel, Selection, ViewInvokeParams, ViewKeyParams};
use magit_lsp::session::Session;
use magit_lsp::tree::NodeKind;
use magit_lsp::view::{Placement, ViewKind, Viewport};
use std::collections::BTreeMap;
use std::sync::Arc;

fn keymaps(config: &Config) -> Arc<BTreeMap<String, Keymap>> {
    Arc::new(keymap::validate(config).expect("resolve keymaps"))
}

fn session_with(profile: Profile, repo: &std::path::Path) -> Session {
    let config = Config {
        profile,
        ..Default::default()
    };
    let mut session = Session::new(repo.to_string_lossy());
    session.set_keymaps(keymaps(&config));
    session
}

fn open_status(session: &mut Session) -> u64 {
    let (_descriptor, render) = session
        .open(
            "v1",
            ViewKind::Status,
            Placement::Main,
            Viewport { cols: 80, rows: 24 },
        )
        .expect("open status view");
    session.ack("v1", render.version).expect("ack");
    render.version
}

fn key(session: &mut Session, key: &str, cursor: Cursor) -> magit_lsp::session::InvokeResult {
    key_with(session, key, cursor, EditorMode::Normal, None)
}

fn key_with(
    session: &mut Session,
    key: &str,
    cursor: Cursor,
    mode: EditorMode,
    selection: Option<Selection>,
) -> magit_lsp::session::InvokeResult {
    session
        .key(&ViewKeyParams {
            view_id: "v1".to_string(),
            version: 1,
            key: key.to_string(),
            editor_mode: mode,
            cursor,
            selection,
        })
        .expect("view/key")
}

fn plus_line(session: &Session, path: &str) -> Cursor {
    let tree = session.views().get("v1").expect("view").tree();
    let line = tree
        .line_nodes
        .iter()
        .position(|line| {
            line.kind == NodeKind::Line
                && line.path.as_deref() == Some(path)
                && line.marker == Some('+')
        })
        .expect("a '+' line");
    Cursor { line, column: 0 }
}

fn plus_line_for_text(session: &Session, path: &str, text: &str) -> Cursor {
    let tree = session.views().get("v1").expect("view").tree();
    let line = tree
        .line_nodes
        .iter()
        .position(|line| {
            line.kind == NodeKind::Line
                && line.path.as_deref() == Some(path)
                && line.marker == Some('+')
                && line.text.as_deref() == Some(text)
        })
        .unwrap_or_else(|| panic!("a '+' line with text {text:?}"));
    Cursor { line, column: 0 }
}

#[test]
fn key_s_stages_the_hunk_like_invoke() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = session_with(Profile::Vim, tmp.path());
    open_status(&mut session);
    let cursor = plus_line(&session, "f.txt");

    let outcome = key(&mut session, "s", cursor);
    assert!(outcome.effects.is_empty(), "{:?}", outcome.effects);
    let render = outcome.render.expect("view/key renders like view/invoke");
    assert_eq!(render.version, 2);
    assert!(
        render.edits.as_ref().is_some_and(|edits| !edits.is_empty()),
        "staging changes the rendered text"
    );
    assert_eq!(hunk_count(&cached_diff(tmp.path())), 1);

    // `view/invoke` reaches the same code path.
    let invoked = session
        .invoke(&ViewInvokeParams {
            view_id: "v1".to_string(),
            command: "status.unstage".to_string(),
            args: None,
            cursor: Some(cursor),
            selection: None,
        })
        .expect("invoke");
    assert!(invoked.effects.is_empty(), "{:?}", invoked.effects);
    assert_eq!(cached_diff(tmp.path()), "");
}

#[test]
fn chords_keep_pending_until_a_complete_match() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = session_with(Profile::Vim, tmp.path());
    open_status(&mut session);
    let cursor = Cursor { line: 0, column: 0 };

    // `g` opens a chord: no effects, no render.
    let pending = key(&mut session, "g", cursor);
    assert!(pending.effects.is_empty(), "{:?}", pending.effects);
    assert!(pending.render.is_none());

    // `r` completes `g r`, which is `common.refresh` in the vim profile.
    let refreshed = key(&mut session, "r", cursor);
    assert!(refreshed.effects.is_empty(), "{:?}", refreshed.effects);
    assert!(refreshed.render.is_some(), "refresh re-renders");

    // `g` then an unbound key passes the whole chord through.
    key(&mut session, "g", cursor);
    let unbound = key(&mut session, "Q", cursor);
    assert_eq!(
        unbound.effects,
        vec![Effect::PassThrough { key: "g Q".to_string() }]
    );
}

#[test]
fn unbound_key_in_a_non_capture_view_passes_through() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let mut session = session_with(Profile::Vim, tmp.path());
    open_status(&mut session);

    let outcome = key(&mut session, "Q", Cursor { line: 0, column: 0 });
    assert_eq!(
        outcome.effects,
        vec![Effect::PassThrough { key: "Q".to_string() }]
    );
    assert!(outcome.render.is_none());
}

#[test]
fn capture_mode_swallows_an_unbound_key_with_a_hint() {
    let config = Config {
        profile: Profile::Vim,
        ..Default::default()
    };
    let maps = keymap::validate(&config).expect("keymaps");
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
    let mut session = session_with(Profile::Vim, tmp.path());
    open_status(&mut session);
    let cursor = plus_line(&session, "f.txt");

    let with_selection = key_with(
        &mut session,
        "Escape",
        cursor,
        EditorMode::Normal,
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

    let without = key(&mut session, "Escape", cursor);
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

    let mut session = Session::new(tmp.path().to_string_lossy());
    session.set_keymaps(keymaps(&config));
    open_status(&mut session);

    let cursor = plus_line_for_text(&session, "f.txt", "NEW line");
    let normal = key(&mut session, "v", cursor);
    assert_eq!(
        normal.effects,
        vec![Effect::PassThrough { key: "v".to_string() }],
        "a visual-only binding must be ignored in normal mode"
    );
    assert_eq!(cached_diff(tmp.path()), "", "nothing may be staged");

    let visual = key_with(
        &mut session,
        "v",
        cursor,
        EditorMode::Visual,
        Some(Selection {
            start_line: cursor.line,
            end_line: cursor.line,
        }),
    );
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
    let mut session = session_with(Profile::Vim, tmp.path());
    open_status(&mut session);
    let cursor = plus_line(&session, "f.txt");

    // The hunk behind the cursor changes after the view was acked.
    write(
        &tmp.path().join("f.txt"),
        &standard_changes().replace("line3 CHANGED", "line3 DIFFERENT"),
    );

    let outcome = key(&mut session, "s", cursor);
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

    let mut vim = session_with(Profile::Vim, tmp.path());
    open_status(&mut vim);
    let notice = vim.keymap_notice("v1").expect("keymap for the status view");
    assert_eq!(notice.scope, "status");
    assert_eq!(notice.version, 1);
    assert!(!notice.capture);
    let log = notice
        .bindings
        .iter()
        .find(|binding| binding.command == "open.log")
        .expect("open.log binding");
    assert_eq!(log.keys, vec!["g l"]);

    let mut standard = session_with(Profile::Standard, tmp.path());
    open_status(&mut standard);
    let notice = standard
        .keymap_notice("v1")
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
    vim.set_keymaps(keymaps(&reloaded));
    let notice = vim
        .keymap_notice("v1")
        .expect("keymap after reload");
    let stage = notice
        .bindings
        .iter()
        .find(|binding| binding.command == "status.stage")
        .expect("status.stage binding");
    assert_eq!(stage.keys, vec!["q"]);
    assert!(registry().get("status.stage").is_some());
}
