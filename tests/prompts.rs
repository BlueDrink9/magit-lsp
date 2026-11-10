//! Prompt transport (R-3.1): the scripted UI, menu option prompts, cancellation
//! and the quit effect.

mod common;

use common::*;
use magit_lsp::config::Profile;
use magit_lsp::menu::Item;
use magit_lsp::presentation::RichCapabilities;
use magit_lsp::protocol::{Effect, PromptChoiceAnswer, PromptKind, PromptValue};
use magit_lsp::view::ViewMode;
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

/// Press `=` then `option` on `view_id` and return the derived pick prompt request.
fn stage_option_pick(
    test: &mut TestSession,
    view_id: &str,
    option: &str,
) -> magit_lsp::protocol::ViewPromptParams {
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
    key.key = option.to_string();
    assert!(matches!(
        test.session.start_key(&key).unwrap(),
        Execution::Pick(_)
    ));
    test.session
        .pending_prompt_request()
        .expect("the staged pick prompt")
}

/// Stage `open.branch`'s `--set-upstream-to` option prompt and return its request.
fn stage_ref_prompt(test: &mut TestSession, view_id: &str) -> magit_lsp::protocol::ViewPromptParams {
    stage_option_pick(test, view_id, "u")
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
fn finishing_and_cancelling_a_document_prompt_resume_every_held_refresh() {
    for cancel in [false, true] {
        let (tmp, _) = RepoFixture::new()
            .commit("init", [("f.txt", TEN_LINES)])
            .modify_standard()
            .build();
        let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
        test.ack();
        let status = test.view_id().to_string();
        let (log, _) = test.open_via_command("open.log");
        test.ack();
        let log = log.id;
        let request = stage_commit_confirm(&mut test, &status);
        test.session.open_document_prompt(&request).unwrap();
        let status_version = test.session.views().get(&status).unwrap().version();
        let log_version = test.session.views().get(&log).unwrap().version();

        assert!(
            test.session.refresh_all().is_empty(),
            "an open prompt holds every refresh"
        );
        assert!(test.session.views().get(&status).unwrap().dirty());
        assert!(test.session.views().get(&log).unwrap().dirty());

        let answer = (!cancel).then_some(PromptValue::Bool(true));
        let completion = test
            .session
            .finish_document_prompt(&request.prompt_id, answer)
            .unwrap();

        assert!(!test.session.has_pending_prompt());
        let resumed: BTreeSet<&str> = completion
            .result
            .opened
            .iter()
            .map(|(render, _)| render.view_id.as_str())
            .collect();
        assert_eq!(
            resumed,
            BTreeSet::from([status.as_str(), log.as_str()]),
            "both repository views resume after cancel={cancel}"
        );
        for (view_id, before) in [(&status, status_version), (&log, log_version)] {
            let view = test.session.views().get(view_id).expect("view stays open");
            assert!(
                view.version() > before,
                "{view_id} resumes its held refresh after cancel={cancel}"
            );
            assert!(!view.dirty(), "{view_id} has no queued refresh left");
        }
    }
}

#[test]
fn a_held_refresh_keeps_text_mode_edits_queued_until_text_mode_ends() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    let status = test.view_id().to_string();
    let (menu, _) = test.open_via_command("open.commit");
    test.ack();
    let menu = menu.id;

    test.focus(&status);
    test.goto_first_marker('+');
    test.set_mode(ViewMode::Text);
    let status_version = test.session.views().get(&status).unwrap().version();
    let status_text = test.text_of(&status);

    let request = stage_author_prompt(&mut test, &menu);
    test.session.open_document_prompt(&request).unwrap();
    assert!(
        test.session.refresh_all().is_empty(),
        "an open prompt holds every refresh"
    );
    git(tmp.path(), &["add", "f.txt"]);

    let completion = test
        .session
        .finish_document_prompt(&request.prompt_id, None)
        .unwrap();

    assert!(!test.session.has_pending_prompt());
    let status_view = test
        .session
        .views()
        .get(&status)
        .expect("the text-mode status stays open");
    assert_eq!(status_view.mode(), ViewMode::Text);
    assert_eq!(
        status_view.version(),
        status_version,
        "a text-mode view is not replaced"
    );
    assert!(status_view.dirty(), "its queued refresh survives the prompt");
    assert_eq!(
        test.text_of(&status),
        status_text,
        "text-mode edits stay intact"
    );
    assert!(
        completion
            .result
            .opened
            .iter()
            .all(|(render, _)| render.view_id == menu),
        "only the normal-mode menu resumes: {:?}",
        completion
            .result
            .opened
            .iter()
            .map(|(render, _)| &render.view_id)
            .collect::<Vec<_>>()
    );

    test.focus(&status);
    test.press("ctrl+t");
    let lines = test.text();
    assert!(
        lines.iter().any(|line| line == "Staged changes (1)"),
        "ending text mode applies the refresh queued during the prompt: {lines:?}"
    );
}

