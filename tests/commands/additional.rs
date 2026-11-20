use super::*;

#[test]
fn discard_targets_bind_worktree_content_before_and_during_confirmation() {
    use magit_lsp::{protocol::PromptValue, session::Execution, tree::NodeKind};
    for kind in [NodeKind::File, NodeKind::Hunk] {
        for edit in ["before", "during", "double"] {
            let (tmp, _) = RepoFixture::new()
                .commit("init", [("f.txt", lines(40).to_string())])
                .modify("f.txt", two_hunk_changes())
                .build();
            let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
            test.ack();
            let node = test
                .tree()
                .nodes
                .iter()
                .find(|node| node.kind == kind && node.section.as_deref() == Some("unstaged"))
                .unwrap();
            let request = offered_request(&test, "status.discard", &node.id);
            if edit == "before" {
                write(&tmp.path().join("f.txt"), "edited before execute\n");
            }
            let before = target_state(&tmp);
            let index_entries = tmp.git(&["ls-files", "--stage", "-z"]);
            let step = test.session.start_dispatch(request.clone()).unwrap();
            if edit == "before" {
                let Execution::Done(result) = step else {
                    panic!("stale target must not prompt");
                };
                assert!(message_texts(&result.effects)
                    .iter()
                    .any(|text| text.contains("changed")));
                assert!(result.render.is_some());
                assert_eq!(target_state(&tmp), before);
                continue;
            }
            let Execution::Prompt(prompt) = step else {
                panic!("expected confirmation");
            };
            assert_eq!(
                target_state(&tmp),
                before,
                "offering a confirmation cannot mutate"
            );
            if edit == "during" {
                write(&tmp.path().join("f.txt"), "edited during confirmation\n");
            }
            let confirmed = target_state(&tmp);
            let Execution::Done(result) = test
                .session
                .resume_prompt(&prompt.prompt_id, Some(PromptValue::Bool(true)))
                .unwrap()
            else {
                panic!("expected completion");
            };
            if edit == "during" {
                assert!(message_texts(&result.effects)
                    .iter()
                    .any(|text| text.contains("changed")));
                assert_eq!(target_state(&tmp), confirmed);
            } else {
                assert!(message_texts(&result.effects).is_empty());
                let after = target_state(&tmp);
                assert_eq!(after.head, before.head);
                // Git checkout may update index stat-cache bytes, but must
                // preserve every staged path/mode/blob exactly.
                assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), index_entries);
                assert_ne!(after.worktree, before.worktree);
                let expected = if kind == NodeKind::File {
                    lines(40).to_string()
                } else {
                    lines(40).with(35, "line35 CHANGED").to_string()
                };
                assert_eq!(
                    std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
                    expected
                );
                let Execution::Done(result) = test.session.start_dispatch(request).unwrap() else {
                    panic!("double-fire must not prompt");
                };
                assert!(message_texts(&result.effects)
                    .iter()
                    .any(|text| text.contains("changed")));
                assert!(result.render.is_some());
                assert_eq!(target_state(&tmp), after);
            }
        }
    }
}

#[test]
fn rich_invoke_key_and_selection_share_fresh_target_validation() {
    use magit_lsp::{
        keymap::EditorMode,
        protocol::{Selection, ViewKeyParams},
        tree::NodeKind,
    };
    for route in ["invoke", "key", "selection"] {
        for double in [false, true] {
            let (tmp, _) = RepoFixture::new()
                .commit("init", [("f.txt", lines(40).to_string())])
                .modify("f.txt", two_hunk_changes())
                .build();
            let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
            test.ack();
            let version = test.version();
            let line = test
                .tree()
                .nodes
                .iter()
                .find(|node| {
                    node.kind == NodeKind::Hunk && node.section.as_deref() == Some("unstaged")
                })
                .unwrap()
                .start_line;
            let cursor = Cursor { line, column: 0 };
            let run = |test: &mut TestSession| {
                if route == "key" {
                    test.session
                        .key(&ViewKeyParams {
                            view_id: test.view_id().into(),
                            version,
                            key: "s".into(),
                            editor_mode: EditorMode::Normal,
                            cursor,
                            selection: None,
                        })
                        .unwrap()
                } else {
                    test.invoke_at_version(
                        "status.stage",
                        version,
                        Some(cursor),
                        (route == "selection").then_some(Selection {
                            start_line: line,
                            end_line: line,
                        }),
                        None,
                    )
                }
            };
            if double {
                let result = run(&mut test);
                assert!(message_texts(&result.effects).is_empty());
                assert!(tmp.git(&["diff", "--cached"]).contains("line5 CHANGED"));
            } else {
                write(&tmp.path().join("f.txt"), "edited after display\n");
            }
            let before = target_state(&tmp);
            let result = run(&mut test);
            assert!(
                message_texts(&result.effects)
                    .iter()
                    .any(|text| text.contains("changed")),
                "{route}/{double}"
            );
            assert!(result.render.is_some());
            assert_eq!(target_state(&tmp), before, "{route}/{double}");
        }
    }
}

