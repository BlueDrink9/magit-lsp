//! Opening the current log from its transient menu.

use magit_lsp_test_support as common;

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

    test.press("l");
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
fn fixup_selection_confirm_commits_staged_change_for_full_selected_oid() {
    let (tmp, _) = RepoFixture::new()
        .commit("selected target subject", [("f.txt", "base\n")])
        .commit("newer HEAD subject", [("f.txt", "head\n")])
        .staged_file("f.txt", "staged fix\n")
        .build();
    let target = git(tmp.path(), &["rev-parse", "HEAD~1"]).trim().to_string();
    let prior_head = git(tmp.path(), &["rev-parse", "HEAD"]).trim().to_string();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Standard, WIDE_VIEWPORT);
    test.ack();
    let caller = test.view_id().to_string();
    test.invoke("commit.fixup");
    let picker = test.view_id().to_string();
    test.ack();
    let log = test.session().views().get(&picker).unwrap();
    let target_line = log
        .tree()
        .nodes
        .iter()
        .find(|node| node.commit.as_deref() == Some(&target))
        .unwrap_or_else(|| {
            panic!(
                "target {target}; nodes {:?}",
                log.tree()
                    .nodes
                    .iter()
                    .map(|node| (&node.id, node.kind, &node.commit))
                    .collect::<Vec<_>>()
            )
        })
        .start_line;
    test.set_cursor(magit_lsp::protocol::Cursor {
        line: target_line,
        column: 0,
    });
    test.invoke_here("common.confirm");
    assert_eq!(test.view_id(), caller);
    let head = git(tmp.path(), &["rev-parse", "HEAD"]).trim().to_string();
    assert_ne!(head, prior_head);
    assert_eq!(
        git(tmp.path(), &["show", "-s", "--format=%s", "HEAD"]).trim(),
        "fixup! selected target subject"
    );
    assert_eq!(git(tmp.path(), &["rev-parse", "HEAD^"]).trim(), prior_head);
    assert_eq!(
        git(tmp.path(), &["show", "--format=", "--name-only", "HEAD"]).trim(),
        "f.txt"
    );
    assert_eq!(
        git(tmp.path(), &["show", "HEAD:f.txt"]).trim(),
        "staged fix"
    );
    assert_eq!(
        git(tmp.path(), &["show", "--format=%P", "--no-patch", "HEAD"]).trim(),
        prior_head
    );
}

#[test]
fn fixup_selection_cancel_preserves_repo_and_restores_caller() {
    use magit_lsp::protocol::Effect;

    let (tmp, _) = RepoFixture::new()
        .commit("selected target subject", [("f.txt", "base\n")])
        .commit("newer HEAD subject", [("f.txt", "head\n")])
        .staged_file("f.txt", "staged fix\n")
        .build();
    let head = git(tmp.path(), &["rev-parse", "HEAD"]);
    let index = git(tmp.path(), &["write-tree"]);
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Standard, WIDE_VIEWPORT);
    test.ack();
    let caller = test.view_id().to_string();
    test.invoke("commit.fixup");
    let picker = test.view_id().to_string();
    test.ack();

    let result = test.invoke("common.cancel");
    assert!(result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::PopView { view_id } if view_id == &picker)));
    assert!(result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::FocusView { view_id } if view_id == &caller)));
    assert_eq!(test.view_id(), caller);
    assert_eq!(git(tmp.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(git(tmp.path(), &["write-tree"]), index);
}

#[test]
fn fixup_git_failure_reports_message_and_restores_usable_caller() {
    use magit_lsp::protocol::{Effect, MessageLevel};

    let (tmp, _) = RepoFixture::new()
        .commit("selected target subject", [("f.txt", "base\n")])
        .build();
    let head = git(tmp.path(), &["rev-parse", "HEAD"]);
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Standard, WIDE_VIEWPORT);
    test.ack();
    let caller = test.view_id().to_string();
    test.invoke("commit.fixup");
    let picker = test.view_id().to_string();
    test.ack();
    let line = test
        .session()
        .views()
        .get(&picker)
        .unwrap()
        .tree()
        .nodes
        .iter()
        .find(|node| node.kind == magit_lsp::tree::NodeKind::Commit)
        .expect("commit row")
        .start_line;
    let result = test.invoke_at(
        "common.confirm",
        magit_lsp::protocol::Cursor { line, column: 0 },
    );
    assert!(result.effects.iter().any(|effect| matches!(effect, Effect::Message { level: MessageLevel::Error, text, .. } if text.contains("git commit") && text.contains("failed"))), "{:?}", result.effects);
    assert!(result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::FocusView { view_id } if view_id == &caller)));
    assert_eq!(test.view_id(), caller);
    assert!(test.session().views().get(&caller).is_some());
    assert_eq!(git(tmp.path(), &["rev-parse", "HEAD"]), head);
}

