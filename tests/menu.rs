//! Menu framework (R-2.7): layout, switches/options/actions, the argument
//! preview, the structured model and the menu stack.

mod common;

use common::*;
use expect_test::expect_file;
use magit_lsp::config::Profile;
use magit_lsp::menu::{Menu, MenuStatus};
use magit_lsp::menus;
use magit_lsp::protocol::{Effect, ViewRender};
use magit_lsp::semantic_tokens;
use magit_lsp::view::{Placement, ViewKind, Viewport};

fn menu_switch(test: &TestSession, view: &str, key: &str) -> bool {
    let menu = test.menu(view).expect("a menu view");
    match menu.item(key) {
        Some(magit_lsp::menu::Item::Switch { on, .. }) => *on,
        other => panic!("expected a switch at {key}, got {other:?}"),
    }
}

#[test]
fn commit_options_survive_menu_pop_and_staging_prompt() {
    use magit_lsp::git::CommitOperation;
    for staged in [false, true] {
        let (tmp, _) = RepoFixture::new().commit("init", [("f.txt", TEN_LINES)]).build();
        std::fs::write(tmp.path().join("f.txt"), "changed\n").unwrap();
        if staged { tmp.git(&["add", "f.txt"]); }
        let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
        let (menu, _) = test.open_via_command("open.commit");
        test.press("- s");
        test.press("- n");
        test.ui().reply_text(Some("Captured <captured@example.com>".into()));
        test.press_ui("=");
        test.press_ui("A");
        if !staged { test.ui().reply_confirm(true); }
        let result = test.press_ui("c");
        assert!(result.effects.iter().any(|effect| matches!(effect, Effect::PopView {view_id} if view_id == &menu.id)));
        let CommitOperation::Create(options) = result.commit.expect("operation survives ancestor routing and prompt") else { panic!("expected create operation") };
        assert!(options.signoff && options.no_verify);
        assert_eq!(options.author.as_deref(), Some("Captured <captured@example.com>"));
        assert!(tmp.git(&["diff", "--cached"]).contains("changed"));
        assert_eq!(tmp.git(&["log", "-1", "--format=%s"]).trim(), "init");
    }
}

#[test]
fn head_actions_route_from_both_profile_menus_with_captured_options() {
    for profile in [Profile::Vim, Profile::Standard] {
        for (key, flags) in [
            ("a", vec!["--amend", "--no-verify"]),
            ("e", vec!["--amend", "--no-edit", "--no-verify"]),
            ("w", vec!["--amend", "--only", "--allow-empty", "--no-verify"]),
        ] {
            let (tmp, _) = RepoFixture::new().commit("init", [("f.txt", TEN_LINES)])
                .staged_file("f.txt", "staged\n").build();
            let (mut test, _) = TestSession::open_status(&tmp, profile);
            let (menu, _) = test.open_via_command("open.commit");
            test.press("- n");
            let result = test.press_ui(key);
            assert!(result.effects.iter().any(|effect| matches!(effect, Effect::PopView { view_id } if view_id == &menu.id)));
            assert_eq!(result.commit.expect("typed HEAD action").argv(), flags);
            assert_eq!(tmp.git(&["show", "HEAD:f.txt"]), TEN_LINES);
            assert_eq!(tmp.git(&["show", ":f.txt"]), "staged\n");
        }
    }
}

