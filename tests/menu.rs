//! Menu framework (R-2.7): layout, switches/options/actions, the argument
//! preview, the structured model and the menu stack.

mod common;

use common::*;
use expect_test::expect_file;
use magit_lsp::config::{Config, Profile};
use magit_lsp::keymap::{self, Keymap};
use magit_lsp::menu::{Menu, MenuStatus};
use magit_lsp::protocol::{Cursor, Effect, MessageLevel, ViewInvokeParams, ViewKeyParams};
use magit_lsp::semantic_tokens;
use magit_lsp::session::{InvokeResult, Session};
use magit_lsp::view::{Placement, ViewDescriptor, ViewKind, Viewport};
use std::collections::BTreeMap;
use std::sync::Arc;

fn keymaps(config: &Config) -> Arc<BTreeMap<String, Keymap>> {
    Arc::new(keymap::validate(config).expect("resolve keymaps"))
}

fn session(repo: &std::path::Path) -> Session {
    let config = Config {
        profile: Profile::Vim,
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
            Viewport { cols: 100, rows: 30 },
        )
        .expect("open status view");
    session.ack("v1", render.version).expect("ack");
    render.version
}

fn invoke(session: &mut Session, view: &str, command: &str) -> InvokeResult {
    session
        .invoke(&ViewInvokeParams {
            view_id: view.to_string(),
            command: command.to_string(),
            args: None,
            cursor: None,
            selection: None,
        })
        .expect("invoke")
}

fn press(session: &mut Session, view: &str, key: &str, cursor: Cursor) -> InvokeResult {
    session
        .key(&ViewKeyParams {
            view_id: view.to_string(),
            version: 1,
            key: key.to_string(),
            editor_mode: magit_lsp::keymap::EditorMode::Normal,
            cursor,
            selection: None,
        })
        .expect("view/key")
}

fn messages(effects: &[Effect]) -> Vec<&str> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Message { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// Open the commit menu from the status view and return its descriptor and
/// initial render, already acknowledged.
fn open_commit(session: &mut Session) -> (ViewDescriptor, magit_lsp::protocol::ViewRender) {
    let outcome = invoke(session, "v1", "open.commit");
    let descriptor = outcome
        .effects
        .iter()
        .find_map(|effect| match effect {
            Effect::PushView { descriptor } => Some(descriptor.clone()),
            _ => None,
        })
        .expect("a pushView effect");
    let (render, _keymap) = outcome.opened.into_iter().next().expect("the menu render");
    session.ack(&descriptor.id, render.version).expect("ack menu");
    (descriptor, render)
}

fn view_ids(session: &Session) -> Vec<String> {
    session
        .views()
        .iter()
        .map(|view| view.descriptor().id.clone())
        .collect()
}

fn menu_switch(session: &Session, view: &str, key: &str) -> bool {
    let menu = session
        .views()
        .get(view)
        .and_then(|view| view.menu())
        .expect("a menu view");
    match menu.item(key) {
        Some(magit_lsp::menu::Item::Switch { on, .. }) => *on,
        other => panic!("expected a switch at {key}, got {other:?}"),
    }
}

fn preview_line(render: &magit_lsp::protocol::ViewRender) -> Option<String> {
    let menu = render.menu.as_ref()?;
    Some(menu.preview())
}

#[test]
fn the_commit_menu_reflows_its_action_columns() {
    for cols in [60u32, 100, 140] {
        let tree = Menu::commit().render(Viewport { cols, rows: 30 });
        let text = tree.lines.join("\n");
        match cols {
            60 => expect_file!["snapshots/menu-60.txt"].assert_eq(&text),
            100 => expect_file!["snapshots/menu-100.txt"].assert_eq(&text),
            140 => expect_file!["snapshots/menu-140.txt"].assert_eq(&text),
            _ => unreachable!(),
        }
    }
}

#[test]
fn the_argument_preview_follows_the_switches_and_options() {
    let mut menu = Menu::commit();
    assert_eq!(menu.preview(), "git commit");

    assert!(menu.toggle_switch("-a"));
    assert_eq!(menu.preview(), "git commit --all");
    assert!(menu.toggle_switch("-n"));
    assert_eq!(menu.preview(), "git commit --all --no-verify");
    assert!(menu.toggle_switch("-a"));
    assert_eq!(menu.preview(), "git commit --no-verify");

    assert!(menu.set_option("=A", Some("Alice".to_string())));
    assert_eq!(
        menu.preview(),
        "git commit --no-verify --author=Alice"
    );

    let tree = menu.render(Viewport { cols: 100, rows: 30 });
    assert!(
        tree.lines.iter().any(|line| line.contains("(--no-verify)")),
        "{:?}",
        tree.lines
    );
    assert!(
        tree.lines.iter().any(|line| line.contains("[x]")),
        "the switch marker is on: {:?}",
        tree.lines
    );
    assert!(
        tree.lines.iter().any(|line| line.ends_with("Alice")),
        "the option value is shown: {:?}",
        tree.lines
    );
}

#[test]
fn toggling_a_switch_updates_the_model_and_emits_a_preview_edit() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = session(tmp.path());
    open_status(&mut session);
    let (descriptor, initial) = open_commit(&mut session);
    assert_eq!(descriptor.kind, ViewKind::Menu("menu.commit".to_string()));
    assert!(descriptor.capture, "menus capture every key");
    assert_eq!(descriptor.placement, Placement::Bottom);
    assert_eq!(descriptor.scope.as_str(), "menu.commit");
    assert!(
        initial.menu.is_some(),
        "view/render carries the structured menu model"
    );

    let cursor = Cursor { line: 0, column: 0 };
    let prefix = press(&mut session, &descriptor.id, "-", cursor);
    assert!(prefix.render.is_none(), "the prefix waits for a letter");
    assert!(prefix.effects.is_empty());

    let toggled = press(&mut session, &descriptor.id, "a", cursor);
    assert!(toggled.effects.is_empty(), "{:?}", toggled.effects);
    let render = toggled.render.expect("a re-render");
    assert!(
        render.edits.as_ref().is_some_and(|edits| !edits.is_empty()),
        "the preview line is sent as a line edit"
    );
    let menu = render.menu.as_ref().expect("the structured menu");
    match menu.item("-a") {
        Some(magit_lsp::menu::Item::Switch { on, .. }) => assert!(*on),
        other => panic!("expected the --all switch, got {other:?}"),
    }
    assert_eq!(menu.preview(), "git commit --all");
    assert!(menu_switch(&session, &descriptor.id, "-a"));
    assert_eq!(preview_line(&render).as_deref(), Some("git commit --all"));
    assert!(
        render
            .edits
            .expect("edits")
            .iter()
            .any(|edit| edit.lines.iter().any(|line| line.contains("[x]"))),
        "the rendered switch marker turns on"
    );
}

