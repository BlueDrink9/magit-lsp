use super::*;

pub(super) fn limited_rich(effects: Value, prompts: Value) -> Value {
    json!({"experimental":{"magit":{
        "v":1,"keys":"dynamic","modifiers":"full",
        "layout":["main"],"effects":effects,"prompts":prompts
    }}})
}

pub(super) fn invoke_rich(server: &mut TestServer, id: i64, view: &str, command: &str) -> Value {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/invoke",
        "params":{"viewId":view,"command":command}}));
    server.response_with_id(id)
}

#[test]
fn rich_acknowledged_edits_reflow_real_menu() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let mut server = TestServer::spawn(&[]);
    server.initialize(limited_rich(json!([]), json!([])));
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let opened = invoke_rich(&mut server, 3, &status, "open.commit");
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == "pushView")
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let initial = server.recv();
    assert_eq!(initial["method"], "view/render");
    assert_eq!(server.recv()["method"], "view/keymap");
    let mut text: Vec<String> = serde_json::from_value(initial["params"]["text"].clone()).unwrap();
    for (version, cols) in (1..).zip([60, 140]) {
        server.send(json!({"jsonrpc":"2.0","method":"view/rendered",
                "params":{"viewId":menu,"version":version}}));
        server.send(json!({"jsonrpc":"2.0","method":"view/resize",
                "params":{"viewId":menu,"cols":cols,"rows":30}}));
        let render = server.recv();
        assert_eq!(render["method"], "view/render", "{render}");
        let params = &render["params"];
        assert_eq!(params["version"], version + 1);
        assert!(params.get("text").is_none(), "{render}");
        assert_eq!(params["baseVersion"], version);
        let edits: Vec<magit_lsp::protocol::LineEdit> =
            serde_json::from_value(params["edits"].clone()).unwrap();
        text = magit_lsp::diff_text::apply_edits(&text, &edits);
        let expected = if cols == 60 {
            include_str!("../snapshots/menu-60.txt")
        } else {
            include_str!("../snapshots/menu-140.txt")
        };
        assert_eq!(text.join("\n"), expected.trim_end());
    }
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn rich_server_folds_and_acknowledged_provider_across_visibility_and_refresh() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut server = TestServer::spawn(&[]);
    let initialized = server.initialize(limited_rich(json!([]), json!([])));
    assert_eq!(initialized["result"]["serverInfo"]["name"], "magit-lsp");
    assert_eq!(
        initialized["result"]["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(
        initialized["result"]["capabilities"]["foldingRangeProvider"],
        true
    );
    server.send(open_view_message(2, &repo_param(tmp.path())));
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"].as_str().unwrap().to_owned();
    let uri = opened["result"]["uri"].as_str().unwrap().to_owned();
    let initial_loading = server.recv();
    assert_eq!(initial_loading["method"], "view/render");
    assert_eq!(server.recv()["method"], "view/keymap");
    let initial = server.wait_for_view_render_after(
        &view,
        initial_loading["params"]["version"].as_u64().unwrap(),
    );
    let base_version = initial["params"]["version"].as_u64().unwrap();
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view,"version":base_version}}));
    let mut text: Vec<String> = serde_json::from_value(initial["params"]["text"].clone()).unwrap();
    // The acquired render has closed sections. Open them so the visibility
    // baseline below is the full tree the fold assertions describe. Derive the
    // baseline from this render: a resync would mark the next one as a full
    // replacement instead of the acknowledged edits the loop expects.
    server.send(json!({"jsonrpc":"2.0","id":9,"method":"view/invoke",
        "params":{"viewId":view,"command":"common.show-all"}}));
    let opened = server.response_with_id(9);
    assert!(opened["error"].is_null(), "{opened}");
    let expanded = server.wait_for_view_render_after(&view, base_version);
    let expanded_version = expanded["params"]["version"].as_u64().unwrap();
    let edits: Vec<magit_lsp::protocol::LineEdit> =
        serde_json::from_value(expanded["params"]["edits"].clone()).unwrap();
    text = magit_lsp::diff_text::apply_edits(&text, &edits);
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered",
        "params":{"viewId":view,"version":expanded_version}}));
    let initial_text = serde_json::json!(text);
    assert!(initial_text
        .as_array()
        .unwrap()
        .iter()
        .any(|line| line.as_str().unwrap().contains("+line3 CHANGED")));
    server.send(
        json!({"jsonrpc":"2.0","id":10,"method":"textDocument/foldingRange",
            "params":{"textDocument":{"uri":uri}}}),
    );
    let initial_ranges = server.response_with_id(10)["result"].clone();
    assert!(
        !initial_ranges.as_array().unwrap().is_empty(),
        "initial expanded tree supplies native fold ranges"
    );
    for (index, command) in [
        "common.hide-all",
        "common.refresh",
        "common.show-all",
        "common.less-detail",
        "common.detail-level-1",
        "common.cycle-visibility",
        "common.hide-section",
        "common.toggle-section",
    ]
    .into_iter()
    .enumerate()
    {
        let version = expanded_version + index as u64;
        server.send(json!({"jsonrpc":"2.0","method":"view/rendered",
                "params":{"viewId":view,"version":version}}));
        if index == 0 {
            server.send(
                json!({"jsonrpc":"2.0","id":11,"method":"textDocument/foldingRange",
                    "params":{"textDocument":{"uri":uri}}}),
            );
            let ranges = server.response_with_id(11)["result"]
                .as_array()
                .unwrap()
                .clone();
            assert!(
                !ranges.is_empty(),
                "acknowledged provider exposes ranges for expanded text"
            );
            let len = initial_text.as_array().unwrap().len() as u64;
            assert!(ranges
                .iter()
                .all(|range| range["startLine"].as_u64().unwrap()
                    < range["endLine"].as_u64().unwrap()
                    && range["endLine"].as_u64().unwrap() < len));
        } else if index == 1 {
            server.send(
                json!({"jsonrpc":"2.0","id":11,"method":"textDocument/foldingRange",
                    "params":{"textDocument":{"uri":uri}}}),
            );
            assert_eq!(
                server.response_with_id(11)["result"],
                json!([]),
                "ack of an edit advances provider snapshot to collapsed text"
            );
        }
        // Resolve a positional fold through structured symbols, never text.
        let line = 0;
        let id = 20 + index as i64;
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/invoke",
                "params":{"viewId":view,"command":command,"version":version,
                    "cursor":{"line":line,"column":0}}}));
        let response = server.response_with_id(id);
        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(response["result"]["effects"], json!([]), "{response}");
        let render = server.recv();
        assert_eq!(render["method"], "view/render", "{render}");
        assert_eq!(render["params"]["version"], version + 1);
        assert_eq!(render["params"]["baseVersion"], version);
        let edits: Vec<magit_lsp::protocol::LineEdit> =
            serde_json::from_value(render["params"]["edits"].clone()).unwrap();
        text = magit_lsp::diff_text::apply_edits(&text, &edits);
        if command == "common.show-all" {
            assert_eq!(
                text,
                serde_json::from_value::<Vec<String>>(initial_text.clone()).unwrap(),
                "{command}"
            );
        } else if index <= 1 {
            assert!(
                text.len() < initial_text.as_array().unwrap().len(),
                "server collapse survives refresh"
            );
            if index == 0 {
                server.send(
                    json!({"jsonrpc":"2.0","id":12,"method":"textDocument/foldingRange",
                        "params":{"textDocument":{"uri":uri}}}),
                );
                assert_eq!(
                    server.response_with_id(12)["result"],
                    initial_ranges,
                    "until ack, providers still describe the previous expanded text"
                );
            }
        }
    }
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn rich_basic_keys_filter_delivery_and_matching_without_disabling_explicit_menu_actions() {
    magit_lsp_test_support::cargo_server_binary!();
    for modifiers in ["basic", "full"] {
        let (tmp, _) = RepoFixture::new()
            .commit("init", [("f.txt", TEN_LINES)])
            .build();
        let config_dir = tempfile::TempDir::new().unwrap();
        let path = config_dir.path().join("config.toml");
        std::fs::write(&path, "profile = \"standard\"\n[keys.status]\n\"open.commit\" = [\"meta+c\", \"shift+Tab\", \"ctrl+shift+Tab\", \"k meta+c\"]\n[keys.common]\nrefresh = [\"ctrl+alt+r\", \"K\"]\n").unwrap();
        let config = path.to_string_lossy().into_owned();
        let mut server = TestServer::spawn(&[("MAGIT_LSP_CONFIG", &config)]);
        let mut caps = limited_rich(json!([]), json!(["text"]));
        caps["experimental"]["magit"]["modifiers"] = json!(modifiers);
        server.initialize(caps);
        server.send(open_view_message(2, &repo_param(tmp.path())));
        let opened = server.response_with_id(2);
        let view = opened["result"]["id"].as_str().unwrap().to_owned();
        let render = server.recv();
        assert_eq!(render["method"], "view/render");
        let keymap = server.recv();
        assert_eq!(keymap["method"], "view/keymap");
        let ready =
            server.wait_for_view_render_after(&view, render["params"]["version"].as_u64().unwrap());
        let version = ready["params"]["version"].as_u64().unwrap();
        server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view,"version":version}}));
        assert_eq!(
            keymap_keys(&keymap, "common.refresh"),
            Some(vec!["K".into(), "ctrl+alt+r".into()])
        );
        let expected = if modifiers == "basic" {
            vec!["shift+Tab".into()]
        } else {
            vec![
                "ctrl+shift+Tab".into(),
                "k meta+c".into(),
                "meta+c".into(),
                "shift+Tab".into(),
            ]
        };
        assert_eq!(keymap_keys(&keymap, "open.commit"), Some(expected));
        server.send(json!({"jsonrpc":"2.0","id":7,"method":"view/key",
            "params":{"viewId":view,"version":version,"key":"ctrl+alt+r","editorMode":"normal","cursor":{"line":0,"column":0}}}));
        assert_eq!(server.response_with_id(7)["result"]["effects"], json!([]));
        let refreshed = server.recv();
        assert_eq!(refreshed["method"], "view/render");
        assert_eq!(
            refreshed["params"]["version"],
            version + 1,
            "basic retains ctrl/alt resolution"
        );
        server.send(
            json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view,"version":version + 1}}),
        );
        if modifiers == "basic" {
            for (id, key) in [(8, "ctrl+shift+Tab"), (9, "k")] {
                server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key",
                    "params":{"viewId":view,"version":version + 1,"key":key,"editorMode":"normal","cursor":{"line":0,"column":0}}}));
                let response = server.response_with_id(id);
                assert_eq!(
                    response["result"]["effects"],
                    json!([{"type":"passThrough","key":key}]),
                    "filtered chords must not leave pending prefixes: {response}"
                );
            }
            server.send(json!({"jsonrpc":"2.0","id":10,"method":"view/key",
                "params":{"viewId":view,"version":version + 1,"key":"shift+Tab","editorMode":"normal","cursor":{"line":0,"column":0}}}));
            let backtab = server.response_with_id(10);
            assert_eq!(
                backtab["result"]["effects"][0]["type"], "pushView",
                "basic retains terminal backtab: {backtab}"
            );
            assert_eq!(server.recv()["method"], "view/render");
            assert_eq!(server.recv()["method"], "view/keymap");
        }
        server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/key",
            "params":{"viewId":view,"version":2,"key":"meta+c","editorMode":"normal","cursor":{"line":0,"column":0}}}));
        let keyed = server.response_with_id(3);
        let pushed = keyed["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|effect| effect["type"] == "pushView");
        let menu = if modifiers == "full" {
            pushed.expect("full retains meta binding")["descriptor"]["id"]
                .as_str()
                .unwrap()
                .to_owned()
        } else {
            assert!(
                pushed.is_none(),
                "basic must not resolve meta binding: {keyed}"
            );
            assert_eq!(keyed["result"]["effects"][0]["type"], "passThrough");
            let explicit = invoke_rich(&mut server, 4, &view, "open.commit");
            explicit["result"]["effects"]
                .as_array()
                .unwrap()
                .iter()
                .find(|effect| effect["type"] == "pushView")
                .expect("explicit invocation is not key-capability disabled")["descriptor"]["id"]
                .as_str()
                .unwrap()
                .to_owned()
        };
        let menu_render = server.recv();
        assert_eq!(menu_render["method"], "view/render");
        assert!(menu_render["params"]["text"]
            .as_array()
            .unwrap()
            .iter()
            .any(|line| line.as_str().unwrap().contains("Amend")));
        let menu_keys = server.recv();
        assert_eq!(menu_keys["method"], "view/keymap");
        // Uppercase printable menu switches remain usable with basic modifiers.
        server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/key",
            "params":{"viewId":menu,"version":1,"key":"-","editorMode":"normal","cursor":{"line":0,"column":0}}}));
        assert_eq!(server.response_with_id(5)["result"]["effects"], json!([]));
        let pending = server.recv();
        assert_eq!(pending["method"], "view/render");
        assert_eq!(server.recv()["method"], "view/keymap");
        server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/key",
            "params":{"viewId":menu,"version":2,"key":"R","editorMode":"normal","cursor":{"line":0,"column":0}}}));
        let toggled = server.response_with_id(6);
        assert_eq!(toggled["result"]["effects"], json!([]), "{toggled}");
        let render = server.recv();
        assert_eq!(render["method"], "view/render");
        assert_ne!(
            render["params"]["text"], menu_render["params"]["text"],
            "menu switch changes server text"
        );
        assert_eq!(server.recv()["method"], "view/keymap");
        shutdown_and_exit(&mut server);
        assert!(server.finish(Duration::from_secs(10)).success());
    }
}

