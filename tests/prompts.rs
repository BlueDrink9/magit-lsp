//! Prompt transport (R-3.1): the scripted UI, menu option prompts, cancellation
//! and the quit effect.

mod common;

use common::*;
use magit_lsp::config::Profile;
use magit_lsp::menu::Item;
use magit_lsp::presentation::RichCapabilities;
use magit_lsp::protocol::{Effect, PromptChoiceAnswer, PromptKind, PromptValue};
use std::collections::BTreeSet;

fn option_value(test: &TestSession, view: &str, key: &str) -> Option<String> {
    let menu = test.menu(view).expect("a menu view");
    match menu.item(key) {
        Some(Item::Option { value, .. }) => value.clone(),
        other => panic!("expected an option at {key}, got {other:?}"),
    }
}

/// Stage `open.commit`'s `--author` option prompt and return its request.
fn stage_author_prompt(test: &mut TestSession, view_id: &str) -> magit_lsp::protocol::ViewPromptParams {
    use magit_lsp::keymap::EditorMode;
    use magit_lsp::protocol::{Cursor, ViewKeyParams};
    use magit_lsp::session::Execution;
    let mut key = ViewKeyParams {
        view_id: view_id.to_string(),
        version: test.version(),
        key: "=".to_string(),
        editor_mode: EditorMode::Normal,
        cursor: Cursor { line: 0, column: 0 },
        selection: None,
    };
    test.session.start_key(&key).unwrap();
    key.key = "A".to_string();
    let Execution::Prompt(request) = test.session.start_key(&key).unwrap() else {
        panic!("option prompt");
    };
    request
}

/// Stage `commit.create`'s staging confirm on an unstaged status view.
fn stage_commit_confirm(test: &mut TestSession, view_id: &str) -> magit_lsp::protocol::ViewPromptParams {
    use magit_lsp::protocol::ViewInvokeParams;
    use magit_lsp::session::Execution;
    let params = ViewInvokeParams {
        view_id: view_id.to_string(),
        command: "commit.create".to_string(),
        version: None,
        args: None,
        cursor: None,
        selection: None,
    };
    let Execution::Prompt(request) = test.session.start_invoke(&params).unwrap() else {
        panic!("commit.create confirm prompt");
    };
    assert_eq!(request.kind, PromptKind::Confirm);
    request
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
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    let (menu, _) = test.open_via_command("open.commit");
    let mut request = ViewKeyParams {
        view_id: menu.id.clone(),
        version: test.version(),
        key: "=".to_string(),
        editor_mode: EditorMode::Normal,
        cursor: Cursor { line: 0, column: 0 },
        selection: None,
    };
    test.session.start_key(&request).unwrap();
    request.key = "A".to_string();
    let Execution::Prompt(first) = test.session.start_key(&request).unwrap() else {
        panic!("option prompt");
    };
    let Execution::Done(cancelled) = test.session.resume_prompt(&first.prompt_id, None).unwrap()
    else {
        panic!("cancelled");
    };
    assert!(cancelled.effects.is_empty());
    request.key = "=".to_string();
    test.session.start_key(&request).unwrap();
    request.key = "A".to_string();
    let Execution::Prompt(second) = test.session.start_key(&request).unwrap() else {
        panic!("second prompt");
    };
    assert_ne!(first.prompt_id, second.prompt_id);
    test.session
        .resume_prompt(
            &first.prompt_id,
            Some(PromptValue::Text("stale".to_string())),
        )
        .unwrap();
    assert!(test.session.has_pending_prompt());
    assert_eq!(option_value(&test, &menu.id, "=A"), None);
    test.session
        .resume_prompt(
            &second.prompt_id,
            Some(PromptValue::Text("Alice".to_string())),
        )
        .unwrap();
    assert_eq!(
        option_value(&test, &menu.id, "=A").as_deref(),
        Some("Alice")
    );
}