#[test]
fn typed_target_rejects_forged_hash_and_cross_render_retargeting() {
    use magit_lsp::tree::NodeKind;
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", lines(40).to_string())])
        .modify("f.txt", two_hunk_changes())
        .build();
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    let node = test
        .tree()
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Hunk)
        .unwrap();
    let request = offered_request(&test, "status.stage", &node.id);
    let before = target_state(&tmp);
    for invalid in ["hash", "node", "version", "command"] {
        let mut forged = request.clone();
        match invalid {
            "hash" => forged.action_target.as_mut().unwrap().mutation_hash = "wrong".into(),
            "node" => forged.action_target.as_mut().unwrap().node_id = "missing".into(),
            "version" => forged.target.version = Some(u64::MAX),
            _ => forged.command = "status.unstage".into(),
        }
        let result = test
            .session
            .dispatch(forged, &magit_lsp::ui::NullUi)
            .unwrap();
        assert!(message_texts(&result.effects)
            .iter()
            .any(|text| text.contains("changed")));
        assert!(result.render.is_some());
        assert_eq!(target_state(&tmp), before);
    }
}

pub(super) fn change_identity_file(root: &std::path::Path, kind: &str, later: bool) {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let path = root.join("f.txt");
    match kind {
        "binary" => std::fs::write(
            path,
            if later {
                b"later\0bytes"
            } else {
                b"shown\0bytes"
            },
        )
        .unwrap(),
        "mode" | "text-mode" | "added-mode" => {
            if kind != "mode" {
                std::fs::write(&path, "shown text\n").unwrap();
            }
            let executable = if kind == "added-mode" { later } else { !later };
            std::fs::set_permissions(
                path,
                std::fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 }),
            )
            .unwrap();
        }
        "type" => {
            std::fs::remove_file(&path).unwrap();
            symlink(
                if later {
                    "later-target"
                } else {
                    "shown-target"
                },
                path,
            )
            .unwrap();
        }
        "untracked-link" => {
            if later {
                std::fs::remove_file(&path).unwrap();
            }
            symlink(
                if later {
                    "later-target"
                } else {
                    "missing-target"
                },
                path,
            )
            .unwrap();
        }
        "directory" => {
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(
                path.join(if later { "new-member" } else { "shown-member" }),
                "member\n",
            )
            .unwrap();
        }
        _ => std::fs::write(
            path,
            if later {
                "later text\n"
            } else {
                "shown text\n"
            },
        )
        .unwrap(),
    }
}

