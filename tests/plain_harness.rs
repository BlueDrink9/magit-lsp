//! Plain editor conformance self-tests and real-binary protocol smoke test.
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
use client_pump::ClientPump;
use fake_editor::{FakeEditor, Profile};
use serde_json::{json, Value};
use std::fs;

fn caps() -> Value {
    json!({"window":{"workDoneProgress":true},"general":{"positionEncodings":["utf-8","utf-16","utf-32"]}})
}

#[test]
fn shared_rich_render_vectors_apply_exact_text_and_intents() {
    #[derive(serde::Deserialize)]
    struct Vector {
        name: String,
        #[serde(rename = "initialLines")]
        initial_lines: Vec<String>,
        #[serde(rename = "initialVersion")]
        initial_version: u64,
        render: magit_lsp::protocol::ViewRender,
        #[serde(rename = "expectedLines")]
        expected_lines: Vec<String>,
        #[serde(rename = "expectedVersion")]
        expected_version: u64,
        #[serde(rename = "expectedMetadata")]
        expected_metadata: Option<Value>,
        outcome: String,
    }
    #[derive(serde::Deserialize)]
    struct Contracts {
        renders: Vec<Vector>,
    }
    for vector in serde_json::from_str::<Contracts>(include_str!("data/client-contracts.json"))
        .unwrap()
        .renders
    {
        let mut editor = editor_model::rich::RichEditor::default();
        editor.views.insert(
            vector.render.view_id.clone(),
            editor_model::rich::ViewState {
                lines: vector.initial_lines,
                version: vector.initial_version,
                ..Default::default()
            },
        );
        let intent = editor.apply(vector.render.clone());
        let state = &editor.views[&vector.render.view_id];
        assert_eq!(state.lines, vector.expected_lines, "{}", vector.name);
        assert_eq!(state.version, vector.expected_version, "{}", vector.name);
        if let Some(expected) = vector.expected_metadata {
            assert_eq!(
                json!({"mode":state.mode,"pending":state.pending,"cursor":state.cursor,"menu":state.menu}),
                expected,
                "{}",
                vector.name
            );
        }
        match vector.outcome.as_str() {
            "ack" => assert_eq!(
                intent,
                editor_model::rich::Intent::Ack(vector.expected_version)
            ),
            "resync" => assert_eq!(intent, editor_model::rich::Intent::Resync),
            other => panic!("unknown outcome {other}"),
        }
    }
}

#[test]
fn shared_prompt_vectors_derive_answers_and_focus() {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Vector {
        name: String,
        params: magit_lsp::protocol::ViewPromptParams,
        response: Value,
        custom_response: Option<Value>,
        answer: Value,
        focus: Option<Vec<String>>,
    }
    #[derive(serde::Deserialize)]
    struct Contracts {
        prompts: Vec<Vector>,
    }
    for vector in serde_json::from_str::<Contracts>(include_str!("data/client-contracts.json"))
        .unwrap()
        .prompts
    {
        let input = editor_model::prompts::PromptInput {
            response: vector.response,
            custom_response: vector.custom_response,
        };
        let outcome = editor_model::prompts::answer(&vector.params, &input);
        assert_eq!(outcome.answer, vector.answer, "{}", vector.name);
        assert_eq!(outcome.focus, vector.focus, "{}", vector.name);
    }
}

#[test]
fn shared_declaration_vectors_match_production_server_admission() {
    #[derive(serde::Deserialize)]
    struct Vector {
        name: String,
        capabilities: Value,
        outcome: String,
        #[serde(default, rename = "errorContains")]
        error_contains: Vec<String>,
    }
    #[derive(serde::Deserialize)]
    struct Contracts {
        declarations: Vec<Vector>,
    }
    let mut seen = std::collections::BTreeSet::new();
    let contracts =
        serde_json::from_str::<Contracts>(include_str!("data/client-contracts.json")).unwrap();
    for vector in contracts.declarations {
        assert!(
            seen.insert(vector.name.clone()),
            "duplicate declaration vector {}",
            vector.name
        );
        let negotiated =
            magit_lsp::presentation::negotiate(&json!({"capabilities": vector.capabilities}));
        match vector.outcome.as_str() {
            "rich" => {
                assert!(
                    vector.error_contains.is_empty(),
                    "{}: an admitted declaration carries no error",
                    vector.name
                );
                let presentation = negotiated.unwrap_or_else(|error| {
                    panic!("{}: expected rich admission, got {error}", vector.name)
                });
                assert!(
                    matches!(presentation, magit_lsp::presentation::Presentation::Rich(_)),
                    "{}: a declared v=1 must select the rich session",
                    vector.name
                );
            }
            "reject" => {
                assert!(
                    !vector.error_contains.is_empty(),
                    "{}: a rejected declaration must name the readable error",
                    vector.name
                );
                let error = negotiated
                    .expect_err(&format!(
                        "{}: expected initialization rejection",
                        vector.name
                    ))
                    .to_string();
                assert!(
                    error.starts_with("capabilities.experimental.magit "),
                    "{}: {error}",
                    vector.name
                );
                for fragment in &vector.error_contains {
                    assert!(
                        error.contains(fragment),
                        "{}: error {error:?} does not mention {fragment:?}",
                        vector.name
                    );
                }
            }
            other => panic!("{}: unknown declaration outcome {other}", vector.name),
        }
    }
    for required in ["minimum-absent-selects-v1", "minimum-one-selects-v1"] {
        assert!(
            seen.contains(required),
            "the contract's {required} acceptance case must stay covered"
        );
    }
}

#[test]
fn plain_key_edit_shape_profiles_replay_order_versions_and_test_only_classification() {
    use editor_model::edit_shapes::{classify, notifications, Shape, PROFILES};
    assert_eq!(
        PROFILES
            .iter()
            .map(|profile| profile.name)
            .collect::<Vec<_>>(),
        [
            "selection-replace-pair",
            "selection-replace-single",
            "rangeless-full-text",
            "batched-changes",
            "paste-multichar",
            "undo-after-repair",
        ]
    );
    for profile in PROFILES {
        let records = notifications(profile.trace);
        assert!(!records.is_empty(), "{}", profile.name);
        assert_eq!(classify(&records), profile.shape, "{}", profile.name);
        let uri = records[0]["params"]["textDocument"]["uri"]
            .as_str()
            .unwrap();
        let mut editor = FakeEditor::new(Profile::Standard);
        editor.open(uri, "abcd");
        let versions = records
            .iter()
            .map(|record| {
                record["params"]["textDocument"]["version"]
                    .as_i64()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        for record in records.iter().cloned() {
            editor.replay_did_change(record);
        }
        assert_eq!(editor.outbound, records, "{} payload/order", profile.name);
        assert_eq!(
            editor.documents[uri].version,
            *versions.last().unwrap(),
            "{} final version",
            profile.name
        );
    }
    assert_eq!(
        classify(&notifications(PROFILES[0].trace)),
        Shape::DeleteThenInsertPair
    );
    assert_eq!(
        classify(&notifications(PROFILES[2].trace)),
        Shape::Other,
        "rangeless changes are explicitly outside accepted shapes"
    );
    assert_eq!(
        classify(&notifications(PROFILES[3].trace)),
        Shape::Other,
        "batched changes are explicitly outside accepted shapes"
    );
    assert_eq!(
        classify(&notifications(PROFILES[4].trace)),
        Shape::Other,
        "multi-character paste is explicitly outside accepted shapes"
    );
    assert_eq!(
        classify(&notifications(PROFILES[5].trace)),
        Shape::Other,
        "undo/repair is explicitly outside accepted shapes"
    );
    let single_insert = json!({"params":{"contentChanges":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"text":"i"}]}});
    assert_eq!(
        classify(&[single_insert]),
        Shape::SingleInsert,
        "single-insert pattern is classified without production policy"
    );
}

#[test]
fn queued_prompt_routing_uses_unique_correlated_request_ids() {
    let prompt_module = editor_model::Module {
        declares: std::borrow::Cow::Borrowed(r#"{"experimental":{"magit":{"prompts":["text"]}}}"#),
        behaves: &[],
    };
    // anchor: rich text-prompt contract in tests/data/client-contracts.json and stdio evidence in tests/server_async.rs.
    let model = editor_model::EditorModel::new(editor_model::RICH_MIN)
        .with_module(prompt_module)
        .strict();
    let mut client =
        ClientPump::with_model(stdio::TestServer::spawn(&[]), Profile::Standard, &model);
    client.queue_prompt(editor_model::prompts::PromptInput {
        response: json!(""),
        custom_response: None,
    });
    client.queue_prompt(editor_model::prompts::PromptInput {
        response: json!("second"),
        custom_response: None,
    });
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("init", [("f.txt", fixtures::TEN_LINES)])
        .build();
    let open = client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/open","params":{"kind":"status","args":{"repo_path":repo.path().to_string_lossy()},"viewport":{"cols":80,"rows":24}}}));
    let status = client.until_response(open)["result"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    client.until(|c| {
        c.rich
            .as_ref()
            .is_some_and(|r| r.views.contains_key(&status))
    });
    let invoke = client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/invoke","params":{"viewId":status,"command":"open.commit"}}));
    let opened = client.until_response(invoke);
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["type"] == "pushView")
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    client.until(|c| c.rich.as_ref().is_some_and(|r| r.views.contains_key(&menu)));
    let key = |key: &str, version| json!({"jsonrpc":"2.0","id":0,"method":"view/key","params":{"viewId":menu,"version":version,"key":key,"editorMode":"normal","cursor":{"line":0,"column":0}}});
    let prefix = client.send(key("=", client.rich.as_ref().unwrap().views[&menu].version));
    assert!(client.until_response(prefix)["error"].is_null());
    let first = client.send(key("A", client.rich.as_ref().unwrap().views[&menu].version));
    let before_answer = client.rich.as_ref().unwrap().views[&menu].version;
    client.until(|c| {
        c.server
            .sent
            .iter()
            .filter(|m| m["method"] == "view/promptRespond")
            .count()
            == 1
    });
    let first_prompt_id = client.prompt_response_ids()[0];
    client.until_response(first_prompt_id);
    let first_reply = client.until_response(first);
    assert_eq!(first_reply["result"]["effects"], json!([]));
    client.until(|c| c.rich.as_ref().unwrap().views[&menu].version > before_answer);
    let second_prefix = client.send(key("=", client.rich.as_ref().unwrap().views[&menu].version));
    assert_eq!(
        client.until_response(second_prefix)["result"]["effects"],
        json!([])
    );
    let clear = client.send(key("A", client.rich.as_ref().unwrap().views[&menu].version));
    assert_eq!(client.until_response(clear)["result"]["effects"], json!([]));
    let after_clear = client.rich.as_ref().unwrap().views[&menu].version;
    client.until(|c| c.rich.as_ref().unwrap().views[&menu].version > after_clear);
    let third_prefix = client.send(key("=", client.rich.as_ref().unwrap().views[&menu].version));
    assert_eq!(
        client.until_response(third_prefix)["result"]["effects"],
        json!([])
    );
    let second = client.send(key("A", client.rich.as_ref().unwrap().views[&menu].version));
    client.until(|c| {
        c.server
            .sent
            .iter()
            .filter(|m| m["method"] == "view/promptRespond")
            .count()
            == 2
    });
    let ids: Vec<_> = client
        .server
        .sent
        .iter()
        .filter(|m| m["method"] == "view/promptRespond")
        .map(|m| m["id"].as_i64().unwrap())
        .collect();
    assert_ne!(ids[0], ids[1]);
    let answers: Vec<_> = client
        .server
        .sent
        .iter()
        .filter(|m| m["method"] == "view/promptRespond")
        .collect();
    assert_eq!(answers[0]["params"]["value"], "");
    assert_eq!(answers[1]["params"]["value"], "second");
    client.until_response(ids[1]);
    assert_eq!(
        client.until_response(second)["result"]["effects"],
        json!([])
    );
    let unknown = client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/promptRespond","params":{"promptId":"unknown-prompt-id","cancelled":false,"value":"ignored"}}));
    assert!(
        client.until_response(unknown)["error"].is_object(),
        "unknown prompt response must remain observable"
    );
    client.finish();
}

#[test]
fn rich_stale_render_isolation() {
    use editor_model::rich::{Intent, RichEditor, ViewState};
    let mut editor = RichEditor::default();
    editor.views.insert(
        "a".into(),
        ViewState {
            lines: vec!["keep".into()],
            version: 1,
            ..ViewState::default()
        },
    );
    let before = editor.views["a"].clone();
    let stale: magit_lsp::protocol::ViewRender = serde_json::from_value(json!({"viewId":"a","version":3,"baseVersion":0,"edits":[{"startLine":0,"endLine":1,"lines":["bad"]}],"mode":"interactive"})).unwrap();
    assert_eq!(editor.apply(stale), Intent::Resync);
    assert_eq!(editor.views["a"], before);
    let other: magit_lsp::protocol::ViewRender = serde_json::from_value(
        json!({"viewId":"b","version":1,"text":["other"],"mode":"interactive"}),
    )
    .unwrap();
    editor.apply(other);
    assert_eq!(editor.views["a"], before);
}

#[test]
fn minimum_editor_models_declare_exact_base_contracts_and_keep_behavior_separate() {
    let plain = editor_model::EditorModel::new(editor_model::PLAIN_MIN);
    assert_eq!(
        plain.capabilities(),
        json!({"workspace":{"applyEdit":true},"textDocument":{"codeAction":{}}})
    );
    let rich = editor_model::EditorModel::new(editor_model::RICH_MIN);
    assert_eq!(
        rich.capabilities(),
        json!({"experimental":{"magit":{"v":1,"keys":"dynamic","modifiers":"basic","layout":["main"],"effects":[],"prompts":[]}}})
    );
    assert!(matches!(
        magit_lsp::presentation::negotiate(&json!({"capabilities":rich.capabilities()})),
        Ok(magit_lsp::presentation::Presentation::Rich(_))
    ));
    let composed = plain.clone().with_module(editor_model::SILENT_STALE_APPLY);
    assert_eq!(composed.capabilities(), plain.capabilities());
    assert_eq!(
        composed.behaviors().collect::<Vec<_>>(),
        ["silent-stale-apply"]
    );
    let declarations = plain.clone().with_module(editor_model::EXTRA_DECLARATIONS);
    assert_eq!(
        declarations.capabilities(),
        json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true}},"textDocument":{"codeAction":{}},"window":{"showDocument":{"support":true}}})
    );
    let empty = magit_lsp::quirks::Resolution {
        enabled: Default::default(),
    };
    let model = editor_model::EditorModel::new(editor_model::PLAIN_MIN)
        .with_module(editor_model::DECOMPOSED_CHANGES);
    let mut modeled = FakeEditor::from_model(Profile::Standard, &model, &empty);
    modeled.open("file:///model.txt", "ab");
    assert!(
        modeled.edit("file:///model.txt", "ac")["params"]["contentChanges"][0]
            .get("range")
            .is_some()
    );
    let mut pump = ClientPump::with_model(stdio::TestServer::spawn(&[]), Profile::Standard, &rich);
    assert!(matches!(
        magit_lsp::presentation::negotiate(&json!({"capabilities":rich.capabilities()})),
        Ok(magit_lsp::presentation::Presentation::Rich(_))
    ));
    pump.finish();
}

