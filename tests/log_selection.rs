//! Server-owned commit selection and copy lifecycle.

mod common;

use common::*;
use magit_lsp::config::Profile;
use magit_lsp::protocol::Effect;
use serde_json::json;

#[test]
fn select_and_copy_uses_structured_commit_identity_and_cancel_is_read_only() {
    for profile in [Profile::Standard, Profile::Vim] {
        let (tmp, _repo) = RepoFixture::new()
            .commit("old commit", [("f.txt", "old\n")])
            .commit("new commit", [("f.txt", "new\n")])
            .build();
        let before = tmp.git(&["rev-parse", "HEAD"]).trim().to_string();
        let (mut test, _) = TestSession::open_status_at(&tmp, profile, WIDE_VIEWPORT);
        test.ack();
        test.invoke("log.select-and-copy");
        let log_id = test.view_id().to_string();
        test.ack();
        let log = test.session().views().get(&log_id).unwrap();
        assert!(test.text_of(&log_id).join("\n").contains("copy commits"));
        assert!(log.descriptor().capture == false);
        let old = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_string();
        let node = log
            .tree()
            .nodes
            .iter()
            .find(|node| node.commit.as_deref() == Some(&old))
            .unwrap();
        let result = test.invoke_at(
            "common.confirm",
            magit_lsp::protocol::Cursor {
                line: node.start_line,
                column: 0,
            },
        );
        assert!(result
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Clipboard { text } if text == &old)));
        assert!(result
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::PopView { view_id } if view_id == &log_id)));
        assert!(result
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::FocusView { view_id } if view_id == "v1")));
        assert_eq!(tmp.git(&["rev-parse", "HEAD"]).trim(), before);
    }
}

#[test]
fn log_menu_options_apply_to_selection_picker_and_copy_full_oid() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("matching result", [("f.txt", "one\n")])
        .commit("unrelated entry", [("f.txt", "two\n")])
        .build();
    let expected = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_string();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Standard, WIDE_VIEWPORT);
    test.ack();
    test.press("l");
    let menu_id = test.view_id().to_string();
    test.ui().reply_text(Some("matching result".into()));
    test.press_ui("=");
    test.press_ui("g");
    test.press("-");
    test.press_ui("g");
    test.press_ui("-");
    test.press_ui("d");
    test.press("c");
    let id = test.view_id().to_string();
    test.ack();
    let view = test.session().views().get(&id).unwrap();
    let text = test.text_of(&id).join("\n");
    assert!(text.contains("matching result"), "{text}");
    assert!(!text.contains("unrelated entry"), "{text}");
    assert!(view
        .tree()
        .nodes
        .iter()
        .any(|node| node.kind == magit_lsp::tree::NodeKind::Commit
            && node.commit.as_deref() == Some(&expected)));
    assert!(!text.contains("refs/"), "{text}");
    let node = view
        .tree()
        .nodes
        .iter()
        .find(|node| node.commit.as_deref() == Some(&expected))
        .unwrap();
    let result = test.invoke_at(
        "common.confirm",
        magit_lsp::protocol::Cursor {
            line: node.start_line,
            column: 0,
        },
    );
    assert!(result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::Clipboard { text } if text == &expected)));
    assert!(result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::FocusView { view_id } if view_id == "v1")));
    assert_ne!(menu_id, id);
}

#[test]
fn free_text_revision_resolves_to_full_oid_and_copies_outside_page() {
    let mut fixture = RepoFixture::new();
    for i in 0..105 {
        fixture = fixture.commit(&format!("commit {i}"), [("f.txt", format!("{i}\n"))]);
    }
    let (tmp, _repo) = fixture.build();
    let expected = tmp.git(&["rev-parse", "HEAD~104"]).trim().to_string();
    let before = tmp.git(&["rev-parse", "HEAD"]).trim().to_string();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Standard, WIDE_VIEWPORT);
    test.ack();
    test.invoke_args("log.select-and-copy", json!({"allowFreeText": true}));
    let picker_id = test.view_ids().last().unwrap().clone();
    let picker = test.session().views().get(&picker_id).unwrap();
    assert!(!picker
        .tree()
        .nodes
        .iter()
        .any(|node| node.commit.as_deref() == Some(&expected)));
    test.ui.reply_text(Some("HEAD~104".to_string()));
    let result = test.invoke_ui("select.enter-rev");
    assert!(result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::Clipboard { text } if text == &expected)));
    assert!(!test.view_ids().contains(&picker_id));
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]).trim(), before);
}