#[test]
fn setting_an_option_reports_the_roadmap_prompt() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let mut session = session(tmp.path());
    open_status(&mut session);
    let (descriptor, _render) = open_commit(&mut session);

    let cursor = Cursor { line: 0, column: 0 };
    press(&mut session, &descriptor.id, "=", cursor);
    let outcome = press(&mut session, &descriptor.id, "A", cursor);
    assert_eq!(
        messages(&outcome.effects),
        vec!["Not implemented yet: option prompts (roadmap R-3.1)"]
    );
    assert!(outcome.render.is_none(), "nothing changed to re-render");
}

#[test]
fn a_planned_action_reports_its_roadmap_and_changes_nothing() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = session(tmp.path());
    open_status(&mut session);
    let (descriptor, _render) = open_commit(&mut session);
    let before = tmp.git(&["status", "--porcelain"]);

    let outcome = press(
        &mut session,
        &descriptor.id,
        "c",
        Cursor { line: 0, column: 0 },
    );
    assert_eq!(
        messages(&outcome.effects),
        vec!["Not implemented yet: Commit (roadmap R-4.1)"]
    );
    assert_eq!(
        outcome.effects,
        vec![Effect::message(
            MessageLevel::Info,
            "Not implemented yet: Commit (roadmap R-4.1)"
        )]
    );
    assert!(outcome.render.is_none());
    assert!(
        session.views().get(&descriptor.id).is_some(),
        "the menu stays open"
    );
    assert_eq!(tmp.git(&["status", "--porcelain"]), before);
}

#[test]
fn escape_q_and_ctrl_c_pop_the_menu_and_restore_the_status_keymap() {
    for key in ["Escape", "q", "ctrl+c"] {
        let (tmp, _repo) = RepoFixture::new()
            .commit("init", [("f.txt", TEN_LINES)])
            .build();
        let mut session = session(tmp.path());
        open_status(&mut session);
        let (descriptor, _render) = open_commit(&mut session);

        let outcome = press(
            &mut session,
            &descriptor.id,
            key,
            Cursor { line: 0, column: 0 },
        );
        assert!(
            outcome.effects.iter().any(
                |effect| matches!(effect, Effect::PopView { view_id } if view_id == &descriptor.id)
            ),
            "{key} pops the menu: {:?}",
            outcome.effects
        );
        assert!(
            outcome
                .effects
                .iter()
                .any(|effect| matches!(effect, Effect::FocusView { view_id } if view_id == "v1")),
            "{key} returns focus to the status view"
        );
        let keymap = outcome.keymap.expect("the status keymap is re-sent");
        assert_eq!(keymap.view_id, "v1");
        assert_eq!(keymap.scope, "status");
        assert!(!keymap.capture);
        assert!(session.views().get(&descriptor.id).is_none());
        assert_eq!(view_ids(&session), vec!["v1".to_string()]);
    }
}

#[test]
fn an_unbound_menu_key_is_swallowed_with_a_hint() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = session(tmp.path());
    open_status(&mut session);
    let (descriptor, _render) = open_commit(&mut session);
    let before = tmp.git(&["status", "--porcelain"]);

    let outcome = press(
        &mut session,
        &descriptor.id,
        "z",
        Cursor { line: 0, column: 0 },
    );
    assert_eq!(messages(&outcome.effects), vec!["Unbound key: z"]);
    assert!(outcome.render.is_none());
    assert_eq!(tmp.git(&["status", "--porcelain"]), before);
}