#[test]
fn recorded_presets_and_named_profiles_bind_declarations_and_registry_behaviors() {
    use editor_model::presets::{model, profile_model, NamedProfile, Preset};
    let plain = model(Preset::StockNeovim);
    let helix = model(Preset::Helix);
    let bundled = model(Preset::BundledNeovim);
    let vscode = model(Preset::BundledVsCode);
    for (preset, identity, expected) in [
        (
            plain,
            0,
            serde_json::from_str::<Value>(include_str!(
                "fixtures/lsp_traces/initialize-plain-neovim-0.12.6.json"
            ))
            .unwrap()["capabilities"]
                .clone(),
        ),
        (
            helix,
            1,
            serde_json::from_str::<Value>(include_str!(
                "fixtures/lsp_traces/initialize-helix-25.07.1.json"
            ))
            .unwrap()["capabilities"]
                .clone(),
        ),
        (bundled, 2, {
            let rows: Value =
                serde_json::from_str(include_str!("fixtures/current-client-capabilities.json"))
                    .unwrap();
            let row = rows
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["client"]["name"] == "Neovim")
                .unwrap();
            json!({"experimental":{"magit":row["magit"]}})
        }),
        (vscode, 3, {
            let rows: Value =
                serde_json::from_str(include_str!("fixtures/current-client-capabilities.json"))
                    .unwrap();
            let row = rows
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["client"]["name"] == "VSCodium")
                .unwrap();
            json!({"experimental":{"magit":row["magit"]}})
        }),
    ] {
        let (preset, identity_value) = preset;
        let capabilities = preset.capabilities();
        assert_eq!(capabilities, expected, "preset {identity}");
        if identity >= 2 {
            assert!(matches!(
                magit_lsp::presentation::negotiate(&json!({"capabilities":capabilities})),
                Ok(magit_lsp::presentation::Presentation::Rich(_))
            ));
        }
        let (client_name, client_version) = (
            identity_value["name"].as_str(),
            identity_value["version"].as_str(),
        );
        let resolution = magit_lsp::quirks::resolve(
            magit_lsp::quirks::Client {
                name: client_name,
                version: client_version,
                capabilities: &capabilities,
            },
            None,
        )
        .unwrap();
        let mut editor = FakeEditor::from_model(Profile::Standard, &preset, &resolution);
        editor.open("file:///preset", "old");
        for quirk in magit_lsp::quirks::REGISTRY
            .iter()
            .filter(|quirk| resolution.enabled.contains(quirk.id))
        {
            match quirk.model {
                Some("neovim-accepts-undeclared-documentChanges") => {
                    editor.profile = Profile::NoDocumentChanges;
                    let edit = json!({"documentChanges":[{"textDocument":{"uri":"file:///preset","version":1},"edits":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":3}},"newText":"new"}]}]});
                    assert_eq!(editor.apply_edit(&edit)["applied"], true);
                    assert_eq!(editor.text("file:///preset"), "new");
                }
                Some("silent-stale-apply") => {
                    editor.edit("file:///preset", "local");
                    let edit = json!({"documentChanges":[{"textDocument":{"uri":"file:///preset","version":1},"edits":[replacement(0,3,"BAD")]},{"textDocument":{"uri":"file:///later","version":1},"edits":[replacement(0,5,"later")]}]});
                    editor.open("file:///later", "stale");
                    editor.edit("file:///later", "local");
                    assert_eq!(editor.apply_edit(&edit)["applied"], true);
                    assert_eq!(editor.text("file:///preset"), "local");
                    assert_eq!(editor.text("file:///later"), "later");
                }
                Some("decomposed-changes") => {
                    let change = editor.edit("file:///preset", "new");
                    assert!(change["params"]["contentChanges"].as_array().unwrap().len() > 1);
                    let mut replay = FakeEditor::new(Profile::Standard);
                    replay.open("file:///preset", "old");
                    for part in change["params"]["contentChanges"].as_array().unwrap() {
                        assert!(part["range"].is_object());
                        assert_eq!(replay.apply_edit(&json!({"changes":{"file:///preset":[{"range":part["range"],"newText":part["text"]}]}}))["applied"], true);
                    }
                    assert_eq!(replay.text("file:///preset"), "new");
                }
                Some(other) => panic!("unhandled resolved registry model {other}"),
                None => {}
            }
        }
    }
    assert_eq!(
        profile_model(NamedProfile::Plain).capabilities(),
        editor_model::EditorModel::new(editor_model::PLAIN_MIN).capabilities()
    );
    assert_eq!(
        profile_model(NamedProfile::RichNative).capabilities()["experimental"]["magit"]["keys"],
        "dynamic"
    );
    assert_eq!(
        profile_model(NamedProfile::RichNative).capabilities()["experimental"]["magit"]
            ["modifiers"],
        "basic"
    );
    assert_eq!(
        profile_model(NamedProfile::RichDoc).capabilities()["experimental"]["magit"]["keys"],
        "static"
    );
    assert_eq!(
        profile_model(NamedProfile::RichDoc).capabilities()["experimental"]["magit"]["modifiers"],
        "full"
    );
    let empty = json!({});
    let unknown = magit_lsp::quirks::resolve(
        magit_lsp::quirks::Client {
            name: Some("unknown"),
            version: Some("1"),
            capabilities: &empty,
        },
        None,
    )
    .unwrap();
    assert!(unknown.enabled.is_empty());
    let mut unknown_editor = FakeEditor::from_resolution(Profile::Standard, &unknown);
    unknown_editor.open("file:///unknown", "old");
    assert_eq!(
        unknown_editor.edit("file:///unknown", "new")["params"]["contentChanges"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(unknown_editor.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":"file:///unknown","version":1},"edits":[]}]}))["applied"], false);
}

#[test]
fn strict_feature_guard_distinguishes_declarations_from_editor_behavior() {
    let plain = editor_model::EditorModel::new(editor_model::PLAIN_MIN).strict();
    let doc_changes = json!({"jsonrpc":"2.0","id":1,"method":"workspace/applyEdit","params":{"edit":{"documentChanges":[{"textDocument":{"uri":"file:///strict","version":1},"edits":[]}]}}});
    assert!(
        editor_model::strict::check(&plain.capabilities(), &doc_changes)
            .unwrap_err()
            .contains("documentChanges")
    );
    let declared = editor_model::EditorModel::new(editor_model::PLAIN_MIN)
        .with_module(editor_model::EXTRA_DECLARATIONS);
    assert!(editor_model::strict::check(&declared.capabilities(), &doc_changes).is_ok());
    let required = [
        (
            "undeclared applyEdit",
            json!({"method":"workspace/applyEdit","params":{"edit":{"changes":{}}}}),
        ),
        (
            "showDocument",
            json!({"method":"window/showDocument","params":{}}),
        ),
        (
            "progress create",
            json!({"method":"window/workDoneProgress/create","params":{}}),
        ),
        ("progress", json!({"method":"$/progress","params":{}})),
    ];
    for (feature, message) in required {
        let mut capabilities = plain.capabilities();
        if feature == "undeclared applyEdit" {
            capabilities["workspace"]["applyEdit"] = json!(false);
        }
        assert!(
            editor_model::strict::check(&capabilities, &message).is_err(),
            "{feature}"
        );
    }
    let rich = editor_model::EditorModel::new(editor_model::RICH_MIN);
    let rejected = [
        json!({"method":"view/render","params":{}}),
        json!({"method":"view/effects","params":{"effects":[{"type":"message"},{"type":"clipboard"}]}}),
        json!({"method":"view/effects","params":{"effects":[{"type":"unknown"}]}}),
        json!({"method":"view/prompt","params":{"kind":"confirm"}}),
        json!({"method":"view/effects","params":{"effects":[{"type":"pushView","descriptor":{"placement":"side"}}]}}),
        json!({"method":"view/unknown","params":{}}),
    ];
    for message in rejected {
        let declarations = if message["method"] == "view/render" {
            plain.capabilities()
        } else {
            rich.capabilities()
        };
        assert!(
            editor_model::strict::check(&declarations, &message).is_err(),
            "{message}"
        );
    }
    assert!(editor_model::strict::check(
        &plain.capabilities(),
        &json!({"method":"view/effects","params":{"effects":[{"type":"message"}]}})
    )
    .is_err());
    let rich_core_message =
        json!({"method":"view/effects","params":{"effects":[{"type":"message"}]}});
    assert!(editor_model::strict::check(&rich.capabilities(), &rich_core_message).is_ok());
    for (feature, declaration, message) in [
        (
            "clipboard",
            json!({"effects":["clipboard"]}),
            json!({"method":"view/effects","params":{"effects":[{"type":"message"},{"type":"clipboard"}]}}),
        ),
        (
            "prompt",
            json!({"prompts":["confirm"]}),
            json!({"method":"view/prompt","params":{"kind":"confirm"}}),
        ),
        (
            "layout",
            json!({"layout":["main","side"]}),
            json!({"method":"view/effects","params":{"effects":[{"type":"pushView","descriptor":{"placement":"side"}}]}}),
        ),
    ] {
        let mut capabilities = rich.capabilities();
        for (key, value) in declaration.as_object().unwrap() {
            capabilities["experimental"]["magit"][key] = value.clone();
        }
        assert!(
            editor_model::strict::check(&capabilities, &message).is_ok(),
            "declared {feature}"
        );
    }
    let behavior = editor_model::EditorModel::new(editor_model::PLAIN_MIN)
        .with_module(editor_model::NEOVIM_UNDECLARED_CHANGES);
    let mut editor = FakeEditor::from_model(
        Profile::NoDocumentChanges,
        &behavior,
        &magit_lsp::quirks::Resolution {
            enabled: Default::default(),
        },
    );
    editor.open("file:///strict", "text");
    let nonempty = json!({"documentChanges":[{"textDocument":{"uri":"file:///strict","version":1},"edits":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":4}},"newText":"edit"}]}]});
    assert!(editor_model::strict::check(
        &behavior.capabilities(),
        &json!({"method":"workspace/applyEdit","params":{"edit":nonempty}})
    )
    .is_err());
    assert_eq!(editor.apply_edit(&nonempty)["applied"], true);
    assert_eq!(editor.text("file:///strict"), "edit");
}

#[test]
fn rich_server_render_ack_and_stale_resync_preserve_caller_reply() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("init", [("f.txt", fixtures::TEN_LINES)])
        .build();
    let model = editor_model::EditorModel::new(editor_model::RICH_MIN).strict();
    let mut client =
        ClientPump::with_model(stdio::TestServer::spawn(&[]), Profile::Standard, &model);
    let open = client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/open","params":{"kind":"status","args":{"repo_path":repo.path().to_string_lossy()},"viewport":{"cols":80,"rows":24}}}));
    let opened = client.until_response(open);
    let descriptor = &opened["result"];
    assert!(descriptor["id"].is_string(), "{opened}");
    let view = descriptor["id"].as_str().unwrap().to_owned();
    client.until(|c| c.rich.as_ref().is_some_and(|r| r.views.contains_key(&view)));
    let state = client.rich.as_ref().unwrap().views[&view].clone();
    let initial = client
        .server
        .notifications
        .iter()
        .find(|m| m["method"] == "view/render" && m["params"]["viewId"] == view)
        .unwrap();
    assert_eq!(
        state.lines,
        serde_json::from_value::<Vec<String>>(initial["params"]["text"].clone()).unwrap()
    );
    assert_eq!(
        state.version,
        initial["params"]["version"].as_u64().unwrap()
    );
    assert!(client
        .server
        .sent
        .iter()
        .any(|m| m["method"] == "view/rendered"
            && m["params"]["viewId"] == view
            && m["params"]["version"] == state.version));
    let unrelated = client
        .send(json!({"jsonrpc":"2.0","id":0,"method":"view/resync","params":{"viewId":view}}));
    client.until(|c| {
        c.server
            .notifications
            .iter()
            .any(|m| m["method"] == "view/render" && m["params"]["viewId"] == view)
    });
    client
        .rich
        .as_mut()
        .unwrap()
        .views
        .get_mut(&view)
        .unwrap()
        .version = 0;
    let new_file = repo.path().join("new-file.txt");
    fs::write(&new_file, "watcher-visible\n").unwrap();
    client.until(|c| c.server.sent.iter().any(|m| m["method"] == "view/resync"));
    let resync = client
        .server
        .sent
        .iter()
        .find(|m| m["method"] == "view/resync")
        .unwrap();
    let resync_index = client
        .server
        .sent
        .iter()
        .position(|m| m["method"] == "view/resync")
        .unwrap();
    assert!(resync["id"].is_number());
    assert_eq!(resync["params"]["viewId"], view);
    client.until(|c| {
        c.rich.as_ref().is_some_and(|r| {
            r.views
                .get(&view)
                .is_some_and(|s| s.lines.iter().any(|line| line.contains("new-file.txt")))
        })
    });
    let recovered = &client.rich.as_ref().unwrap().views[&view];
    let ack = client
        .server
        .sent
        .iter()
        .skip(resync_index + 1)
        .find(|m| {
            m["method"] == "view/rendered"
                && m["params"]["viewId"] == view
                && m["params"]["version"] == recovered.version
        })
        .expect("recovery ack");
    assert_eq!(ack["params"]["version"], recovered.version);
    let response = client.until_response(unrelated);
    assert!(response.get("result").is_some(), "{response}");
    assert_eq!(repo.git(&["status", "--short"]), "?? new-file.txt\n");
    client.finish();
}

#[test]
fn strict_guard_covers_response_effects_and_declared_feature_matrix() {
    let plain = editor_model::EditorModel::new(editor_model::PLAIN_MIN).capabilities();
    let rich = editor_model::EditorModel::new(editor_model::RICH_MIN).capabilities();
    let static_rich =
        editor_model::presets::profile_model(editor_model::presets::NamedProfile::RichDoc)
            .capabilities();
    let dynamic_rich =
        editor_model::presets::profile_model(editor_model::presets::NamedProfile::RichNative)
            .capabilities();
    let ordinary = json!({"jsonrpc":"2.0","id":1,"result":{"ok":true}});
    assert!(editor_model::strict::check(&plain, &ordinary).is_ok());
    for placement in ["main", "side", "bottom", "float"] {
        let response =
            json!({"jsonrpc":"2.0","id":1,"result":{"descriptor":{"placement":placement}}});
        assert_eq!(
            editor_model::strict::check(&rich, &response).is_ok(),
            placement == "main",
            "base declaration {placement}"
        );
        let mut declared = rich.clone();
        declared["experimental"]["magit"]["layout"] = json!([placement]);
        assert!(
            editor_model::strict::check(&declared, &response).is_ok(),
            "declared {placement}"
        );
    }
    for (name, declaration, effect) in [
        ("clipboard", "clipboard", json!({"type":"clipboard"})),
        ("visitFile", "visitFile", json!({"type":"visitFile"})),
        (
            "editDocument",
            "editDocument",
            json!({"type":"editDocument"}),
        ),
    ] {
        let response = json!({"jsonrpc":"2.0","id":1,"result":{"effects":[effect]}});
        assert!(
            editor_model::strict::check(&rich, &response).is_err(),
            "undeclared {name}"
        );
        let mut declared = rich.clone();
        declared["experimental"]["magit"]["effects"] = json!([declaration]);
        assert!(
            editor_model::strict::check(&declared, &response).is_ok(),
            "declared {name}"
        );
    }
    let mut all = rich.clone();
    all["experimental"]["magit"]["layout"] = json!(["main", "side", "bottom", "float"]);
    all["experimental"]["magit"]["effects"] = json!(["clipboard", "visitFile", "editDocument"]);
    let render = json!({"method":"view/render","params":{}});
    let keymap = json!({"method":"view/keymap","params":{}});
    assert!(editor_model::strict::check(&plain, &render).is_err());
    assert!(editor_model::strict::check(&plain, &keymap).is_err());
    for (profile, message) in [
        ("rich-doc", &render),
        ("rich-doc", &keymap),
        ("rich-native", &render),
        ("rich-native", &keymap),
    ] {
        let capabilities = if profile == "rich-doc" {
            &static_rich
        } else {
            &dynamic_rich
        };
        assert_eq!(
            editor_model::strict::check(capabilities, message).is_ok(),
            message["method"] == "view/render" || profile == "rich-native",
            "{profile} {}",
            message["method"]
        );
    }
    for prompt in ["text", "confirm", "choice", "multiChoice"] {
        let message = json!({"method":"view/prompt","params":{"kind":prompt}});
        assert!(
            editor_model::strict::check(&rich, &message).is_err(),
            "undeclared {prompt}"
        );
        let mut declared = rich.clone();
        declared["experimental"]["magit"]["prompts"] = json!([prompt]);
        assert!(
            editor_model::strict::check(&declared, &message).is_ok(),
            "declared {prompt}"
        );
    }
    let secret_text = json!({"method":"view/prompt","params":{"kind":"text","secret":true}});
    assert!(editor_model::strict::check(&rich, &secret_text).is_err());
    let mut text_declared = rich.clone();
    text_declared["experimental"]["magit"]["prompts"] = json!(["text"]);
    assert!(editor_model::strict::check(&text_declared, &secret_text).is_err());
    let mut secret_declared = rich.clone();
    secret_declared["experimental"]["magit"]["prompts"] = json!(["secret"]);
    assert!(editor_model::strict::check(&secret_declared, &secret_text).is_err());
    let mut both_declared = rich.clone();
    both_declared["experimental"]["magit"]["prompts"] = json!(["text", "secret"]);
    assert!(editor_model::strict::check(&both_declared, &secret_text).is_ok());
    let unknown_secret = json!({"method":"view/prompt","params":{"kind":"unknown","secret":true}});
    assert!(editor_model::strict::check(&both_declared, &unknown_secret).is_err());
    let mixed = json!({"jsonrpc":"2.0","id":1,"result":{"effects":[{"type":"clipboard"},{"type":"unknown"}]}});
    assert!(editor_model::strict::check(&all, &mixed).is_err());
    for (feature, declaration, message) in [
        (
            "applyEdit",
            json!({"workspace":{"applyEdit":true}}),
            json!({"method":"workspace/applyEdit","params":{"edit":{"changes":{}}}}),
        ),
        (
            "documentChanges",
            json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true}}}),
            json!({"method":"workspace/applyEdit","params":{"edit":{"documentChanges":[]}}}),
        ),
        (
            "showDocument",
            json!({"window":{"showDocument":{"support":true}}}),
            json!({"method":"window/showDocument","params":{}}),
        ),
        (
            "progress create",
            json!({"window":{"workDoneProgress":true}}),
            json!({"method":"window/workDoneProgress/create","params":{}}),
        ),
        (
            "progress",
            json!({"window":{"workDoneProgress":true}}),
            json!({"method":"$/progress","params":{}}),
        ),
    ] {
        let mut capabilities = plain.clone();
        for (key, value) in declaration.as_object().unwrap() {
            capabilities[key] = value.clone();
        }
        assert!(
            editor_model::strict::check(&capabilities, &message).is_ok(),
            "declared {feature}"
        );
    }
    assert!(
        editor_model::strict::check(&all, &json!({"method":"view/unknown","params":{}})).is_err()
    );
}

#[test]
fn real_binary_lifecycle_and_current_unsupported_methods_are_honest() {
    let mut client = ClientPump::new(stdio::TestServer::spawn(&[]), Profile::Standard, caps());
    let uri = "file:///tmp/plain-harness.txt";
    let frame = client.editor.open(uri, "hello\n");
    client.send(frame);
    let frame = client.editor.edit(uri, "edited\n");
    client.send(frame);
    let frame = client.editor.save(uri);
    client.send(frame);
    assert_eq!(client.editor.text(uri), "edited\n");
    let frame = client.editor.code_action(
        uri,
        json!({"start":{"line":0,"character":0},"end":{"line":0,"character":1}}),
    );
    let ca_id = client.send(frame);
    let frame = client.editor.execute_command("magit.command", json!([]));
    let ec_id = client.send(frame);
    // Await the second request first: the pump must retain the first response.
    let ec = client.until_response(ec_id);
    assert!(ec["error"].is_object());
    let ca = client.until_response(ca_id);
    assert!(ca.get("result").is_some() || ca["error"].is_object());
    let frame = client.editor.close(uri);
    client.send(frame);
    assert!(!client.editor.documents.contains_key(uri));
    client.finish();
}

#[test]
fn plain_did_open_apply_edit_is_pumped_and_echoed() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", fixtures::TEN_LINES)])
        .modify_standard()
        .build();
    let path = repo.path().join(".magit");
    fixtures::write(&path, "disk sentinel\n");
    let uri = format!("file://{}", path.display());
    let model = editor_model::EditorModel::new(editor_model::PLAIN_MIN).strict();
    let mut client =
        ClientPump::with_model(stdio::TestServer::spawn(&[]), Profile::Standard, &model);
    let open = client.editor.open(&uri, "stale\n");
    client.send(open);
    client.until(|client| client.editor.text(&uri).contains("f.txt"));

    let requests: Vec<_> = client
        .editor
        .outbound
        .iter()
        .filter(|message| message["method"] == "workspace/applyEdit")
        .collect();
    assert!(!requests.is_empty(), "server sent no workspace/applyEdit");
    assert!(client.server.sent.iter().any(|message| {
        message["id"] == requests[0]["id"] && message["result"]["applied"] == true
    }));
    assert!(client.server.sent.iter().any(|message| {
        message["method"] == "textDocument/didChange"
            && message["params"]["textDocument"]["uri"] == uri
    }));
    assert_eq!(
        client
            .server
            .sent
            .iter()
            .filter(|message| message["method"] == "textDocument/didOpen")
            .count(),
        1
    );
    assert!(client
        .server
        .sent
        .iter()
        .all(|message| message["method"] != "view/render"));
    assert!(client
        .server
        .notifications
        .iter()
        .all(|message| message["method"] != "view/render"));
    assert_eq!(fs::read(&path).unwrap(), b"disk sentinel\n");
    client.finish();
}

#[test]
fn all_documented_profiles_have_assertable_behavior() {
    let profiles = [
        Profile::DecomposedChanges,
        Profile::DebouncedDidChange,
        Profile::ReorderEcho,
        Profile::NoDocumentChanges,
        Profile::NoShowDocument,
        Profile::CrlfNormalise,
        Profile::Utf16,
    ];
    for p in profiles {
        let mut e = FakeEditor::new(p);
        let uri = "file:///x";
        e.open(uri, "a\nb\n");
        match p {
            Profile::DecomposedChanges => {
                let n = e.edit(uri, "A\nB\n");
                let c = n["params"]["contentChanges"].as_array().unwrap();
                assert_eq!(c.len(), 3);
                assert!(c.iter().all(|v| v["range"].is_object()));
            }
            Profile::DebouncedDidChange => {
                e.edit(uri, "queued");
                assert!(e.has_pending());
                let sent = e.flush();
                assert!(!e.has_pending());
                assert_eq!(sent.len(), 1);
                assert_eq!(sent[0]["params"]["contentChanges"][0]["text"], "queued");
            }
            Profile::ReorderEcho => {
                let req = json!({"jsonrpc":"2.0","id":17,"method":"workspace/applyEdit","params":{"edit":{"changes":{"file:///x":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"z"}]}}}});
                let response = e.respond(&req);
                let echo = e.edit(uri, "echo");
                assert_eq!(response["id"], 17);
                assert_eq!(response["result"]["applied"], true);
                assert_eq!(echo["params"]["textDocument"]["version"], 3);
                assert_eq!(e.text(uri), "echo");
                assert_eq!(e.outbound[0]["method"], "workspace/applyEdit");
            }
            Profile::NoDocumentChanges => {
                assert_eq!(e.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":uri,"version":1},"edits":[]}]}))["applied"],false);
                assert_eq!(e.apply_edit(&json!({"changes":{"file:///x":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"z"}]}}))["applied"],true);
            }
            Profile::NoShowDocument => {
                assert_eq!(
                    e.respond(&json!({"id":5,"method":"window/showDocument"}))["result"]["success"],
                    false
                );
            }
            Profile::CrlfNormalise => {
                e.documents.get_mut(uri).unwrap().text = "a\r\nb".into();
                e.apply_edit(&json!({"changes":{"file:///x":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"z"}]}}));
                assert!(!e.text(uri).contains("\r\n"));
            }
            Profile::Utf16 => {
                e.documents.get_mut(uri).unwrap().text = "😀x".into();
                assert_eq!(e.apply_edit(&json!({"changes":{"file:///x":[{"range":{"start":{"line":0,"character":2},"end":{"line":0,"character":3}},"newText":"y"}]}}))["applied"],true);
                assert_eq!(e.text(uri), "😀y");
            }
            Profile::Standard => unreachable!(),
        }
    }
}