#[test]
fn plus_extends_log_page_and_preserves_commit_identity() {
    let mut fixture = RepoFixture::new();
    for i in 0..105 {
        fixture = fixture.commit(&format!("commit {i}"), [("f.txt", format!("{i}\n"))]);
    }
    let (tmp, _repo) = fixture.build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Standard, WIDE_VIEWPORT);
    test.ack();
    test.invoke("log.current");
    test.ack();
    let id = test.view_id().to_string();
    let before: Vec<_> = test
        .tree()
        .nodes
        .iter()
        .filter_map(|n| n.commit.clone())
        .collect();
    assert_eq!(before.len(), 100);
    assert!(test.text_of(&id).join("\n").contains("load more"));
    test.press("Plus");
    test.ack();
    let view = test.session().views().get(&id).unwrap();
    let after: Vec<_> = view
        .tree()
        .nodes
        .iter()
        .filter_map(|n| n.commit.clone())
        .collect();
    assert_eq!(after.len(), 105);
    assert!(before.iter().all(|oid| after.contains(oid)));
    assert!(!test.text_of(&id).join("\n").contains("load more"));
}

#[test]
fn multi_selection_copies_marked_full_oids_in_log_order_and_restores_caller() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("old commit", [("f.txt", "old\n")])
        .commit("middle commit", [("f.txt", "middle\n")])
        .commit("new commit", [("f.txt", "new\n")])
        .build();
    let expected = ["HEAD~2", "HEAD~1"].map(|rev| tmp.git(&["rev-parse", rev]).trim().to_string());
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Standard, WIDE_VIEWPORT);
    test.ack();
    test.invoke_args("log.select-and-copy", json!({"multi": true}));
    test.ack();
    let picker_id = test.view_id().to_string();
    let lines: Vec<_> = test
        .tree()
        .nodes
        .iter()
        .filter(|node| {
            node.commit
                .as_deref()
                .is_some_and(|oid| expected.contains(&oid.to_string()))
        })
        .map(|node| (node.start_line, node.commit.clone().unwrap()))
        .collect();
    let version = test.acked_version().expect("acknowledged coordinates");
    for (line, _) in &lines {
        test.invoke_at_version(
            "select.toggle-mark",
            version,
            Some(magit_lsp::protocol::Cursor {
                line: *line,
                column: 0,
            }),
            None,
            None,
        );
    }
    let result = test.invoke_at_version(
        "common.confirm",
        version,
        Some(magit_lsp::protocol::Cursor {
            line: lines[0].0,
            column: 0,
        }),
        None,
        None,
    );
    let ordered: Vec<_> = lines.iter().map(|(_, oid)| oid.clone()).collect();
    assert!(result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::Clipboard { text } if text == &ordered.join("\n"))));
    assert!(result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::PopView { view_id } if view_id == &picker_id)));
    assert!(result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::FocusView { view_id } if view_id == "v1")));
    assert_eq!(test.view_id(), "v1");
}

#[test]
fn confirming_noncommit_header_keeps_picker_open_without_clipboard() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("commit", [("f.txt", "content\n")])
        .build();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Standard, WIDE_VIEWPORT);
    test.ack();
    test.invoke("log.select-and-copy");
    test.ack();
    let picker_id = test.view_id().to_string();
    let header = test
        .tree()
        .nodes
        .iter()
        .find(|node| node.commit.is_none())
        .unwrap()
        .start_line;
    let result = test.invoke_at(
        "common.confirm",
        magit_lsp::protocol::Cursor {
            line: header,
            column: 0,
        },
    );
    assert!(!result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::Clipboard { .. })));
    assert!(test.view_ids().contains(&picker_id));
}

fn add_new_head(tmp: &common::RepoFixture) {
    common::write(&tmp.path().join("f.txt"), "reordered\n");
    tmp.git(&["add", "f.txt"]);
    tmp.git(&["commit", "-m", "reordered"]);
}