#[test]
fn log_path_prompt_binds_the_menu_options_not_a_render_version() {
    use magit_lsp::keymap::EditorMode;
    use magit_lsp::protocol::{Cursor, ViewKeyParams};
    use magit_lsp::session::Execution;
    for close in [false, true] {
        let (tmp, _) = RepoFixture::new()
            .commit("init", [("f.txt", TEN_LINES)])
            .build();
        let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
        let (menu, _) = test.open_via_command("open.log");
        let mut key = ViewKeyParams {
            view_id: menu.id.clone(),
            version: test.version(),
            key: "=".to_string(),
            editor_mode: EditorMode::Normal,
            cursor: Cursor { line: 0, column: 0 },
            selection: None,
        };
        test.session.start_key(&key).unwrap();
        key.key = "f".to_string();
        let Execution::Prompt(prompt) = test.session.start_key(&key).unwrap() else {
            panic!("path prompt");
        };
        test.session.resize(&menu.id, STATUS_VIEWPORT).unwrap();
        let refreshed = test.session.refresh(&menu.id).unwrap().unwrap();
        test.session.ack(&menu.id, refreshed.version).unwrap();
        if close {
            test.session.close(&menu.id).unwrap();
        }
        let Execution::Done(_) = test
            .session
            .resume_prompt(
                &prompt.prompt_id,
                Some(PromptValue::Text("f.txt".to_string())),
            )
            .unwrap()
        else {
            panic!("path completion");
        };
        if close {
            assert!(test.session.views().get(&menu.id).is_none());
        } else {
            assert_eq!(
                option_value(&test, &menu.id, "=f").as_deref(),
                Some("f.txt")
            );
        }
        assert_eq!(tmp.git(&["status", "--porcelain"]), "");
    }
}

#[test]
fn opening_a_document_prompt_pushes_one_prompt_view_above_its_origin() {
    use magit_lsp::view::{Placement, ViewKind};
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    let (menu, _) = test.open_via_command("open.commit");
    let request = stage_author_prompt(&mut test, &menu.id);
    let depth = test.session().views().iter().count();

    let result = test.session.open_document_prompt(&request).unwrap();

    assert_eq!(test.session().views().iter().count(), depth + 1);
    assert!(test.session.prompt_active());
    let prompt_id = test
        .session
        .prompt_view_id()
        .expect("the open prompt view id")
        .to_string();
    assert_eq!(result.effects.len(), 1, "{:?}", result.effects);
    match &result.effects[0] {
        Effect::PushView { descriptor } => {
            assert_eq!(descriptor.kind, ViewKind::Prompt);
            assert_eq!(descriptor.placement, Placement::Main);
            assert_eq!(descriptor.id, prompt_id);
        }
        other => panic!("expected PushView, got {other:?}"),
    }
    assert_eq!(result.opened.len(), 1);
    let (render, _keymap) = &result.opened[0];
    let text = render.text.as_ref().expect("a full initial render").join("\n");
    assert!(text.contains("Override the author"), "{text}");
    let previous = test
        .session()
        .views()
        .previous_of(&prompt_id)
        .map(|view| view.descriptor().id.clone());
    assert_eq!(previous.as_deref(), Some(menu.id.as_str()));

    assert!(
        test.session.open_document_prompt(&request).is_err(),
        "a second open while the prompt is active must fail"
    );
}

#[test]
fn finishing_a_document_prompt_pops_it_and_applies_the_answer() {
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    let (menu, _) = test.open_via_command("open.commit");
    let request = stage_author_prompt(&mut test, &menu.id);
    let depth = test.session().views().iter().count();
    test.session.open_document_prompt(&request).unwrap();
    let prompt_id = test
        .session
        .prompt_view_id()
        .expect("the open prompt view id")
        .to_string();

    let completion = test
        .session
        .finish_document_prompt(&request.prompt_id, Some(PromptValue::Text("Alice".to_string())))
        .unwrap();

    assert_eq!(
        completion.result.effects,
        vec![
            Effect::PopView {
                view_id: prompt_id.clone()
            },
            Effect::FocusView {
                view_id: menu.id.clone()
            },
        ]
    );
    assert_eq!(test.session().views().iter().count(), depth);
    assert!(!test.session.prompt_active());
    assert!(test.session.prompt_view_id().is_none());
    assert!(!test.session.has_pending_prompt());
    assert_eq!(option_value(&test, &menu.id, "=A").as_deref(), Some("Alice"));
    let Execution::Done(done) = completion.next else {
        panic!("finishing cannot prompt again");
    };
    let render = done.render.expect("the updated menu preview render");
    let menu_preview = render.menu.as_ref().expect("the structured menu");
    assert_eq!(menu_preview.preview(), "git commit --author=Alice");
}