#[test]
fn closing_a_document_prompt_returns_every_surviving_refresh() {
    for close_origin in [false, true] {
        let (tmp, _) = RepoFixture::new()
            .commit("init", [("f.txt", TEN_LINES)])
            .modify_standard()
            .build();
        let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
        test.ack();
        let status = test.view_id().to_string();
        let (log, _) = test.open_via_command("open.log");
        test.ack();
        let log = log.id;
        let request = stage_commit_confirm(&mut test, &status);
        test.session.open_document_prompt(&request).unwrap();
        let prompt_id = test
            .session
            .prompt_view_id()
            .expect("the open prompt view id")
            .to_string();
        let status_version = test.session.views().get(&status).unwrap().version();
        let log_version = test.session.views().get(&log).unwrap().version();

        assert!(
            test.session.refresh_all().is_empty(),
            "an open prompt holds every refresh"
        );
        assert!(test.session.views().get(&status).unwrap().dirty());
        assert!(test.session.views().get(&log).unwrap().dirty());

        let target = if close_origin {
            &status
        } else {
            &prompt_id
        };
        let result = test.session.close(target).unwrap();

        assert!(!test.session.has_pending_prompt());
        assert!(
            test.session.views().get(&prompt_id).is_none(),
            "the prompt view closes after close_origin={close_origin}"
        );
        let expected: BTreeSet<&str> = if close_origin {
            assert!(test.session.views().get(&status).is_none());
            BTreeSet::from([log.as_str()])
        } else {
            BTreeSet::from([status.as_str(), log.as_str()])
        };
        let resumed: BTreeSet<&str> = result
            .opened
            .iter()
            .map(|(render, _)| render.view_id.as_str())
            .collect();
        assert_eq!(
            resumed, expected,
            "close returns every surviving refresh after close_origin={close_origin}"
        );
        for (view_id, before) in [(&status, status_version), (&log, log_version)] {
            let Some(view) = test.session.views().get(view_id) else {
                assert!(close_origin, "{view_id} survives unless it was the origin");
                continue;
            };
            assert!(
                view.version() > before,
                "{view_id} returns a fresh render after close_origin={close_origin}"
            );
            assert!(!view.dirty(), "{view_id} has no queued refresh left");
            assert!(
                result
                    .opened
                    .iter()
                    .any(|(render, _)| render.view_id == view_id.as_str() && render.version > before),
                "{view_id} has a close render above its held version"
            );
        }
    }
}