#[test]
fn structured_menu_roundtrips_with_server_activation_steps() {
    use magit_lsp::menu::{Args, Group, Item, Reader};
    let cases = [
        ("-a", vec!["-", "a"]),
        ("=A", vec!["=", "A"]),
        ("g l", vec!["g", "l"]),
        ("ctrl+t", vec!["ctrl+t"]),
        ("Enter", vec!["Enter"]),
        ("G", vec!["G"]),
        ("Plus", vec!["Plus"]),
    ];
    let mut items = vec![
        Item::Switch {
            key: "-a".into(),
            flag: "--all".into(),
            label: "All".into(),
            on: false,
            status: MenuStatus::Implemented,
        },
        Item::Option {
            key: "=A".into(),
            flag: "--author=".into(),
            label: "Author".into(),
            value: None,
            reader: Reader::Text,
            status: MenuStatus::Implemented,
        },
    ];
    items.extend(cases[2..].iter().map(|(key, _)| Item::Action {
        key: (*key).into(),
        command: "commit.extend".into(),
        label: "Planned action".into(),
        status: MenuStatus::Planned {
            roadmap_id: "R-4.1".into(),
        },
    }));
    let menu = Menu {
        id: "activation".into(),
        title: "Activation".into(),
        groups: vec![Group {
            title: "Items".into(),
            items,
        }],
        saved_args: Args::default(),
    };
    let wire = serde_json::to_value(&menu).unwrap();
    let restored: Menu = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(
        restored, menu,
        "activation metadata must not become stored item state"
    );
    assert_eq!(serde_json::to_value(&restored).unwrap(), wire);
    let keys: Vec<_> = restored.groups[0]
        .items
        .iter()
        .map(|item| item.key().to_string())
        .collect();
    for (row, (key, steps)) in wire["groups"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .zip(cases)
    {
        assert_eq!(row["key"], key);
        assert_eq!(row["activationSteps"], serde_json::json!(steps));
        let mut pending = Vec::new();
        for (index, step) in row["activationSteps"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let result =
                magit_lsp::keyhandler::resolve_menu(&mut pending, step.as_str().unwrap(), &keys);
            assert_eq!(
                result,
                if index + 1 == steps.len() {
                    magit_lsp::keyhandler::MenuResolution::Complete(key.into())
                } else {
                    magit_lsp::keyhandler::MenuResolution::Pending
                },
                "{key}: step {index}"
            );
        }
        assert!(pending.is_empty());
    }
}

fn preview_line(render: &ViewRender) -> Option<String> {
    let menu = render.menu.as_ref()?;
    Some(menu.preview())
}

#[test]
fn the_commit_menu_reflows_its_action_columns() {
    for cols in [60u32, 140] {
        let tree = menus::named("commit")
            .expect("commit menu")
            .render(Viewport { cols, rows: 30 });
        expect_file![format!("snapshots/menu-{cols}.txt")].assert_eq(&tree.lines.join("\n"));
    }

    // The 60-column render wraps the widest switch line; the continuation
    // keeps its flag and switch marker tokens over exactly their text and
    // the label token stays on the first line.
    let tree = menus::named("commit")
        .expect("commit menu")
        .render(Viewport { cols: 60, rows: 30 });
    let label_line = tree
        .lines
        .iter()
        .position(|line| line.contains("Claim authorship and reset author date"))
        .expect("the wrapped label line");
    let label = "Claim authorship and reset author date";
    let start = tree.lines[label_line].find(label).expect("label text");
    assert!(tree.tokens.iter().any(|token| token.line == label_line
        && token.start == start
        && token.length == label.len()
        && token.token_type == "magit.menu.label"));
    let flag_line = label_line + 1;
    assert_eq!(
        tree.lines[flag_line].trim(),
        "(--reset-author)  [ ]",
        "the flag and marker continue on the next line"
    );
    for (text, length, token_type) in [
        ("--reset-author", "--reset-author".len(), "magit.menu.flag.off"),
        ("[ ]", 3, "magit.menu.switch.off"),
    ] {
        let start = tree.lines[flag_line].find(text).expect("continuation text");
        assert!(
            tree.tokens.iter().any(|token| token.line == flag_line
                && token.start == start
                && token.length == length
                && token.token_type == token_type),
            "no {token_type} token on the continuation line"
        );
    }
}

#[test]
fn resizing_an_acked_commit_menu_reflows_incrementally_and_keeps_arguments() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _) = TestSession::open_status_at(
        &tmp,
        Profile::Vim,
        Viewport {
            cols: 140,
            rows: 30,
        },
    );
    test.ack();
    let (_descriptor, _) = test.open_via_command("open.commit");
    test.ack();

    test.press("-");
    test.ack();
    test.press("a");
    test.ack();
    test.ui().reply_text(Some("Alice".to_string()));
    test.press_ui("=");
    test.ack();
    test.press_ui("A");
    test.ack();

    let base_version = test.acked_version().expect("acked menu version");
    let base_text = test.text();
    let render = test
        .resize(Viewport { cols: 60, rows: 30 })
        .expect("menu resize emits a render");
    assert_eq!(render.base_version, Some(base_version));
    assert_eq!(render.version, base_version + 1);
    let edits = render.edits.as_ref().expect("incremental resize edits");
    assert!(render.text.is_none(), "acked clients receive edits only");

    let resized_text = magit_lsp::diff_text::apply_edits(&base_text, edits);
    let mut canonical = menus::named("commit").expect("commit menu");
    canonical.toggle_switch("-a");
    canonical.set_option("=A", Some("Alice".to_string()));
    let expected = canonical.render(Viewport { cols: 60, rows: 30 });
    assert_eq!(resized_text, expected.lines);
    assert_eq!(
        render.menu.as_ref().expect("menu model").preview(),
        "git commit --all --author=Alice"
    );

    let default_narrow = menus::named("commit")
        .expect("commit menu")
        .render(Viewport { cols: 60, rows: 30 });
    expect_file!["snapshots/menu-60.txt"].assert_eq(&default_narrow.lines.join("\n"));
    assert_ne!(
        resized_text, default_narrow.lines,
        "resize retains switch and option state"
    );
    assert_eq!(
        test.acked_version(),
        Some(base_version),
        "resize is not an implicit ack"
    );
}