#[test]
fn cancelling_a_document_prompt_leaves_the_menu_option_unset() {
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    let (menu, _) = test.open_via_command("open.commit");
    let request = stage_author_prompt(&mut test, &menu.id);
    let depth = test.session().views().iter().count();
    test.session.open_document_prompt(&request).unwrap();
    let prompt_id = test
        .session
        .prompt_view_id()
        .expect("the open prompt view id")
        .to_string();

    let completion = test
        .session
        .finish_document_prompt(&request.prompt_id, None)
        .unwrap();

    assert_eq!(
        completion.result.effects,
        vec![
            Effect::PopView {
                view_id: prompt_id.clone()
            },
            Effect::FocusView {
                view_id: menu.id.clone()
            },
        ]
    );
    assert_eq!(test.session().views().iter().count(), depth);
    assert!(!test.session.prompt_active());
    assert_eq!(option_value(&test, &menu.id, "=A"), None);
    let preview = test
        .menu(&menu.id)
        .map(|menu| menu.preview())
        .expect("the menu");
    assert_eq!(preview, "git commit");
    assert!(matches!(completion.next, Execution::Done(_)));
}

#[test]
fn finishing_with_a_mismatched_prompt_id_changes_nothing() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    let (menu, _) = test.open_via_command("open.commit");
    let request = stage_author_prompt(&mut test, &menu.id);
    test.session.open_document_prompt(&request).unwrap();
    let depth = test.session().views().iter().count();
    let prompt_id = test
        .session
        .prompt_view_id()
        .expect("the open prompt view id")
        .to_string();

    let error = test
        .session
        .finish_document_prompt("session-prompt-unknown", Some(PromptValue::Text("Alice".to_string())))
        .expect_err("an unknown prompt id must fail");
    assert!(
        error.to_string().contains("no longer active"),
        "{error}"
    );
    assert_eq!(test.session().views().iter().count(), depth);
    assert!(test.session.prompt_active());
    assert_eq!(test.session.prompt_view_id(), Some(prompt_id.as_str()));
    assert!(test.session.has_pending_prompt());
    assert_eq!(option_value(&test, &menu.id, "=A"), None);

    let unknown = test
        .session
        .open_document_prompt(&magit_lsp::protocol::ViewPromptParams {
            prompt_id: "session-prompt-unknown".to_string(),
            ..request.clone()
        })
        .expect_err("a mismatched open must fail");
    assert!(unknown.to_string().contains("no longer active"), "{unknown}");
    assert_eq!(test.session().views().iter().count(), depth);
    assert!(test.session.prompt_active());
}

#[test]
fn a_refresh_during_a_document_prompt_is_held_for_the_addressed_view() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    let origin = test.view_id().to_string();
    let request = stage_commit_confirm(&mut test, &origin);
    test.session.open_document_prompt(&request).unwrap();
    let prompt_id = test
        .session
        .prompt_view_id()
        .expect("the open prompt view id")
        .to_string();
    let origin_version = test.session.views().get(&origin).unwrap().version();
    let prompt_version = test.session.views().get(&prompt_id).unwrap().version();

    assert!(test.session.refresh(&origin).unwrap().is_none());
    assert!(test.session.refresh(&prompt_id).unwrap().is_none());

    assert_eq!(
        test.session.views().get(&origin).unwrap().version(),
        origin_version
    );
    assert_eq!(
        test.session.views().get(&prompt_id).unwrap().version(),
        prompt_version
    );
    assert!(test.session.views().get(&origin).unwrap().dirty());
    assert!(test.session.views().get(&prompt_id).unwrap().dirty());
}