#[test]
fn registry_models_drive_fake_editor_observable_behavior() {
    use magit_lsp::quirks::{self, Client, Resolution, REGISTRY};
    for quirk in REGISTRY.iter().filter(|q| q.model.is_some()) {
        let resolution = Resolution {
            enabled: [quirk.id].into_iter().collect(),
        };
        let profile = if quirk.model == Some("neovim-accepts-undeclared-documentChanges") {
            Profile::NoDocumentChanges
        } else {
            Profile::Standard
        };
        let model = editor_model::EditorModel::new(editor_model::PLAIN_MIN);
        let mut editor = FakeEditor::from_model(profile, &model, &resolution);
        let uri = "file:///model";
        editor.open(uri, "old");
        match quirk.model.unwrap() {
            "neovim-accepts-undeclared-documentChanges" => {
                assert_eq!(editor.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":uri,"version":1},"edits":[]}]}))["applied"], true);
            }
            "silent-stale-apply" => {
                editor.edit(uri, "new");
                let result = editor.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":uri,"version":1},"edits":[replacement(0,3,"BAD")]}]}));
                assert_eq!(result["applied"], true);
                assert_eq!(editor.text(uri), "new");
            }
            "decomposed-changes" => {
                let notification = editor.edit(uri, "new");
                assert!(notification["params"]["contentChanges"][0]["range"].is_object());
            }
            other => panic!("unhandled registry model: {other}"),
        }
    }
    let empty = json!({});
    let neovim = quirks::resolve(
        Client {
            name: Some("Neovim"),
            version: Some("0.12.6"),
            capabilities: &empty,
        },
        None,
    )
    .unwrap();
    let mut editor = FakeEditor::from_resolution(Profile::NoDocumentChanges, &neovim);
    let uri = "file:///combined";
    editor.open(uri, "old");
    assert_eq!(
        editor.apply_edit(
            &json!({"documentChanges":[{"textDocument":{"uri":uri,"version":1},"edits":[]}]})
        )["applied"],
        true
    );
    editor.edit(uri, "new");
    editor.open("file:///later", "old");
    editor.edit("file:///later", "later-local");
    let stale = editor.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":uri,"version":1},"edits":[replacement(0,3,"FIRST") ]},{"textDocument":{"uri":"file:///later","version":1},"edits":[replacement(0,11,"LATER")]}]}));
    assert_eq!(stale["applied"], true);
    assert_eq!(editor.text(uri), "new");
    assert_eq!(editor.text("file:///later"), "LATER");
    let helix = quirks::resolve(
        Client {
            name: Some("helix"),
            version: Some("25.07.1"),
            capabilities: &empty,
        },
        None,
    )
    .unwrap();
    let mut editor = FakeEditor::from_resolution(Profile::Standard, &helix);
    editor.open(uri, "old");
    assert!(editor.edit(uri, "new")["params"]["contentChanges"][0]["range"].is_object());
}

#[test]
fn plain_eof_append_does_not_repair_a_native_intermediate_echo_with_a_stale_version() {
    use magit_lsp::docstore::Encoding;
    use magit_lsp::presentation::plain::{hash_text, DocumentFormat, PlainDocument};

    let uri = "file:///repo/.magit";
    let before = "Head: main\n\nUntracked files (0)\n";
    let target = format!("{before}file:///repo/.git/COMMIT_EDITMSG\n");
    let mut editor = FakeEditor::new(Profile::Standard);
    let opened = editor.open(uri, before);
    let mut document =
        PlainDocument::new(uri, Encoding::Utf8, true, DocumentFormat::from_text(before));
    assert!(document
        .open(
            opened["params"]["textDocument"]["version"]
                .as_i64()
                .unwrap() as i32,
            before
        )
        .is_none());
    let edit = document.render(&target).unwrap();
    let range = &edit.edit["documentChanges"][0]["edits"][0]["range"];
    // Native Neovim util.apply_text_edits appends at virtual EOF with
    // nvim_buf_set_lines, then removes the extra final line. Its first
    // didChange can precede the result and its cleanup didChange follow it
    // (plain-commit lsp.log, 20261117-073832-04b2ec, lines 78–85).
    assert_ne!(
        range["start"]["line"].as_u64().unwrap(),
        before.lines().count() as u64
    );
    let frames = editor.dispatch(&json!({"jsonrpc":"2.0","id":edit.id,"method":"workspace/applyEdit","params":{"edit":edit.edit}}));
    assert_eq!(frames.last().unwrap()["result"]["applied"], true);
    let echo = frames
        .into_iter()
        .find(|frame| frame["method"] == "textDocument/didChange")
        .expect("the applied edit is echoed");
    let observed = editor.text(uri).to_owned();
    assert!(document
        .change(
            echo["params"]["textDocument"]["version"].as_i64().unwrap() as i32,
            observed
        )
        .is_none());
    let repair = document.response(edit.id, true);
    assert!(
        repair.is_none(),
        "the applied edit must not produce a repair"
    );
    assert_eq!(editor.text(uri), target);
    assert_eq!(hash_text(editor.text(uri)), hash_text(&target));
    assert!(!document.take_format_warning());
}

#[test]
fn text_edits_preserve_terminal_newlines_and_unicode_boundaries() {
    use magit_lsp::docstore::Encoding;
    use magit_lsp::presentation::plain::text_edits;

    for (old, new) in [
        ("alpha\n", "alpha\nbeta\n"),
        ("alpha\r\n", "alpha\r\nbeta\r\n"),
        ("alpha\r\nbeta\r\n", "alpha\nbeta\n"),
        ("alpha\nbeta\n", "alpha\r\nbeta\r\n"),
        ("x😀\n", "x🧪\n"),
        ("", "X\n"),
        ("X\n", ""),
    ] {
        let edit = &text_edits(old, new, Encoding::Utf16)[0];
        let range = &edit["range"];
        let start_line = range["start"]["line"].as_u64().unwrap() as usize;
        let start_char = range["start"]["character"].as_u64().unwrap() as usize;
        let end_line = range["end"]["line"].as_u64().unwrap() as usize;
        let end_char = range["end"]["character"].as_u64().unwrap() as usize;
        if old.ends_with('\n') && new.ends_with('\n') {
            assert_ne!(start_line, old.lines().count(), "{old:?} -> {new:?}");
        }
        let offset = |text: &str, line: usize, character: usize| {
            let mut lines = text.split_inclusive('\n');
            let prefix: usize = lines.by_ref().take(line).map(str::len).sum();
            let row = text[prefix..].split('\n').next().unwrap();
            let mut units = 0;
            let mut bytes = 0;
            for ch in row.chars() {
                if units == character {
                    break;
                }
                units += ch.len_utf16();
                bytes += ch.len_utf8();
            }
            assert_eq!(units, character);
            prefix + bytes
        };
        let start = offset(old, start_line, start_char);
        let end = offset(old, end_line, end_char);
        let reconstructed = format!(
            "{}{}{}",
            &old[..start],
            edit["newText"].as_str().unwrap(),
            &old[end..]
        );
        assert_eq!(reconstructed, new, "{old:?} -> {new:?}");
    }
}

#[test]
fn versioned_apply_edit_validates_before_mutating_and_supports_multiple_changes() {
    let mut e = FakeEditor::new(Profile::Standard);
    let u = "file:///d";
    e.open(u, "a\nb\n");
    let result=e.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":u,"version":1},"edits":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"A"},{"range":{"start":{"line":1,"character":0},"end":{"line":1,"character":1}},"newText":"B"}]}]}));
    assert_eq!(result["applied"], true);
    assert_eq!(e.text(u), "A\nB\n");
    let before = e.text(u).to_owned();
    let stale = e.apply_edit(
        &json!({"documentChanges":[{"textDocument":{"uri":u,"version":1},"edits":[]}]}),
    );
    assert_eq!(stale["applied"], false);
    assert!(stale["failureReason"].is_string());
    assert_eq!(e.text(u), before);
}

#[test]
fn overlapping_and_malformed_edits_reject_atomically_across_documents() {
    let mut e = FakeEditor::new(Profile::Standard);
    e.open("file:///a", "abcd");
    e.open("file:///b", "wxyz");
    let result=e.apply_edit(&json!({"changes":{"file:///a":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":2}},"newText":"A"},{"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":3}},"newText":"B"}],"file:///b":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"W"}]}}));
    assert_eq!(result["applied"], false);
    assert_eq!(e.text("file:///a"), "abcd");
    assert_eq!(e.text("file:///b"), "wxyz");
}

#[test]
fn utf16_surrogate_midpoint_is_rejected_and_crlf_lines_are_addressable() {
    let mut e = FakeEditor::new(Profile::Utf16);
    e.open("file:///u", "😀x\r\ny");
    assert_eq!(e.apply_edit(&json!({"changes":{"file:///u":[{"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":2}},"newText":"z"}]}}))["applied"],false);
    assert_eq!(e.apply_edit(&json!({"changes":{"file:///u":[{"range":{"start":{"line":1,"character":0},"end":{"line":1,"character":1}},"newText":"Y"}]}}))["applied"],true);
    assert_eq!(e.text("file:///u"), "😀x\r\nY");
}

fn replacement(start: u64, end: u64, text: &str) -> Value {
    json!({"range":{"start":{"line":0,"character":start},"end":{"line":0,"character":end}},"newText":text})
}

#[test]
fn original_snapshot_order_and_atomic_rejection_matrix() {
    let a = "file:///a";
    let b = "file:///b";
    let mut e = FakeEditor::new(Profile::Standard);
    e.open(a, "abcdef");
    e.open(b, "uvwxyz");
    // Deliberately reversed and length-changing: all offsets use the original.
    assert_eq!(e.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":a,"version":1},"edits":[replacement(4,6,"!"),replacement(0,1,"LONG")]},{"textDocument":{"uri":b,"version":1},"edits":[replacement(0,1,"U")]}]}))["applied"],true);
    assert_eq!(e.text(a), "LONGbcd!");
    assert_eq!(e.text(b), "Uvwxyz");
    let valid = json!({"textDocument":{"uri":a,"version":2},"edits":[replacement(0,1,"X")]});
    let invalids = vec![
        json!({"textDocument":{"uri":b,"version":1},"edits":[]}),
        json!({"textDocument":{"uri":b,"version":"2"},"edits":[]}),
        json!({"textDocument":{"uri":b},"edits":[]}),
        json!({"textDocument":{"uri":b,"version":2},"edits":{}}),
        json!({"textDocument":{"uri":b,"version":2},"edits":[replacement(0,99,"x")]}),
        json!({"textDocument":{"uri":b,"version":2},"edits":[replacement(3,1,"x")]}),
        json!({"textDocument":{"uri":b,"version":2},"edits":[replacement(0,3,"x"),replacement(2,4,"y")]}),
        json!({"textDocument":{"uri":a,"version":2},"edits":[]}),
        json!({"kind":"create","uri":"file:///new"}),
    ];
    for invalid in invalids {
        let result = e.apply_edit(&json!({"documentChanges":[valid.clone(),invalid]}));
        assert_eq!(result["applied"], false, "{result}");
        assert!(result["failureReason"].is_string());
        assert_eq!(e.text(a), "LONGbcd!");
        assert_eq!(e.text(b), "Uvwxyz");
        assert_eq!(e.documents[a].version, 2);
        assert_eq!(e.documents[b].version, 2);
    }
    for edit in [
        json!({"documentChanges":{}}),
        json!({"changes":{a:[replacement(0,1,"X")],b:{}}}),
        json!({"changes":{a:[replacement(0,1,"X")],"file:///missing":[]}}),
        json!({"documentChanges":[],"changes":{}}),
    ] {
        assert_eq!(e.apply_edit(&edit)["applied"], false);
        assert_eq!(e.text(a), "LONGbcd!");
        assert_eq!(e.documents[a].version, 2);
    }
}

#[test]
fn utf8_utf16_utf32_bounds_use_original_crlf_snapshots() {
    for (encoding, start, end, invalid) in [
        ("utf-8", 4, 5, Some(1)),
        ("utf-16", 2, 3, Some(1)),
        ("utf-32", 1, 2, None),
    ] {
        let mut e = FakeEditor::new(Profile::Standard);
        e.encoding = encoding;
        e.open("file:///u", "😀x\r\ny");
        if let Some(mid) = invalid {
            assert_eq!(
                e.apply_edit(&json!({"changes":{"file:///u":[replacement(mid,start,"bad")]}}))
                    ["applied"],
                false
            );
            assert_eq!(e.text("file:///u"), "😀x\r\ny");
        }
        assert_eq!(
            e.apply_edit(&json!({"changes":{"file:///u":[replacement(start,end,"Y")]}}))["applied"],
            true
        );
        assert_eq!(e.text("file:///u"), "😀Y\r\ny");
        assert_eq!(
            e.apply_edit(&json!({"changes":{"file:///u":[replacement(end+1,end+1,"bad")]}}))
                ["applied"],
            false
        );
    }
}

#[test]
fn many_decomposed_ranges_replay_sequentially_in_one_notification() {
    for encoding in ["utf-8", "utf-16", "utf-32"] {
        let uri = "file:///decomposed";
        let mut e = FakeEditor::new(Profile::DecomposedChanges);
        e.encoding = encoding;
        e.open(uri, "😀old\r\ntail");
        let expected = (0..25)
            .map(|i| format!("😀 line {i}\n"))
            .collect::<String>();
        let notification = e.edit(uri, &expected);
        let changes = notification["params"]["contentChanges"].as_array().unwrap();
        assert_eq!(changes.len(), 26);
        assert_eq!(notification["params"]["textDocument"]["version"], 2);
        let mut replay = FakeEditor::new(Profile::Standard);
        replay.encoding = encoding;
        replay.open(uri, "😀old\r\ntail");
        for change in changes {
            assert!(change["range"].is_object());
            assert_eq!(
                replay.apply_edit(
                    &json!({"changes":{uri:[{"range":change["range"],"newText":change["text"]}]}})
                )["applied"],
                true
            );
        }
        assert_eq!(replay.text(uri), expected);
        assert_eq!(e.text(uri), expected);
    }
}

#[test]
fn debounce_releases_no_echo_until_flush_on_actual_transport() {
    let mut client = ClientPump::new(
        stdio::TestServer::spawn(&[]),
        Profile::DebouncedDidChange,
        caps(),
    );
    let uri = "file:///debounce";
    let open = client.editor.open(uri, "a");
    client.send(open);
    let start = client.server.sent.len();
    client.dispatch(&json!({"jsonrpc":"2.0","id":"script/debounce","method":"workspace/applyEdit","params":{"edit":{"changes":{uri:[replacement(0,1,"b")]}}}}));
    assert_eq!(client.server.sent.len(), start + 1);
    assert!(client.editor.has_pending());
    assert_eq!(client.editor.text(uri), "b");
    for frame in client.editor.flush() {
        client.send(frame);
    }
    assert!(!client.editor.has_pending());
    assert_eq!(client.server.sent.len(), start + 2);
    assert_eq!(client.server.sent[start]["id"], "script/debounce");
    assert_eq!(
        client.server.sent[start + 1]["method"],
        "textDocument/didChange"
    );
    assert_eq!(
        client.server.sent[start + 1]["params"]["textDocument"]["version"],
        2
    );
    client.finish();
}

#[test]
fn shared_dispatch_captures_real_send_order_for_echo_profiles() {
    for (profile, expected) in [
        (Profile::Standard, ["textDocument/didChange", "response"]),
        (Profile::ReorderEcho, ["response", "textDocument/didChange"]),
    ] {
        let mut client = ClientPump::new(stdio::TestServer::spawn(&[]), profile, caps());
        let open = client.editor.open("file:///order", "a");
        client.send(open);
        // Scripted server request, but actual dispatcher and pipe writes, not production Plain acceptance.
        let request = json!({"jsonrpc":"2.0","id":"script/apply","method":"workspace/applyEdit","params":{"edit":{"documentChanges":[{"textDocument":{"uri":"file:///order","version":1},"edits":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"b"}]}]}}});
        let start = client.server.sent.len();
        client.dispatch(&request);
        let writes = &client.server.sent[start..];
        assert_eq!(
            writes
                .iter()
                .map(|m| m["method"].as_str().unwrap_or("response"))
                .collect::<Vec<_>>(),
            expected
        );
        assert!(writes.iter().any(|m| m["id"] == "script/apply"));
        assert_eq!(client.editor.text("file:///order"), "b");
        client.dispatch(&json!({"jsonrpc":"2.0","id":"script/show","method":"window/showDocument","params":{"uri":"file:///order"}}));
        assert_eq!(
            client.server.sent.last().unwrap()["result"]["success"],
            true
        );
        client.dispatch(&json!({"jsonrpc":"2.0","id":"script/unknown","method":"unknown"}));
        assert_eq!(client.server.sent.last().unwrap()["error"]["code"], -32601);
        client.finish();
    }
}

#[test]
fn live_binary_progress_requests_are_serviced_while_command_runs() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("init", [("f.txt", fixtures::TEN_LINES)])
        .modify_standard()
        .build();
    let head = repo.git(&["rev-parse", "HEAD"]);
    let mut client = ClientPump::new(stdio::TestServer::spawn(&[]), Profile::Standard, caps());
    let id=client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/open","params":{"kind":"status","args":{"repo_path":repo.path().to_string_lossy()},"viewport":{"cols":80,"rows":24}}}));
    let opened = client.until_response(id);
    let view = opened["result"]["id"]
        .as_str()
        .expect("opened view")
        .to_owned();
    let id=client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/invoke","params":{"viewId":view,"command":"status.stage-all"}}));
    let response = client.until_response(id);
    assert!(response.get("result").is_some(), "{response}");
    assert!(repo
        .git(&["diff", "--cached", "--name-only"])
        .contains("f.txt"));
    // Cancel only when the actual commit editor is waiting. Cancelling on begin
    // races the setup rev-parse, whose killed process returns an RPC error.
    client.cancel_on_edit = true;
    let id=client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/invoke","params":{"viewId":view,"command":"commit.create"}}));
    assert!(client.until_response(id).get("result").is_some());
    client.until(|c| {
        !c.cancel_on_edit
            && c.server
                .notifications
                .iter()
                .filter(|m| m["method"] == "$/progress" && m["params"]["value"]["kind"] == "end")
                .count()
                == 2
    });
    let requests: Vec<_> = client
        .editor
        .outbound
        .iter()
        .filter(|m| m["method"] == "window/workDoneProgress/create")
        .collect();
    assert_eq!(requests.len(), 2);
    assert_ne!(requests[0]["id"], requests[1]["id"]);
    for request in requests {
        assert!(client
            .server
            .sent
            .iter()
            .any(|m| m["id"] == request["id"] && m.get("result") == Some(&Value::Null)));
        let token = &request["params"]["token"];
        let phases: Vec<_> = client
            .server
            .notifications
            .iter()
            .filter(|m| m["method"] == "$/progress" && m["params"]["token"] == *token)
            .map(|m| m["params"]["value"]["kind"].as_str().unwrap())
            .collect();
        assert_eq!(phases.first(), Some(&"begin"));
        assert_eq!(phases.last(), Some(&"end"));
        assert!(phases[1..phases.len() - 1].iter().all(|p| *p == "report"));
    }
    assert!(client
        .server
        .sent
        .iter()
        .any(|m| m["method"] == "window/workDoneProgress/cancel"));
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    client.finish();
}

fn plain_code_actions(client: &mut ClientPump, uri: &str, line: u32) -> Vec<Value> {
    let request = client.editor.code_action(
        uri,
        json!({"start":{"line":line,"character":0},"end":{"line":line,"character":0}}),
    );
    let id = client.send(request);
    client.until_response(id)["result"]
        .as_array()
        .unwrap()
        .clone()
}

fn execute_plain_action(client: &mut ClientPump, args: Value) -> Value {
    let request = client
        .editor
        .execute_command("magit.command", json!([args]));
    let id = client.send(request);
    client.until_response(id)
}

fn plain_gate_open(repo: &fixtures::RepoFixture) -> (ClientPump, String) {
    let uri = format!("file://{}/.magit", repo.path().display());
    // Like plain_e2e, accept the command's opaque response payload. All editor
    // interaction must still use standard LSP, not custom requests/notices.
    let model = editor_model::EditorModel::new(editor_model::PLAIN_MIN);
    let mut client =
        ClientPump::with_model(stdio::TestServer::spawn(&[]), Profile::Standard, &model);
    let open = client.editor.open(&uri, "");
    client.send(open);
    client.until(|c| !c.editor.text(&uri).is_empty());
    (client, uri)
}

fn plain_gate_finish(client: ClientPump) {
    assert!(
        client
            .server
            .notifications
            .iter()
            .chain(&client.editor.outbound)
            .all(|message| !message["method"]
                .as_str()
                .is_some_and(|method| method.starts_with("view/"))),
        "Plain interaction must not require custom notifications or requests"
    );
    client.finish();
}

fn plain_gate_action(
    client: &mut ClientPump,
    uri: &str,
    row: &str,
    command: &str,
    key: Option<&str>,
) -> Value {
    // Text locates the cursor only; the opaque server target owns identity.
    let line = client
        .editor
        .text(uri)
        .lines()
        .position(|line| line.contains(row))
        .unwrap_or_else(|| panic!("missing cursor row {row}: {}", client.editor.text(uri)))
        as u32;
    plain_code_actions(client, uri, line)
        .into_iter()
        .find(|action| {
            let args = &action["command"]["arguments"][0];
            args["command"] == command && key.is_none_or(|key| args["target"]["itemKey"] == key)
        })
        .unwrap_or_else(|| panic!("missing {command} {key:?} on {row}"))["command"]["arguments"][0]
        .clone()
}

fn plain_gate_run(
    client: &mut ClientPump,
    uri: &str,
    row: &str,
    command: &str,
    key: Option<&str>,
) -> Value {
    let action = plain_gate_action(client, uri, row, command, key);
    let result = execute_plain_action(client, action);
    assert!(result["error"].is_null(), "{result}");
    result
}

fn plain_gate_state(
    repo: &fixtures::RepoFixture,
) -> (String, String, String, String, Vec<u8>, Vec<u8>, bool) {
    (
        repo.git(&["rev-parse", "HEAD"]),
        repo.git(&["ls-files", "--stage"]),
        repo.git(&["show-ref"]),
        repo.git(&["status", "--porcelain"]),
        fs::read(repo.path().join("f.txt")).unwrap(),
        fs::read(repo.path().join(".git/HEAD")).unwrap(),
        repo.path().join(".git/rebase-merge").exists()
            || repo.path().join(".git/rebase-apply").exists(),
    )
}

fn plain_gate_plan(client: &mut ClientPump, uri: &str, base_subject: &str) {
    plain_gate_run(client, uri, "", "open.rebase", None);
    plain_gate_run(client, uri, "Rebase", "menu.run-action", Some("i"));
    plain_gate_run(client, uri, base_subject, "common.confirm", None);
}

#[test]
fn plain_gate_background_refresh_preserves_todo_offer_without_rich_notifications() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("watcher base", [("f.txt", "base\n")])
        .commit("keep replay", [("keep", "keep\n")])
        .commit("drop replay", [("drop", "drop\n")])
        .build();
    let base = repo.git(&["rev-parse", "HEAD~2"]);
    let (mut client, uri) = plain_gate_open(&repo);
    let open = plain_gate_action(&mut client, &uri, "", "open.rebase", None);
    let status = open["target"]["viewId"].as_str().unwrap().to_owned();
    assert!(execute_plain_action(&mut client, open)["error"].is_null());
    plain_gate_run(&mut client, &uri, "Rebase", "menu.run-action", Some("i"));
    plain_gate_run(&mut client, &uri, "watcher base", "common.confirm", None);
    plain_gate_run(&mut client, &uri, "drop replay", "rebase.todo-drop", None);
    let submit = plain_gate_action(&mut client, &uri, "keep replay", "rebase.todo-submit", None);
    let todo = client.editor.text(&uri).to_owned();

    // Observe the background status through the production resync endpoint as
    // a barrier for a real watcher burst, not a timer or a fabricated render.
    for present in [true, false] {
        let marker = repo.path().join("watcher-proof");
        if present {
            fs::write(&marker, "background refresh\n").unwrap();
        } else {
            fs::remove_file(&marker).unwrap();
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            let id = client.send(
                json!({"jsonrpc":"2.0","id":0,"method":"view/resync","params":{"viewId":status}}),
            );
            let response = client.until_response(id);
            let lines = response["result"]["text"]
                .as_array()
                .expect("full status resync");
            if lines.iter().any(|line| {
                line.as_str()
                    .is_some_and(|line| line.contains("watcher-proof"))
            }) == present
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "watcher did not refresh background status"
            );
        }
    }
    assert_eq!(
        client.editor.text(&uri),
        todo,
        "background status must not replace todo"
    );
    assert!(
        client
            .server
            .notifications
            .iter()
            .all(|message| !message["method"]
                .as_str()
                .is_some_and(|method| method.starts_with("view/"))),
        "background refresh leaked a rich notification: {:?}",
        client.server.notifications
    );
    let result = execute_plain_action(&mut client, submit);
    assert!(
        !result["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["text"] == magit_lsp::errors::STALE_VIEW),
        "unchanged todo offer rejected: {result}\n{}",
        client.server.stderr_text()
    );
    assert!(result["error"].is_null(), "{result}");
    assert_eq!(
        repo.git(&["log", "--reverse", "--format=%s"]),
        "watcher base\nkeep replay\n"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD^"]), base);
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
    plain_gate_finish(client);
}

#[test]
fn plain_gate_fixup_selected_target_cancel_confirm_and_readable_failure() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("chosen target", [("f.txt", "base\n")])
        .commit("unselected tip", [("f.txt", "tip\n")])
        .staged_file("f.txt", "fixup content\n")
        .build();
    let before = plain_gate_state(&repo);
    let (mut client, uri) = plain_gate_open(&repo);
    let status = client.editor.text(&uri).to_owned();
    plain_gate_run(&mut client, &uri, "", "open.commit", None);
    plain_gate_run(&mut client, &uri, "Commit", "menu.run-action", Some("f"));
    plain_gate_run(&mut client, &uri, "chosen target", "common.cancel", None);
    assert_eq!(client.editor.text(&uri), status);
    assert_eq!(plain_gate_state(&repo), before);

    plain_gate_run(&mut client, &uri, "", "open.commit", None);
    plain_gate_run(&mut client, &uri, "Commit", "menu.run-action", Some("f"));
    let confirm = plain_gate_action(&mut client, &uri, "chosen target", "common.confirm", None);
    let target = repo.git(&["rev-parse", "HEAD^"]).trim().to_owned();
    assert_eq!(confirm["target"]["nodeId"], format!("log:commit:{target}"));
    let result = execute_plain_action(&mut client, confirm);
    assert!(result["error"].is_null(), "{result}");
    client.until(|c| !c.editor.text(&uri).contains("Log —"));
    assert_eq!(repo.git(&["rev-parse", "HEAD^"]), before.0);
    assert_eq!(repo.git(&["show", "HEAD:f.txt"]), "fixup content\n");
    assert_eq!(
        repo.git(&["cat-file", "commit", "HEAD"])
            .split_once("\n\n")
            .unwrap()
            .1,
        "fixup! chosen target\n"
    );
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
    assert!(
        client.editor.text(&uri).contains("Head:"),
        "confirm returns to status"
    );

    // No staged change: Git failure remains a readable standard-LSP message.
    let committed = plain_gate_state(&repo);
    let notifications = client.server.notifications.len();
    plain_gate_run(&mut client, &uri, "", "open.commit", None);
    plain_gate_run(&mut client, &uri, "Commit", "menu.run-action", Some("f"));
    plain_gate_run(&mut client, &uri, "chosen target", "common.confirm", None);
    client.until(|c| {
        c.server
            .notifications
            .iter()
            .skip(notifications)
            .any(|message| {
                message["method"] == "window/showMessage"
                    && matches!(message["params"]["type"].as_u64(), Some(1 | 2))
                    && message["params"]["message"]
                        .as_str()
                        .is_some_and(|text| !text.trim().is_empty() && !text.starts_with('{'))
            })
    });
    assert_eq!(plain_gate_state(&repo), committed);
    plain_gate_finish(client);
}

