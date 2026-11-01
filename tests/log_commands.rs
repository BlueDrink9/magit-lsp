//! Opening the current log from its transient menu.

mod common;

use common::*;
use magit_lsp::config::Profile;
use magit_lsp::view::ViewKind;

#[test]
fn current_log_opens_and_quit_returns_to_status() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("first log subject", [("f.txt", TEN_LINES)])
        .commit("second log subject", [("f.txt", "changed\n")])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();

    test.press("g l");
    test.press("l");
    let log_id = test.view_id().to_string();
    let descriptor = test.session().views().get(&log_id).unwrap().descriptor();
    assert_eq!(descriptor.kind, ViewKind::Log);
    let text = test.text_of(&log_id).join("\n");
    assert!(text.contains("first log subject"), "{text}");
    assert!(text.contains("second log subject"), "{text}");

    let returned = test.invoke("common.quit");
    assert!(!returned.effects.is_empty());
    test.focus("v1");
    assert!(test.session().views().get("v1").is_some());
}

#[test]
fn log_menu_opens_log_and_q_returns_to_status_in_each_profile() {
    for (profile, opener) in [(Profile::Standard, "l"), (Profile::Vim, "g l")] {
        let (tmp, _repo) = RepoFixture::new()
            .commit("first log subject", [("f.txt", TEN_LINES)])
            .build();
        let (mut test, _) = TestSession::open_status_at(&tmp, profile, WIDE_VIEWPORT);
        test.ack();
        test.press(opener);
        let menu_id = test.view_id().to_string();
        assert!(
            test.session()
                .views()
                .get(&menu_id)
                .unwrap()
                .descriptor()
                .capture
        );
        test.press("l");
        let log_id = test.view_id().to_string();
        let descriptor = test.session().views().get(&log_id).unwrap().descriptor();
        assert_eq!(descriptor.kind, ViewKind::Log);
        assert!(!descriptor.capture);

        let outcome = test.press("q");
        assert!(outcome.effects.iter().any(
            |effect| matches!(effect, magit_lsp::protocol::Effect::PopView { view_id } if view_id == &log_id)
        ));
        assert!(outcome.effects.iter().any(
            |effect| matches!(effect, magit_lsp::protocol::Effect::FocusView { view_id } if view_id == "v1")
        ));
    }
}

#[test]
fn path_filter_is_reachable_through_individual_menu_keys() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("first target change", [("target.txt", "one\n")])
        .commit("unrelated change", [("other.txt", "other\n")])
        .commit("second target change", [("target.txt", "two\n")])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Standard, WIDE_VIEWPORT);

    test.press("l");
    test.ui().reply_text(Some("target.txt".into()));
    test.press_ui("=");
    test.press_ui("f");
    test.press("l");

    let log_id = test.view_id().to_string();
    let text = test.text_of(&log_id).join("\n");
    assert!(text.contains("first target change"), "{text}");
    assert!(text.contains("second target change"), "{text}");
    assert!(!text.contains("unrelated change"), "{text}");
}

#[test]
fn log_native_search_and_motion_keys_pass_through_and_are_unbound_in_select_scope() {
    use magit_lsp::protocol::Effect;

    for profile in [Profile::Standard, Profile::Vim] {
        let (tmp, _repo) = RepoFixture::new()
            .commit("log entry", [("f.txt", TEN_LINES)])
            .build();
        let (mut test, _) = TestSession::open_status_at(&tmp, profile, WIDE_VIEWPORT);
        test.ack();
        test.press(if profile == Profile::Vim { "g l" } else { "l" });
        test.press("l");

        for key in ["/", "?", "n", "p"] {
            let keymap = test.keymaps();
            let log = keymap.get("log").expect("log keymap");
            let select = keymap.get("log.select").expect("log.select keymap");
            for command in ["select.toggle-mark", "select.enter-rev"] {
                assert!(
                    select
                        .bindings
                        .iter()
                        .any(|binding| binding.command == command),
                    "{command} missing from log.select"
                );
            }
            assert!(
                !log.bindings
                    .iter()
                    .any(|binding| binding.keys.iter().any(|keys| keys == key)),
                "{key} bound in log"
            );
            assert!(
                !select
                    .bindings
                    .iter()
                    .any(|binding| binding.keys.iter().any(|keys| keys == key)),
                "{key} bound in log.select"
            );
            let result = test.press(key);
            assert!(
                result.effects.iter().any(
                    |effect| matches!(effect, Effect::PassThrough { key: passed } if passed == key)
                ),
                "{key}: {:?}",
                result.effects
            );
        }
    }
}

#[test]
fn invalid_log_arguments_return_effects_and_keep_the_menu_stack_consistent() {
    use magit_lsp::protocol::Effect;
    let (tmp, _repo) = RepoFixture::new()
        .commit("entry", [("f.txt", "text\n")])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Standard, WIDE_VIEWPORT);
    test.press("l");
    let menu = test.view_id().to_string();
    test.ui().reply_text(Some("0".into()));
    test.press_ui("=");
    test.press_ui("n");
    let result = test.press("l");
    assert!(result.effects.iter().any(|effect| matches!(effect, Effect::Message { level: magit_lsp::protocol::MessageLevel::Error, text, .. } if text.contains("positive integer"))), "{:?}", result.effects);
    assert!(result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::PopView { view_id } if view_id == &menu)));
    assert!(result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::FocusView { view_id } if view_id == "v1")));
    assert!(test.session().views().get(&menu).is_none());
    assert_eq!(test.view_id(), "v1");
    assert!(!test
        .session()
        .views()
        .iter()
        .any(|view| view.descriptor().kind == ViewKind::Log));

    test.press("l");
    let grep_menu = test.view_id().to_string();
    test.ui().reply_text(Some("[".into()));
    test.press_ui("=");
    test.press_ui("g");
    let result = test.press("l");
    assert!(
        result.effects.iter().any(|effect| matches!(
            effect,
            Effect::Message {
                level: magit_lsp::protocol::MessageLevel::Error,
                ..
            }
        )),
        "{:?}",
        result.effects
    );
    assert!(result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::PopView { view_id } if view_id == &grep_menu)));
    assert!(result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::FocusView { view_id } if view_id == "v1")));
    assert_eq!(test.view_id(), "v1");
    assert!(!test
        .session()
        .views()
        .iter()
        .any(|view| view.descriptor().kind == ViewKind::Log));

    test.press("l");
    let retry = test.press("l");
    assert!(retry.opened.iter().any(|(render, _)| test
        .session()
        .views()
        .get(&render.view_id)
        .is_some_and(|view| view.descriptor().kind == ViewKind::Log)));
}