#[test]
fn the_argument_preview_follows_the_switches_and_options() {
    let mut menu = menus::named("commit").expect("commit menu");
    assert_eq!(menu.preview(), "git commit");

    assert!(menu.toggle_switch("-a"));
    assert_eq!(menu.preview(), "git commit --all");
    assert!(menu.toggle_switch("-n"));
    assert_eq!(menu.preview(), "git commit --all --no-verify");
    assert!(menu.toggle_switch("-a"));
    assert_eq!(menu.preview(), "git commit --no-verify");

    assert!(menu.set_option("=A", Some("Alice".to_string())));
    assert_eq!(menu.preview(), "git commit --no-verify --author=Alice");

    let tree = menu.render(Viewport {
        cols: 100,
        rows: 30,
    });
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
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    let (descriptor, initial) = test.open_via_command("open.commit");
    test.ack();
    assert_eq!(descriptor.kind, ViewKind::Menu("menu.commit".to_string()));
    assert!(descriptor.capture, "menus capture every key");
    assert_eq!(descriptor.placement, Placement::Bottom);
    assert_eq!(descriptor.scope.as_str(), "menu.commit");
    assert!(
        initial.menu.is_some(),
        "view/render carries the structured menu model"
    );
    let prefix = test.press("-");
    assert!(prefix.effects.is_empty());
    let prefix_render = prefix.render.expect("the pending prefix re-renders");
    assert_eq!(prefix_render.pending, "-");
    test.ack();

    let toggled = test.press("a");
    assert!(toggled.effects.is_empty(), "{:?}", toggled.effects);
    let render = toggled.render.expect("a re-render");
    assert_eq!(render.pending, "", "completing the switch clears pending");
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
    assert!(menu_switch(&test, &descriptor.id, "-a"));
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
fn setting_an_option_prompts_and_applies_the_value() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    let (_descriptor, _render) = test.open_via_command("open.commit");
    test.ack();

    test.ui().reply_text(Some("Alice".to_string()));
    let prefix = test.press_ui("=");
    let prefix_render = prefix.render.expect("the pending prefix re-renders");
    assert_eq!(prefix_render.pending, "=");
    test.ack();
    let outcome = test.press_ui("A");
    assert!(outcome.effects.is_empty(), "{:?}", outcome.effects);
    let render = outcome.render.expect("the option change re-renders");
    let menu = render.menu.as_ref().expect("the structured menu");
    match menu.item("=A") {
        Some(magit_lsp::menu::Item::Option { value, .. }) => {
            assert_eq!(value.as_deref(), Some("Alice"))
        }
        other => panic!("expected the author option, got {other:?}"),
    }
    assert_eq!(menu.preview(), "git commit --author=Alice");
    assert!(
        render.edits.expect("edits").iter().any(|edit| edit
            .lines
            .iter()
            .any(|line| line.contains("--author=Alice"))),
        "the preview and value appear in the edit"
    );

    let requests = test.ui().requests();
    assert_eq!(requests.len(), 1, "one prompt was asked");
    assert_eq!(requests[0].kind, magit_lsp::protocol::PromptKind::Text);
    assert_eq!(requests[0].title, "Override the author (--author=)");
    assert_eq!(requests[0].placeholder.as_deref(), Some("--author"));
}