#[test]
fn rich_missing_effects_refuses_before_git_or_editor_and_never_emits_optional_effects() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", "initial\n")])
        .modify("f.txt", "changed\n")
        .build();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let index = tmp.git(&["ls-files", "--stage", "-z"]);
    let draft = std::fs::read(tmp.path().join(".git/COMMIT_EDITMSG")).unwrap();
    let trace = tmp.path().join("trace.jsonl");
    let mut server = TestServer::spawn(&[("GIT_TRACE2_EVENT", trace.to_str().unwrap())]);
    assert!(server.initialize(limited_rich(
        json!([]),
        json!(["confirm", "text", "secret"])
    ))["error"]
        .is_null());
    let view = open_view(&mut server, 2, &repo_param(tmp.path()));
    for (id, command, required) in [
        (3, "common.copy-value", "clipboard"),
        (4, "common.visit", "visitFile"),
        (5, "log.select-and-copy", "clipboard"),
        (6, "commit.create", "editDocument"),
        (7, "commit.amend", "editDocument"),
        (8, "commit.reword", "editDocument"),
    ] {
        let response = invoke_rich(&mut server, id, &view, command);
        let effects = response["result"]["effects"].as_array().unwrap();
        assert_eq!(effects.len(), 1, "{response}");
        assert_eq!(effects[0]["type"], "message", "{response}");
        assert!(
            effects[0]["text"].as_str().unwrap().contains(required),
            "{response}"
        );
    }
    let opened = invoke_rich(&mut server, 9, &view, "open.commit");
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == "pushView")
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap();
    // Menu-key dispatch must refuse before its commit continuation can stage or spawn.
    server.send(
        json!({"jsonrpc":"2.0","id":10,"method":"view/key","params":{
            "viewId":menu,"version":1,"key":"c","editorMode":"normal","cursor":{"line":0,"column":0}
        }}),
    );
    let keyed = server.response_with_id(10);
    assert!(
        keyed["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == "message"
                && effect["text"].as_str().unwrap().contains("editDocument")),
        "{keyed}"
    );
    server.shutdown_and_exit();
    let mut observed = server.notifications.clone();
    let (exit, remaining) = server.finish_with_messages(Duration::from_secs(10));
    assert!(exit.success());
    observed.extend(remaining);
    assert!(!observed
        .iter()
        .any(|message| message["method"] == "view/prompt"));
    assert!(!observed.iter().any(|message| {
        message["params"]["effects"]
            .as_array()
            .is_some_and(|effects| {
                effects.iter().any(|effect| {
                    matches!(
                        effect["type"].as_str(),
                        Some("editDocument" | "clipboard" | "visitFile")
                    )
                })
            })
    }));
    for line in read(&trace).lines() {
        let event: Value = serde_json::from_str(line).unwrap();
        if event["event"] == "start" {
            assert!(
                !event["argv"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|arg| arg == "commit" || arg == "add"),
                "mutation started: {event}"
            );
        }
    }
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), index);
    assert_eq!(
        std::fs::read(tmp.path().join(".git/COMMIT_EDITMSG")).unwrap(),
        draft
    );
    assert!(!tmp.path().join(".git/index.lock").exists());
}