#[test]
fn plain_gate_rebase_selected_base_edited_todo_continue_and_cancel() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("selected base", [("f.txt", "base\n")])
        .commit("first replay", [("one", "one\n")])
        .commit("drop replay", [("two", "two\n")])
        .commit("last replay", [("three", "three\n")])
        .build();
    let before = plain_gate_state(&repo);
    let base = repo.git(&["rev-parse", "HEAD~3"]);
    let first = repo.git(&["rev-parse", "HEAD~2"]);
    let (mut client, uri) = plain_gate_open(&repo);
    // Both selection cancellation and edited-plan cancellation are read-only.
    plain_gate_run(&mut client, &uri, "", "open.rebase", None);
    plain_gate_run(&mut client, &uri, "Rebase", "menu.run-action", Some("i"));
    plain_gate_run(&mut client, &uri, "selected base", "common.cancel", None);
    assert_eq!(plain_gate_state(&repo), before);
    plain_gate_plan(&mut client, &uri, "selected base");
    plain_gate_run(&mut client, &uri, "drop replay", "rebase.todo-drop", None);
    plain_gate_run(&mut client, &uri, "first replay", "common.cancel", None);
    assert_eq!(plain_gate_state(&repo), before);

    plain_gate_plan(&mut client, &uri, "selected base");
    let drop = plain_gate_action(&mut client, &uri, "drop replay", "rebase.todo-drop", None);
    assert_eq!(
        drop["target"]["nodeId"],
        format!("rebase-todo:{}", repo.git(&["rev-parse", "HEAD~1"]).trim())
    );
    let plan_text = client.editor.text(&uri).to_owned();
    for (case, mut invalid) in [drop.clone(), drop.clone()].into_iter().enumerate() {
        if case == 0 {
            invalid["target"]["nodeId"] = json!("rebase-todo:unknown");
        } else {
            invalid["viewGeneration"] = json!(drop["viewGeneration"].as_u64().unwrap() - 1);
        }
        let refused = execute_plain_action(&mut client, invalid);
        assert!(refused["error"].is_null(), "{refused}");
        assert!(refused["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == "message"
                && effect["text"] == magit_lsp::errors::STALE_VIEW));
        assert_eq!(client.editor.text(&uri), plan_text);
        assert_eq!(plain_gate_state(&repo), before);
    }
    assert!(execute_plain_action(&mut client, drop)["error"].is_null());
    plain_gate_run(&mut client, &uri, "first replay", "rebase.todo-edit", None);
    plain_gate_run(
        &mut client,
        &uri,
        "first replay",
        "rebase.todo-submit",
        None,
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), first);
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
    assert!(repo.path().join(".git/rebase-merge").exists());
    plain_gate_run(&mut client, &uri, "", "open.rebase", None);
    plain_gate_run(&mut client, &uri, "Rebase", "menu.run-action", Some("r"));
    assert_eq!(
        repo.git(&["log", "--reverse", "--format=%s"]),
        "selected base\nfirst replay\nlast replay\n"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD~2"]), base);
    assert_eq!(
        repo.git(&["ls-tree", "-r", "--name-only", "HEAD"]),
        "f.txt\none\nthree\n"
    );
    assert_eq!(repo.git(&["show", "HEAD:f.txt"]), "base\n");
    assert_eq!(repo.git(&["show", "HEAD:one"]), "one\n");
    assert_eq!(repo.git(&["show", "HEAD:three"]), "three\n");
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
    assert!(!repo.path().join(".git/rebase-merge").exists());
    plain_gate_finish(client);
}