#[test]
fn escape_q_and_ctrl_c_pop_the_menu_and_restore_the_status_keymap() {
    for key in ["Escape", "q", "ctrl+c"] {
        let (tmp, _repo) = RepoFixture::new()
            .commit("init", [("f.txt", TEN_LINES)])
            .build();
        let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
        test.ack();
        let (descriptor, _render) = test.open_via_command("open.commit");
        test.ack();

        let outcome = test.press(key);
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
        assert!(test.session().views().get(&descriptor.id).is_none());
        assert_eq!(test.view_ids(), vec!["v1".to_string()]);
    }
}

#[test]
fn an_unbound_menu_key_is_swallowed_with_a_hint() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    let (_descriptor, _render) = test.open_via_command("open.commit");
    test.ack();
    let before = tmp.git(&["status", "--porcelain"]);

    let outcome = test.press("z");
    assert_eq!(message_texts(&outcome.effects), vec!["Unbound key: z"]);
    assert!(outcome.render.is_none());
    assert_eq!(tmp.git(&["status", "--porcelain"]), before);
}

#[test]
fn menu_help_replaces_the_current_menu() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    let (commit, _render) = test.open_via_command("open.commit");
    test.ack();

    let replaced = test.press("?");
    let dispatch = replaced
        .effects
        .iter()
        .find_map(|effect| match effect {
            Effect::PushView { descriptor } => Some(descriptor.clone()),
            _ => None,
        })
        .expect("the dispatch menu is pushed");
    assert_eq!(dispatch.kind, ViewKind::Menu("menu.dispatch".to_string()));
    assert!(
        replaced
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::PopView { view_id } if view_id == &commit.id)),
        "the old menu is popped: {:?}",
        replaced.effects
    );
    assert!(
        replaced
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::FocusView { view_id } if view_id == "v1")),
        "focus returns to status before the dispatch opens"
    );
    assert_eq!(test.view_ids(), vec!["v1".to_string(), dispatch.id.clone()]);
    assert!(test.session().views().get(&commit.id).is_none());

    let popped = test.press("q");
    assert!(popped
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::PopView { view_id } if view_id == &dispatch.id)));
    assert_eq!(test.view_ids(), vec!["v1".to_string()]);
    assert_eq!(popped.keymap.expect("keymap").view_id, "v1");
}

#[test]
fn opening_a_top_level_menu_from_a_menu_replaces_it() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();

    let (dispatch, _render) = test.open_via_command("open.dispatch");
    test.ack();

    // `c` on the dispatch is `open.commit`, which belongs to the status view:
    // the dispatch is popped and the commit menu takes its place.
    let replaced = test.press("c");
    let commit = replaced
        .effects
        .iter()
        .find_map(|effect| match effect {
            Effect::PushView { descriptor } => Some(descriptor.clone()),
            _ => None,
        })
        .expect("the commit menu is pushed");
    assert_eq!(commit.kind, ViewKind::Menu("menu.commit".to_string()));
    assert!(replaced
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::PopView { view_id } if view_id == &dispatch.id)));
    assert_eq!(test.view_ids(), vec!["v1".to_string(), commit.id.clone()]);
}

#[test]
fn save_and_reset_args_restore_the_saved_switches() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    let (descriptor, _render) = test.open_via_command("open.commit");
    test.ack();

    test.press("-");
    test.press("a");
    let saved = test.invoke("menu.save-args");
    assert_eq!(message_texts(&saved.effects), vec!["Saved menu arguments"]);
    assert!(saved.render.is_some(), "saving re-renders");

    test.press("-");
    test.press("a");
    assert!(!menu_switch(&test, &descriptor.id, "-a"));

    let reset = test.invoke("menu.reset-args");
    assert_eq!(message_texts(&reset.effects), vec!["Reset menu arguments"]);
    assert!(menu_switch(&test, &descriptor.id, "-a"));
    let render = reset.render.expect("resetting re-renders");
    assert_eq!(render.menu.expect("menu").preview(), "git commit --all");
}

