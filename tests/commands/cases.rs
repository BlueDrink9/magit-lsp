use super::*;

#[cfg(unix)]
#[test]
fn hunk_mode_metadata_is_render_bound_for_typed_cursor_and_visual_actions() {
    use magit_lsp::protocol::Selection;
    use magit_lsp::session::Execution;
    use magit_lsp::tree::NodeKind;
    use std::os::unix::fs::PermissionsExt;
    for command in ["status.stage", "status.unstage"] {
        for executable_head in [false, true] {
            for displayed_transition in [false, true] {
                for stale in [false, true] {
                    for route in [
                        "typed",
                        "start",
                        "cursor",
                        "line",
                        "visual-hunk",
                        "visual-lines",
                    ] {
                        let label = format!(
                            "{command}/{executable_head}/{displayed_transition}/{stale}/{route}"
                        );
                        let original = lines(20).to_string();
                        let changed = lines(20).with(5, "line5 CHANGED").to_string();
                        let (tmp, _) = RepoFixture::new()
                            .commit("init", [("f.txt", original.clone())])
                            .build();
                        let path = tmp.path().join("f.txt");
                        let set_mode = |executable| {
                            std::fs::set_permissions(
                                &path,
                                std::fs::Permissions::from_mode(if executable {
                                    0o755
                                } else {
                                    0o644
                                }),
                            )
                            .unwrap()
                        };
                        set_mode(executable_head);
                        tmp.git(&["add", "f.txt"]);
                        tmp.git(&["commit", "--amend", "--no-edit"]);
                        std::fs::write(&path, &changed).unwrap();
                        set_mode(executable_head ^ displayed_transition);
                        let staged = command == "status.unstage";
                        if staged {
                            tmp.git(&["add", "f.txt"]);
                        }
                        let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
                        test.ack();
                        let version = test.version();
                        let hunk = test
                            .tree()
                            .nodes
                            .iter()
                            .find(|node| {
                                node.kind == NodeKind::Hunk
                                    && node.section.as_deref()
                                        == Some(if staged { "staged" } else { "unstaged" })
                            })
                            .unwrap();
                        let request = offered_request(&test, command, &hunk.id);
                        let header = hunk.start_line;
                        let changed_rows: Vec<_> = test
                            .tree()
                            .nodes
                            .iter()
                            .filter(|node| {
                                node.kind == NodeKind::Line
                                    && node.hunk_id == hunk.hunk_id
                                    && matches!(node.marker, Some('+') | Some('-'))
                            })
                            .map(|node| node.start_line)
                            .collect();
                        assert_eq!(changed_rows.len(), 2, "{label}");
                        if stale {
                            set_mode(executable_head ^ !displayed_transition);
                            if staged {
                                tmp.git(&["add", "f.txt"]);
                            }
                        } else if staged {
                            // Unstage authorizes the index side, not the worktree.
                            std::fs::write(&path, "independent worktree edit\n").unwrap();
                            set_mode(!(executable_head ^ displayed_transition));
                        }
                        let index_before = tmp.git(&["ls-files", "--stage", "f.txt"]);
                        let before = target_state(&tmp);
                        let result = match route {
                            "typed" => test
                                .session
                                .dispatch(request, &magit_lsp::ui::NullUi)
                                .unwrap(),
                            "start" => match test.session.start_dispatch(request).unwrap() {
                                Execution::Done(result) => result,
                                _ => panic!("{label}: stage/unstage must not prompt"),
                            },
                            _ => {
                                let selection = match route {
                                    "visual-hunk" => Some(Selection {
                                        start_line: header,
                                        end_line: header,
                                    }),
                                    "visual-lines" => Some(Selection {
                                        start_line: changed_rows[1],
                                        end_line: changed_rows[0],
                                    }),
                                    _ => None,
                                };
                                test.invoke_at_version(
                                    command,
                                    version,
                                    Some(Cursor {
                                        line: if route == "line" {
                                            changed_rows[1]
                                        } else {
                                            header
                                        },
                                        column: 0,
                                    }),
                                    selection,
                                    None,
                                )
                            }
                        };
                        assert!(result.render.is_some(), "{label}: must refresh");
                        if stale {
                            let messages = message_texts(&result.effects);
                            assert!(
                                messages.iter().any(
                                    |text| text.contains("changed") && text.contains("refresh")
                                ),
                                "{label}: {messages:?}"
                            );
                            assert_eq!(target_state(&tmp), before, "{label}");
                            assert_eq!(
                                tmp.git(&["ls-files", "--stage", "f.txt"]),
                                index_before,
                                "{label}"
                            );
                        } else {
                            assert!(
                                message_texts(&result.effects).is_empty(),
                                "{label}: {:?}",
                                result.effects
                            );
                            let after = target_state(&tmp);
                            assert_eq!(after.head, before.head, "{label}");
                            assert_eq!(after.worktree, before.worktree, "{label}");
                            assert_eq!(
                                tmp.git(&["show", ":f.txt"]),
                                if staged { &original } else { &changed }.to_string(),
                                "{label}"
                            );
                            let expected_mode =
                                if executable_head ^ (!staged && displayed_transition) {
                                    "100755"
                                } else {
                                    "100644"
                                };
                            assert_eq!(
                                tmp.git(&["ls-files", "--stage", "f.txt"])
                                    .split_whitespace()
                                    .next(),
                                Some(expected_mode),
                                "{label}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn typed_targets_double_fire_and_edit_after_offer_preserve_exact_state() {
    use magit_lsp::tree::NodeKind;
    for command in ["status.stage", "status.unstage"] {
        for kind in [NodeKind::File, NodeKind::Hunk] {
            for edit in [false, true] {
                let (tmp, _) = RepoFixture::new()
                    .commit("init", [("f.txt", lines(40).to_string())])
                    .modify("f.txt", two_hunk_changes())
                    .build();
                if command == "status.unstage" {
                    tmp.git(&["add", "f.txt"]);
                }
                let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
                test.ack();
                let node = test
                    .tree()
                    .nodes
                    .iter()
                    .find(|node| {
                        node.kind == kind
                            && node.section.as_deref()
                                == Some(if command == "status.stage" {
                                    "unstaged"
                                } else {
                                    "staged"
                                })
                    })
                    .unwrap();
                let request = offered_request(&test, command, &node.id);
                if edit {
                    write(&tmp.path().join("f.txt"), "external edit\n");
                    if command == "status.unstage" {
                        tmp.git(&["add", "f.txt"]);
                    }
                } else {
                    let before = target_state(&tmp);
                    let result = test
                        .session
                        .dispatch(request.clone(), &magit_lsp::ui::NullUi)
                        .unwrap();
                    assert!(
                        message_texts(&result.effects).is_empty(),
                        "{command}/{kind:?}"
                    );
                    let after = target_state(&tmp);
                    assert_eq!(after.head, before.head);
                    assert_eq!(after.worktree, before.worktree);
                    assert_ne!(after.index, before.index, "first action must really mutate");
                }
                let before = target_state(&tmp);
                let result = test
                    .session
                    .dispatch(request, &magit_lsp::ui::NullUi)
                    .unwrap();
                assert!(
                    message_texts(&result.effects)
                        .iter()
                        .any(|text| text.contains("changed")),
                    "{command}/{kind:?}/{edit}"
                );
                assert!(result.render.is_some());
                assert_eq!(target_state(&tmp), before, "{command}/{kind:?}/{edit}");
            }
        }
    }
}

#[test]
fn identical_sibling_targets_never_retarget_after_one_is_consumed() {
    use magit_lsp::tree::NodeKind;
    let base = "a\nb\nc\nold\nd\ne\nf\nseparator\n".repeat(2);
    let changed = base.replace("old", "new");
    for command in ["status.stage", "status.unstage"] {
        for sibling in [0, 1] {
            let (tmp, _) = RepoFixture::new()
                .commit("init", [("f.txt", base.clone())])
                .modify("f.txt", changed.clone())
                .build();
            if command == "status.unstage" {
                tmp.git(&["add", "f.txt"]);
            }
            let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
            test.ack();
            let hunks: Vec<_> = test
                .tree()
                .nodes
                .iter()
                .filter(|node| node.kind == NodeKind::Hunk)
                .collect();
            assert_eq!(hunks.len(), 2);
            assert_ne!(hunks[0].id, hunks[1].id);
            let locations: Vec<_> = hunks.iter().map(|node| node.start_line).collect();
            let requests: Vec<_> = hunks
                .iter()
                .map(|node| offered_request(&test, command, &node.id))
                .collect();
            let before = target_state(&tmp);
            let result = test
                .session
                .dispatch(requests[sibling].clone(), &magit_lsp::ui::NullUi)
                .unwrap();
            assert!(message_texts(&result.effects).is_empty());
            let after = target_state(&tmp);
            assert_eq!(after.head, before.head);
            assert_eq!(after.worktree, before.worktree);
            assert_ne!(after.index, before.index);
            let staged = tmp.git(&["show", ":f.txt"]);
            let expected = if command == "status.stage" {
                if sibling == 0 {
                    base.replacen("old", "new", 1)
                } else {
                    format!(
                        "{}{}",
                        &base[..base.len() / 2],
                        &changed[changed.len() / 2..]
                    )
                }
            } else if sibling == 0 {
                changed.replacen("new", "old", 1)
            } else {
                format!(
                    "{}{}",
                    &changed[..changed.len() / 2],
                    &base[base.len() / 2..]
                )
            };
            assert_eq!(staged, expected, "correct identical sibling");
            for (request, line) in requests.into_iter().zip(locations) {
                let version = request.target.version.unwrap();
                let result = test
                    .session
                    .dispatch(request, &magit_lsp::ui::NullUi)
                    .unwrap();
                assert!(message_texts(&result.effects)
                    .iter()
                    .any(|text| text.contains("changed")));
                assert!(result.render.is_some());
                assert_eq!(target_state(&tmp), after);
                let result = test.invoke_at_version(
                    command,
                    version,
                    Some(Cursor { line, column: 0 }),
                    None,
                    None,
                );
                assert!(message_texts(&result.effects)
                    .iter()
                    .any(|text| text.contains("changed")));
                assert!(result.render.is_some());
                assert_eq!(
                    target_state(&tmp),
                    after,
                    "rich input must not rebind an identical sibling"
                );
            }
        }
    }
}

#[test]
fn mixed_side_identical_hunks_refuse_wrong_side_actions_for_all_dispatch_paths() {
    use magit_lsp::{
        dispatch::{DispatchRequest, DispatchTarget},
        keymap::EditorMode,
        protocol::ViewKeyParams,
        session::Execution,
        tree::NodeKind,
    };
    let base = "a\nb\nc\nold\nd\ne\nf\nseparator\n".repeat(2);
    let changed = base.replace("old", "new");
    for command in ["status.discard", "status.unstage"] {
        for kind in [NodeKind::Hunk, NodeKind::Line] {
            for route in ["start_dispatch", "dispatch", "invoke", "key"] {
                let (tmp, _) = RepoFixture::new()
                    .commit("init", [("f.txt", base.clone())])
                    .modify("f.txt", changed.clone())
                    .build();
                let mut child = std::process::Command::new("git")
                    .current_dir(tmp.path())
                    .args(["add", "-p", "--", "f.txt"])
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .unwrap();
                use std::io::Write;
                child.stdin.take().unwrap().write_all(b"y\nn\n").unwrap();
                assert!(child.wait().unwrap().success());
                let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
                test.ack();
                let wanted_section = if command == "status.discard" {
                    "staged"
                } else {
                    "unstaged"
                };
                let node = test
                    .tree()
                    .nodes
                    .iter()
                    .find(|node| {
                        node.kind == kind && node.section.as_deref() == Some(wanted_section)
                    })
                    .unwrap();
                assert!(
                    magit_lsp::dispatch::offer_target(test.tree(), command, &node.id).is_none()
                );
                let line = node.start_line;
                let version = test.version();
                let before = target_state(&tmp);
                let result = match route {
                    "start_dispatch" | "dispatch" => {
                        let request = DispatchRequest {
                            command: command.into(),
                            target: DispatchTarget {
                                view_id: test.view_id().into(),
                                version: Some(version),
                                cursor: Some(Cursor { line, column: 0 }),
                                selection: None,
                                key: None,
                            },
                            action_target: None,
                            args: None,
                        };
                        if route == "dispatch" {
                            test.session
                                .dispatch(request, &magit_lsp::ui::NullUi)
                                .unwrap()
                        } else {
                            match test.session.start_dispatch(request).unwrap() {
                                Execution::Done(result) => result,
                                _ => panic!("wrong-side action must not prompt"),
                            }
                        }
                    }
                    "invoke" => test.invoke_at_version(
                        command,
                        version,
                        Some(Cursor { line, column: 0 }),
                        None,
                        None,
                    ),
                    _ => test
                        .session
                        .key(&ViewKeyParams {
                            view_id: test.view_id().into(),
                            version,
                            key: if command == "status.unstage" {
                                "u"
                            } else {
                                "x"
                            }
                            .into(),
                            editor_mode: EditorMode::Normal,
                            cursor: Cursor { line, column: 0 },
                            selection: None,
                        })
                        .unwrap(),
                };
                assert_eq!(target_state(&tmp), before, "{command}/{kind:?}/{route}");
                if route == "invoke" || route == "key" {
                    assert!(
                        message_texts(&result.effects)
                            .iter()
                            .any(|text| text.contains("Nothing")),
                        "{command}/{kind:?}/{route}: {:?}",
                        message_texts(&result.effects)
                    );
                }
            }
        }
    }
}

#[test]
fn mixed_side_correct_target_stages_the_displayed_unstaged_occurrence() {
    use magit_lsp::{
        dispatch::{DispatchRequest, DispatchTarget},
        tree::NodeKind,
    };
    let base = "a\nb\nc\nold\nd\ne\nf\nseparator\n".repeat(2);
    let changed = base.replace("old", "new");
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", base.clone())])
        .modify("f.txt", changed.clone())
        .build();
    let mut child = std::process::Command::new("git")
        .current_dir(tmp.path())
        .args(["add", "-p", "--", "f.txt"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(b"y\nn\n").unwrap();
    assert!(child.wait().unwrap().success());
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let node = test
        .tree()
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Hunk && node.section.as_deref() == Some("unstaged"))
        .unwrap();
    let request = DispatchRequest {
        command: "status.stage".into(),
        target: DispatchTarget {
            view_id: test.view_id().into(),
            version: Some(test.version()),
            cursor: Some(Cursor {
                line: node.start_line,
                column: 0,
            }),
            selection: None,
            key: None,
        },
        action_target: Some(
            magit_lsp::dispatch::offer_target(test.tree(), "status.stage", &node.id).unwrap(),
        ),
        args: None,
    };
    let result = test
        .session
        .dispatch(request, &magit_lsp::ui::NullUi)
        .unwrap();
    assert!(message_texts(&result.effects).is_empty());
    assert_eq!(tmp.git(&["show", ":f.txt"]), changed);
    assert_eq!(tmp.git(&["show", "HEAD:f.txt"]), base);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
        changed
    );
}

#[test]
fn offered_rename_and_unborn_targets_validate_both_endpoints_and_absent_head() {
    use magit_lsp::tree::NodeKind;
    for unborn in [false, true] {
        for edit in [false, true] {
            let (tmp, _) = if unborn {
                RepoFixture::new()
                    .unborn()
                    .staged_file("new.txt", "new bytes\n")
                    .build()
            } else {
                RepoFixture::new()
                    .commit("init", [("old.txt", "rename bytes\n")])
                    .rename("old.txt", "new.txt")
                    .build()
            };
            let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
            test.ack();
            let node = test
                .tree()
                .nodes
                .iter()
                .find(|node| {
                    node.kind == NodeKind::File && node.section.as_deref() == Some("staged")
                })
                .unwrap();
            let request = offered_request(&test, "status.unstage", &node.id);
            if edit {
                let path = if unborn { "new.txt" } else { "old.txt" };
                write(&tmp.path().join(path), "concurrent endpoint\n");
                tmp.git(&["add", path]);
            }
            let before = target_state(&tmp);
            let result = test
                .session
                .dispatch(request.clone(), &magit_lsp::ui::NullUi)
                .unwrap();
            let after = target_state(&tmp);
            if edit {
                assert!(message_texts(&result.effects)
                    .iter()
                    .any(|text| text.contains("changed")));
                assert_eq!(after, before);
            } else {
                assert!(message_texts(&result.effects).is_empty());
                assert_eq!(after.head, before.head);
                assert_eq!(after.worktree, before.worktree);
                assert_ne!(after.index, before.index);
                if unborn {
                    assert_eq!(tmp.git(&["ls-files"]), "");
                    assert!(after.head.is_none());
                } else {
                    assert_eq!(tmp.git(&["ls-files"]), "old.txt\n");
                    assert_eq!(tmp.git(&["show", ":old.txt"]), "rename bytes\n");
                }
                let result = test
                    .session
                    .dispatch(request, &magit_lsp::ui::NullUi)
                    .unwrap();
                assert!(message_texts(&result.effects)
                    .iter()
                    .any(|text| text.contains("changed")));
                assert_eq!(target_state(&tmp), after);
            }
        }
    }
}