#[test]
fn closing_a_document_prompt_keeps_text_mode_refreshes_queued() {
    for close_origin in [false, true] {
        let (tmp, _) = RepoFixture::new()
            .commit("init", [("f.txt", TEN_LINES)])
            .modify_standard()
            .build();
        let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
        test.ack();
        let status = test.view_id().to_string();
        let (menu, _) = test.open_via_command("open.commit");
        test.ack();
        let menu = menu.id;

        test.focus(&status);
        test.goto_first_marker('+');
        test.set_mode(ViewMode::Text);
        let status_version = test.session.views().get(&status).unwrap().version();
        let status_text = test.text_of(&status);

        let request = stage_author_prompt(&mut test, &menu);
        test.session.open_document_prompt(&request).unwrap();
        let prompt_id = test
            .session
            .prompt_view_id()
            .expect("the open prompt view id")
            .to_string();
        assert!(
            test.session.refresh_all().is_empty(),
            "an open prompt holds every refresh"
        );
        git(tmp.path(), &["add", "f.txt"]);

        let target = if close_origin { &menu } else { &prompt_id };
        let result = test.session.close(target).unwrap();

        assert!(!test.session.has_pending_prompt());
        assert!(
            result
                .opened
                .iter()
                .all(|(render, _)| render.view_id != status),
            "the text-mode status refresh stays queued after close_origin={close_origin}"
        );
        let status_view = test
            .session
            .views()
            .get(&status)
            .expect("the text-mode status stays open");
        assert_eq!(status_view.mode(), ViewMode::Text);
        assert_eq!(
            status_view.version(),
            status_version,
            "a text-mode view is not replaced by close_origin={close_origin}"
        );
        assert!(
            status_view.dirty(),
            "its queued refresh survives close_origin={close_origin}"
        );
        assert_eq!(
            test.text_of(&status),
            status_text,
            "text-mode edits stay intact after close_origin={close_origin}"
        );
        if close_origin {
            assert!(test.session.views().get(&menu).is_none());
        } else {
            assert!(test.session.views().get(&menu).is_some(), "origin stays open");
        }

        test.focus(&status);
        test.press("ctrl+t");
        let lines = test.text();
        assert!(
            lines.iter().any(|line| line == "Staged changes (1)"),
            "ending text mode applies the refresh queued through close_origin={close_origin}: {lines:?}"
        );
    }
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
fn a_ref_option_rejects_an_unknown_ref_or_remote() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    let (menu, _) = test.open_via_command("open.branch");
    let request = stage_ref_prompt(&mut test, &menu.id);
    let depth = test.session().views().iter().count();
    test.session.open_document_prompt(&request).unwrap();
    let prompt_id = test
        .session
        .prompt_view_id()
        .expect("the open prompt view id")
        .to_string();

    let document = prompt_document(&test, &prompt_id, Some("no-such-ref"));
    let completion = test
        .session
        .answer_document_prompt(&format!("prompt:{}:submit", request.prompt_id), &document)
        .unwrap();

    let magit_lsp::session::Execution::Done(result) = completion.next else {
        panic!("the rejected answer ends the prompt");
    };
    assert_eq!(
        result.effects,
        vec![Effect::message(
            magit_lsp::protocol::MessageLevel::Warn,
            "Unknown ref or remote: no-such-ref".to_string()
        )]
    );
    assert_eq!(test.session().views().iter().count(), depth);
    assert_eq!(option_value(&test, &menu.id, "=u"), None);
    assert!(!test.session.prompt_active());
    assert_eq!(test.view_id(), menu.id);
}

#[test]
fn a_ref_option_stages_a_pick_with_ref_items() {
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    let (menu, _) = test.open_via_command("open.branch");
    let request = stage_option_pick(&mut test, &menu.id, "u");

    let pick = test
        .session
        .pending_pick_request()
        .expect("the staged pick");
    assert!(
        pick.items.iter().any(|item| item.id == "main"),
        "refs: {:?}",
        pick.items
    );
    assert!(pick.allow_free_text);
    assert!(!pick.pick_id.is_empty());
    assert_eq!(request.kind, PromptKind::Choice);
    assert_eq!(request.allow_custom, Some(true));
    assert!(request
        .items
        .as_ref()
        .expect("the choice items")
        .iter()
        .any(|item| item.id == "main"));

    test.session.open_document_prompt(&request).unwrap();
    let prompt_id = test
        .session
        .prompt_view_id()
        .expect("the open prompt view id")
        .to_string();
    let document = prompt_document(&test, &prompt_id, None);
    let completion = test
        .session
        .answer_document_prompt(&format!("prompt:{}:item:main", request.prompt_id), &document)
        .unwrap();
    assert!(matches!(completion.next, Execution::Done(_)));
    assert_eq!(option_value(&test, &menu.id, "=u").as_deref(), Some("main"));
    assert!(!test.session.prompt_active());
}