#[test]
fn pressing_a_set_option_again_clears_it_without_prompting() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    let (_descriptor, _render) = test.open_via_command("open.commit");
    test.ack();

    // Set `=A` with one queued text reply.
    test.ui().reply_text(Some("Alice".to_string()));
    test.press("=");
    let set = test.press_ui("A");
    let menu = set
        .render
        .expect("setting an option re-renders")
        .menu
        .expect("the structured menu");
    match menu.item("=A") {
        Some(magit_lsp::menu::Item::Option {
            value: Some(value),
            ..
        }) => assert_eq!(value, "Alice"),
        other => panic!("expected the set author option, got {other:?}"),
    }

    // Press `=A` again with no reply queued: if the code path prompted, the
    // scripted UI would error out and this line would panic.
    test.press("=");
    let clear = test.press_ui("A");
    let menu = clear
        .render
        .expect("clearing an option re-renders")
        .menu
        .expect("the structured menu");
    match menu.item("=A") {
        Some(magit_lsp::menu::Item::Option { value: None, .. }) => {}
        other => panic!("expected the cleared author option, got {other:?}"),
    }
    assert_eq!(menu.preview(), "git commit");
    assert_eq!(
        test.text(),
        menus::named("commit")
            .expect("commit menu")
            .render(WIDE_VIEWPORT)
            .lines,
        "the cleared menu renders like a fresh one"
    );

    // The clearing press asked no prompt at all.
    let requests = test.ui().requests();
    assert_eq!(
        requests.len(),
        1,
        "only the original set prompted: {requests:?}"
    );
}

#[test]
fn opening_a_menu_with_saved_args_initializes_and_restores_the_arguments() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();

    let saved = serde_json::json!({
        "saved_args": {
            "switches": ["-a"],
            "options": { "=A": "Alice" },
        }
    });
    let (descriptor, render) = test.open_view_with_args(
        "saved-menu",
        ViewKind::Menu("menu.commit".to_string()),
        Placement::Bottom,
        WIDE_VIEWPORT,
        &saved,
    );
    let menu = render.menu.as_ref().expect("the structured menu");
    match menu.item("-a") {
        Some(magit_lsp::menu::Item::Switch { on: true, .. }) => {}
        other => panic!("expected the saved --all switch on, got {other:?}"),
    }
    match menu.item("=A") {
        Some(magit_lsp::menu::Item::Option {
            value: Some(value),
            ..
        }) => assert_eq!(value, "Alice"),
        other => panic!("expected the saved author option, got {other:?}"),
    }
    assert_eq!(menu.preview(), "git commit --all --author=Alice");
    assert_eq!(
        test.menu(&descriptor.id).expect("the menu view").saved_args,
        magit_lsp::menu::Args {
            switches: vec!["-a".to_string()],
            options: [("=A".to_string(), "Alice".to_string())].into_iter().collect(),
        },
        "the open args also become the reset defaults"
    );

    // Change the arguments, then restore the ones the view was opened with.
    test.press("- a");
    assert!(
        !menu_switch(&test, &descriptor.id, "-a"),
        "toggling changes the saved argument"
    );
    let reset = test.invoke("menu.reset-args");
    assert_eq!(message_texts(&reset.effects), vec!["Reset menu arguments"]);
    assert!(menu_switch(&test, &descriptor.id, "-a"));
    let menu = test.menu(&descriptor.id).expect("the menu view");
    match menu.item("=A") {
        Some(magit_lsp::menu::Item::Option {
            value: Some(value),
            ..
        }) => assert_eq!(value, "Alice"),
        other => panic!("expected the restored author option, got {other:?}"),
    }
    assert_eq!(menu.preview(), "git commit --all --author=Alice");

    // Saving now stores the current arguments as the new reset defaults:
    // add `--no-verify`, save, then disturb everything and reset.
    test.press("- n");
    let save = test.invoke("menu.save-args");
    assert_eq!(message_texts(&save.effects), vec!["Saved menu arguments"]);
    test.press("- n");
    test.press("= A");
    let menu = test.menu(&descriptor.id).expect("the menu view");
    assert_eq!(menu.preview(), "git commit --all");
    let reset = test.invoke("menu.reset-args");
    let menu = reset
        .render
        .expect("resetting re-renders")
        .menu
        .expect("the structured menu");
    assert_eq!(
        menu.preview(),
        "git commit --all --no-verify --author=Alice",
        "reset restores the arguments saved after the open"
    );
}

