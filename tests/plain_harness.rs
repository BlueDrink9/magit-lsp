//! Plain editor conformance self-tests and real-binary protocol smoke test.
#[path = "common/fake_editor.rs"]
mod fake_editor;
#[path = "common/editor_model/mod.rs"]
mod editor_model;
#[path = "common/mod.rs"]
mod fixtures;
#[path = "common/stdio.rs"]
mod stdio;
#[path = "common/client_pump.rs"]
mod client_pump;
use client_pump::ClientPump;
use fake_editor::{FakeEditor, Profile};
use serde_json::{json, Value};
use std::fs;

fn caps() -> Value { json!({"window":{"workDoneProgress":true},"general":{"positionEncodings":["utf-8","utf-16","utf-32"]}}) }

#[test]
fn shared_rich_render_vectors_apply_exact_text_and_intents() {
    #[derive(serde::Deserialize)]
    struct Vector { name: String, #[serde(rename="initialLines")] initial_lines: Vec<String>, #[serde(rename="initialVersion")] initial_version: u64, render: magit_lsp::protocol::ViewRender, #[serde(rename="expectedLines")] expected_lines: Vec<String>, #[serde(rename="expectedVersion")] expected_version: u64, #[serde(rename="expectedMetadata")] expected_metadata: Option<Value>, outcome: String }
    #[derive(serde::Deserialize)]
    struct Contracts { renders: Vec<Vector> }
    for vector in serde_json::from_str::<Contracts>(include_str!("data/client-contracts.json")).unwrap().renders {
        let mut editor = editor_model::rich::RichEditor::default();
        editor.views.insert(vector.render.view_id.clone(), editor_model::rich::ViewState { lines: vector.initial_lines, version: vector.initial_version, ..Default::default() });
        let intent = editor.apply(vector.render.clone());
        let state = &editor.views[&vector.render.view_id];
        assert_eq!(state.lines, vector.expected_lines, "{}", vector.name);
        assert_eq!(state.version, vector.expected_version, "{}", vector.name);
        if let Some(expected) = vector.expected_metadata { assert_eq!(json!({"mode":state.mode,"pending":state.pending,"cursor":state.cursor,"menu":state.menu}), expected, "{}", vector.name); }
        match vector.outcome.as_str() { "ack" => assert_eq!(intent, editor_model::rich::Intent::Ack(vector.expected_version)), "resync" => assert_eq!(intent, editor_model::rich::Intent::Resync), other => panic!("unknown outcome {other}") }
    }
}

#[test]
fn shared_prompt_vectors_derive_answers_and_focus() {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Vector { name: String, params: magit_lsp::protocol::ViewPromptParams, response: Value, custom_response: Option<Value>, answer: Value, focus: Option<Vec<String>> }
    #[derive(serde::Deserialize)] struct Contracts { prompts: Vec<Vector> }
    for vector in serde_json::from_str::<Contracts>(include_str!("data/client-contracts.json")).unwrap().prompts {
        let input = editor_model::prompts::PromptInput { response: vector.response, custom_response: vector.custom_response };
        let outcome = editor_model::prompts::answer(&vector.params, &input);
        assert_eq!(outcome.answer, vector.answer, "{}", vector.name);
        assert_eq!(outcome.focus, vector.focus, "{}", vector.name);
    }
}

#[test]
fn queued_prompt_routing_uses_unique_correlated_request_ids() {
    let prompt_module = editor_model::Module { declares: std::borrow::Cow::Borrowed(r#"{"experimental":{"magit":{"prompts":["text"]}}}"#), behaves: &[] };
    // anchor: rich text-prompt contract in tests/data/client-contracts.json and stdio evidence in tests/server_async.rs.
    let model = editor_model::EditorModel::new(editor_model::RICH_MIN)
        .with_module(prompt_module)
        .strict();
    let mut client = ClientPump::with_model(stdio::TestServer::spawn(&[]), Profile::Standard, &model);
    client.queue_prompt(editor_model::prompts::PromptInput { response: json!(""), custom_response: None });
    client.queue_prompt(editor_model::prompts::PromptInput { response: json!("second"), custom_response: None });
    let (repo, _) = fixtures::RepoFixture::new().commit("init", [("f.txt", fixtures::TEN_LINES)]).build();
    let open = client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/open","params":{"kind":"status","args":{"repo_path":repo.path().to_string_lossy()},"viewport":{"cols":80,"rows":24}}}));
    let status = client.until_response(open)["result"]["id"].as_str().unwrap().to_owned();
    client.until(|c| c.rich.as_ref().is_some_and(|r| r.views.contains_key(&status)));
    let invoke = client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/invoke","params":{"viewId":status,"command":"open.commit"}}));
    let opened = client.until_response(invoke);
    let menu = opened["result"]["effects"].as_array().unwrap().iter().find(|e| e["type"] == "pushView").unwrap()["descriptor"]["id"].as_str().unwrap().to_owned();
    client.until(|c| c.rich.as_ref().is_some_and(|r| r.views.contains_key(&menu)));
    let key = |key: &str, version| json!({"jsonrpc":"2.0","id":0,"method":"view/key","params":{"viewId":menu,"version":version,"key":key,"editorMode":"normal","cursor":{"line":0,"column":0}}});
    let prefix = client.send(key("=", client.rich.as_ref().unwrap().views[&menu].version));
    assert!(client.until_response(prefix)["error"].is_null());
    let first = client.send(key("A", client.rich.as_ref().unwrap().views[&menu].version));
    let before_answer = client.rich.as_ref().unwrap().views[&menu].version;
    client.until(|c| c.server.sent.iter().filter(|m| m["method"] == "view/promptRespond").count() == 1);
    let first_prompt_id = client.prompt_response_ids()[0];
    client.until_response(first_prompt_id);
    let first_reply = client.until_response(first);
    assert_eq!(first_reply["result"]["effects"], json!([]));
    client.until(|c| c.rich.as_ref().unwrap().views[&menu].version > before_answer);
    let second_prefix = client.send(key("=", client.rich.as_ref().unwrap().views[&menu].version));
    assert_eq!(client.until_response(second_prefix)["result"]["effects"], json!([]));
    let clear = client.send(key("A", client.rich.as_ref().unwrap().views[&menu].version));
    assert_eq!(client.until_response(clear)["result"]["effects"], json!([]));
    let after_clear = client.rich.as_ref().unwrap().views[&menu].version;
    client.until(|c| c.rich.as_ref().unwrap().views[&menu].version > after_clear);
    let third_prefix = client.send(key("=", client.rich.as_ref().unwrap().views[&menu].version));
    assert_eq!(client.until_response(third_prefix)["result"]["effects"], json!([]));
    let second = client.send(key("A", client.rich.as_ref().unwrap().views[&menu].version));
    client.until(|c| c.server.sent.iter().filter(|m| m["method"] == "view/promptRespond").count() == 2);
    let ids: Vec<_> = client.server.sent.iter().filter(|m| m["method"] == "view/promptRespond").map(|m| m["id"].as_i64().unwrap()).collect();
    assert_ne!(ids[0], ids[1]);
    let answers: Vec<_> = client.server.sent.iter().filter(|m| m["method"] == "view/promptRespond").collect();
    assert_eq!(answers[0]["params"]["value"], "");
    assert_eq!(answers[1]["params"]["value"], "second");
    client.until_response(ids[1]);
    assert_eq!(client.until_response(second)["result"]["effects"], json!([]));
    let unknown = client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/promptRespond","params":{"promptId":"unknown-prompt-id","cancelled":false,"value":"ignored"}}));
    assert!(client.until_response(unknown)["error"].is_object(), "unknown prompt response must remain observable");
    client.finish();
}

#[test]
fn rich_stale_render_isolation() {
    use editor_model::rich::{Intent, RichEditor, ViewState};
    let mut editor = RichEditor::default();
    editor.views.insert("a".into(), ViewState { lines: vec!["keep".into()], version: 1, ..ViewState::default() });
    let before = editor.views["a"].clone();
    let stale: magit_lsp::protocol::ViewRender = serde_json::from_value(json!({"viewId":"a","version":3,"baseVersion":0,"edits":[{"startLine":0,"endLine":1,"lines":["bad"]}],"mode":"interactive"})).unwrap();
    assert_eq!(editor.apply(stale), Intent::Resync);
    assert_eq!(editor.views["a"], before);
    let other: magit_lsp::protocol::ViewRender = serde_json::from_value(json!({"viewId":"b","version":1,"text":["other"],"mode":"interactive"})).unwrap();
    editor.apply(other);
    assert_eq!(editor.views["a"], before);
}

#[test]
fn minimum_editor_models_declare_exact_base_contracts_and_keep_behavior_separate() {
    let plain = editor_model::EditorModel::new(editor_model::PLAIN_MIN);
    assert_eq!(plain.capabilities(), json!({"workspace":{"applyEdit":true},"textDocument":{"codeAction":{}}}));
    let rich = editor_model::EditorModel::new(editor_model::RICH_MIN);
    assert_eq!(rich.capabilities(), json!({"experimental":{"magit":{"v":1,"keys":"dynamic","modifiers":"basic","layout":["main"],"effects":[],"prompts":[]}}}));
    assert!(matches!(magit_lsp::presentation::negotiate(&json!({"capabilities":rich.capabilities()})), Ok(magit_lsp::presentation::Presentation::Rich(_))));
    let composed = plain.clone().with_module(editor_model::SILENT_STALE_APPLY);
    assert_eq!(composed.capabilities(), plain.capabilities());
    assert_eq!(composed.behaviors().collect::<Vec<_>>(), ["silent-stale-apply"]);
    let declarations = plain.clone().with_module(editor_model::EXTRA_DECLARATIONS);
    assert_eq!(declarations.capabilities(), json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true}},"textDocument":{"codeAction":{}},"window":{"showDocument":{"support":true}}}));
    let empty = magit_lsp::quirks::Resolution { enabled: Default::default() };
    let model = editor_model::EditorModel::new(editor_model::PLAIN_MIN).with_module(editor_model::DECOMPOSED_CHANGES);
    let mut modeled = FakeEditor::from_model(Profile::Standard, &model, &empty);
    modeled.open("file:///model.txt", "ab");
    assert!(modeled.edit("file:///model.txt", "ac")["params"]["contentChanges"][0].get("range").is_some());
    let mut pump = ClientPump::with_model(stdio::TestServer::spawn(&[]), Profile::Standard, &rich);
    assert!(matches!(magit_lsp::presentation::negotiate(&json!({"capabilities":rich.capabilities()})), Ok(magit_lsp::presentation::Presentation::Rich(_))));
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
        (plain, 0, serde_json::from_str::<Value>(include_str!("fixtures/lsp_traces/initialize-plain-neovim-0.12.6.json")).unwrap()["capabilities"].clone()),
        (helix, 1, serde_json::from_str::<Value>(include_str!("fixtures/lsp_traces/initialize-helix-25.07.1.json")).unwrap()["capabilities"].clone()),
        (bundled, 2, { let rows: Value = serde_json::from_str(include_str!("fixtures/current-client-capabilities.json")).unwrap(); let row = rows.as_array().unwrap().iter().find(|r| r["client"]["name"] == "Neovim").unwrap(); json!({"experimental":{"magit":row["magit"]}}) }),
        (vscode, 3, { let rows: Value = serde_json::from_str(include_str!("fixtures/current-client-capabilities.json")).unwrap(); let row = rows.as_array().unwrap().iter().find(|r| r["client"]["name"] == "VSCodium").unwrap(); json!({"experimental":{"magit":row["magit"]}}) }),
    ] {
        let (preset, identity_value) = preset;
        let capabilities = preset.capabilities();
        assert_eq!(capabilities, expected, "preset {identity}");
        if identity >= 2 { assert!(matches!(magit_lsp::presentation::negotiate(&json!({"capabilities":capabilities})), Ok(magit_lsp::presentation::Presentation::Rich(_)))); }
        let (client_name, client_version) = (identity_value["name"].as_str(), identity_value["version"].as_str());
        let resolution = magit_lsp::quirks::resolve(magit_lsp::quirks::Client { name: client_name, version: client_version, capabilities: &capabilities }, None).unwrap();
        let mut editor = FakeEditor::from_model(Profile::Standard, &preset, &resolution);
        editor.open("file:///preset", "old");
        for quirk in magit_lsp::quirks::REGISTRY.iter().filter(|quirk| resolution.enabled.contains(quirk.id)) {
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
    assert_eq!(profile_model(NamedProfile::Plain).capabilities(), editor_model::EditorModel::new(editor_model::PLAIN_MIN).capabilities());
    assert_eq!(profile_model(NamedProfile::RichNative).capabilities()["experimental"]["magit"]["keys"], "dynamic");
    assert_eq!(profile_model(NamedProfile::RichNative).capabilities()["experimental"]["magit"]["modifiers"], "basic");
    assert_eq!(profile_model(NamedProfile::RichDoc).capabilities()["experimental"]["magit"]["keys"], "static");
    assert_eq!(profile_model(NamedProfile::RichDoc).capabilities()["experimental"]["magit"]["modifiers"], "full");
    let empty = json!({});
    let unknown = magit_lsp::quirks::resolve(magit_lsp::quirks::Client { name: Some("unknown"), version: Some("1"), capabilities: &empty }, None).unwrap();
    assert!(unknown.enabled.is_empty());
    let mut unknown_editor = FakeEditor::from_resolution(Profile::Standard, &unknown);
    unknown_editor.open("file:///unknown", "old");
    assert_eq!(unknown_editor.edit("file:///unknown", "new")["params"]["contentChanges"].as_array().unwrap().len(), 1);
    assert_eq!(unknown_editor.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":"file:///unknown","version":1},"edits":[]}]}))["applied"], false);
}

#[test]
fn strict_feature_guard_distinguishes_declarations_from_editor_behavior() {
    let plain = editor_model::EditorModel::new(editor_model::PLAIN_MIN).strict();
    let doc_changes = json!({"jsonrpc":"2.0","id":1,"method":"workspace/applyEdit","params":{"edit":{"documentChanges":[{"textDocument":{"uri":"file:///strict","version":1},"edits":[]}]}}});
    assert!(editor_model::strict::check(&plain.capabilities(), &doc_changes).unwrap_err().contains("documentChanges"));
    let declared = editor_model::EditorModel::new(editor_model::PLAIN_MIN).with_module(editor_model::EXTRA_DECLARATIONS);
    assert!(editor_model::strict::check(&declared.capabilities(), &doc_changes).is_ok());
    let required = [
        ("undeclared applyEdit", json!({"method":"workspace/applyEdit","params":{"edit":{"changes":{}}}})),
        ("showDocument", json!({"method":"window/showDocument","params":{}})),
        ("progress create", json!({"method":"window/workDoneProgress/create","params":{}})),
        ("progress", json!({"method":"$/progress","params":{}})),
    ];
    for (feature, message) in required {
        let mut capabilities = plain.capabilities();
        if feature == "undeclared applyEdit" { capabilities["workspace"]["applyEdit"] = json!(false); }
        assert!(editor_model::strict::check(&capabilities, &message).is_err(), "{feature}");
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
        let declarations = if message["method"] == "view/render" { plain.capabilities() } else { rich.capabilities() };
        assert!(editor_model::strict::check(&declarations, &message).is_err(), "{message}");
    }
    assert!(editor_model::strict::check(
        &plain.capabilities(),
        &json!({"method":"view/effects","params":{"effects":[{"type":"message"}]}})
    ).is_err());
    let rich_core_message = json!({"method":"view/effects","params":{"effects":[{"type":"message"}]}});
    assert!(editor_model::strict::check(&rich.capabilities(), &rich_core_message).is_ok());
    for (feature, declaration, message) in [
        ("clipboard", json!({"effects":["clipboard"]}), json!({"method":"view/effects","params":{"effects":[{"type":"message"},{"type":"clipboard"}]}})),
        ("prompt", json!({"prompts":["confirm"]}), json!({"method":"view/prompt","params":{"kind":"confirm"}})),
        ("layout", json!({"layout":["main","side"]}), json!({"method":"view/effects","params":{"effects":[{"type":"pushView","descriptor":{"placement":"side"}}]}})),
    ] {
        let mut capabilities = rich.capabilities();
        for (key, value) in declaration.as_object().unwrap() { capabilities["experimental"]["magit"][key] = value.clone(); }
        assert!(editor_model::strict::check(&capabilities, &message).is_ok(), "declared {feature}");
    }
    let behavior = editor_model::EditorModel::new(editor_model::PLAIN_MIN).with_module(editor_model::NEOVIM_UNDECLARED_CHANGES);
    let mut editor = FakeEditor::from_model(Profile::NoDocumentChanges, &behavior, &magit_lsp::quirks::Resolution { enabled: Default::default() });
    editor.open("file:///strict", "text");
    let nonempty = json!({"documentChanges":[{"textDocument":{"uri":"file:///strict","version":1},"edits":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":4}},"newText":"edit"}]}]});
    assert!(editor_model::strict::check(&behavior.capabilities(), &json!({"method":"workspace/applyEdit","params":{"edit":nonempty}})).is_err());
    assert_eq!(editor.apply_edit(&nonempty)["applied"], true);
    assert_eq!(editor.text("file:///strict"), "edit");
}

#[test]
fn rich_server_render_ack_and_stale_resync_preserve_caller_reply() {
    let (repo, _) = fixtures::RepoFixture::new().commit("init", [("f.txt", fixtures::TEN_LINES)]).build();
    let model = editor_model::EditorModel::new(editor_model::RICH_MIN).strict();
    let mut client = ClientPump::with_model(stdio::TestServer::spawn(&[]), Profile::Standard, &model);
    let open = client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/open","params":{"kind":"status","args":{"repo_path":repo.path().to_string_lossy()},"viewport":{"cols":80,"rows":24}}}));
    let opened = client.until_response(open);
    let descriptor = &opened["result"];
    assert!(descriptor["id"].is_string(), "{opened}");
    let view = descriptor["id"].as_str().unwrap().to_owned();
    client.until(|c| c.rich.as_ref().is_some_and(|r| r.views.contains_key(&view)));
    let state = client.rich.as_ref().unwrap().views[&view].clone();
    let initial = client.server.notifications.iter().find(|m| m["method"] == "view/render" && m["params"]["viewId"] == view).unwrap();
    assert_eq!(state.lines, serde_json::from_value::<Vec<String>>(initial["params"]["text"].clone()).unwrap());
    assert_eq!(state.version, initial["params"]["version"].as_u64().unwrap());
    assert!(client.server.sent.iter().any(|m| m["method"] == "view/rendered" && m["params"]["viewId"] == view && m["params"]["version"] == state.version));
    let unrelated = client.send(json!({"jsonrpc":"2.0","id":0,"method":"view/resync","params":{"viewId":view}}));
    client.until(|c| c.server.notifications.iter().any(|m| m["method"] == "view/render" && m["params"]["viewId"] == view));
    client.rich.as_mut().unwrap().views.get_mut(&view).unwrap().version = 0;
    let new_file = repo.path().join("new-file.txt");
    fs::write(&new_file, "watcher-visible\n").unwrap();
    client.until(|c| c.server.sent.iter().any(|m| m["method"] == "view/resync"));
    let resync = client.server.sent.iter().find(|m| m["method"] == "view/resync").unwrap();
    let resync_index = client.server.sent.iter().position(|m| m["method"] == "view/resync").unwrap();
    assert!(resync["id"].is_number());
    assert_eq!(resync["params"]["viewId"], view);
    client.until(|c| c.rich.as_ref().is_some_and(|r| r.views.get(&view).is_some_and(|s| s.lines.iter().any(|line| line.contains("new-file.txt")))));
    let recovered = &client.rich.as_ref().unwrap().views[&view];
    let ack = client.server.sent.iter().skip(resync_index + 1).find(|m| m["method"] == "view/rendered" && m["params"]["viewId"] == view && m["params"]["version"] == recovered.version).expect("recovery ack");
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
    let static_rich = editor_model::presets::profile_model(editor_model::presets::NamedProfile::RichDoc).capabilities();
    let dynamic_rich = editor_model::presets::profile_model(editor_model::presets::NamedProfile::RichNative).capabilities();
    let ordinary = json!({"jsonrpc":"2.0","id":1,"result":{"ok":true}});
    assert!(editor_model::strict::check(&plain, &ordinary).is_ok());
    for placement in ["main", "side", "bottom", "float"] {
        let response = json!({"jsonrpc":"2.0","id":1,"result":{"descriptor":{"placement":placement}}});
        assert_eq!(editor_model::strict::check(&rich, &response).is_ok(), placement == "main", "base declaration {placement}");
        let mut declared = rich.clone();
        declared["experimental"]["magit"]["layout"] = json!([placement]);
        assert!(editor_model::strict::check(&declared, &response).is_ok(), "declared {placement}");
    }
    for (name, declaration, effect) in [
        ("clipboard", "clipboard", json!({"type":"clipboard"})),
        ("visitFile", "visitFile", json!({"type":"visitFile"})),
        ("editDocument", "editDocument", json!({"type":"editDocument"})),
    ] {
        let response = json!({"jsonrpc":"2.0","id":1,"result":{"effects":[effect]}});
        assert!(editor_model::strict::check(&rich, &response).is_err(), "undeclared {name}");
        let mut declared = rich.clone();
        declared["experimental"]["magit"]["effects"] = json!([declaration]);
        assert!(editor_model::strict::check(&declared, &response).is_ok(), "declared {name}");
    }
    let mut all = rich.clone();
    all["experimental"]["magit"]["layout"] = json!(["main", "side", "bottom", "float"]);
    all["experimental"]["magit"]["effects"] = json!(["clipboard", "visitFile", "editDocument"]);
    let render = json!({"method":"view/render","params":{}});
    let keymap = json!({"method":"view/keymap","params":{}});
    assert!(editor_model::strict::check(&plain, &render).is_err());
    assert!(editor_model::strict::check(&plain, &keymap).is_err());
    for (profile, message) in [("rich-doc", &render), ("rich-doc", &keymap), ("rich-native", &render), ("rich-native", &keymap)] {
        let capabilities = if profile == "rich-doc" { &static_rich } else { &dynamic_rich };
        assert_eq!(editor_model::strict::check(capabilities, message).is_ok(), message["method"] == "view/render" || profile == "rich-native", "{profile} {}", message["method"]);
    }
    for prompt in ["text", "confirm", "choice", "multiChoice"] {
        let message = json!({"method":"view/prompt","params":{"kind":prompt}});
        assert!(editor_model::strict::check(&rich, &message).is_err(), "undeclared {prompt}");
        let mut declared = rich.clone();
        declared["experimental"]["magit"]["prompts"] = json!([prompt]);
        assert!(editor_model::strict::check(&declared, &message).is_ok(), "declared {prompt}");
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
        ("applyEdit", json!({"workspace":{"applyEdit":true}}), json!({"method":"workspace/applyEdit","params":{"edit":{"changes":{}}}})),
        ("documentChanges", json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true}}}), json!({"method":"workspace/applyEdit","params":{"edit":{"documentChanges":[]}}})),
        ("showDocument", json!({"window":{"showDocument":{"support":true}}}), json!({"method":"window/showDocument","params":{}})),
        ("progress create", json!({"window":{"workDoneProgress":true}}), json!({"method":"window/workDoneProgress/create","params":{}})),
        ("progress", json!({"window":{"workDoneProgress":true}}), json!({"method":"$/progress","params":{}})),
    ] {
        let mut capabilities = plain.clone();
        for (key, value) in declaration.as_object().unwrap() { capabilities[key] = value.clone(); }
        assert!(editor_model::strict::check(&capabilities, &message).is_ok(), "declared {feature}");
    }
    assert!(editor_model::strict::check(&all, &json!({"method":"view/unknown","params":{}})).is_err());
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
    let mut client = ClientPump::with_model(stdio::TestServer::spawn(&[]), Profile::Standard, &model);
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
        let resolution = Resolution { enabled: [quirk.id].into_iter().collect() };
        let profile = if quirk.model == Some("neovim-accepts-undeclared-documentChanges") { Profile::NoDocumentChanges } else { Profile::Standard };
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
    let neovim = quirks::resolve(Client { name: Some("Neovim"), version: Some("0.12.6"), capabilities: &empty }, None).unwrap();
    let mut editor = FakeEditor::from_resolution(Profile::NoDocumentChanges, &neovim);
    let uri = "file:///combined";
    editor.open(uri, "old");
    assert_eq!(editor.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":uri,"version":1},"edits":[]}]}))["applied"], true);
    editor.edit(uri, "new");
    editor.open("file:///later", "old");
    editor.edit("file:///later", "later-local");
    let stale = editor.apply_edit(&json!({"documentChanges":[{"textDocument":{"uri":uri,"version":1},"edits":[replacement(0,3,"FIRST") ]},{"textDocument":{"uri":"file:///later","version":1},"edits":[replacement(0,11,"LATER")]}]}));
    assert_eq!(stale["applied"], true);
    assert_eq!(editor.text(uri), "new");
    assert_eq!(editor.text("file:///later"), "LATER");
    let helix = quirks::resolve(Client { name: Some("helix"), version: Some("25.07.1"), capabilities: &empty }, None).unwrap();
    let mut editor = FakeEditor::from_resolution(Profile::Standard, &helix);
    editor.open(uri, "old");
    assert!(editor.edit(uri, "new")["params"]["contentChanges"][0]["range"].is_object());
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
    let mut client = ClientPump::new(stdio::TestServer::spawn(&[]), Profile::DebouncedDidChange, caps());
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
    client.until_response(id)["result"].as_array().unwrap().clone()
}

fn execute_plain_action(client: &mut ClientPump, args: Value) -> Value {
    let request = client.editor.execute_command("magit.command", json!([args]));
    let id = client.send(request);
    client.until_response(id)
}

#[test]
fn plain_commit_editor_manual_instructions_cancel_pending_git_run() {
    for (case, declared, profile, succeeds) in [
        ("absent support", false, Profile::Standard, None),
        ("declared success", true, Profile::Standard, Some(true)),
        ("declared rejection", true, Profile::NoShowDocument, Some(false)),
    ] {
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f.txt", "before\n")]).modify_standard().build();
    let head = repo.git(&["rev-parse", "HEAD"]);
    let uri = format!("file://{}/.magit", repo.path().display());
    let mut capabilities = caps();
    capabilities["workspace"] = json!({"applyEdit":true,"workspaceEdit":{"documentChanges":true}});
    capabilities["textDocument"] = json!({"codeAction":{}});
    if declared { capabilities["window"]["showDocument"] = json!({"support":true}); }
    let mut client = ClientPump::new(stdio::TestServer::spawn(&[]), profile, capabilities);
    let open = client.editor.open(&uri, "stale\n");
    client.send(open);
    client.until(|c| c.editor.text(&uri).contains("f.txt"));

    let file_line = client.editor.text(&uri).lines().position(|line| line.contains("f.txt")).unwrap() as u32;
    let actions = plain_code_actions(&mut client, &uri, file_line);
    let stage = actions.iter().find(|action| action["command"]["arguments"][0]["command"] == "status.stage")
        .expect("standard code action offers stage")["command"]["arguments"][0].clone();
    assert!(execute_plain_action(&mut client, stage)["error"].is_null());

    let text = client.editor.text(&uri).to_owned();
    let line = text.lines().position(|line| line.contains("f.txt")).unwrap_or(0) as u32;
    let actions = plain_code_actions(&mut client, &uri, line);
    let open_commit = actions.iter().find(|action| action["command"]["arguments"][0]["command"] == "open.commit")
        .expect("standard code action offers commit menu")["command"]["arguments"][0].clone();
    assert!(execute_plain_action(&mut client, open_commit)["error"].is_null());

    let menu_text = client.editor.text(&uri).to_owned();
    let menu_line = menu_text.lines().position(|line| line.contains("Commit")).unwrap_or(0) as u32;
    let actions = plain_code_actions(&mut client, &uri, menu_line);
    let create = actions.iter().find(|action| {
        action["command"]["arguments"][0]["command"] == "menu.run-action"
            && action["command"]["arguments"][0]["target"]["itemKey"].is_string()
    }).expect("standard code action offers commit creation")
        ["command"]["arguments"][0].clone();

    let result = execute_plain_action(&mut client, create);
    assert!(result.get("result").is_some(), "{result}");
    let expected_path = repo.path().join(".git/COMMIT_EDITMSG").to_string_lossy().to_string();
    let expected_uri = format!("file://{expected_path}");
    client.until(|c| c.editor.text(&uri).contains(&expected_uri));
    assert!(client.editor.text(&uri).lines().any(|line| line == expected_uri), "pending URI row absent: {}", client.editor.text(&uri));
    let token = client.server.notifications.iter()
        .find(|m| m["method"] == "$/progress" && m["params"]["value"]["kind"] == "begin")
        .expect("commit progress began")["params"]["token"].clone();
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
        let offered = client.until_response(id)["result"].as_array().unwrap().clone();
        let confirm = offered.iter().find(|action| action["command"]["arguments"][0]["command"] == "common.confirm")
            .expect("attached draft offers Confirm")["command"]["arguments"][0].clone();
        assert_eq!(confirm["version"], 2);
        let mut stale = confirm.clone();
        stale["version"] = json!(1);
        let rejected = execute_plain_action(&mut client, stale);
        assert!(rejected["error"].is_object(), "stale version must fail: {rejected}");
        assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
        assert!(execute_plain_action(&mut client, confirm)["error"].is_null());
        client.until(|c| !c.editor.text(&uri).contains(&expected_path));
        assert_eq!(repo.git(&["log", "-1", "--format=%B"]), format!("{edited}\n"));
        assert_eq!(repo.git(&["status", "--porcelain"]), "");
        let terminal = client.editor.code_action(
            &expected_uri,
            json!({"start":{"line":0,"character":0},"end":{"line":0,"character":0}}),
        );
        let id = client.send(terminal);
        assert!(client.until_response(id)["result"].as_array().unwrap().is_empty());

        std::fs::write(repo.path().join("f.txt"), "abort draft worktree change\n").unwrap();
        client.until(|c| c.editor.text(&uri).contains("f.txt"));
        let status = client.editor.text(&uri).to_owned();
        let file_line = status.lines().position(|line| line.contains("f.txt")).unwrap() as u32;
        let stage = plain_code_actions(&mut client, &uri, file_line).into_iter()
            .find(|action| action["command"]["arguments"][0]["command"] == "status.stage")
            .expect("stage second draft change")["command"]["arguments"][0].clone();
        assert!(execute_plain_action(&mut client, stage)["error"].is_null());
        let status = client.editor.text(&uri).to_owned();
        let file_line = status.lines().position(|line| line.contains("f.txt")).unwrap() as u32;
        let open_commit = plain_code_actions(&mut client, &uri, file_line).into_iter()
            .find(|action| action["command"]["arguments"][0]["command"] == "open.commit")
            .expect("open second commit menu")["command"]["arguments"][0].clone();
        assert!(execute_plain_action(&mut client, open_commit)["error"].is_null());
        let menu = client.editor.text(&uri).to_owned();
        let line = menu.lines().position(|line| line.contains("Commit")).unwrap_or(0) as u32;
        let create = plain_code_actions(&mut client, &uri, line).into_iter()
            .find(|action| action["command"]["arguments"][0]["command"] == "menu.run-action")
            .expect("start abortable commit")["command"]["arguments"][0].clone();
        assert!(execute_plain_action(&mut client, create)["error"].is_null());
        client.until(|c| c.editor.text(&uri).contains(&expected_path));
        let initial = std::fs::read_to_string(&expected_path).unwrap();
        let did_open = client.editor.open(&expected_uri, &initial);
        client.send(did_open);
        let did_change = client.editor.edit(&expected_uri, "Abort this commit draft\n");
        client.send(did_change);
        let actions = client.editor.code_action(
            &expected_uri,
            json!({"start":{"line":0,"character":0},"end":{"line":0,"character":0}}),
        );
        let id = client.send(actions);
        let offered = client.until_response(id)["result"].as_array().unwrap().clone();
        let abort = offered.iter().find(|action| action["title"] == "Abort commit")
            .expect("attached draft offers Abort")["command"]["arguments"][0].clone();
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
        assert!(client.until_response(id)["result"].as_array().unwrap().is_empty());
        assert_eq!(repo.git(&["rev-parse", "HEAD"]), abort_head);
        assert_eq!(repo.git(&["write-tree"]), abort_tree);
        assert_eq!(repo.git(&["status", "--porcelain"]), abort_status);
    } else {
        client.send(json!({"jsonrpc":"2.0","method":"window/workDoneProgress/cancel","params":{"token":token}}));
        client.until(|c| {
            let begins = c.server.notifications.iter().filter(|m| m["method"] == "$/progress" && m["params"]["value"]["kind"] == "begin").count();
            let ends = c.server.notifications.iter().filter(|m| m["method"] == "$/progress" && m["params"]["value"]["kind"] == "end").count();
            begins > 0 && ends >= begins && !c.editor.text(&uri).contains(&expected_path)
        });
    }

    let requests: Vec<_> = client.editor.outbound.iter().filter(|m| m["method"] == "window/showDocument").collect();
    assert_eq!(requests.len(), usize::from(declared), "{case}");
    if let Some(success) = succeeds {
        let request = requests[0];
        assert_eq!(request["params"]["uri"], format!("file://{expected_path}"), "{case}");
        assert_eq!(request["params"]["external"], false, "{case}");
        assert_eq!(request["params"]["takeFocus"], true, "{case}");
        let reply = client.server.sent.iter().find(|m| m["id"] == request["id"] && m["method"].is_null()).expect("correlated showDocument response");
        assert_eq!(reply["result"]["success"], success, "{case}");
    }
    let manual = client.server.notifications.iter().find(|m| m["method"] == "window/showMessage"
        && m["params"]["message"].as_str().is_some_and(|message| message.contains("COMMIT_EDITMSG")));
    if succeeds == Some(true) { assert!(manual.is_none(), "success must not use manual fallback: {case}"); }
    else {
        let manual = manual.expect("Plain editor instructions use standard window/showMessage");
        assert!(manual["params"]["message"].as_str().unwrap().contains(&expected_path), "{case}");
        assert!(manual["params"]["message"].as_str().unwrap().contains("Open"), "{case}");
        assert!(manual["params"]["message"].as_str().unwrap().contains("Confirm"), "{case}");
        assert!(manual["params"]["message"].as_str().unwrap().contains("Abort"), "{case}");
        if succeeds == Some(false) { assert!(client.server.sent.iter().any(|m| m["method"] == "window/workDoneProgress/cancel"), "cancel after fallback: {case}"); }
    }
    assert!(
        client.server.notifications.iter().all(|m| m["method"] != "view/effects"),
        "unexpected custom notifications: {:?}",
        client.server.notifications.iter().filter(|m| m["method"] == "view/effects").collect::<Vec<_>>()
    );
    if declared {
        assert_eq!(repo.git(&["rev-parse", "HEAD"]), head, "{case}");
    } else {
        assert_ne!(repo.git(&["rev-parse", "HEAD"]), head, "manual Confirm must commit: {case}");
    }
    client.finish();
    }
}
