//! Prompt transport (R-3.1): the scripted UI, menu option prompts, cancellation
//! and the quit effect.

mod common;

use common::*;
use magit_lsp::config::Profile;
use magit_lsp::menu::Item;
use magit_lsp::protocol::{Effect, PromptChoiceAnswer, PromptKind, PromptValue};

fn option_value(test: &TestSession, view: &str, key: &str) -> Option<String> {
    let menu = test.menu(view).expect("a menu view");
    match menu.item(key) {
        Some(Item::Option { value, .. }) => value.clone(),
        other => panic!("expected an option at {key}, got {other:?}"),
    }
}

#[test]
fn a_scripted_text_answer_sets_the_menu_option_and_re_renders() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    let (descriptor, _render) = test.open_via_command("open.commit");
    test.ack();

    test.ui().reply_text(Some("Alice".to_string()));

    test.press_ui("=");
    let outcome = test.press_ui("A");
    assert!(outcome.effects.is_empty(), "{:?}", outcome.effects);
    let render = outcome.render.expect("setting an option re-renders");
    let menu = render.menu.as_ref().expect("the structured menu");
    assert_eq!(menu.preview(), "git commit --author=Alice");

    let requests = test.ui().requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].kind, PromptKind::Text);
    assert_eq!(requests[0].title, "Override the author (--author=)");
    assert_eq!(requests[0].placeholder.as_deref(), Some("--author"));
    assert_eq!(
        option_value(&test, &descriptor.id, "=A").as_deref(),
        Some("Alice")
    );
}

#[test]
fn cancelling_an_option_prompt_leaves_the_option_and_repo_untouched() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    let (descriptor, _render) = test.open_via_command("open.commit");
    test.ack();
    let before = tmp.git(&["status", "--porcelain"]);

    test.ui().reply_cancel();

    test.press_ui("=");
    let outcome = test.press_ui("A");
    assert!(outcome.effects.is_empty(), "{:?}", outcome.effects);
    assert!(
        outcome.render.is_none(),
        "a cancelled prompt renders nothing"
    );
    assert_eq!(option_value(&test, &descriptor.id, "=A"), None);
    let preview = test
        .menu(&descriptor.id)
        .map(|menu| menu.preview())
        .expect("the menu");
    assert_eq!(preview, "git commit");
    assert_eq!(tmp.git(&["status", "--porcelain"]), before);
}

#[test]
fn a_choice_reader_prompts_with_allow_custom_and_empty_items() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    let (descriptor, _render) = test.open_via_command("open.diff");
    test.ack();

    test.ui().reply_choice(Some(PromptChoiceAnswer::Custom {
        custom: "histogram".to_string(),
    }));

    test.press_ui("=");
    let outcome = test.press_ui("a");
    let render = outcome.render.expect("setting an option re-renders");
    let menu = render.menu.as_ref().expect("the structured menu");
    assert_eq!(menu.preview(), "git diff --diff-algorithm=histogram");

    let requests = test.ui().requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].kind, PromptKind::Choice);
    assert_eq!(requests[0].allow_custom, Some(true));
    assert_eq!(requests[0].items, Some(Vec::new()));
    assert_eq!(
        option_value(&test, &descriptor.id, "=a").as_deref(),
        Some("histogram")
    );
}

#[test]
fn common_quit_emits_the_quit_effect_and_closes_every_view() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    test.open_via_command("open.commit");
    test.ack();

    test.focus("v1");
    let outcome = test.invoke("common.quit");
    assert_eq!(outcome.effects, vec![Effect::Quit]);
    assert!(outcome.render.is_none());
    assert!(
        test.session().views().is_empty(),
        "quit closes every view of the session"
    );
}

#[test]
fn a_cancelled_identity_cannot_answer_a_later_menu_prompt() {
    use magit_lsp::keymap::EditorMode;
    use magit_lsp::protocol::{Cursor, ViewKeyParams};
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new().commit("init", [("f.txt", TEN_LINES)]).build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    let (menu, _) = test.open_via_command("open.commit");
    let mut request = ViewKeyParams {
        view_id: menu.id.clone(), version: test.version(), key: "=".to_string(),
        editor_mode: EditorMode::Normal, cursor: Cursor { line: 0, column: 0 }, selection: None,
    };
    test.session.start_key(&request).unwrap();
    request.key = "A".to_string();
    let Execution::Prompt(first) = test.session.start_key(&request).unwrap() else { panic!("option prompt"); };
    let Execution::Done(cancelled) = test.session.resume_prompt(&first.prompt_id, None).unwrap() else { panic!("cancelled"); };
    assert!(cancelled.effects.is_empty());
    request.key = "=".to_string();
    test.session.start_key(&request).unwrap();
    request.key = "A".to_string();
    let Execution::Prompt(second) = test.session.start_key(&request).unwrap() else { panic!("second prompt"); };
    assert_ne!(first.prompt_id, second.prompt_id);
    test.session.resume_prompt(&first.prompt_id, Some(PromptValue::Text("stale".to_string()))).unwrap();
    assert!(test.session.has_pending_prompt());
    assert_eq!(option_value(&test, &menu.id, "=A"), None);
    test.session.resume_prompt(&second.prompt_id, Some(PromptValue::Text("Alice".to_string()))).unwrap();
    assert_eq!(option_value(&test, &menu.id, "=A").as_deref(), Some("Alice"));
}

#[test]
fn log_path_prompt_binds_the_menu_options_not_a_render_version() {
    use magit_lsp::keymap::EditorMode;
    use magit_lsp::protocol::{Cursor, ViewKeyParams};
    use magit_lsp::session::Execution;
    for close in [false, true] {
        let (tmp, _) = RepoFixture::new().commit("init", [("f.txt", TEN_LINES)]).build();
        let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
        let (menu, _) = test.open_via_command("open.log");
        let mut key = ViewKeyParams { view_id: menu.id.clone(), version: test.version(),
            key: "=".to_string(), editor_mode: EditorMode::Normal,
            cursor: Cursor { line: 0, column: 0 }, selection: None };
        test.session.start_key(&key).unwrap();
        key.key = "f".to_string();
        let Execution::Prompt(prompt) = test.session.start_key(&key).unwrap() else { panic!("path prompt"); };
        test.session.resize(&menu.id, STATUS_VIEWPORT).unwrap();
        let refreshed = test.session.refresh(&menu.id).unwrap().unwrap();
        test.session.ack(&menu.id, refreshed.version).unwrap();
        if close { test.session.close(&menu.id).unwrap(); }
        let Execution::Done(_) = test.session.resume_prompt(&prompt.prompt_id, Some(PromptValue::Text("f.txt".to_string()))).unwrap() else { panic!("path completion"); };
        if close {
            assert!(test.session.views().get(&menu.id).is_none());
        } else {
            assert_eq!(option_value(&test, &menu.id, "=f").as_deref(), Some("f.txt"));
        }
        assert_eq!(tmp.git(&["status", "--porcelain"]), "");
    }
}
