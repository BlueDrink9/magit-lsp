//! Strict server executions over recorded capability declarations.
#[path = "common/client_pump.rs"]
mod client_pump;
#[path = "common/editor_model/mod.rs"]
mod editor_model;
#[path = "common/fake_editor.rs"]
mod fake_editor;
#[path = "common/mod.rs"]
mod fixtures;
#[path = "common/stdio.rs"]
mod stdio;

use magit_lsp::file_uri::uri_from_path;
use serde_json::{json, Value};

fn declared(capabilities: &Value, path: &str) -> bool {
    path.split('.')
        .fold(Some(capabilities), |value, key| {
            value.and_then(|v| v.get(key))
        })
        .is_some_and(|value| value == true || value.is_object())
}

fn supported(capabilities: &Value, scenario: &editor_model::ScenarioRequirement) -> bool {
    scenario
        .required
        .iter()
        .all(|path| declared(capabilities, path))
        || scenario
            .alternative
            .iter()
            .all(|path| declared(capabilities, path))
}

fn all_capabilities() -> editor_model::EditorModel {
    editor_model::EditorModel::new(editor_model::PLAIN_MIN)
        .with_module(editor_model::EXTRA_DECLARATIONS)
        .with_module(editor_model::Module {
            declares: std::borrow::Cow::Borrowed(r#"{"window":{"workDoneProgress":true}}"#),
            behaves: &[],
        })
        .strict()
}

fn stage(model: &editor_model::EditorModel) -> (String, String, String) {
    stage_profile(model, fake_editor::Profile::Standard, None)
}

fn stage_profile(
    model: &editor_model::EditorModel,
    profile: fake_editor::Profile,
    expected_end: Option<usize>,
) -> (String, String, String) {
    let special_parent = tempfile::tempdir().expect("tempdir");
    let repo_parent = special_parent.path().join("repo é#%");
    std::fs::create_dir(&repo_parent).expect("repository parent");
    let (repo, _) = fixtures::RepoFixture::new_in(&repo_parent)
        .commit("init", [("f.txt", fixtures::TEN_LINES)])
        .modify_standard()
        .build();
    let before = repo.git(&["write-tree"]);
    let head = repo.git(&["rev-parse", "HEAD"]);
    let before_status = repo.git(&["status", "--porcelain"]);
    let before_text = fixtures::read(&repo.path().join("f.txt"));
    let capabilities = model.capabilities();
    let rich = capabilities["experimental"]["magit"].is_object();
    let eligible = supported(&capabilities, &editor_model::CAPABILITY_SCENARIOS[0]);
    let mut client =
        client_pump::ClientPump::with_model(stdio::TestServer::spawn(&[]), profile, model);
    if rich {
        let id = client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/open","params":{"kind":"status","args":{"repo_path":repo.path().to_string_lossy()},"viewport":{"cols":80,"rows":24}}}));
        let opened = client.until_response(id);
        let view = opened["result"]["id"].as_str().expect("status opened");
        let id = client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/invoke","params":{"viewId":view,"command":"status.stage-all"}}));
        assert!(client.until_response(id)["error"].is_null());
    } else {
        let uri = uri_from_path(&repo.path().join(".magit"))
            .expect("native path has a file URI")
            .as_str()
            .to_owned();
        let initial = if expected_end.is_some() {
            "é😀x"
        } else {
            "outdated\n"
        };
        let open = client.editor.open(&uri, initial);
        client.send(open);
        client.until(|c| {
            c.editor.text(&uri) != initial
                || c.server
                    .notifications
                    .iter()
                    .any(|m| m["method"] == "window/showMessage")
        });
        if let Some(end) = expected_end {
            let apply = client
                .editor
                .outbound
                .iter()
                .find(|m| m["method"] == "workspace/applyEdit")
                .expect("real render edit");
            let edit = &apply["params"]["edit"];
            let document_changes =
                declared(&capabilities, "workspace.workspaceEdit.documentChanges");
            assert_eq!(edit["documentChanges"].is_array(), document_changes);
            let edits = if document_changes {
                &edit["documentChanges"][0]["edits"]
            } else {
                &edit["changes"][&uri]
            };
            assert_eq!(
                edits[0]["range"]["end"],
                json!({"line":0,"character":end}),
                "negotiated encoding determines range"
            );
            let response = client
                .server
                .sent
                .iter()
                .position(|m| m["id"] == apply["id"] && m["method"].is_null())
                .expect("apply response");
            let echo = client
                .server
                .sent
                .iter()
                .position(|m| m["method"] == "textDocument/didChange")
                .expect("render echo");
            assert_eq!(
                response < echo,
                profile == fake_editor::Profile::ReorderEcho
            );
        }
        if eligible {
            let line = client
                .editor
                .text(&uri)
                .lines()
                .position(|line| line.contains("f.txt"))
                .expect("changed file row");
            let request = client.editor.code_action(
                &uri,
                json!({"start":{"line":line,"character":0},"end":{"line":line,"character":0}}),
            );
            let id = client.send(request);
            let actions = client.until_response(id);
            let action = actions["result"]
                .as_array()
                .expect("code actions")
                .iter()
                .find(|a| a["command"]["arguments"][0]["command"] == "status.stage")
                .expect("stage file action");
            let request = client
                .editor
                .execute_command("magit.command", action["command"]["arguments"].clone());
            let id = client.send(request);
            assert!(client.until_response(id)["error"].is_null());
        } else {
            assert_eq!(client.editor.text(&uri), "outdated\n");
            assert!(
                client
                    .server
                    .notifications
                    .iter()
                    .any(|m| m["method"] == "window/showMessage"
                        && m["params"]["message"]
                            .as_str()
                            .is_some_and(|s| s.contains("applyEdit"))),
                "missing readable refusal"
            );
        }
    }
    let tree = repo.git(&["write-tree"]);
    if eligible {
        assert_ne!(tree, before, "stage-all must actually change the index");
        assert_eq!(
            repo.git(&["show", ":f.txt"]),
            std::fs::read_to_string(repo.path().join("f.txt")).unwrap()
        );
    } else {
        assert_eq!(tree, before, "refused operation changed the index");
        assert_eq!(repo.git(&["status", "--porcelain"]), before_status);
        assert_eq!(fixtures::read(&repo.path().join("f.txt")), before_text);
    }
    let state = (
        tree,
        repo.git(&["status", "--porcelain"]),
        std::fs::read_to_string(repo.path().join("f.txt")).unwrap(),
    );
    assert_eq!(
        repo.git(&["rev-parse", "HEAD"]),
        head,
        "staging/refusal must preserve HEAD"
    );
    client.finish();
    state
}

#[test]
fn recorded_presets_and_pairwise_capability_scenarios_are_strict() {
    let plain = editor_model::EditorModel::new(editor_model::PLAIN_MIN).capabilities();
    assert!(
        editor_model::strict::check(&plain, &json!({"result":{"effects":[]}})).is_ok(),
        "empty command result requires no rich feature"
    );
    for effect in ["clipboard", "visitFile", "editDocument", "passThrough"] {
        assert!(
            editor_model::strict::check(&plain, &json!({"result":{"effects":[{"type":effect}]}}))
                .is_err(),
            "Plain must not consume optional/custom effect {effect}"
        );
    }
    assert!(editor_model::strict::check(
        &plain,
        &json!({"result":{"effects":[{"type":"pushView","descriptor":{"placement":"bottom"}}]}})
    )
    .is_ok());
    assert!(editor_model::strict::check(
        &plain,
        &json!({"method":"view/effects","params":{"effects":[{"type":"pushView"}]}})
    )
    .is_err());
    let scenarios = editor_model::CAPABILITY_SCENARIOS;
    let mut executions = 0;
    let baseline_model = all_capabilities();
    let baseline = stage(&baseline_model);
    // Constrained to valid initialize declarations: codeAction remains available,
    // rich v1 stays intact, and only independent standard-LSP optional bits vary.
    // An orthogonal-array covering set: every pair takes all four values.
    let pairwise = [
        [false, false, false, false],
        [false, false, true, true],
        [false, true, false, true],
        [false, true, true, false],
        [true, false, false, true],
        [true, false, true, false],
        [true, true, false, false],
        [true, true, true, true],
    ];
    for a in 0..4 {
        for b in a + 1..4 {
            let pairs: std::collections::BTreeSet<_> =
                pairwise.iter().map(|row| (row[a], row[b])).collect();
            assert_eq!(pairs.len(), 4, "pairwise coverage {a}/{b}");
        }
    }
    for preset in editor_model::presets::RECORDED_PRESETS {
        let (model, _) = editor_model::presets::model(*preset);
        assert_eq!(
            stage(&model.clone().strict()),
            baseline,
            "recorded preset {}",
            preset_name(*preset)
        );
        executions += 1;
        for row in pairwise {
            let declarations = json!({"workspace":{"applyEdit":row[0],"workspaceEdit":{"documentChanges":row[1]}},"window":{"showDocument":{"support":row[2]},"workDoneProgress":row[3]}});
            let scenario_model = model
                .clone()
                .with_module(editor_model::Module {
                    declares: std::borrow::Cow::Owned(declarations.to_string()),
                    behaves: &[],
                })
                .strict();
            let capabilities = scenario_model.capabilities();
            for scenario in scenarios {
                let missing = !supported(&capabilities, scenario);
                let state = stage(&scenario_model);
                if !missing {
                    assert_eq!(
                        state,
                        baseline,
                        "baseline mismatch: {} {} {row:?}",
                        scenario.name,
                        preset_name(*preset)
                    );
                }
                executions += 1;
            }
        }
    }
    assert_eq!(executions, 36);
    eprintln!("capability lattice: {} presets, {} staging scenario, {} combinations + 1 independent baseline", editor_model::presets::RECORDED_PRESETS.len(), scenarios.len(), executions);
}

#[test]
fn encoding_ranges_and_document_changes_cross_echo_order() {
    let baseline = stage(&all_capabilities());
    let mut executions = 0;
    for (encoding, end) in [("utf-8", 7), ("utf-16", 4), ("utf-32", 3)] {
        for document_changes in [false, true] {
            for profile in [
                fake_editor::Profile::Standard,
                fake_editor::Profile::ReorderEcho,
            ] {
                let model = editor_model::EditorModel::new(editor_model::PLAIN_MIN).with_module(editor_model::Module {
                    declares: std::borrow::Cow::Owned(json!({"general":{"positionEncodings":[encoding]},"workspace":{"workspaceEdit":{"documentChanges":document_changes}}}).to_string()), behaves: &[],
                }).strict();
                assert_eq!(stage_profile(&model, profile, Some(end)), baseline);
                executions += 1;
            }
        }
    }
    assert_eq!(executions, 12);
    eprintln!("encoding/render scenario: 12 combinations + 1 independent baseline");
}

// Managed commit drafts require Unix domain sockets, so this scenario and its
// helpers are exercised only where the server can create them.
#[cfg(unix)]
fn actions(client: &mut client_pump::ClientPump, uri: &str, row: &str) -> Vec<Value> {
    let line = client
        .editor
        .text(uri)
        .lines()
        .position(|line| line.contains(row))
        .expect("cursor row");
    let request = client.editor.code_action(
        uri,
        json!({"start":{"line":line,"character":0},"end":{"line":line,"character":0}}),
    );
    let id = client.send(request);
    client.until_response(id)["result"]
        .as_array()
        .expect("actions")
        .clone()
}

#[cfg(unix)]
fn run_action(client: &mut client_pump::ClientPump, action: &Value) -> Value {
    let request = client
        .editor
        .execute_command("magit.command", action["command"]["arguments"].clone());
    let id = client.send(request);
    let response = client.until_response(id);
    assert!(response["error"].is_null(), "{response}");
    response
}

#[cfg(unix)]
fn draft_chain(
    show: bool,
    progress: bool,
    document_changes: bool,
) -> (String, String, String, String) {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("init", [("f.txt", fixtures::TEN_LINES)])
        .modify_standard()
        .build();
    fixtures::git(repo.path(), &["add", "f.txt"]);
    let before = (
        repo.git(&["rev-parse", "HEAD"]),
        repo.git(&["write-tree"]),
        repo.git(&["status", "--porcelain"]),
        fixtures::read(&repo.path().join("f.txt")),
    );
    let model = editor_model::EditorModel::new(editor_model::PLAIN_MIN).with_module(editor_model::Module {
                    declares: std::borrow::Cow::Owned(json!({"workspace":{"workspaceEdit":{"documentChanges":document_changes}},"window":{"showDocument":{"support":show},"workDoneProgress":progress}}).to_string()), behaves: &[],
                }).strict();
    let capabilities = model.capabilities();
    let fallbacks: Vec<_> = editor_model::DRAFT_REQUIREMENTS
        .iter()
        .filter(|requirement| !declared(&capabilities, requirement.capability))
        .map(|requirement| requirement.fallback)
        .collect();
    use editor_model::DraftFallback::{DocumentAbort, ManualOpen, ReopenFromDisk};
    let refresh = !fallbacks.contains(&ReopenFromDisk);
    let manual = fallbacks.contains(&ManualOpen);
    let abort = fallbacks.contains(&DocumentAbort);
    let mut client = client_pump::ClientPump::with_model_params(
        stdio::TestServer::spawn(&[]),
        fake_editor::Profile::Standard,
        &model,
        json!({"rootUri": uri_from_path(repo.path()).expect("native path has a file URI").as_str()}),
    );
    let uri = uri_from_path(&repo.path().join(".magit"))
        .expect("native path has a file URI")
        .as_str()
        .to_owned();
    let open = client.editor.open(&uri, "old\n");
    client.send(open);
    client.until(|c| c.editor.text(&uri).contains("f.txt"));
    let offered = actions(&mut client, &uri, "f.txt");
    run_action(
        &mut client,
        offered
            .iter()
            .find(|a| a["command"]["arguments"][0]["command"] == "open.commit")
            .expect("commit menu"),
    );
    let draft_path = repo.path().join(".git/COMMIT_EDITMSG");
    let draft_uri = uri_from_path(&draft_path)
        .expect("native path has a file URI")
        .as_str()
        .to_owned();
    // The pre-opened document makes template refresh observable, not merely showDocument.
    let open = client.editor.open(&draft_uri, "Previous message\n");
    client.send(open);
    let offered = actions(&mut client, &uri, "Commit");
    run_action(
        &mut client,
        offered
            .iter()
            .find(|a| {
                a["command"]["arguments"][0]["command"] == "menu.run-action"
                    && a["command"]["arguments"][0]["target"]["itemKey"] == "c"
            })
            .expect("create commit"),
    );
    client.until(|c| {
        (!refresh || c.editor.text(&draft_uri) != "Previous message\n")
            && (if !manual {
                c.editor.outbound.iter().any(|m| {
                    m["method"] == "window/showDocument" && m["params"]["uri"] == draft_uri
                })
            } else {
                c.server.notifications.iter().any(|m| {
                    m["method"] == "window/showMessage"
                        && m["params"]["message"]
                            .as_str()
                            .is_some_and(|s| s.contains("COMMIT_EDITMSG") && s.contains("Open"))
                })
            })
    });
    let edits_draft = |message: &Value| {
        message["method"] == "workspace/applyEdit"
            && (message["params"]["edit"]["documentChanges"]
                .as_array()
                .is_some_and(|changes| {
                    changes
                        .iter()
                        .any(|change| change["textDocument"]["uri"] == draft_uri)
                })
                || message["params"]["edit"]["changes"]
                    .get(draft_uri.as_str())
                    .is_some())
    };
    let template_index = client.editor.outbound.iter().position(edits_draft);
    assert_eq!(
        template_index.is_some(),
        refresh,
        "versioned template refresh requires documentChanges"
    );
    if refresh {
        assert_eq!(
            client.editor.text(&draft_uri),
            fixtures::read(&draft_path),
            "refresh uses Git's actual template"
        );
    } else {
        assert_eq!(
            client.editor.text(&draft_uri),
            "Previous message\n",
            "unsupported refresh must leave open document unchanged"
        );
        let open = client.editor.open(&draft_uri, &fixtures::read(&draft_path));
        client.send(open);
    }
    let shows: Vec<_> = client
        .editor
        .outbound
        .iter()
        .enumerate()
        .filter(|(_, m)| m["method"] == "window/showDocument" && m["params"]["uri"] == draft_uri)
        .collect();
    assert_eq!(shows.len(), usize::from(!manual));
    if !manual && refresh {
        assert!(
            template_index.unwrap() < shows[0].0,
            "template must precede showDocument"
        );
    }
    if manual {
        let message = client
            .server
            .notifications
            .iter()
            .find(|m| {
                m["method"] == "window/showMessage"
                    && m["params"]["message"]
                        .as_str()
                        .is_some_and(|s| s.contains("COMMIT_EDITMSG"))
            })
            .unwrap();
        let text = message["params"]["message"].as_str().unwrap();
        assert!(
            text.contains(&draft_path.display().to_string())
                && text.contains("Confirm")
                && text.contains("Abort"),
            "readable manual fallback: {text}"
        );
    }
    assert_eq!(
        client.editor.text(&draft_uri),
        fixtures::read(&draft_path),
        "mixed-rung chain converges on the real template"
    );
    if !abort {
        let token = client
            .server
            .notifications
            .iter()
            .find(|m| m["method"] == "$/progress" && m["params"]["value"]["kind"] == "begin")
            .expect("progress begin")["params"]["token"]
            .clone();
        assert!(client
            .editor
            .outbound
            .iter()
            .any(|m| m["method"] == "window/workDoneProgress/create"
                && m["params"]["token"] == token));
        client.send(json!({"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":token}}));
        client.until(|c| {
            c.server.notifications.iter().any(|m| {
                m["method"] == "$/progress"
                    && m["params"]["token"] == token
                    && m["params"]["value"]["kind"] == "end"
            })
        });
    } else {
        let offered = actions(&mut client, &draft_uri, "");
        let response = run_action(
            &mut client,
            offered
                .iter()
                .find(|a| a["command"]["arguments"][0]["command"] == "common.cancel")
                .expect("document Abort fallback"),
        );
        assert_eq!(
            response["result"]["type"], "aborted",
            "semantic Abort acknowledged"
        );
        assert!(client
            .editor
            .outbound
            .iter()
            .all(|m| m["method"] != "window/workDoneProgress/create"));
        assert!(client
            .server
            .notifications
            .iter()
            .all(|m| m["method"] != "$/progress"));
    }
    client.until(|c| !c.editor.text(&uri).contains("COMMIT_EDITMSG"));
    assert!(
        actions(&mut client, &draft_uri, "").is_empty(),
        "cancelled draft detached"
    );
    assert_eq!(
        (
            repo.git(&["rev-parse", "HEAD"]),
            repo.git(&["write-tree"]),
            repo.git(&["status", "--porcelain"]),
            fixtures::read(&repo.path().join("f.txt"))
        ),
        before,
        "cancel preserves Git state"
    );
    let state = (
        repo.git(&["write-tree"]),
        repo.git(&["status", "--porcelain"]),
        fixtures::read(&repo.path().join("f.txt")),
        client
            .editor
            .text(&uri)
            .replace(&repo.path().display().to_string(), "<repo>"),
    );
    client.finish();
    state
}