#[test]
fn menu_tokens_decode_to_the_documented_types() {
    let tree = menus::named("commit")
        .expect("commit menu")
        .render(Viewport {
            cols: 100,
            rows: 30,
        });
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
        "magit.menu.flag.off",
        "magit.menu.switch.off",
        "magit.menu.option",
        "magit.menu.preview",
        "magit.menu.planned",
    ] {
        assert!(seen.contains(&expected), "missing {expected}: {seen:?}");
    }

    // The switch marker and the flag text change type when the switch is on.
    let mut menu = menus::named("commit").expect("commit menu");
    let flag_token = |menu: &Menu| {
        let tree = menu.render(Viewport {
            cols: 100,
            rows: 30,
        });
        let line = tree
            .lines
            .iter()
            .position(|line| line.contains("(--all)"))
            .expect("the --all flag line");
        let start = tree.lines[line].find("--all").expect("the --all text");
        tree.tokens
            .iter()
            .find(|token| token.line == line && token.start == start && token.length == 5)
            .expect("a token on the flag text")
            .token_type
            .clone()
    };
    assert_eq!(flag_token(&menu), "magit.menu.flag.off");

    menu.toggle_switch("-a");
    let tree = menu.render(Viewport {
        cols: 100,
        rows: 30,
    });
    let data = semantic_tokens::encode_lines(&tree.lines, &tree.tokens);
    let seen: Vec<&str> = data
        .iter()
        .map(|token| types[token.token_type as usize])
        .collect();
    assert!(seen.contains(&"magit.menu.switch.on"), "{seen:?}");
    assert!(seen.contains(&"magit.menu.flag.on"), "{seen:?}");
    assert_eq!(flag_token(&menu), "magit.menu.flag.on");
}

/// The token type covering `offset` bytes into the first line containing `text`.
fn token_at(tree: &magit_lsp::tree::Tree, text: &str, offset: usize) -> Option<String> {
    let line = tree.lines.iter().position(|line| line.contains(text))?;
    let start = tree.lines[line].find(text)? + offset;
    tree.tokens
        .iter()
        .find(|token| token.line == line && token.start == start)
        .map(|token| token.token_type.clone())
}

#[test]
fn a_pending_prefix_dims_items_that_cannot_complete_it() {
    let menu = menus::named("commit").expect("commit menu");
    let tree = menu.render_with_pending(
        Viewport {
            cols: 100,
            rows: 30,
        },
        &["-".to_string()],
    );

    // An option cannot follow `-`, so its whole key is disabled.
    assert_eq!(
        token_at(&tree, "=A", 0).as_deref(),
        Some("magit.menu.disabled")
    );
    // A switch can: its `-` is the pending prefix and `a` is the ordinary key.
    assert_eq!(
        token_at(&tree, "-a", 0).as_deref(),
        Some("magit.menu.pending")
    );
    assert_eq!(token_at(&tree, "-a", 1).as_deref(), Some("magit.menu.key"));
    // An action cannot follow `-` either.
    assert_eq!(
        token_at(&tree, "  c  Commit", 2).as_deref(),
        Some("magit.menu.disabled")
    );

    // With no pending steps every item is normal again.
    let normal = menu.render(Viewport {
        cols: 100,
        rows: 30,
    });
    assert_eq!(
        token_at(&normal, "=A", 0).as_deref(),
        Some("magit.menu.option")
    );
}