#[test]
fn plain_gate_rebase_conflict_is_readable_and_abort_restores_exact_state() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("conflict base", [("f.txt", "base\n")])
        .commit("first change", [("f.txt", "first\n")])
        .commit("second change", [("f.txt", "second\n")])
        .build();
    let before = plain_gate_state(&repo);
    let base = repo.git(&["rev-parse", "HEAD~2"]);
    let second = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let (mut client, uri) = plain_gate_open(&repo);
    plain_gate_plan(&mut client, &uri, "conflict base");
    plain_gate_run(&mut client, &uri, "second change", "rebase.todo-up", None);
    let result = plain_gate_run(
        &mut client,
        &uri,
        "second change",
        "rebase.todo-submit",
        None,
    );
    assert!(
        result["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == "message"
                && effect["text"]
                    .as_str()
                    .is_some_and(|text| !text.trim().is_empty())),
        "{result}"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), base);
    assert_eq!(repo.git(&["status", "--porcelain"]), "UU f.txt\n");
    assert_eq!(repo.git(&["show", ":1:f.txt"]), "first\n");
    assert_eq!(repo.git(&["show", ":2:f.txt"]), "base\n");
    assert_eq!(repo.git(&["show", ":3:f.txt"]), "second\n");
    assert_eq!(repo.git(&["rev-parse", "REBASE_HEAD"]).trim(), second);
    assert!(fs::read_to_string(repo.path().join("f.txt"))
        .unwrap()
        .starts_with("<<<<<<< HEAD\nbase\n=======\nsecond\n>>>>>>> "));
    plain_gate_run(&mut client, &uri, "", "open.rebase", None);
    plain_gate_run(&mut client, &uri, "Rebase", "menu.run-action", Some("a"));
    assert_eq!(plain_gate_state(&repo), before);

    // Resolve the same conflict and continue through offered Plain actions.
    plain_gate_plan(&mut client, &uri, "conflict base");
    plain_gate_run(&mut client, &uri, "first change", "rebase.todo-drop", None);
    plain_gate_run(&mut client, &uri, "second change", "rebase.todo-up", None);
    plain_gate_run(
        &mut client,
        &uri,
        "second change",
        "rebase.todo-submit",
        None,
    );
    assert_eq!(repo.git(&["status", "--porcelain"]), "UU f.txt\n");
    fs::write(repo.path().join("f.txt"), "resolved\n").unwrap();
    // Conflict rows need not render resolved file text. The structured section
    // action stages the tracked resolution without waiting for a text change.
    plain_gate_run(&mut client, &uri, "Unstaged changes", "status.stage", None);
    plain_gate_run(&mut client, &uri, "", "open.rebase", None);
    plain_gate_run(&mut client, &uri, "Rebase", "menu.run-action", Some("r"));
    assert_eq!(
        repo.git(&["log", "--reverse", "--format=%s"]),
        "conflict base\nsecond change\n"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD^"]), base);
    assert_eq!(repo.git(&["show", "HEAD:f.txt"]), "resolved\n");
    assert_eq!(fs::read(repo.path().join("f.txt")).unwrap(), b"resolved\n");
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
    assert_eq!(repo.git(&["symbolic-ref", "HEAD"]), "refs/heads/main\n");
    assert!(!repo.path().join(".git/rebase-merge").exists());
    plain_gate_finish(client);
}