#[test]
fn finishing_a_document_prompt_renders_a_held_dirty_origin_once() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    let origin = test.view_id().to_string();
    let depth = test.session().views().iter().count();
    let request = stage_commit_confirm(&mut test, &origin);
    test.session.open_document_prompt(&request).unwrap();
    assert_eq!(test.session().views().iter().count(), depth + 1);
    let version = test.session.views().get(&origin).unwrap().version();
    assert!(test.session.refresh(&origin).unwrap().is_none());
    assert!(test.session.views().get(&origin).unwrap().dirty());

    let completion = test
        .session
        .finish_document_prompt(&request.prompt_id, Some(PromptValue::Bool(true)))
        .unwrap();

    assert_eq!(test.session().views().iter().count(), depth);
    assert!(!test.session.has_pending_prompt());
    let origin_view = test.session.views().get(&origin).expect("the origin stays open");
    assert_eq!(origin_view.version(), version + 1);
    assert!(!origin_view.dirty());
    assert_eq!(
        completion.result.opened.len(),
        1,
        "the held refresh renders the origin once"
    );
    let (render, _keymap) = &completion.result.opened[0];
    assert_eq!(render.view_id, origin);
    assert_eq!(render.version, version + 1);
    assert!(
        completion.result.effects.iter().any(|effect| matches!(
            effect,
            Effect::FocusView { view_id } if view_id == &origin
        )),
        "focus returns to the origin: {:?}",
        completion.result.effects
    );

    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    let origin = test.view_id().to_string();
    let depth = test.session().views().iter().count();
    let request = stage_commit_confirm(&mut test, &origin);
    test.session.open_document_prompt(&request).unwrap();
    let version = test.session.views().get(&origin).unwrap().version();

    let completion = test
        .session
        .finish_document_prompt(&request.prompt_id, Some(PromptValue::Bool(true)))
        .unwrap();

    assert_eq!(test.session().views().iter().count(), depth);
    assert!(
        completion.result.opened.is_empty(),
        "a clean origin renders no extra copy"
    );
    assert_eq!(test.session.views().get(&origin).unwrap().version(), version);
}

#[test]
fn closing_the_prompt_view_clears_the_continuation() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    let (menu, _) = test.open_via_command("open.commit");
    let request = stage_author_prompt(&mut test, &menu.id);
    let depth = test.session().views().iter().count();
    test.session.open_document_prompt(&request).unwrap();
    let prompt_id = test
        .session
        .prompt_view_id()
        .expect("the open prompt view id")
        .to_string();
    assert!(test.session.has_pending_prompt());

    test.session.close(&prompt_id).unwrap();

    assert!(!test.session.has_pending_prompt());
    assert!(!test.session.prompt_active());
    assert!(test.session.views().get(&prompt_id).is_none());
    assert!(test.session.views().get(&menu.id).is_some(), "origin stays open");
    assert_eq!(test.session().views().iter().count(), depth);
}

#[test]
fn closing_the_origin_view_closes_its_prompt_too() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    let (menu, _) = test.open_via_command("open.commit");
    let request = stage_author_prompt(&mut test, &menu.id);
    let depth = test.session().views().iter().count();
    test.session.open_document_prompt(&request).unwrap();
    let prompt_id = test
        .session
        .prompt_view_id()
        .expect("the open prompt view id")
        .to_string();

    test.session.close(&menu.id).unwrap();

    assert!(!test.session.has_pending_prompt());
    assert!(test.session.views().get(&menu.id).is_none());
    assert!(test.session.views().get(&prompt_id).is_none());
    assert_eq!(test.session().views().iter().count(), depth - 1);
}

/// The open prompt view's rendered tree as document text, with an optional
/// replacement of the single field line.
fn prompt_document(test: &TestSession, prompt_id: &str, field: Option<&str>) -> String {
    use magit_lsp::tree::NodeKind;
    let view = test
        .session
        .views()
        .get(prompt_id)
        .expect("the open prompt view");
    let tree = view.tree();
    let mut lines = tree.lines.clone();
    if let Some(value) = field {
        let node = tree
            .nodes
            .iter()
            .find(|node| node.kind == NodeKind::Field)
            .expect("a prompt field node");
        lines[node.start_line] = value.to_string();
    }
    lines.join("\n")
}

#[test]
fn answering_a_text_document_prompt_submits_the_edited_field() {
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    let (menu, _) = test.open_via_command("open.commit");
    let request = stage_author_prompt(&mut test, &menu.id);
    let depth = test.session().views().iter().count();
    test.session.open_document_prompt(&request).unwrap();
    let prompt_id = test
        .session
        .prompt_view_id()
        .expect("the open prompt view id")
        .to_string();
    assert_eq!(
        test.session.active_prompt_id(),
        Some(request.prompt_id.as_str())
    );

    let document = prompt_document(&test, &prompt_id, Some("Alice"));
    let completion = test
        .session
        .answer_document_prompt(&format!("prompt:{}:submit", request.prompt_id), &document)
        .unwrap();

    assert_eq!(
        completion.result.effects,
        vec![
            Effect::PopView {
                view_id: prompt_id.clone()
            },
            Effect::FocusView {
                view_id: menu.id.clone()
            },
        ]
    );
    assert_eq!(test.session().views().iter().count(), depth);
    assert!(!test.session.prompt_active());
    assert!(test.session.active_prompt_id().is_none());
    assert_eq!(option_value(&test, &menu.id, "=A").as_deref(), Some("Alice"));
    assert!(matches!(completion.next, Execution::Done(_)));
}