#[test]
fn a_remote_option_lists_remotes() {
    use magit_lsp::menu::Reader;
    use magit_lsp::session::Execution;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    tmp.git(&["remote", "add", "origin", "https://example.test/repo.git"]);
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    let (menu, _) = test.open_via_command("open.branch");
    let request = stage_option_pick(&mut test, &menu.id, "p");

    let pick = test
        .session
        .pending_pick_request()
        .expect("the staged pick");
    assert!(
        pick.items.iter().any(|item| item.id == "origin"),
        "remotes: {:?}",
        pick.items
    );

    test.session.open_document_prompt(&request).unwrap();
    let prompt_id = test
        .session
        .prompt_view_id()
        .expect("the open prompt view id")
        .to_string();
    let document = prompt_document(&test, &prompt_id, None);
    let completion = test
        .session
        .answer_document_prompt(
            &format!("prompt:{}:item:origin", request.prompt_id),
            &document,
        )
        .unwrap();
    assert!(matches!(completion.next, Execution::Done(_)));
    assert_eq!(option_value(&test, &menu.id, "=p").as_deref(), Some("origin"));

    assert!(test
        .session()
        .validate_option_value(Reader::Remote, "origin")
        .is_ok());
    assert!(test
        .session()
        .validate_option_value(Reader::Remote, "nope")
        .unwrap_err()
        .contains("Unknown remote"));
}

#[test]
fn revision_option_values_validate_against_the_repository() {
    use magit_lsp::menu::Reader;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let (test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    let head = tmp.git(&["rev-parse", "HEAD"]).trim().to_string();

    assert!(test
        .session()
        .validate_option_value(Reader::Ref, "main")
        .is_ok());
    assert!(test
        .session()
        .validate_option_value(Reader::Ref, "HEAD")
        .is_ok());
    assert!(test
        .session()
        .validate_option_value(Reader::Ref, "no-such-ref")
        .unwrap_err()
        .contains("Unknown ref or remote"));
    assert!(test
        .session()
        .validate_option_value(Reader::Commit, &head)
        .is_ok());
    assert!(test
        .session()
        .validate_option_value(Reader::Commit, "0000000000000000000000000000000000000000")
        .unwrap_err()
        .contains("Unknown commit"));
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
fn a_commit_message_option_picks_a_log_commit_into_the_menu_option() {
    use magit_lsp::protocol::Cursor;
    use magit_lsp::tree::NodeKind;
    use magit_lsp::view::ViewKind;
    let (tmp, _repo) = RepoFixture::new()
        .commit("old commit", [("f.txt", "old\n")])
        .commit("new commit", [("f.txt", "new\n")])
        .build();
    let expected = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_string();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    let (menu, _) = test.open_via_command("open.commit");
    test.ack();

    test.press_ui("=");
    test.press_ui("C");
    let log_id = test.view_id().to_string();
    let log = test
        .session()
        .views()
        .get(&log_id)
        .expect("the selection log");
    assert_eq!(log.descriptor().kind, ViewKind::Log);
    let text = test.text_of(&log_id).join("\n");
    assert!(
        text.contains("Select a commit for --reuse-message="),
        "{text}"
    );
    assert!(log
        .tree()
        .nodes
        .iter()
        .any(|node| node.kind == NodeKind::Commit));
    assert!(!test.session.prompt_active());

    test.ack();
    let node = test
        .session()
        .views()
        .get(&log_id)
        .unwrap()
        .tree()
        .nodes
        .iter()
        .find(|node| node.commit.as_deref() == Some(&expected))
        .expect("the HEAD~1 commit row")
        .start_line;
    let result = test.invoke_at(
        "common.confirm",
        Cursor {
            line: node,
            column: 0,
        },
    );
    assert!(
        result
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::PopView { view_id } if view_id == &log_id)),
        "{:?}",
        result.effects
    );
    assert_eq!(test.view_id(), menu.id);
    assert_eq!(
        option_value(&test, &menu.id, "=C").as_deref(),
        Some(expected.as_str())
    );
}