#[test]
fn positional_selection_requires_ack_and_never_uses_reordered_unacked_tree() {
    for via_key in [false, true] {
        let (tmp, _repo) = RepoFixture::new()
            .commit("old", [("f.txt", "old\n")])
            .commit("new", [("f.txt", "new\n")])
            .build();
        let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Standard, WIDE_VIEWPORT);
        test.ack();
        test.invoke_args("log.select-and-copy", json!({"multi": true}));
        let picker = test.view_id().to_string();
        let old_line = test
            .tree()
            .nodes
            .iter()
            .find(|n| n.commit.is_some())
            .unwrap()
            .start_line;
        add_new_head(&tmp);
        test.refresh();
        let result = if via_key {
            test.press_at_version("Enter", test.version())
        } else {
            test.invoke_at(
                "common.confirm",
                magit_lsp::protocol::Cursor {
                    line: old_line,
                    column: 0,
                },
            )
        };
        assert!(!result
            .effects
            .iter()
            .any(|e| matches!(e, Effect::Clipboard { .. })));
        assert!(test.view_ids().contains(&picker));
        let mark = test.invoke_at(
            "select.toggle-mark",
            magit_lsp::protocol::Cursor {
                line: old_line,
                column: 0,
            },
        );
        assert!(!mark
            .effects
            .iter()
            .any(|e| matches!(e, Effect::Clipboard { .. })));
        assert!(test.view_ids().contains(&picker));
    }
}

#[test]
fn acknowledged_selection_identity_survives_reordered_unacked_refresh() {
    for via_key in [false, true] {
        let (tmp, _repo) = RepoFixture::new()
            .commit("old", [("f.txt", "old\n")])
            .commit("new", [("f.txt", "new\n")])
            .build();
        let expected = tmp.git(&["rev-parse", "HEAD"]).trim().to_string();
        let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Standard, WIDE_VIEWPORT);
        test.ack();
        test.invoke("log.select-and-copy");
        test.ack();
        let line = test
            .acked_tree()
            .nodes
            .iter()
            .find(|n| n.commit.as_deref() == Some(&expected))
            .unwrap()
            .start_line;
        let version = test.acked_version().expect("acknowledged coordinates");
        test.goto(line);
        add_new_head(&tmp);
        test.refresh();
        let result = if via_key {
            test.press_at_version("Enter", version)
        } else {
            test.invoke_at_version(
                "common.confirm",
                version,
                Some(magit_lsp::protocol::Cursor { line, column: 0 }),
                None,
                None,
            )
        };
        assert!(
            result
                .effects
                .iter()
                .any(|e| matches!(e, Effect::Clipboard { text } if text == &expected)),
            "via_key={via_key}, effects={:?}",
            result.effects
        );
    }
}