#[test]
fn log_menu_opens_log_and_q_returns_to_status_in_each_profile() {
    for (profile, opener) in [(Profile::Standard, "l"), (Profile::Vim, "l")] {
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
fn branch_variants_open_distinct_visible_logs_from_menu_keys() {
    let (tmp, _) = log_variants_fixture();
    for (key, extra) in [
        ("l", vec![]),
        ("L", vec!["local only"]),
        ("b", vec!["local only", "remote only"]),
        ("a", vec!["local only", "remote only", "tag only"]),
    ] {
        let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Standard, WIDE_VIEWPORT);
        test.press("l");
        test.press(key);
        let log = test.session().views().get(test.view_id()).unwrap();
        assert_eq!(log.descriptor().kind, ViewKind::Log);
        let text = test.text_of(test.view_id()).join("\n");
        let expected: Vec<_> = [
            "base history",
            "main history",
            "side history",
            "merge history",
        ]
        .into_iter()
        .chain(extra)
        .collect();
        for subject in [
            "base history",
            "main history",
            "side history",
            "merge history",
            "local only",
            "remote only",
            "tag only",
        ] {
            assert_eq!(
                text.contains(subject),
                expected.contains(&subject),
                "{key}: {subject}: {text}"
            );
        }
        assert!(text.contains("main"), "{text}");
        assert!(text.contains('*'), "{text}");
        assert_eq!(
            log.tree()
                .nodes
                .iter()
                .filter(|n| n.commit.is_some())
                .count(),
            expected.len()
        );
    }
}

#[test]
fn history_filters_and_composition_reach_visible_log_through_menu() {
    let (tmp, _) = log_variants_fixture();
    for (switches, until, compose, expected) in [
        (
            vec![],
            true,
            false,
            vec!["base history", "main history", "side history"],
        ),
        (vec!["M"], false, false, vec!["merge history"]),
        (
            vec!["m"],
            false,
            false,
            vec!["base history", "main history", "side history"],
        ),
        (
            vec!["p"],
            false,
            false,
            vec!["base history", "main history", "merge history"],
        ),
        (vec!["p", "m"], false, true, vec!["main history"]),
    ] {
        let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Standard, WIDE_VIEWPORT);
        test.press("l");
        for switch in switches {
            test.press("=");
            test.press(switch);
        }
        let mut options = vec![];
        if until {
            options.push(("u", "2000-01-01T23:59:59+0000"));
        }
        if compose {
            options.extend([("a", "Test User"), ("g", "main history"), ("f", "main.txt")]);
        }
        for (key, value) in options {
            test.ui().reply_text(Some(value.into()));
            test.press_ui("=");
            test.press_ui(key);
        }
        test.press("l");
        let log = test.session().views().get(test.view_id()).unwrap();
        assert_eq!(log.descriptor().kind, ViewKind::Log);
        let text = test.text_of(test.view_id()).join("\n");
        for subject in [
            "base history",
            "main history",
            "side history",
            "merge history",
            "local only",
            "remote only",
            "tag only",
        ] {
            assert_eq!(
                text.contains(subject),
                expected.contains(&subject),
                "{subject}: {text}"
            );
        }
        assert_eq!(
            log.tree()
                .nodes
                .iter()
                .filter(|n| n.commit.is_some())
                .count(),
            expected.len()
        );
    }
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
        test.press("l");
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

#[test]
fn contradictory_merge_filters_report_error_and_leave_menu_usable() {
    use magit_lsp::protocol::{Effect, MessageLevel};

    let (tmp, _) = log_variants_fixture();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Standard, WIDE_VIEWPORT);
    test.press("l");
    let menu = test.view_id().to_string();
    for key in ["M", "m"] {
        test.press("=");
        test.press_ui(key);
    }
    let result = test.press("l");
    assert!(result.effects.iter().any(|effect| matches!(effect, Effect::Message { level: MessageLevel::Error, text, .. } if text.contains("cannot include and exclude merges"))), "{:?}", result.effects);
    assert!(!result.effects.iter().any(|effect| matches!(effect, Effect::PushView { descriptor, .. } if descriptor.kind == ViewKind::Log)), "{:?}", result.effects);
    assert_ne!(test.view_id(), menu);
    assert_eq!(test.view_id(), "v1");
    assert!(test.session().views().get(test.view_id()).is_some());

    test.press("l");
    test.press("=");
    test.press_ui("m");
    test.press("l");
    assert_eq!(
        test.session()
            .views()
            .get(test.view_id())
            .unwrap()
            .descriptor()
            .kind,
        ViewKind::Log
    );
}