#[test]
fn a_non_matching_key_keeps_pending_and_hints() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    let (descriptor, _render) = test.open_via_command("open.commit");
    test.ack();

    test.press("-");
    let before = tmp.git(&["status", "--porcelain"]);

    // `c` is an action, but with `-` pending only switches are live.
    let outcome = test.press("c");
    assert_eq!(message_texts(&outcome.effects), vec!["Unbound key: -c"]);
    assert!(outcome.render.is_none(), "the pending state is unchanged");
    assert_eq!(tmp.git(&["status", "--porcelain"]), before);
    let notice = test.keymap_notice().expect("the menu keymap");
    assert_eq!(notice.pending, "-", "a non-matching key keeps the prefix");
    assert!(test.session().views().get(&descriptor.id).is_some());
}

#[test]
fn backspace_pops_one_pending_step() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    let (descriptor, _render) = test.open_via_command("open.commit");
    test.ack();

    test.press("-");
    let popped = test.press("Backspace");
    assert!(popped.effects.is_empty(), "{:?}", popped.effects);
    let render = popped.render.expect("popping a step re-renders");
    assert_eq!(render.pending, "");
    assert!(
        test.session().views().get(&descriptor.id).is_some(),
        "the menu stays open"
    );
    let tree = test
        .session()
        .views()
        .get(&descriptor.id)
        .expect("view")
        .tree();
    assert_eq!(
        token_at(tree, "=A", 0).as_deref(),
        Some("magit.menu.option"),
        "items are normal again"
    );
}

#[test]
fn backspace_with_empty_pending_does_nothing() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    let (descriptor, _render) = test.open_via_command("open.commit");
    test.ack();

    let outcome = test.press("Backspace");
    assert!(outcome.effects.is_empty(), "{:?}", outcome.effects);
    assert!(outcome.render.is_none(), "nothing changed");
    assert!(
        test.session().views().get(&descriptor.id).is_some(),
        "the menu stays open"
    );
    assert_eq!(test.keymap_notice().expect("the menu keymap").pending, "");
}

#[test]
fn the_keymap_reports_pending_and_live_keys() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    let (_descriptor, _render) = test.open_via_command("open.commit");
    test.ack();

    let notice = test.keymap_notice().expect("the menu keymap");
    assert_eq!(notice.pending, "");
    assert!(notice.live_keys.contains(&"c".to_string()));
    assert!(notice.live_keys.contains(&"-a".to_string()));
    assert!(!notice.live_keys.contains(&"Backspace".to_string()));

    test.press("-");
    let notice = test.keymap_notice().expect("the menu keymap");
    assert_eq!(notice.pending, "-");
    assert!(notice.live_keys.contains(&"-a".to_string()));
    assert!(!notice.live_keys.contains(&"c".to_string()));
    assert!(notice.live_keys.contains(&"Backspace".to_string()));
}

#[test]
fn the_registry_exposes_open_commit_and_the_menu_commands() {
    use magit_lsp::commands::{registry, CommandStatus};
    for id in [
        "open.commit",
        "open.dispatch",
        "menu.save-args",
        "menu.reset-args",
    ] {
        let command = registry()
            .get(id)
            .unwrap_or_else(|| panic!("{id} is registered"));
        assert_eq!(
            command.status,
            CommandStatus::Implemented,
            "{id} is implemented"
        );
    }

    // A menu action's status mirrors its registry status, so a menu item can
    // never report a state the registry does not know about.
    let menu = menus::named("commit").expect("commit menu");
    for key in ["e", "f", "a"] {
        let item = menu.item(key).expect("commit menu item");
        let magit_lsp::menu::Item::Action { command, .. } = item else {
            panic!("{key} is an action");
        };
        let registered = registry()
            .get(&command)
            .unwrap_or_else(|| panic!("{command} is registered"));
        match &registered.status {
            CommandStatus::Implemented => {
                assert_eq!(item.status(), &MenuStatus::Implemented);
            }
            CommandStatus::Planned { roadmap_id } => assert_eq!(
                item.status(),
                &MenuStatus::Planned {
                    roadmap_id: roadmap_id.to_string()
                }
            ),
        }
    }
}