#[test]
fn cancelling_a_text_document_prompt_leaves_the_option_unset() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    let (menu, _) = test.open_via_command("open.commit");
    let request = stage_author_prompt(&mut test, &menu.id);
    let depth = test.session().views().iter().count();
    test.session.open_document_prompt(&request).unwrap();
    let prompt_id = test
        .session
        .prompt_view_id()
        .expect("the open prompt view id")
        .to_string();

    let document = prompt_document(&test, &prompt_id, None);
    let completion = test
        .session
        .answer_document_prompt(&format!("prompt:{}:cancel", request.prompt_id), &document)
        .unwrap();

    assert_eq!(test.session().views().iter().count(), depth);
    assert!(!test.session.prompt_active());
    assert_eq!(option_value(&test, &menu.id, "=A"), None);
    assert!(matches!(completion.next, magit_lsp::session::Execution::Done(_)));
}

#[test]
fn answering_a_confirm_document_prompt_yes_applies_and_no_cancels() {
    use magit_lsp::session::Execution;
    for (answer, applies) in [("yes", true), ("no", false)] {
        let (tmp, _) = RepoFixture::new()
            .commit("init", [("f.txt", TEN_LINES)])
            .modify_standard()
            .build();
        let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
        test.ack();
        let origin = test.view_id().to_string();
        let depth = test.session().views().iter().count();
        let request = stage_commit_confirm(&mut test, &origin);
        test.session.open_document_prompt(&request).unwrap();
        let prompt_id = test
            .session
            .prompt_view_id()
            .expect("the open prompt view id")
            .to_string();

        let document = prompt_document(&test, &prompt_id, None);
        let completion = test
            .session
            .answer_document_prompt(&format!("prompt:{}:{answer}", request.prompt_id), &document)
            .unwrap();

        assert_eq!(test.session().views().iter().count(), depth);
        assert!(!test.session.has_pending_prompt());
        assert!(!test.session.prompt_active());
        let Execution::Done(done) = completion.next else {
            panic!("a confirm answer cannot prompt again");
        };
        assert_eq!(done.commit.is_some(), applies, "{answer}: {done:?}");
        assert_eq!(
            done.effects
                .iter()
                .any(|effect| matches!(effect, Effect::Message { text, .. } if text == "Committing…")),
            applies,
            "{answer}: {:?}",
            done.effects
        );
    }
}

#[test]
fn editing_a_non_field_line_repairs_and_keeps_the_prompt_open() {
    use magit_lsp::errors::STALE_VIEW;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    let (menu, _) = test.open_via_command("open.commit");
    let request = stage_author_prompt(&mut test, &menu.id);
    let depth = test.session().views().iter().count();
    test.session.open_document_prompt(&request).unwrap();
    let prompt_id = test
        .session
        .prompt_view_id()
        .expect("the open prompt view id")
        .to_string();
    let version = test.session.views().get(&prompt_id).unwrap().version();
    let mut lines = prompt_document(&test, &prompt_id, None)
        .split('\n')
        .map(str::to_string)
        .collect::<Vec<_>>();
    lines[0] = "tampered title".to_string();
    let document = lines.join("\n");

    let completion = test
        .session
        .answer_document_prompt(&format!("prompt:{}:submit", request.prompt_id), &document)
        .unwrap();

    assert_eq!(
        completion.result.effects,
        vec![Effect::message(
            magit_lsp::protocol::MessageLevel::Warn,
            STALE_VIEW.to_string()
        )]
    );
    let render = completion.result.render.expect("the repaired document re-renders");
    assert_eq!(render.view_id, prompt_id);
    assert_eq!(render.version, version + 1);
    assert!(matches!(
        completion.next,
        magit_lsp::session::Execution::Done(_)
    ));
    assert_eq!(test.session().views().iter().count(), depth + 1);
    assert!(test.session.prompt_active());
    assert_eq!(test.session.prompt_view_id(), Some(prompt_id.as_str()));
    assert!(test.session.has_pending_prompt());
    assert_eq!(option_value(&test, &menu.id, "=A"), None);
}