#[test]
fn plain_e2e_stages_file_and_hunk_rejects_stale_unstages_and_commits_saved_draft() {
    let original = fixtures::TEN_LINES;
    let changed = original
        .replace("line1\n", "first-change\n")
        .replace("line10\n", "last-change\n");
    let partial = original.replace("line1\n", "first-change\n");
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("a.txt", "before\n"), ("f.txt", original)])
        .modify("a.txt", "after\n")
        .modify("f.txt", &changed)
        .build();
    let head = repo.git(&["rev-parse", "HEAD"]);
    let uri = format!("file://{}/.magit", repo.path().display());
    let model = editor_model::EditorModel::new(editor_model::PLAIN_MIN)
        .with_module(editor_model::EXTRA_DECLARATIONS);
    let mut client =
        ClientPump::with_model(stdio::TestServer::spawn(&[]), Profile::Standard, &model);
    let open = client.editor.open(&uri, "stale\n");
    client.send(open);
    client.until(|c| c.editor.text(&uri).contains("last-change"));

    // Find UI rows only to place the cursor. Execute the server's opaque target,
    // never reconstruct a node or hunk identity from the rendered text.
    let action = |client: &mut ClientPump, row_text: &str, command: &str| {
        let line = client
            .editor
            .text(&uri)
            .lines()
            .position(|line| line.contains(row_text))
            .expect("rendered cursor row") as u32;
        plain_code_actions(client, &uri, line)
            .into_iter()
            .find(|action| action["command"]["arguments"][0]["command"] == command)
            .unwrap_or_else(|| panic!("missing {command} on {row_text}"))["command"]["arguments"][0]
            .clone()
    };
    let file = action(&mut client, "a.txt", "status.stage");
    assert!(execute_plain_action(&mut client, file)["error"].is_null());
    assert_eq!(repo.git(&["show", ":a.txt"]), "after\n");
    assert_eq!(repo.git(&["show", ":f.txt"]), original);

    let hunk = action(&mut client, "first-change", "status.stage");
    let staged = execute_plain_action(&mut client, hunk.clone());
    assert!(staged["error"].is_null(), "{staged}");
    assert_eq!(
        repo.git(&["show", ":f.txt"]),
        partial,
        "only the addressed hunk is staged"
    );
    assert_eq!(
        fs::read_to_string(repo.path().join("f.txt")).unwrap(),
        changed
    );
    let index = repo.git(&["write-tree"]);
    let stale = execute_plain_action(&mut client, hunk);
    assert!(stale["error"].is_null(), "{stale}");
    assert!(
        stale["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == "message"
                && effect["text"] == magit_lsp::errors::STALE_VIEW),
        "{stale}"
    );
    client.until(|c| {
        c.server.notifications.iter().any(|m| {
            m["method"] == "window/showMessage"
                && m["params"]["message"] == magit_lsp::errors::STALE_VIEW
        })
    });
    assert_eq!(
        repo.git(&["write-tree"]),
        index,
        "stale action must not mutate the index"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(
        fs::read_to_string(repo.path().join("f.txt")).unwrap(),
        changed
    );

    let unstage = action(&mut client, "first-change", "status.unstage");
    assert!(execute_plain_action(&mut client, unstage)["error"].is_null());
    assert_eq!(repo.git(&["show", ":f.txt"]), original);
    assert_eq!(
        repo.git(&["show", ":a.txt"]),
        "after\n",
        "unstaging the hunk keeps the staged file"
    );
    let restage = action(&mut client, "f.txt", "status.stage");
    assert!(execute_plain_action(&mut client, restage)["error"].is_null());
    assert_eq!(repo.git(&["show", ":f.txt"]), changed);

    let menu = action(&mut client, "f.txt", "open.commit");
    assert!(execute_plain_action(&mut client, menu)["error"].is_null());
    let menu_line = client
        .editor
        .text(&uri)
        .lines()
        .position(|line| line.contains("Commit"))
        .expect("commit menu") as u32;
    let create = plain_code_actions(&mut client, &uri, menu_line)
        .into_iter()
        .find(|action| {
            action["command"]["arguments"][0]["command"] == "menu.run-action"
                && action["command"]["arguments"][0]["target"]["itemKey"] == "c"
        })
        .expect("commit creation row")["command"]["arguments"][0]
        .clone();
    assert!(execute_plain_action(&mut client, create)["error"].is_null());
    let draft_path = repo.path().join(".git/COMMIT_EDITMSG");
    let draft_uri = format!("file://{}", draft_path.display());
    client.until(|c| {
        c.editor.outbound.iter().any(|message| {
            message["method"] == "window/showDocument" && message["params"]["uri"] == draft_uri
        })
    });
    let template = fs::read_to_string(&draft_path).unwrap();
    let open = client.editor.open(&draft_uri, &template);
    client.send(open);
    let message = "Plain end-to-end commit\n\nSaved and closed through standard LSP.\n";
    let edit = client.editor.edit(&draft_uri, message);
    client.send(edit);
    fs::write(&draft_path, message).unwrap();
    let save = client.editor.save(&draft_uri);
    client.send(save);
    // A code-action response is a synchronization barrier after didSave. Saving
    // alone must not finish the Git operation; closing confirms the disk text.
    assert!(!plain_code_actions(&mut client, &draft_uri, 0).is_empty());
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    let close = client.editor.close(&draft_uri);
    client.send(close);
    client.until(|c| {
        !c.editor.text(&uri).contains("COMMIT_EDITMSG") && repo.git(&["rev-parse", "HEAD"]) != head
    });
    let commit = repo.git(&["cat-file", "commit", "HEAD"]);
    assert_eq!(commit.split_once("\n\n").unwrap().1, message);
    assert_eq!(repo.git(&["rev-parse", "HEAD^"]), head);
    assert_eq!(repo.git(&["show", "HEAD:a.txt"]), "after\n");
    assert_eq!(repo.git(&["show", "HEAD:f.txt"]), changed);
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
    assert!(
        client
            .server
            .notifications
            .iter()
            .all(|message| !message["method"]
                .as_str()
                .is_some_and(|method| method.starts_with("view/"))),
        "unexpected custom notifications: {:?}",
        client
            .server
            .notifications
            .iter()
            .filter(|message| message["method"]
                .as_str()
                .is_some_and(|method| method.starts_with("view/")))
            .collect::<Vec<_>>()
    );
    client.finish();
}