#[test]
fn rich_prompt_document_edit_adopts_fields_and_repairs_other_lines() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let mut server = TestServer::spawn(&[]);
    server.initialize(limited_rich(json!(["editDocument"]), json!(["confirm"])));
    let view = open_view(&mut server, 2, &repo_param(tmp.path()));

    let opened = invoke_rich(&mut server, 3, &view, "open.commit");
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == "pushView")
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(server.recv()["method"], json!("view/render"));
    assert_eq!(server.recv()["method"], json!("view/keymap"));

    // `=` prefixes the option; `A` asks for the author and, because the client
    // declares no text prompt, opens a document prompt instead.
    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/key",
        "params":{"viewId":menu,"version":1,"key":"=","editorMode":"normal",
            "cursor":{"line":0,"column":0}}}));
    assert_eq!(server.response_with_id(4)["result"]["effects"], json!([]));
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/key",
        "params":{"viewId":menu,"version":1,"key":"A","editorMode":"normal",
            "cursor":{"line":0,"column":0}}}));
    let mut key_response = None;
    let (prompt_view, uri, initial) = loop {
        let message = server.recv();
        if message["method"] == json!("view/render") && message["params"]["viewId"] != json!(menu) {
            assert!(message["params"]["text"].is_array(), "full prompt text");
            let view_id = message["params"]["viewId"].as_str().unwrap().to_owned();
            break (
                view_id.clone(),
                format!("magit://prompt/{view_id}"),
                message,
            );
        }
        if message["id"] == json!(5) {
            key_response = Some(message);
        }
    };
    let key_response = key_response.expect("the pushed prompt descriptor");
    assert!(
        key_response["result"]["effects"]
            .as_array()
            .is_some_and(|effects| effects
                .iter()
                .any(|effect| effect["type"] == json!("pushView"))),
        "{key_response}"
    );
    let initial_version = initial["params"]["version"].as_u64().unwrap();
    assert_eq!(
        initial["params"]["text"][0],
        json!("Override the author (--author=)")
    );
    assert_eq!(initial["params"]["text"][2], json!(""));
    assert_eq!(initial["params"]["text"][3], json!("Submit"));
    assert_eq!(initial["params"]["text"][4], json!("Cancel"));

    let mut lines: Vec<String> = initial["params"]["text"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| line.as_str().unwrap().to_owned())
        .collect();
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen",
        "params":{"textDocument":{"uri":uri,"languageId":"magit-prompt","version":1,
            "text":lines.join("\n")}}}));
    lines[2] = "Alice".to_owned();
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange",
        "params":{"textDocument":{"uri":uri,"version":2},
            "contentChanges":[{"text":lines.join("\n")}]}}));

    // An adopted field needs no revert and produces no client-visible output.
    let quiet = server.drain_notifications();
    assert!(
        !quiet.iter().any(|message| {
            message["method"] == json!("workspace/applyEdit")
                || message["method"] == json!("view/prompt")
                || message["method"] == json!("window/showMessage")
        }),
        "a field edit must not be reverted or reported: {quiet:?}"
    );
    assert!(
        !quiet.iter().any(
            |message| message["params"]["effects"]
                .as_array()
                .is_some_and(|effects| effects
                    .iter()
                    .any(|effect| effect["type"] == json!("editDocument")))
        ),
        "a field edit must not emit a document edit: {quiet:?}"
    );

    // Editing a non-field line is repaired: the stored title and field survive.
    lines[0] = "Tampered title".to_owned();
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange",
        "params":{"textDocument":{"uri":uri,"version":3},
            "contentChanges":[{"text":lines.join("\n")}]}}));
    let after = server.drain_notifications();
    let repaired = after
        .iter()
        .find(|message| {
            message["method"] == json!("view/render")
                && message["params"]["viewId"] == json!(prompt_view)
                && message["params"]["version"].as_u64() > Some(initial_version)
        })
        .unwrap_or_else(|| panic!("a repaired prompt render: {after:?}"));
    assert_eq!(
        repaired["params"]["text"][0],
        json!("Override the author (--author=)")
    );
    assert_eq!(repaired["params"]["text"][2], json!("Alice"));
    assert!(
        !after.iter().any(|message| {
            message["method"] == json!("workspace/applyEdit")
                || message["method"] == json!("view/prompt")
                || message["method"] == json!("window/showMessage")
        }),
        "a repair must not revert, prompt natively or report an error: {after:?}"
    );

    // The repaired prompt is still the active one.
    let refused = invoke_rich(&mut server, 6, &view, "status.stage-all");
    assert!(
        refused["result"]["effects"][0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("pending prompt")),
        "{refused}"
    );

    assert!(!server
        .notifications
        .iter()
        .any(|message| { message["method"] == json!("view/prompt") }));
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn rich_missing_confirm_opens_a_document_prompt_without_mutation() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", "initial\n")])
        .modify("f.txt", "changed\n")
        .build();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let index = tmp.git(&["ls-files", "--stage", "-z"]);
    let mut server = TestServer::spawn(&[]);
    server.initialize(limited_rich(json!(["editDocument"]), json!([])));
    let view = open_view(&mut server, 2, &repo_param(tmp.path()));
    let response = invoke_rich(&mut server, 3, &view, "commit.create");
    let effects = response["result"]["effects"].as_array().unwrap();
    assert_eq!(effects[0]["type"], "pushView", "{response}");
    assert_eq!(effects[0]["descriptor"]["kind"], "prompt", "{response}");
    // The pending document prompt refuses the next command.
    let refused = invoke_rich(&mut server, 4, &view, "open.commit");
    assert!(
        refused["result"]["effects"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Answer or cancel the pending prompt"),
        "{refused}"
    );
    server.shutdown_and_exit();
    let mut observed = server.notifications.clone();
    let (exit, remaining) = server.finish_with_messages(Duration::from_secs(10));
    assert!(exit.success());
    observed.extend(remaining);
    assert!(!observed
        .iter()
        .any(|message| message["method"] == "view/prompt"));
    assert!(!observed.iter().any(|message| {
        message["params"]["effects"]
            .as_array()
            .is_some_and(|effects| {
                effects
                    .iter()
                    .any(|effect| effect["type"] == "editDocument")
            })
    }));
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), index);
    assert_eq!(read(&tmp.path().join("f.txt")), "changed\n");
    assert!(!tmp.path().join(".git/index.lock").exists());
}