#[test]
fn an_unknown_prompt_row_changes_nothing() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    let (menu, _) = test.open_via_command("open.commit");
    let request = stage_author_prompt(&mut test, &menu.id);
    let depth = test.session().views().iter().count();
    test.session.open_document_prompt(&request).unwrap();
    let prompt_id = test
        .session
        .prompt_view_id()
        .expect("the open prompt view id")
        .to_string();

    let document = prompt_document(&test, &prompt_id, None);
    let error = test
        .session
        .answer_document_prompt(&format!("prompt:{}:bogus", request.prompt_id), &document)
        .expect_err("an unknown row must fail");
    assert!(error.to_string().contains("unknown prompt row"), "{error}");

    assert_eq!(test.session().views().iter().count(), depth + 1);
    assert!(test.session.prompt_active());
    assert_eq!(test.session.prompt_view_id(), Some(prompt_id.as_str()));
    assert!(test.session.has_pending_prompt());
    assert_eq!(option_value(&test, &menu.id, "=A"), None);
}

#[test]
fn a_client_with_no_prompt_declarations_stages_the_author_prompt_as_a_document() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    let (menu, _render) = test.open_via_command("open.commit");
    test.ack();
    test.session.set_rich_capabilities(Some(RichCapabilities {
        keys: magit_lsp::protocol::KeymapDelivery::Dynamic,
        modifiers: magit_lsp::protocol::KeyModifiers::Full,
        layout: BTreeSet::new(),
        effects: BTreeSet::new(),
        prompts: BTreeSet::new(),
        pick: None,
    }));

    let request = stage_author_prompt(&mut test, &menu.id);

    assert_eq!(request.kind, PromptKind::Text);
    assert!(test.session.has_pending_prompt());
}

#[test]
fn reconciling_without_an_open_prompt_returns_none() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);

    assert!(!test.session.prompt_active());
    assert!(test
        .session
        .reconcile_prompt_document("anything")
        .unwrap()
        .is_none());
}

#[test]
fn a_field_edit_reconciles_to_the_typed_author_without_a_render() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    let (menu, _) = test.open_via_command("open.commit");
    let request = stage_author_prompt(&mut test, &menu.id);
    test.session.open_document_prompt(&request).unwrap();
    let prompt_id = test
        .session
        .prompt_view_id()
        .expect("the open prompt view id")
        .to_string();

    let document = prompt_document(&test, &prompt_id, Some("Alice"));
    let reconciled = test
        .session
        .reconcile_prompt_document(&document)
        .unwrap()
        .expect("the prompt is open");
    assert!(!reconciled.repaired);
    assert!(reconciled.render.is_none());

    test.session
        .answer_document_prompt(&format!("prompt:{}:submit", request.prompt_id), &document)
        .unwrap();

    assert_eq!(option_value(&test, &menu.id, "=A").as_deref(), Some("Alice"));
    assert!(!test.session.prompt_active());
}

#[test]
fn a_title_edit_repairs_the_prompt_document_at_a_higher_version() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    let (menu, _) = test.open_via_command("open.commit");
    let request = stage_author_prompt(&mut test, &menu.id);
    test.session.open_document_prompt(&request).unwrap();
    let prompt_id = test
        .session
        .prompt_view_id()
        .expect("the open prompt view id")
        .to_string();
    let version = test.session.views().get(&prompt_id).unwrap().version();
    let mut lines = prompt_document(&test, &prompt_id, None)
        .split('\n')
        .map(str::to_string)
        .collect::<Vec<_>>();
    lines[0] = "tampered title".to_string();
    let document = lines.join("\n");

    let reconciled = test
        .session
        .reconcile_prompt_document(&document)
        .unwrap()
        .expect("the prompt is open");

    assert!(reconciled.repaired);
    let render = reconciled.render.expect("a repaired document re-renders");
    assert_eq!(render.view_id, prompt_id);
    assert_eq!(render.version, version + 1);
    assert!(test.session.prompt_active());
    assert_eq!(test.session.prompt_view_id(), Some(prompt_id.as_str()));
}