#[test]
fn plain_commit_editor_manual_instructions_cancel_pending_git_run() {
    for (case, declared, profile, succeeds) in [
        ("absent support", false, Profile::Standard, None),
        ("declared success", true, Profile::Standard, Some(true)),
        (
            "declared rejection",
            true,
            Profile::NoShowDocument,
            Some(false),
        ),
    ] {
        let (repo, _) = fixtures::RepoFixture::new()
            .commit("initial", [("f.txt", "before\n")])
            .modify_standard()
            .build();
        let head = repo.git(&["rev-parse", "HEAD"]);
        let uri = format!("file://{}/.magit", repo.path().display());
        let mut capabilities = caps();
        capabilities["workspace"] =
            json!({"applyEdit":true,"workspaceEdit":{"documentChanges":true}});
        capabilities["textDocument"] = json!({"codeAction":{}});
        if declared {
            capabilities["window"]["showDocument"] = json!({"support":true});
        }
        let mut client = ClientPump::new(stdio::TestServer::spawn(&[]), profile, capabilities);
        let open = client.editor.open(&uri, "stale\n");
        client.send(open);
        client.until(|c| c.editor.text(&uri).contains("f.txt"));

        let file_line = client
            .editor
            .text(&uri)
            .lines()
            .position(|line| line.contains("f.txt"))
            .unwrap() as u32;
        let actions = plain_code_actions(&mut client, &uri, file_line);
        let stage = actions
            .iter()
            .find(|action| action["command"]["arguments"][0]["command"] == "status.stage")
            .expect("standard code action offers stage")["command"]["arguments"][0]
            .clone();
        assert!(execute_plain_action(&mut client, stage)["error"].is_null());

        let text = client.editor.text(&uri).to_owned();
        let line = text
            .lines()
            .position(|line| line.contains("f.txt"))
            .unwrap_or(0) as u32;
        let actions = plain_code_actions(&mut client, &uri, line);
        let open_commit = actions
            .iter()
            .find(|action| action["command"]["arguments"][0]["command"] == "open.commit")
            .expect("standard code action offers commit menu")["command"]["arguments"][0]
            .clone();
        assert!(execute_plain_action(&mut client, open_commit)["error"].is_null());

        let menu_text = client.editor.text(&uri).to_owned();
        let menu_line = menu_text
            .lines()
            .position(|line| line.contains("Commit"))
            .unwrap_or(0) as u32;
        let actions = plain_code_actions(&mut client, &uri, menu_line);
        let create = actions
            .iter()
            .find(|action| {
                action["command"]["arguments"][0]["command"] == "menu.run-action"
                    && action["command"]["arguments"][0]["target"]["itemKey"].is_string()
            })
            .expect("standard code action offers commit creation")["command"]["arguments"][0]
            .clone();

        let result = execute_plain_action(&mut client, create);
        assert!(result.get("result").is_some(), "{result}");
        let expected_path = repo
            .path()
            .join(".git/COMMIT_EDITMSG")
            .to_string_lossy()
            .to_string();
        let expected_uri = format!("file://{expected_path}");
        client.until(|c| c.editor.text(&uri).contains(&expected_uri));
        assert!(
            client
                .editor
                .text(&uri)
                .lines()
                .any(|line| line == expected_uri),
            "pending URI row absent: {}",
            client.editor.text(&uri)
        );
        let token = client
            .server
            .notifications
            .iter()
            .find(|m| m["method"] == "$/progress" && m["params"]["value"]["kind"] == "begin")
            .expect("commit progress began")["params"]["token"]
            .clone();
        if !declared {
            let initial = std::fs::read_to_string(&expected_path).unwrap();
            let did_open = client.editor.open(&expected_uri, &initial);
            client.send(did_open);
            let edited = "Exact synchronized commit body\n\nReviewed manually.\n";
            let did_change = client.editor.edit(&expected_uri, edited);
            client.send(did_change);
            let actions = client.editor.code_action(
                &expected_uri,
                json!({"start":{"line":0,"character":0},"end":{"line":0,"character":0}}),
            );
            let id = client.send(actions);
            let offered = client.until_response(id)["result"]
                .as_array()
                .unwrap()
                .clone();
            let confirm = offered
                .iter()
                .find(|action| action["command"]["arguments"][0]["command"] == "common.confirm")
                .expect("attached draft offers Confirm")["command"]["arguments"][0]
                .clone();
            assert_eq!(confirm["version"], 2);
            let mut stale = confirm.clone();
            stale["version"] = json!(1);
            let rejected = execute_plain_action(&mut client, stale);
            assert!(
                rejected["error"].is_object(),
                "stale version must fail: {rejected}"
            );
            assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
            assert!(execute_plain_action(&mut client, confirm)["error"].is_null());
            client.until(|c| !c.editor.text(&uri).contains(&expected_path));
            assert_eq!(
                repo.git(&["log", "-1", "--format=%B"]),
                format!("{edited}\n")
            );
            assert_eq!(repo.git(&["status", "--porcelain"]), "");
            let terminal = client.editor.code_action(
                &expected_uri,
                json!({"start":{"line":0,"character":0},"end":{"line":0,"character":0}}),
            );
            let id = client.send(terminal);
            assert!(client.until_response(id)["result"]
                .as_array()
                .unwrap()
                .is_empty());
            let close = client.editor.close(&expected_uri);
            client.send(close);

            std::fs::write(repo.path().join("f.txt"), "abort draft worktree change\n").unwrap();
            client.until(|c| c.editor.text(&uri).contains("f.txt"));
            let status = client.editor.text(&uri).to_owned();
            let file_line = status
                .lines()
                .position(|line| line.contains("f.txt"))
                .unwrap() as u32;
            let stage = plain_code_actions(&mut client, &uri, file_line)
                .into_iter()
                .find(|action| action["command"]["arguments"][0]["command"] == "status.stage")
                .expect("stage second draft change")["command"]["arguments"][0]
                .clone();
            assert!(execute_plain_action(&mut client, stage)["error"].is_null());
            let status = client.editor.text(&uri).to_owned();
            let file_line = status
                .lines()
                .position(|line| line.contains("f.txt"))
                .unwrap() as u32;
            let open_commit = plain_code_actions(&mut client, &uri, file_line)
                .into_iter()
                .find(|action| action["command"]["arguments"][0]["command"] == "open.commit")
                .expect("open second commit menu")["command"]["arguments"][0]
                .clone();
            assert!(execute_plain_action(&mut client, open_commit)["error"].is_null());
            let menu = client.editor.text(&uri).to_owned();
            let line = menu
                .lines()
                .position(|line| line.contains("Commit"))
                .unwrap_or(0) as u32;
            let create = plain_code_actions(&mut client, &uri, line)
                .into_iter()
                .find(|action| action["command"]["arguments"][0]["command"] == "menu.run-action")
                .expect("start abortable commit")["command"]["arguments"][0]
                .clone();
            assert!(execute_plain_action(&mut client, create)["error"].is_null());
            client.until(|c| c.editor.text(&uri).contains(&expected_path));
            let initial = std::fs::read_to_string(&expected_path).unwrap();
            let did_open = client.editor.open(&expected_uri, &initial);
            client.send(did_open);
            let did_change = client
                .editor
                .edit(&expected_uri, "Abort this commit draft\n");
            client.send(did_change);
            let actions = client.editor.code_action(
                &expected_uri,
                json!({"start":{"line":0,"character":0},"end":{"line":0,"character":0}}),
            );
            let id = client.send(actions);
            let offered = client.until_response(id)["result"]
                .as_array()
                .unwrap()
                .clone();
            let abort = offered
                .iter()
                .find(|action| action["title"] == "Abort commit")
                .unwrap_or_else(|| {
                    panic!(
                        "attached draft offers Abort: {offered:?}\n{}",
                        client.server.stderr_text()
                    )
                })["command"]["arguments"][0]
                .clone();
            assert_eq!(abort["command"], "common.cancel");
            assert_eq!(abort["version"], 2);
            let abort_head = repo.git(&["rev-parse", "HEAD"]);
            let abort_tree = repo.git(&["write-tree"]);
            let abort_status = repo.git(&["status", "--porcelain"]);
            let mut foreign = abort.clone();
            foreign["runId"] = json!("foreign-run");
            assert!(execute_plain_action(&mut client, foreign)["error"].is_object());
            let mut stale = abort.clone();
            stale["version"] = json!(1);
            assert!(execute_plain_action(&mut client, stale)["error"].is_object());
            assert_eq!(repo.git(&["rev-parse", "HEAD"]), abort_head);
            assert_eq!(repo.git(&["write-tree"]), abort_tree);
            assert_eq!(repo.git(&["status", "--porcelain"]), abort_status);
            let aborted = execute_plain_action(&mut client, abort);
            assert!(aborted["error"].is_null(), "Abort failed: {aborted}");
            assert_eq!(aborted["result"]["type"], "aborted");
            client.until(|c| !c.editor.text(&uri).contains(&expected_path));
            let actions = client.editor.code_action(
                &expected_uri,
                json!({"start":{"line":0,"character":0},"end":{"line":0,"character":0}}),
            );
            let id = client.send(actions);
            assert!(client.until_response(id)["result"]
                .as_array()
                .unwrap()
                .is_empty());
            assert_eq!(repo.git(&["rev-parse", "HEAD"]), abort_head);
            assert_eq!(repo.git(&["write-tree"]), abort_tree);
            assert_eq!(repo.git(&["status", "--porcelain"]), abort_status);
        } else {
            client.send(json!({"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":token}}));
            client.until(|c| {
                let begins = c
                    .server
                    .notifications
                    .iter()
                    .filter(|m| {
                        m["method"] == "$/progress" && m["params"]["value"]["kind"] == "begin"
                    })
                    .count();
                let ends = c
                    .server
                    .notifications
                    .iter()
                    .filter(|m| {
                        m["method"] == "$/progress" && m["params"]["value"]["kind"] == "end"
                    })
                    .count();
                begins > 0 && ends >= begins && !c.editor.text(&uri).contains(&expected_path)
            });
        }

        let requests: Vec<_> = client
            .editor
            .outbound
            .iter()
            .filter(|m| m["method"] == "window/showDocument")
            .collect();
        assert_eq!(requests.len(), usize::from(declared), "{case}");
        if let Some(success) = succeeds {
            let request = requests[0];
            assert_eq!(
                request["params"]["uri"],
                format!("file://{expected_path}"),
                "{case}"
            );
            assert_eq!(request["params"]["external"], false, "{case}");
            assert_eq!(request["params"]["takeFocus"], true, "{case}");
            let reply = client
                .server
                .sent
                .iter()
                .find(|m| m["id"] == request["id"] && m["method"].is_null())
                .expect("correlated showDocument response");
            assert_eq!(reply["result"]["success"], success, "{case}");
        }
        let manual = client.server.notifications.iter().find(|m| {
            m["method"] == "window/showMessage"
                && m["params"]["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("COMMIT_EDITMSG"))
        });
        if succeeds == Some(true) {
            assert!(
                manual.is_none(),
                "success must not use manual fallback: {case}"
            );
        } else {
            let manual = manual.expect("Plain editor instructions use standard window/showMessage");
            assert!(
                manual["params"]["message"]
                    .as_str()
                    .unwrap()
                    .contains(&expected_path),
                "{case}"
            );
            assert!(
                manual["params"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("Open"),
                "{case}"
            );
            assert!(
                manual["params"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("Confirm"),
                "{case}"
            );
            assert!(
                manual["params"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("Abort"),
                "{case}"
            );
            if succeeds == Some(false) {
                assert!(
                    client
                        .server
                        .sent
                        .iter()
                        .any(|m| m["method"] == "window/workDoneProgress/cancel"),
                    "cancel after fallback: {case}"
                );
            }
        }
        assert!(
            client
                .server
                .notifications
                .iter()
                .all(|m| m["method"] != "view/effects"),
            "unexpected custom notifications: {:?}",
            client
                .server
                .notifications
                .iter()
                .filter(|m| m["method"] == "view/effects")
                .collect::<Vec<_>>()
        );
        if declared {
            assert_eq!(repo.git(&["rev-parse", "HEAD"]), head, "{case}");
        } else {
            assert_ne!(
                repo.git(&["rev-parse", "HEAD"]),
                head,
                "manual Confirm must commit: {case}"
            );
        }
        client.finish();
    }
}

#[test]
fn plain_stale_commit_draft_change_reports_rejection_without_custom_effects() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "before\n")])
        .modify_standard()
        .build();
    let uri = format!("file://{}/.magit", repo.path().display());
    let mut capabilities = caps();
    capabilities["workspace"] = json!({"applyEdit":true,"workspaceEdit":{"documentChanges":true}});
    capabilities["textDocument"] = json!({"codeAction":{}});
    let mut client = ClientPump::new(
        stdio::TestServer::spawn(&[]),
        Profile::Standard,
        capabilities,
    );
    let open = client.editor.open(&uri, "stale\n");
    client.send(open);
    client.until(|c| c.editor.text(&uri).contains("f.txt"));

    let text = client.editor.text(&uri).to_owned();
    let line = text
        .lines()
        .position(|line| line.contains("f.txt"))
        .unwrap_or(0) as u32;
    let stage = plain_code_actions(&mut client, &uri, line)
        .into_iter()
        .find(|action| action["command"]["arguments"][0]["command"] == "status.stage")
        .expect("standard code action offers stage")["command"]["arguments"][0]
        .clone();
    assert!(execute_plain_action(&mut client, stage)["error"].is_null());

    let text = client.editor.text(&uri).to_owned();
    let line = text
        .lines()
        .position(|line| line.contains("f.txt"))
        .unwrap_or(0) as u32;
    let open_commit = plain_code_actions(&mut client, &uri, line)
        .into_iter()
        .find(|action| action["command"]["arguments"][0]["command"] == "open.commit")
        .expect("standard code action offers commit menu")["command"]["arguments"][0]
        .clone();
    assert!(execute_plain_action(&mut client, open_commit)["error"].is_null());

    let menu_text = client.editor.text(&uri).to_owned();
    let menu_line = menu_text
        .lines()
        .position(|line| line.contains("Commit"))
        .unwrap_or(0) as u32;
    let create = plain_code_actions(&mut client, &uri, menu_line)
        .into_iter()
        .find(|action| action["command"]["arguments"][0]["command"] == "menu.run-action")
        .expect("standard code action offers commit creation")["command"]["arguments"][0]
        .clone();
    let result = execute_plain_action(&mut client, create);
    assert!(result.get("result").is_some(), "{result}");

    let expected_path = repo
        .path()
        .join(".git/COMMIT_EDITMSG")
        .to_string_lossy()
        .to_string();
    let expected_uri = format!("file://{expected_path}");
    client.until(|c| c.editor.text(&uri).contains(&expected_uri));
    let token = client
        .server
        .notifications
        .iter()
        .find(|m| m["method"] == "$/progress" && m["params"]["value"]["kind"] == "begin")
        .expect("commit progress began")["params"]["token"]
        .clone();

    let initial = std::fs::read_to_string(&expected_path).unwrap();
    let did_open = client.editor.open(&expected_uri, &initial);
    client.send(did_open);
    // Version 1 is already open: replaying it is a stale, duplicated change.
    client.send(
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":expected_uri,"version":1},
            "contentChanges":[{"text":"Rejected stale body\n"}]
        }}),
    );
    client.until(|c| {
        c.server.notifications.iter().any(|m| {
            m["method"] == "window/showMessage"
                && m["params"]["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("commit draft:"))
        })
    });
    let rejection = client
        .server
        .notifications
        .iter()
        .find(|m| {
            m["method"] == "window/showMessage"
                && m["params"]["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("commit draft:"))
        })
        .unwrap();
    assert!(rejection["params"]["message"]
        .as_str()
        .unwrap()
        .contains("Your text is retained"));
    assert!(
        client
            .server
            .notifications
            .iter()
            .all(|m| m["method"] != "view/effects"),
        "Plain draft failures must not emit custom effects: {:?}",
        client
            .server
            .notifications
            .iter()
            .filter(|m| m["method"] == "view/effects")
            .collect::<Vec<_>>()
    );

    client.send(
        json!({"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":token}}),
    );
    client.until(|c| !c.editor.text(&uri).contains(&expected_path));
    client.finish();
}