pub(super) fn file_identity_matrix(action: &str, stale: bool, selection: bool) {
    use magit_lsp::protocol::{PromptValue, Selection, ViewInvokeParams};
    use magit_lsp::session::Execution;
    for kind in [
        "binary",
        "mode",
        "text-mode",
        "added-mode",
        "type",
        "collapsed",
        "untracked",
        "untracked-link",
        "directory",
    ] {
        let untracked = matches!(kind, "untracked" | "untracked-link" | "directory");
        if untracked && action == "unstage" {
            continue;
        }
        if untracked && selection {
            continue;
        }
        let initial = if kind == "binary" {
            "base\0bytes"
        } else {
            "base text\n"
        };
        let (tmp, _) = RepoFixture::new()
            .commit("init", [("base.txt", "base\n")])
            .build();
        tmp.git(&["config", "core.fileMode", "true"]);
        if !untracked {
            std::fs::write(tmp.path().join("f.txt"), initial).unwrap();
            tmp.git(&["add", "f.txt"]);
            tmp.git(&["commit", "-m", "tracked base"]);
        }
        change_identity_file(tmp.path(), kind, false);
        if action == "unstage" {
            tmp.git(&["add", "f.txt"]);
        }
        let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
        test.ack();
        let section = if action == "unstage" {
            "staged"
        } else if untracked {
            "untracked"
        } else {
            "unstaged"
        };
        let path = if kind == "directory" {
            "f.txt/"
        } else {
            "f.txt"
        };
        let mut line = test.goto_file(section, path);
        if kind == "collapsed" {
            test.invoke_at("common.hide-section", Cursor { line, column: 0 });
            line = test.goto_file(section, path);
            assert!(
                test.tree()
                    .nodes
                    .iter()
                    .find(|node| node.start_line == line
                        && node.kind == magit_lsp::tree::NodeKind::File)
                    .unwrap()
                    .children
                    .is_empty()
            );
        }
        let version = test.version();
        if stale {
            change_identity_file(tmp.path(), kind, true);
            if action == "unstage" {
                tmp.git(&["add", "f.txt"]);
            }
        } else {
            // Single-file authorization must not fingerprint the repository;
            // unstage must also ignore edits to this file's worktree side.
            std::fs::write(tmp.path().join("base.txt"), "unrelated edit\n").unwrap();
            if action == "unstage" {
                change_identity_file(tmp.path(), kind, true);
            }
        }
        let head = tmp.git(&["rev-parse", "HEAD"]);
        let index = std::fs::read(tmp.path().join(".git/index")).unwrap();
        let index_entries = tmp.git(&["ls-files", "--stage", "-z"]);
        let worktree = identity_worktree(tmp.path());
        let step = test
            .session
            .start_invoke(&ViewInvokeParams {
                view_id: test.view_id().to_string(),
                command: format!("status.{action}"),
                version: Some(version),
                cursor: if selection {
                    None
                } else {
                    Some(Cursor { line, column: 0 })
                },
                selection: selection.then_some(Selection {
                    start_line: line,
                    end_line: line,
                }),
                args: None,
            })
            .unwrap();
        let result = match step {
            Execution::Prompt(prompt) => {
                assert!(
                    action == "discard" && (!stale || selection),
                    "{kind}/{action}: stale file action prompted"
                );
                let Execution::Done(result) = test
                    .session
                    .resume_prompt(&prompt.prompt_id, Some(PromptValue::Bool(true)))
                    .unwrap()
                else {
                    panic!("discard completed");
                };
                result
            }
            Execution::Done(result) => {
                assert!(
                    stale || action != "discard",
                    "{kind}: fresh discard did not prompt"
                );
                result
            }
            Execution::Pick(pick) => panic!("unexpected pick for {kind}/{action}: {pick:?}"),
        };
        let messages = message_texts(&result.effects);
        assert_eq!(
            tmp.git(&["rev-parse", "HEAD"]),
            head,
            "{kind}/{action}: HEAD"
        );
        if stale {
            assert!(
                messages.iter().any(|text| text.contains("changed")),
                "{kind}/{action}: {messages:?}"
            );
            assert!(
                result.render.is_some(),
                "{kind}/{action}: stale rejection must refresh"
            );
            assert_eq!(
                std::fs::read(tmp.path().join(".git/index")).unwrap(),
                index,
                "{kind}/{action}: index"
            );
            assert_eq!(
                identity_worktree(tmp.path()),
                worktree,
                "{kind}/{action}: worktree"
            );
        } else {
            assert!(
                !messages
                    .iter()
                    .any(|text| text.contains("changed") || text.contains("failed")),
                "{kind}/{action}: {messages:?}"
            );
            assert_eq!(
                std::fs::read_to_string(tmp.path().join("base.txt")).unwrap(),
                "unrelated edit\n"
            );
            match action {
                "stage" => {
                    assert_eq!(
                        identity_worktree(tmp.path()),
                        worktree,
                        "{kind}: stage worktree"
                    );
                    if kind == "directory" {
                        assert_eq!(tmp.git(&["show", ":f.txt/shown-member"]), "member\n");
                        assert_eq!(tmp.git(&["ls-files", "f.txt/"]), "f.txt/shown-member\n");
                    } else {
                        let expected = match kind {
                            "binary" => "shown\0bytes",
                            "mode" => initial,
                            "type" => "shown-target",
                            "untracked-link" => "missing-target",
                            _ => "shown text\n",
                        };
                        assert_eq!(
                            tmp.git(&["show", ":f.txt"]),
                            expected,
                            "{kind}: stage bytes"
                        );
                        let mode = if matches!(kind, "type" | "untracked-link") {
                            "120000"
                        } else if matches!(kind, "mode" | "text-mode") {
                            "100755"
                        } else {
                            "100644"
                        };
                        assert_eq!(
                            tmp.git(&["ls-files", "--stage", "f.txt"])
                                .split_whitespace()
                                .next(),
                            Some(mode)
                        );
                    }
                }
                "unstage" => {
                    assert_eq!(
                        identity_worktree(tmp.path()),
                        worktree,
                        "{kind}: unstage worktree"
                    );
                    assert_eq!(
                        tmp.git(&["show", ":f.txt"]),
                        initial,
                        "{kind}: unstage bytes"
                    );
                    assert_eq!(
                        tmp.git(&["ls-files", "--stage", "f.txt"])
                            .split_whitespace()
                            .next(),
                        Some("100644")
                    );
                    assert_eq!(
                        tmp.git(&["write-tree"]),
                        tmp.git(&["rev-parse", "HEAD^{tree}"])
                    );
                }
                "discard" => {
                    // Git restore legitimately updates the index stat cache,
                    // but must preserve every staged path/blob/mode.
                    assert_eq!(
                        tmp.git(&["ls-files", "--stage", "-z"]),
                        index_entries,
                        "{kind}: discard index"
                    );
                    if untracked {
                        assert!(std::fs::symlink_metadata(tmp.path().join("f.txt")).is_err());
                    } else {
                        use std::os::unix::fs::PermissionsExt;
                        let metadata = std::fs::symlink_metadata(tmp.path().join("f.txt")).unwrap();
                        assert!(metadata.is_file());
                        assert_eq!(metadata.permissions().mode() & 0o777, 0o644);
                        assert_eq!(
                            std::fs::read(tmp.path().join("f.txt")).unwrap(),
                            initial.as_bytes()
                        );
                    }
                }
                _ => unreachable!(),
            }
        }
    }
}