#[test]
fn menus_push_and_pop_in_stack_order() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let mut session = session(tmp.path());
    open_status(&mut session);
    let (commit, _render) = open_commit(&mut session);

    let pushed = press(
        &mut session,
        &commit.id,
        "?",
        Cursor { line: 0, column: 0 },
    );
    let dispatch = pushed
        .effects
        .iter()
        .find_map(|effect| match effect {
            Effect::PushView { descriptor } => Some(descriptor.clone()),
            _ => None,
        })
        .expect("the dispatch menu is pushed");
    assert_eq!(
        dispatch.kind,
        ViewKind::Menu("menu.dispatch".to_string())
    );
    assert_eq!(
        view_ids(&session),
        vec![
            "v1".to_string(),
            commit.id.clone(),
            dispatch.id.clone()
        ]
    );

    let popped = press(
        &mut session,
        &dispatch.id,
        "Escape",
        Cursor { line: 0, column: 0 },
    );
    assert!(popped.effects.iter().any(
        |effect| matches!(effect, Effect::PopView { view_id } if view_id == &dispatch.id)
    ));
    assert_eq!(view_ids(&session), vec!["v1".to_string(), commit.id.clone()]);
    assert_eq!(popped.keymap.expect("keymap").view_id, commit.id);

    let back = press(
        &mut session,
        &commit.id,
        "Escape",
        Cursor { line: 0, column: 0 },
    );
    assert_eq!(view_ids(&session), vec!["v1".to_string()]);
    assert_eq!(back.keymap.expect("keymap").view_id, "v1");
}

#[test]
fn save_and_reset_args_restore_the_saved_switches() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let mut session = session(tmp.path());
    open_status(&mut session);
    let (descriptor, _render) = open_commit(&mut session);
    let cursor = Cursor { line: 0, column: 0 };

    press(&mut session, &descriptor.id, "-", cursor);
    press(&mut session, &descriptor.id, "a", cursor);
    let saved = invoke(&mut session, &descriptor.id, "menu.save-args");
    assert_eq!(messages(&saved.effects), vec!["Saved menu arguments"]);
    assert!(saved.render.is_some(), "saving re-renders");

    press(&mut session, &descriptor.id, "-", cursor);
    press(&mut session, &descriptor.id, "a", cursor);
    assert!(!menu_switch(&session, &descriptor.id, "-a"));

    let reset = invoke(&mut session, &descriptor.id, "menu.reset-args");
    assert_eq!(messages(&reset.effects), vec!["Reset menu arguments"]);
    assert!(menu_switch(&session, &descriptor.id, "-a"));
    let render = reset.render.expect("resetting re-renders");
    assert_eq!(
        render.menu.expect("menu").preview(),
        "git commit --all"
    );
}

#[test]
fn menu_tokens_decode_to_the_documented_types() {
    let tree = Menu::commit().render(Viewport { cols: 100, rows: 30 });
    let data = semantic_tokens::encode_lines(&tree.lines, &tree.tokens);
    let types = semantic_tokens::TOKEN_TYPES;
    let mut seen = Vec::new();
    for token in &data {
        seen.push(types[token.token_type as usize]);
    }
    for expected in [
        "magit.menu.title",
        "magit.menu.group",
        "magit.menu.key",
        "magit.menu.label",
        "magit.menu.flag",
        "magit.menu.switch.off",
        "magit.menu.option",
        "magit.menu.preview",
        "magit.menu.planned",
    ] {
        assert!(
            seen.contains(&expected),
            "missing {expected}: {seen:?}"
        );
    }

    // The switch marker changes type when the switch is on.
    let mut menu = Menu::commit();
    menu.toggle_switch("-a");
    let tree = menu.render(Viewport { cols: 100, rows: 30 });
    let data = semantic_tokens::encode_lines(&tree.lines, &tree.tokens);
    let seen: Vec<&str> = data
        .iter()
        .map(|token| types[token.token_type as usize])
        .collect();
    assert!(seen.contains(&"magit.menu.switch.on"), "{seen:?}");
}

#[test]
fn the_registry_exposes_open_commit_and_the_menu_commands() {
    use magit_lsp::commands::{registry, CommandStatus};
    for id in ["open.commit", "open.dispatch", "menu.save-args", "menu.reset-args"] {
        let command = registry().get(id).unwrap_or_else(|| panic!("{id} is registered"));
        assert_eq!(
            command.status,
            CommandStatus::Implemented,
            "{id} is implemented"
        );
    }

    // A menu action's status is separate from the registry: every commit
    // action is Planned until R-4.1.
    let menu = Menu::commit();
    for key in ["c", "e", "f", "a"] {
        match menu.item(key).map(|item| item.status()) {
            Some(MenuStatus::Planned { roadmap_id }) => assert_eq!(roadmap_id, "R-4.1"),
            other => panic!("{key} is planned, got {other:?}"),
        }
    }
}