#[cfg(unix)]
#[test]
fn template_refresh_mixed_fallback_chain_and_progress_cancel() {
    let baseline = draft_chain(true, true, true);
    let mut executions = 0;
    for show in [false, true] {
        for progress in [false, true] {
            for document_changes in [false, true] {
                assert_eq!(
                    draft_chain(show, progress, document_changes),
                    baseline,
                    "draft fallback chain {show}/{progress}/{document_changes}"
                );
                executions += 1;
            }
        }
    }
    assert_eq!(executions, 8);
    eprintln!("draft mixed-chain/cancel scenario: 8 combinations + 1 independent baseline; total 3 scenarios, 56 combinations + 3 baselines");
}

fn preset_name(preset: editor_model::presets::Preset) -> &'static str {
    match preset {
        editor_model::presets::Preset::StockNeovim => "stock-neovim",
        editor_model::presets::Preset::Helix => "helix",
        editor_model::presets::Preset::BundledNeovim => "bundled-neovim",
        editor_model::presets::Preset::BundledVsCode => "bundled-vscode",
    }
}

fn rich_protocol_declaration() -> Value {
    json!({"v":1,"keys":"dynamic","modifiers":"full","layout":["main"],"effects":[],"prompts":[]})
}

#[test]
fn rich_protocol_version_initialize_wire_advertises_and_rejects_before_rich_open() {
    // The accepted session advertises exactly the version it speaks.
    let mut server = stdio::TestServer::spawn(&[]);
    let response = server.initialize(json!({"experimental":{"magit":rich_protocol_declaration()}}));
    assert!(response["error"].is_null(), "{response}");
    assert_eq!(
        response["result"]["capabilities"]["experimental"]["magit"]["protocolVersion"],
        json!(1)
    );
    server.shutdown_and_exit();

    // A minimum above the selected version is a readable initialize error, not a session.
    let mut server = stdio::TestServer::spawn(&[]);
    let mut declaration = rich_protocol_declaration();
    declaration["minimumVersion"] = json!(2);
    let response = server.initialize(json!({"experimental":{"magit":declaration}}));
    let message = response["error"]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("readable rejection expected: {response}"));
    assert!(
        message.contains("minimumVersion 2")
            && message.contains("selected rich protocol version 1"),
        "{message}"
    );
    assert!(
        response["result"].is_null(),
        "rejection must open no rich session: {response}"
    );
    server.shutdown_and_exit();

    // Plain initialization still negotiates and reports only standard capability state.
    let mut server = stdio::TestServer::spawn(&[]);
    let response = server.initialize(json!({"window":{"workDoneProgress":true}}));
    assert!(response["error"].is_null(), "{response}");
    assert_eq!(
        response["result"]["capabilities"]["experimental"]["magit"]["protocolVersion"],
        json!(1)
    );
    let effective =
        &response["result"]["capabilities"]["experimental"]["magit"]["effective"]["capabilities"];
    assert!(
        effective.get("applyEdit").is_some(),
        "Plain keeps standard capabilities: {effective}"
    );
    assert!(
        effective.get("keys").is_none(),
        "Plain must not gain rich capabilities: {effective}"
    );
    server.shutdown_and_exit();
}