#[test]
fn a_stale_commit_option_is_refused_before_dispatch() {
    use magit_lsp::protocol::Cursor;
    use magit_lsp::tree::NodeKind;
    use magit_lsp::view::ViewKind;
    let (tmp, _repo) = RepoFixture::new()
        .commit("old commit", [("f.txt", "old\n")])
        .commit("middle commit", [("f.txt", "middle\n")])
        .commit("new commit", [("f.txt", "new\n")])
        .build();
    let expected = tmp.git(&["rev-parse", "HEAD"]).trim().to_string();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    let (menu, _) = test.open_via_command("open.commit");
    test.ack();

    test.press_ui("=");
    test.press_ui("C");
    let log_id = test.view_id().to_string();
    let log = test
        .session()
        .views()
        .get(&log_id)
        .expect("the selection log");
    assert_eq!(log.descriptor().kind, ViewKind::Log);
    let text = test.text_of(&log_id).join("\n");
    assert!(
        text.contains("Select a commit for --reuse-message="),
        "{text}"
    );
    assert!(log
        .tree()
        .nodes
        .iter()
        .any(|node| node.kind == NodeKind::Commit));
    assert!(!test.session.prompt_active());

    test.ack();
    let node = test
        .session()
        .views()
        .get(&log_id)
        .unwrap()
        .tree()
        .nodes
        .iter()
        .find(|node| node.commit.as_deref() == Some(&expected))
        .expect("the HEAD commit row")
        .start_line;
    let result = test.invoke_at(
        "common.confirm",
        Cursor {
            line: node,
            column: 0,
        },
    );
    assert!(
        result
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::PopView { view_id } if view_id == &log_id)),
        "{:?}",
        result.effects
    );
    assert_eq!(test.view_id(), menu.id);
    assert_eq!(
        option_value(&test, &menu.id, "=C").as_deref(),
        Some(expected.as_str())
    );

    tmp.git(&["reset", "--hard", "HEAD~1"]);
    tmp.git(&["reflog", "expire", "--expire=now", "--all"]);
    tmp.git(&["gc", "--prune=now"]);
    let head = tmp.git(&["rev-parse", "HEAD"]).trim().to_string();
    let count = tmp.git(&["rev-list", "--count", "HEAD"]).trim().to_string();

    let result = test.press_ui("c");

    assert_eq!(result.effects.len(), 1, "{:?}", result.effects);
    let Effect::Message { level, text, .. } = &result.effects[0] else {
        panic!("expected one warn message, got {:?}", result.effects);
    };
    assert_eq!(*level, magit_lsp::protocol::MessageLevel::Warn);
    assert!(text.contains("is no longer valid"), "{text}");
    assert!(!result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::PopView { .. })));
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]).trim(), head);
    assert_eq!(tmp.git(&["rev-list", "--count", "HEAD"]).trim(), count);
    assert_eq!(test.view_id(), menu.id);
}

#[test]
fn cancelling_the_commit_message_log_leaves_the_option_unset() {
    use magit_lsp::view::ViewKind;
    let (tmp, _repo) = RepoFixture::new()
        .commit("old commit", [("f.txt", "old\n")])
        .commit("new commit", [("f.txt", "new\n")])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    let (menu, _) = test.open_via_command("open.commit");
    test.ack();

    test.press_ui("=");
    test.press_ui("c");
    let log_id = test.view_id().to_string();
    assert_eq!(
        test.session().views().get(&log_id).unwrap().descriptor().kind,
        ViewKind::Log
    );

    let result = test.invoke("common.cancel");
    assert!(
        result
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::PopView { view_id } if view_id == &log_id)),
        "{:?}",
        result.effects
    );
    assert_eq!(test.view_id(), menu.id);
    assert_eq!(option_value(&test, &menu.id, "=c"), None);
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