#[test]
fn rich_confirm_row_enter_answers_without_mutation() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", "initial\n")])
        .modify("f.txt", "changed\n")
        .build();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let index = tmp.git(&["ls-files", "--stage", "-z"]);
    let mut server = TestServer::spawn(&[]);
    server.initialize(limited_rich(json!(["editDocument"]), json!([])));
    let view = open_view(&mut server, 2, &repo_param(tmp.path()));
    let opened = invoke_rich(&mut server, 3, &view, "commit.create");
    assert!(
        opened["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == json!("pushView")
                && effect["descriptor"]["kind"] == json!("prompt")),
        "{opened}"
    );
    let (prompt, version, text) = loop {
        let message = server.recv();
        if message["method"] == json!("view/render") && message["params"]["viewId"] != json!(view) {
            let prompt = message["params"]["viewId"].as_str().unwrap().to_owned();
            let version = message["params"]["version"].as_u64().unwrap();
            let text: Vec<String> =
                serde_json::from_value(message["params"]["text"].clone()).unwrap();
            break (prompt, version, text);
        }
    };
    let no_row = text.iter().position(|line| line == "No").expect("No row");

    // Enter on a non-row line is not an answer: the prompt stays active.
    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/key",
        "params":{"viewId":prompt,"version":version,"key":"Enter","editorMode":"normal",
            "cursor":{"line":0,"column":0}}}));
    let non_row = server.response_with_id(4);
    assert!(
        !non_row["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == json!("popView")),
        "{non_row}"
    );
    let refused = invoke_rich(&mut server, 5, &view, "status.stage-all");
    assert!(
        refused["result"]["effects"][0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("pending prompt")),
        "{refused}"
    );

    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/key",
        "params":{"viewId":prompt,"version":version,"key":"Enter","editorMode":"normal",
            "cursor":{"line":no_row,"column":0}}}));
    let answered = server.response_with_id(6);
    assert!(
        answered["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == json!("popView") && effect["viewId"] == json!(prompt)),
        "{answered}"
    );
    server.drain_notifications();
    assert!(!server
        .notifications
        .iter()
        .any(|message| message["method"] == json!("view/prompt")));
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), index);
    assert_eq!(read(&tmp.path().join("f.txt")), "changed\n");
    assert!(!tmp.path().join(".git/index.lock").exists());
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn rich_text_prompt_submit_row_answers_with_the_reconciled_field() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let mut server = TestServer::spawn(&[]);
    server.initialize(limited_rich(json!(["editDocument"]), json!(["confirm"])));
    let view = open_view(&mut server, 2, &repo_param(tmp.path()));
    let opened = invoke_rich(&mut server, 3, &view, "open.commit");
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == "pushView")
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(server.recv()["method"], json!("view/render"));
    assert_eq!(server.recv()["method"], json!("view/keymap"));

    // `=` prefixes the option; `A` asks for the author and opens a document
    // prompt because the client declares no text prompt.
    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/key",
        "params":{"viewId":menu,"version":1,"key":"=","editorMode":"normal",
            "cursor":{"line":0,"column":0}}}));
    assert_eq!(server.response_with_id(4)["result"]["effects"], json!([]));
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/key",
        "params":{"viewId":menu,"version":1,"key":"A","editorMode":"normal",
            "cursor":{"line":0,"column":0}}}));
    let (prompt, uri, initial) = loop {
        let message = server.recv();
        if message["method"] == json!("view/render") && message["params"]["viewId"] != json!(menu) {
            let view_id = message["params"]["viewId"].as_str().unwrap().to_owned();
            break (
                view_id.clone(),
                format!("magit://prompt/{view_id}"),
                message,
            );
        }
    };
    let version = initial["params"]["version"].as_u64().unwrap();
    let mut lines: Vec<String> = initial["params"]["text"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| line.as_str().unwrap().to_owned())
        .collect();
    let submit = lines
        .iter()
        .position(|line| line == "Submit")
        .expect("Submit row");

    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen",
        "params":{"textDocument":{"uri":uri,"languageId":"magit-prompt","version":1,
            "text":lines.join("\n")}}}));
    lines[submit - 1] = "Alice".to_owned();
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange",
        "params":{"textDocument":{"uri":uri,"version":2},
            "contentChanges":[{"text":lines.join("\n")}]}}));
    let quiet = server.drain_notifications();
    assert!(
        !quiet
            .iter()
            .any(|message| message["method"] == json!("view/prompt")),
        "{quiet:?}"
    );

    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/key",
        "params":{"viewId":prompt,"version":version,"key":"Enter","editorMode":"normal",
            "cursor":{"line":submit,"column":0}}}));
    let answered = server.response_with_id(6);
    assert!(
        answered["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == json!("popView") && effect["viewId"] == json!(prompt)),
        "{answered}"
    );
    let after = server.drain_notifications();
    let menu_reflects = server
        .notifications
        .iter()
        .chain(after.iter())
        .any(|message| {
            message["method"] == json!("view/render")
                && message["params"]["viewId"] == json!(menu)
                && message["params"]["text"].as_array().is_some_and(|lines| {
                    lines.iter().any(|line| {
                        line.as_str().is_some_and(|line| {
                            line.contains("Override the author") && line.contains("Alice")
                        })
                    })
                })
        });
    assert!(
        menu_reflects,
        "menu does not reflect the author: {:?}",
        server.notifications
    );
    assert!(!server
        .notifications
        .iter()
        .chain(after.iter())
        .any(|message| message["method"] == json!("view/prompt")));
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn rich_full_caps_delivers_clipboard_and_visit_file() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", "initial\n")])
        .modify("f.txt", "changed\n")
        .build();
    let mut server = TestServer::spawn(&[]);
    server.initialize_rich(json!({}));
    let (view, acquired) = super::open_view_versioned(&mut server, 2, &repo_param(tmp.path()));
    let uri = format!(
        "magit://status/{}",
        tmp.path().to_string_lossy().trim_start_matches('/')
    );
    // The hunk is only in the outline once the closed section is opened.
    let version = super::show_all_and_ack(&mut server, 3, &view, acquired);
    // Resolve the request position from the server's structured outline.
    let line = first_hunk_line(&mut server, &uri);
    for (id, command, effect) in [
        (4, "common.copy-value", "clipboard"),
        (5, "common.visit", "visitFile"),
    ] {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/invoke",
            "params":{"viewId":view,"version":version,"command":command,"cursor":{"line":line,"column":0}}}));
        let response = server.response_with_id(id);
        assert!(
            response["result"]["effects"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value["type"] == effect),
            "{response}"
        );
    }
    server.shutdown_and_exit();
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn rich_main_only_nested_push_pop_preserves_caller_and_full_caps_keep_placement() {
    magit_lsp_test_support::cargo_server_binary!();
    for full in [false, true] {
        let (tmp, _) = RepoFixture::new()
            .commit("init", [("f.txt", "initial\n")])
            .build();
        let mut server = TestServer::spawn(&[]);
        if full {
            server.initialize_rich(json!({}));
        } else {
            server.initialize(limited_rich(json!([]), json!([])));
        }
        let caller = open_view(&mut server, 2, &repo_param(tmp.path()));
        let opened = invoke_rich(&mut server, 3, &caller, "open.commit");
        let descriptor = &opened["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|effect| effect["type"] == "pushView")
            .unwrap()["descriptor"];
        assert_eq!(
            descriptor["placement"],
            if full { "bottom" } else { "main" }
        );
        let child = descriptor["id"].as_str().unwrap();
        let popped = invoke_rich(&mut server, 4, child, "common.quit");
        let effects = popped["result"]["effects"].as_array().unwrap();
        assert!(
            effects
                .iter()
                .any(|effect| effect["type"] == "popView" && effect["viewId"] == child),
            "{popped}"
        );
        assert!(
            effects
                .iter()
                .any(|effect| effect["type"] == "focusView" && effect["viewId"] == caller),
            "{popped}"
        );
        let reopened = invoke_rich(&mut server, 5, &caller, "open.commit");
        assert!(
            reopened["result"]["effects"]
                .as_array()
                .unwrap()
                .iter()
                .any(|effect| effect["type"] == "pushView"),
            "{reopened}"
        );
        server.shutdown_and_exit();
        assert!(server.finish(Duration::from_secs(10)).success());
    }
}

#[test]
fn managed_draft_shutdown_cancels_saved_checkpoint_and_rejects_pipelined_late_input() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", "initial\n")])
        .staged_file("f.txt", "staged\n")
        .build();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let staged = tmp.git(&["ls-files", "--stage", "-z"]);
    let mut server = TestServer::spawn(&[]);
    server.initialize_rich(json!({}));
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/invoke","params":{"viewId":status,"command":"commit.create"}}));
    let mut response = false;
    let mut edit = None;
    while !response || edit.is_none() {
        let message = server.recv();
        if message["id"] == 4 {
            response = true;
        }
        if message["method"] == "view/effects" {
            edit = message["params"]["effects"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["type"] == "editDocument")
                .cloned()
                .or(edit);
        }
    }
    let edit = edit.unwrap();
    let messages = [
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":edit["uri"],"languageId":"gitcommit","version":1,"text":edit["text"]}}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":edit["uri"],"version":7},"contentChanges":[{"text":"Saved but never confirmed\n"}]}}),
    ];
    server.send_raw(&messages.into_iter().flat_map(frame).collect::<Vec<_>>());
    let messages = [
        json!({"jsonrpc":"2.0","id":19,"method":"shutdown","params":null}),
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":edit["uri"],"version":8},"contentChanges":[{"text":"Too late\n"}]}}),
        json!({"jsonrpc":"2.0","id":20,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[{"command":"common.confirm","runId":edit["runId"],"uri":edit["uri"],"version":8,"source":"synchronized"}]}}),
        json!({"jsonrpc":"2.0","id":21,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[{"command":"common.confirm","runId":edit["runId"],"uri":edit["uri"],"version":7,"source":"saved-file"}]}}),
    ];
    server.send_raw(&messages.into_iter().flat_map(frame).collect::<Vec<_>>());
    assert_eq!(server.response_with_id(19)["result"], Value::Null);
    assert!(server.response_with_id(20)["error"].is_object());
    assert!(server.response_with_id(21)["error"].is_object());
    // Shutdown removes the run before late file sync; unrelated files are ignored.
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), staged);
    assert_eq!(
        read(&tmp.path().join(".git/COMMIT_EDITMSG")),
        edit["text"].as_str().unwrap()
    );
    server.send(json!({"jsonrpc":"2.0","method":"exit","params":null}));
    assert!(server.finish(Duration::from_secs(10)).success());
}