#[test]
fn selection_rejects_current_unacked_and_discarded_ack_versions() {
    use magit_lsp::errors;
    use magit_lsp::protocol::Cursor;

    for via_key in [false, true] {
        for acknowledge_new in [false, true] {
            let (tmp, _repo) = RepoFixture::new()
                .commit("old", [("f.txt", "old\n")])
                .commit("new", [("f.txt", "new\n")])
                .build();
            let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Standard, WIDE_VIEWPORT);
            test.ack();
            test.invoke_args("log.select-and-copy", json!({"multi": true}));
            test.ack();
            let picker = test.view_id().to_string();
            let version = test.acked_version().expect("acknowledged coordinates");
            let snapshot = test.acked_tree().clone();
            let node = snapshot
                .nodes
                .iter()
                .find(|node| node.commit.is_some())
                .unwrap();
            let line = node.start_line;
            let old_oid = node.commit.clone();
            test.goto(line);
            add_new_head(&tmp);
            let head = tmp.git(&["rev-parse", "HEAD"]);
            let index = tmp.git(&["ls-files", "--stage"]);
            test.refresh().expect("reordered render");
            let replacement = test.tree().line_nodes[line].node.clone();
            assert_ne!(test.tree().node(&replacement).unwrap().commit, old_oid);
            assert_eq!(
                test.session().views().get(&picker).unwrap().acked_tree(),
                Some(&snapshot)
            );
            let request_version = if acknowledge_new {
                test.ack();
                version // Old coordinates cannot bind to a newer ack.
            } else {
                test.version() // Even exact current coordinates need an ack here.
            };
            let ack_snapshot = test.acked_tree().clone();
            for (command, key) in [("select.toggle-mark", "Space"), ("common.confirm", "Enter")] {
                let result = if via_key {
                    test.press_at_version(key, request_version)
                } else {
                    test.invoke_at_version(
                        command,
                        request_version,
                        Some(Cursor { line, column: 0 }),
                        None,
                        None,
                    )
                };
                assert_eq!(message_texts(&result.effects), vec![errors::STALE_VIEW]);
                assert!(result.render.is_none());
                let view = test
                    .session()
                    .views()
                    .get(&picker)
                    .expect("picker remains open");
                assert!(
                    view.selected_commits(None).is_empty(),
                    "refused mark must not mutate selection"
                );
                assert_eq!(view.acked_tree(), Some(&ack_snapshot));
                assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
                assert_eq!(tmp.git(&["ls-files", "--stage"]), index);
                assert_eq!(tmp.git(&["status", "--short"]), "");
            }
            // Selection scope alone must not gate non-positional actions.
            let refreshed = test.invoke_at_version(
                "common.refresh",
                0,
                Some(Cursor { line, column: 0 }),
                None,
                None,
            );
            assert!(refreshed.effects.is_empty());
            assert!(refreshed.render.is_some());
            let cancelled = test.press_at_version("Escape", 0);
            assert!(cancelled
                .effects
                .iter()
                .any(|effect| matches!(effect, Effect::PopView { .. })));
        }
    }
}

#[test]
fn invalid_or_canceled_free_text_keeps_picker_open_without_mutation() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("commit", [("f.txt", "content\n")])
        .build();
    let before = tmp.git(&["rev-parse", "HEAD"]).trim().to_string();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Standard, WIDE_VIEWPORT);
    test.ack();
    test.invoke_args("log.select-and-copy", json!({"allowFreeText": true}));
    let picker_id = test.view_id().to_string();
    test.ui.reply_text(Some("not-a-revision".to_string()));
    let invalid = test.invoke_ui("select.enter-rev");
    assert!(!invalid
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::Clipboard { .. })));
    assert!(test.view_ids().contains(&picker_id));
    test.ui.reply_text(None);
    let canceled = test.invoke_ui("select.enter-rev");
    assert!(!canceled
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::Clipboard { .. })));
    assert!(test.view_ids().contains(&picker_id));
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]).trim(), before);
}

#[test]
fn escape_cancels_selection_without_copying_or_mutating_repository() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("commit", [("f.txt", "content\n")])
        .build();
    let before = tmp.git(&["rev-parse", "HEAD"]).trim().to_string();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    test.invoke("log.select-and-copy");
    let id = test.view_id().to_string();
    let result = test.press("Escape");
    assert!(!result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::Clipboard { .. })));
    assert!(result
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::PopView { view_id } if view_id == &id)));
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]).trim(), before);
}

#[test]
fn cancel_restores_recorded_caller_when_another_view_is_open() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("commit", [("f.txt", "content\n")])
        .build();
    let before = tmp.git(&["rev-parse", "HEAD"]).trim().to_string();
    let (mut test, _) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();
    test.invoke("log.select-and-copy");
    let picker = test.view_id().to_string();
    test.ack();
    test.open_view(
        "v-other",
        magit_lsp::view::ViewKind::ProcessLog,
        magit_lsp::view::Placement::Main,
        WIDE_VIEWPORT,
    );
    test.focus(&picker);
    for key in ["q", "Escape"] {
        let result = test.press(key);
        assert!(
            result
                .effects
                .iter()
                .any(|effect| matches!(effect, Effect::FocusView { view_id } if view_id == "v1")),
            "{key}: {:?}",
            result.effects
        );
        assert!(!result
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Clipboard { .. })));
        assert_eq!(tmp.git(&["rev-parse", "HEAD"]).trim(), before);
        if key == "q" {
            test.invoke("log.select-and-copy");
            test.ack();
            test.focus(&picker);
        }
    }
}
