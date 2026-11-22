#[path = "common/editor_model/mod.rs"]
mod editor_model;
#[path = "common/fake_editor.rs"]
mod fake_editor;
#[path = "common/mod.rs"]
mod fixtures;
#[path = "common/stdio.rs"]
mod stdio;

use fake_editor::{FakeEditor, Profile};
use magit_lsp::dispatch::ActionTarget;
use magit_lsp::errors::STALE_VIEW;
use magit_lsp::file_uri::uri_from_path;
use serde_json::{json, Value};

#[test]
fn action_target_uses_strict_camel_case_wire_fields() {
    let target = ActionTarget {
        node_id: "unstaged/f.txt".into(),
        mutation_hash: "hash".into(),
        selection: None,
    };
    let wire = serde_json::to_value(&target).unwrap();
    assert_eq!(wire["nodeId"], "unstaged/f.txt");
    assert_eq!(wire["mutationHash"], "hash");
    assert!(wire.get("node_id").is_none());
    assert!(serde_json::from_value::<ActionTarget>(json!({
        "node_id": "unstaged/f.txt",
        "mutation_hash": "hash"
    }))
    .is_err());
}

fn next_apply(server: &mut stdio::TestServer) -> Value {
    loop {
        let message = server.recv();
        if message["method"] == "workspace/applyEdit" {
            return message;
        }
    }
}

/// Open the repository's `.magit` status document as a Plain client.
fn open_plain(repo: &fixtures::RepoFixture) -> (stdio::TestServer, FakeEditor, String) {
    open_plain_with_capabilities(
        repo,
        json!({
            "workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},
            "textDocument":{"codeAction":{"dynamicRegistration":true},"general":{"positionEncodings":["utf-16"]}}
        }),
    )
}

fn open_plain_with_capabilities(
    repo: &fixtures::RepoFixture,
    capabilities: Value,
) -> (stdio::TestServer, FakeEditor, String) {
    open_plain_with_capabilities_and_env(repo, capabilities, &[])
}

fn open_plain_with_capabilities_and_env(
    repo: &fixtures::RepoFixture,
    capabilities: Value,
    env: &[(&str, &str)],
) -> (stdio::TestServer, FakeEditor, String) {
    let uri = uri_from_path(&repo.path().join(".magit"))
        .expect("native path has a file URI")
        .as_str()
        .to_owned();
    let mut server = stdio::TestServer::spawn(env);
    let mut init = stdio::initialize_message();
    init["params"]["capabilities"] = capabilities;
    init["params"]["workspaceFolders"] = json!([{"uri":uri_from_path(repo.path()).expect("native path has a file URI").as_str(),"name":"repo"}]);
    server.send(init);
    let initialized = server.response_with_id(1);
    assert_eq!(
        initialized["result"]["capabilities"]["codeActionProvider"],
        true
    );
    assert_eq!(
        initialized["result"]["capabilities"]["executeCommandProvider"]["commands"],
        json!(["magit.command"])
    );
    server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    let mut editor = FakeEditor::new(Profile::Standard);
    server.send(editor.open(&uri, "old"));
    let edit = next_apply(&mut server);
    for frame in editor.dispatch(&edit) {
        server.send(frame);
    }
    (server, editor, uri)
}

/// The offered `status.discard` argument on the `f.txt` row.
fn offered_discard(
    server: &mut stdio::TestServer,
    editor: &FakeEditor,
    uri: &str,
    id: i64,
) -> Value {
    let row = editor
        .text(uri)
        .lines()
        .position(|line| line.contains("f.txt"))
        .expect("file row") as u32;
    let discard = offered_for_range(
        server,
        uri,
        json!({"start":{"line":row,"character":0},"end":{"line":row,"character":0}}),
        "status.discard",
        id,
    );
    assert!(!discard.is_null(), "no status.discard action offered");
    discard
}

#[test]
fn plain_discard_opens_a_document_confirm_and_cancel_changes_nothing() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .modify("f.txt", "changed\n")
        .build();
    let caps = json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},"textDocument":{"codeAction":{"disabledSupport":true}}});
    let (mut server, mut editor, uri) = open_plain_with_capabilities(&repo, caps);

    let discard = offered_discard(&mut server, &editor, &uri, 91);
    let before = repo_state(&repo);
    let opened = execute_offered(&mut server, &mut editor, discard, 92);
    assert!(opened["error"].is_null(), "discard offer failed: {opened}");

    let prompt_text = editor.text(&uri).to_owned();
    assert!(
        prompt_text.contains("Discard changes to f.txt?"),
        "discard confirm missing: {prompt_text}"
    );
    exact_row(&prompt_text, "Yes");
    exact_row(&prompt_text, "No");
    assert!(
        !server
            .sent
            .iter()
            .any(|message| message["method"] == "view/prompt"),
        "Plain discard must not request a native prompt"
    );
    assert_eq!(
        repo_state(&repo),
        before,
        "opening the prompt must not touch Git"
    );

    let no = exact_row(&prompt_text, "No");
    let answer = offered_prompt_answer(&mut server, &uri, no, 93);
    let cancelled = execute_offered(&mut server, &mut editor, answer, 94);
    assert!(
        cancelled["error"].is_null(),
        "discard cancel failed: {cancelled}"
    );
    let closed = editor.text(&uri).to_owned();
    assert!(
        !closed.contains("Discard changes to f.txt?"),
        "discard prompt stayed open: {closed}"
    );
    assert!(
        closed.lines().any(|line| line.contains("f.txt")),
        "status document not restored: {closed}"
    );
    assert_eq!(repo_state(&repo), before, "cancel must not touch Git");

    let discard = offered_discard(&mut server, &editor, &uri, 95);
    execute_offered(&mut server, &mut editor, discard, 96);
    let prompt_text = editor.text(&uri).to_owned();
    assert!(
        prompt_text.contains("Discard changes to f.txt?"),
        "discard confirm missing after cancel: {prompt_text}"
    );
    assert_eq!(
        repo_state(&repo),
        before,
        "opening the prompt must not touch Git"
    );

    let yes = exact_row(&prompt_text, "Yes");
    let answer = offered_prompt_answer(&mut server, &uri, yes, 97);
    let confirmed = execute_offered(&mut server, &mut editor, answer, 98);
    assert!(
        confirmed["error"].is_null(),
        "discard confirm failed: {confirmed}"
    );
    assert_eq!(fixtures::read(&repo.path().join("f.txt")), "one\n");
    let done = editor.text(&uri).to_owned();
    assert!(
        !done.lines().any(|line| line.contains("f.txt")),
        "status still lists the discarded change: {done}"
    );
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn unavailable_actions_are_omitted_without_disabled_support() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .modify("f.txt", "changed\n")
        .build();
    let caps = json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},"textDocument":{"codeAction":{}}});
    let (mut server, editor, uri) = open_plain_with_capabilities(&repo, caps);
    let row = editor
        .text(&uri)
        .lines()
        .position(|line| line.contains("f.txt"))
        .unwrap() as u32;
    server.send(json!({"jsonrpc":"2.0","id":93,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":row,"character":0},"end":{"line":row,"character":0}},"context":{"diagnostics":[]}}}));
    let response = server.response_with_id(93);
    let actions = response["result"].as_array().unwrap();
    assert!(actions
        .iter()
        .any(|action| action["command"]["arguments"][0]["command"] == "status.stage"));
    assert!(
        actions
            .iter()
            .any(|action| action["command"]["arguments"][0]["command"] == "status.discard"),
        "discard is a normal Plain content action: {response}"
    );
    assert!(
        actions.iter().all(|action| action["data"].is_null()),
        "unavailable placeholders must be omitted without disabled support: {response}"
    );
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn code_actions_work_without_execute_command_declaration() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .modify("f.txt", "changed\n")
        .build();
    let caps = json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true}},"textDocument":{"codeAction":{"disabledSupport":true}}});
    let (mut server, mut editor, uri) = open_plain_with_capabilities(&repo, caps);
    let presentation_path = repo.path().join(".magit");
    let disk_sentinel = "disk-owned presentation sentinel\n";
    std::fs::write(&presentation_path, disk_sentinel).unwrap();
    let row = editor
        .text(&uri)
        .lines()
        .position(|line| line.contains("f.txt"))
        .unwrap() as u32;
    server.send(json!({"jsonrpc":"2.0","id":94,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":row,"character":0},"end":{"line":row,"character":0}},"context":{"diagnostics":[]}}}));
    let response = server.response_with_id(94);
    assert!(response["result"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action["command"]["arguments"][0]["command"] == "status.stage"));
    let stage = offered_stage(&mut server, &editor, &uri, 95);
    let before_editor = editor.text(&uri).to_owned();
    assert!(execute_offered(&mut server, &mut editor, stage, 95)["error"].is_null());
    assert_eq!(repo.git(&["show", ":f.txt"]), "changed\n");
    assert_ne!(editor.text(&uri), before_editor);
    assert_eq!(fixtures::read(&presentation_path), disk_sentinel);
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn code_actions_are_unsupported_without_declaration() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .modify("f.txt", "changed\n")
        .build();
    let caps = json!({"workspace":{"applyEdit":true}});
    let (mut server, editor, uri) = open_plain_with_capabilities(&repo, caps);
    let row = editor
        .text(&uri)
        .lines()
        .position(|line| line.contains("f.txt"))
        .unwrap() as u32;
    server.send(json!({"jsonrpc":"2.0","id":96,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":row,"character":0},"end":{"line":row,"character":0}},"context":{"diagnostics":[]}}}));
    assert!(!server.response_with_id(96)["result"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action["command"]["arguments"][0]["command"] == "status.stage"));
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn scoped_planned_and_unavailable_commands_are_disabled_and_forged_requests_are_refused() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .modify("f.txt", "changed\n")
        .build();
    let caps = json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},"textDocument":{"codeAction":{"disabledSupport":true}}});
    let (mut server, mut editor, uri) = open_plain_with_capabilities(&repo, caps);
    let row = editor
        .text(&uri)
        .lines()
        .position(|line| line.contains("f.txt"))
        .unwrap() as u32;
    server.send(json!({"jsonrpc":"2.0","id":95,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":row,"character":0},"end":{"line":row,"character":0}},"context":{"diagnostics":[]}}}));
    let actions = server.response_with_id(95)["result"]
        .as_array()
        .unwrap()
        .clone();
    let planned = actions
        .iter()
        .find(|action| action["data"]["command"] == "status.reverse")
        .expect("scoped planned offer");
    assert!(planned["disabled"]["reason"]
        .as_str()
        .unwrap()
        .contains("R-3.6"));
    assert!(planned["command"].is_null() && planned["data"]["target"].is_null());
    let ids: Vec<_> = actions
        .iter()
        .filter_map(|a| {
            a["data"]["command"]
                .as_str()
                .or_else(|| a["command"]["arguments"][0]["command"].as_str())
        })
        .collect();
    let unique: std::collections::BTreeSet<_> = ids.iter().copied().collect();
    assert_eq!(ids.len(), unique.len(), "duplicate command offer");
    let before = repo_state(&repo);
    let refused = execute_offered(&mut server, &mut editor, planned["data"].clone(), 96);
    assert!(refused["result"]["effects"][0]["text"]
        .as_str()
        .unwrap()
        .contains("planned"));
    assert_eq!(repo_state(&repo), before);
    let stage = offered_stage(&mut server, &editor, &uri, 97);
    assert!(execute_offered(&mut server, &mut editor, stage, 98)["error"].is_null());
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn planned_and_unavailable_entries_are_omitted_without_disabled_support() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .modify("f.txt", "changed\n")
        .build();
    let caps = json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},"textDocument":{"codeAction":{}}});
    let (mut server, editor, uri) = open_plain_with_capabilities(&repo, caps);
    let row = editor
        .text(&uri)
        .lines()
        .position(|line| line.contains("f.txt"))
        .unwrap() as u32;
    server.send(json!({"jsonrpc":"2.0","id":99,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":row,"character":0},"end":{"line":row,"character":0}},"context":{"diagnostics":[]}}}));
    let response = server.response_with_id(99);
    let actions = response["result"].as_array().unwrap();
    assert!(!actions.iter().any(
        |a| a["data"]["command"] == "status.reverse" || a["data"]["command"] == "status.apply"
    ));
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn malformed_plain_envelope_is_refused_without_claiming_rich_commands() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .modify("f.txt", "changed\n")
        .build();
    let caps = json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},"textDocument":{"codeAction":{"disabledSupport":true}}});
    let (mut server, _editor, uri) = open_plain_with_capabilities(&repo, caps);
    let before = repo_state(&repo);
    server.send(json!({"jsonrpc":"2.0","id":101,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[{"command":"status.reverse","documentUri":uri}]}}));
    let response = server.response_with_id(101);
    assert!(response["result"]["effects"][0]["text"]
        .as_str()
        .unwrap()
        .contains("planned"));
    assert_eq!(repo_state(&repo), before);
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

/// Request code actions on the `f.txt` row and return the offered stage argument.
fn offered_stage(server: &mut stdio::TestServer, editor: &FakeEditor, uri: &str, id: i64) -> Value {
    let row = editor
        .text(uri)
        .lines()
        .position(|line| line.contains("f.txt"))
        .expect("file row") as u32;
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":row,"character":0},"end":{"line":row,"character":0}},"context":{"diagnostics":[]}}}));
    let response = server.response_with_id(id);
    response["result"]
        .as_array()
        .expect("actions")
        .iter()
        .find(|action| action["command"]["arguments"][0]["command"] == "status.stage")
        .map(|action| action["command"]["arguments"][0].clone())
        .unwrap_or_else(|| panic!("no stage action offered: {response}"))
}

fn offered_section(
    server: &mut stdio::TestServer,
    editor: &FakeEditor,
    uri: &str,
    section: &str,
    command: &str,
    id: i64,
) -> Value {
    let row = editor
        .text(uri)
        .lines()
        .position(|line| line.starts_with(section))
        .expect("section row") as u32;
    offered_for_range(
        server,
        uri,
        json!({"start":{"line":row,"character":0},"end":{"line":row,"character":0}}),
        command,
        id,
    )
}

/// Execute an offered Plain action, acknowledging any render edits it causes.
fn execute_offered(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    args: Value,
    id: i64,
) -> Value {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[args]}}));
    loop {
        let message = server.recv();
        if message["method"] == "workspace/applyEdit" {
            for frame in editor.dispatch(&message) {
                server.send(frame);
            }
            continue;
        }
        if message["id"] == json!(id) {
            return message;
        }
    }
}

/// Send a `view/invoke`, driving Plain choice-document renders into the editor.
///
/// Plain delivers prompt/choice renders as `workspace/applyEdit` notifications
/// rather than response effects, so they must be acknowledged before the render
/// is visible to the client.
fn invoke_plain(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    id: i64,
    params: Value,
) -> Value {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/invoke","params":params}));
    loop {
        let message = server.recv();
        if message["method"] == "workspace/applyEdit" {
            for frame in editor.dispatch(&message) {
                server.send(frame);
            }
            continue;
        }
        if message["id"] == json!(id) {
            return message;
        }
    }
}

/// The real status view ID and displayed generation from an offered action.
fn offered_status_view(
    server: &mut stdio::TestServer,
    editor: &FakeEditor,
    uri: &str,
    row: u32,
    id: i64,
) -> (String, u64) {
    let args = offered_for_range(
        server,
        uri,
        json!({"start":{"line":row,"character":0},"end":{"line":row,"character":0}}),
        "open.commit",
        id,
    );
    assert!(!args.is_null(), "open.commit not offered on the file row");
    let view_id = args["target"]["viewId"]
        .as_str()
        .expect("status view ID")
        .to_owned();
    let generation = args["viewGeneration"]
        .as_u64()
        .expect("status view generation");
    (view_id, generation)
}

/// Exact `(HEAD, index f.txt, worktree f.txt)` repository state.
fn repo_state(repo: &fixtures::RepoFixture) -> (String, String, String) {
    (
        repo.git(&["rev-parse", "HEAD"]),
        repo.git(&["show", ":f.txt"]),
        fixtures::read(&repo.path().join("f.txt")),
    )
}

/// Assert a stale Plain action was refused readably without an RPC error.
fn assert_refused(response: &Value) {
    assert!(
        response["error"].is_null(),
        "refusal must be a normal result: {response}"
    );
    let effects = response["result"]["effects"]
        .as_array()
        .unwrap_or_else(|| panic!("refusal effects: {response}"));
    assert!(
        effects.iter().any(|effect| effect["type"] == "message"
            && effect["level"] == "warn"
            && effect["text"] == STALE_VIEW),
        "expected readable refusal: {response}"
    );
}

fn action_ids(server: &mut stdio::TestServer, uri: &str, range: Value, id: i64) -> Vec<String> {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":range,"context":{"diagnostics":[]}}}));
    server.response_with_id(id)["result"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| {
            a["command"]["arguments"][0]["target"]["nodeId"]
                .as_str()
                .map(str::to_owned)
        })
        .collect()
}

#[derive(Debug)]
struct OfferedTarget {
    command: String,
    content_node: Option<String>,
    semantic_view: Option<String>,
}

fn action_targets(
    server: &mut stdio::TestServer,
    uri: &str,
    line: u32,
    id: i64,
) -> Vec<OfferedTarget> {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":line,"character":0},"end":{"line":line,"character":0}},"context":{"diagnostics":[]}}}));
    server.response_with_id(id)["result"]
        .as_array()
        .unwrap()
        .iter()
        .map(|action| {
            let args = &action["command"]["arguments"][0];
            let target = &args["target"];
            let content_node = target["mutationHash"]
                .as_str()
                .and_then(|_| target["nodeId"].as_str())
                .map(str::to_owned);
            let semantic_view = target["viewId"].as_str().map(str::to_owned);
            assert_ne!(
                content_node.is_some(),
                semantic_view.is_some(),
                "target must be exactly one typed variant: {target}"
            );
            OfferedTarget {
                command: args["command"].as_str().unwrap().to_owned(),
                content_node,
                semantic_view,
            }
        })
        .collect()
}

fn offered_for_range(
    server: &mut stdio::TestServer,
    uri: &str,
    range: Value,
    command: &str,
    id: i64,
) -> Value {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":range,"context":{"diagnostics":[]}}}));
    server.response_with_id(id)["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["command"]["arguments"][0]["command"] == command)
        .map(|action| action["command"]["arguments"][0].clone())
        .unwrap_or(Value::Null)
}

#[test]
fn titles_use_repository_style_configured_chord_and_structured_path() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .modify("f.txt", "changed\n")
        .build();
    let config_dir = tempfile::tempdir().unwrap();
    let config_path = config_dir.path().join("user.toml");
    let uri = uri_from_path(&repo.path().join(".magit"))
        .expect("native path has a file URI")
        .as_str()
        .to_owned();
    let mut identity: Option<Value> = None;
    for (style, expected, repo_override) in [
        ("plain", "Stage the change at point", false),
        ("mnemonic", "z Stage the change at point", false),
        ("path", "f.txt: Stage the change at point", false),
        ("mnemonicPath", "z f.txt: Stage the change at point", false),
        ("mnemonicPath", "f.txt: Stage the change at point", true),
    ] {
        std::fs::write(&config_path, format!("trust_repo_config = true\n[actions]\ntitle_style = \"{style}\"\n[keys.status]\nstage = [\"z\"]\n")).unwrap();
        let repo_config = repo.path().join(".magit-lsp.toml");
        if repo_override {
            std::fs::write(&repo_config, "[actions]\ntitle_style = \"path\"\n").unwrap();
        } else {
            let _ = std::fs::remove_file(&repo_config);
        }
        let mut server = stdio::TestServer::spawn(&[(
            magit_lsp::config::CONFIG_ENV,
            config_path.to_str().unwrap(),
        )]);
        let mut init = stdio::initialize_message();
        init["params"]["capabilities"] = json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},"textDocument":{"codeAction":{"dynamicRegistration":true},"general":{"positionEncodings":["utf-16"]}}});
        init["params"]["workspaceFolders"] = json!([{"uri":uri_from_path(repo.path()).expect("native path has a file URI").as_str(),"name":"repo"}]);
        server.send(init);
        assert!(server.response_with_id(1)["result"].is_object());
        server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
        let mut editor = FakeEditor::new(Profile::Standard);
        server.send(editor.open(&uri, "old"));
        let edit = next_apply(&mut server);
        for frame in editor.dispatch(&edit) {
            server.send(frame);
        }
        let row = editor
            .text(&uri)
            .lines()
            .position(|line| line.contains("f.txt"))
            .unwrap() as u32;
        server.send(json!({"jsonrpc":"2.0","id":71,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":row,"character":0},"end":{"line":row,"character":0}},"context":{"diagnostics":[]}}}));
        let response = server.response_with_id(71);
        let action = response["result"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["command"]["arguments"][0]["command"] == "status.stage")
            .unwrap();
        assert_eq!(action["title"], expected);
        assert_eq!(action["command"]["title"], expected);
        assert_eq!(action["command"]["command"], "magit.command");
        let args = action["command"]["arguments"][0].clone();
        if let Some(previous) = &identity {
            assert_eq!(args["command"], previous["command"]);
            assert_eq!(args["target"], previous["target"]);
        } else {
            identity = Some(args);
        }
        server.shutdown_and_exit();
        let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
        assert!(status.success());
    }
}

#[test]
fn selection_range_offers_exact_target_and_stages_only_selected_change() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\ntwo\nthree\n")])
        .build();
    fixtures::write(&repo.path().join("f.txt"), "ONE\ntwo\nTHREE\n");
    let uri = uri_from_path(&repo.path().join(".magit"))
        .expect("native path has a file URI")
        .as_str()
        .to_owned();
    let mut server = stdio::TestServer::spawn(&[]);
    let mut init = stdio::initialize_message();
    init["params"]["capabilities"] = json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},"textDocument":{"codeAction":{}}});
    init["params"]["workspaceFolders"] = json!([{"uri":uri_from_path(repo.path()).expect("native path has a file URI").as_str(),"name":"repo"}]);
    server.send(init);
    assert!(server.response_with_id(1)["result"].is_object());
    server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    let mut editor = FakeEditor::new(Profile::Standard);
    server.send(editor.open(&uri, "old"));
    let edit = next_apply(&mut server);
    for frame in editor.dispatch(&edit) {
        server.send(frame);
    }
    let text = editor.text(&uri).to_owned();
    let changed_rows: Vec<u32> = text
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            line.contains("one")
                || line.contains("ONE")
                || line.contains("three")
                || line.contains("THREE")
        })
        .map(|(row, _)| row as u32)
        .collect();
    assert_eq!(changed_rows.len(), 4, "{text}");
    let args = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":changed_rows[0],"character":0},"end":{"line":changed_rows[1]+1,"character":0}}),
        "status.stage",
        81,
    );
    assert!(!args.is_null(), "selection action missing");
    assert_eq!(
        args["target"]["selection"],
        json!({"startLine":changed_rows[0],"endLine":changed_rows[1]})
    );
    assert!(repo_state(&repo).1.contains("one\ntwo\nthree\n"));
    let mut forged = args.clone();
    forged["target"]["selection"]["endLine"] = json!(changed_rows[3]);
    let refused = execute_offered(&mut server, &mut editor, forged, 84);
    assert_refused(&refused);
    assert_eq!(repo.git(&["show", ":f.txt"]), "one\ntwo\nthree\n");
    let response = execute_offered(&mut server, &mut editor, args, 82);
    assert!(response["error"].is_null());
    assert_eq!(repo.git(&["show", ":f.txt"]), "ONE\ntwo\nthree\n");
    assert_eq!(
        fixtures::read(&repo.path().join("f.txt")),
        "ONE\ntwo\nTHREE\n"
    );
    let staged_text = editor.text(&uri).to_owned();
    let staged_rows: Vec<u32> = staged_text
        .lines()
        .enumerate()
        .filter(|(_, line)| line.ends_with("-one") || line.ends_with("+ONE"))
        .map(|(row, _)| row as u32)
        .collect();
    assert_eq!(staged_rows.len(), 2, "{staged_text}");
    let unstage = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":staged_rows[0],"character":0},"end":{"line":staged_rows[1]+1,"character":0}}),
        "status.unstage",
        85,
    );
    assert!(!unstage.is_null());
    let response = execute_offered(&mut server, &mut editor, unstage, 86);
    assert!(response["error"].is_null());
    assert_eq!(repo.git(&["show", ":f.txt"]), "one\ntwo\nthree\n");
    let refreshed = editor.text(&uri).to_owned();
    let context_row = refreshed
        .lines()
        .position(|line| line.trim_end().ends_with("two"))
        .unwrap() as u32;
    let context = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":context_row,"character":0},"end":{"line":context_row+1,"character":0}}),
        "status.stage",
        87,
    );
    assert!(!context.is_null());
    let response = execute_offered(&mut server, &mut editor, context, 88);
    assert!(response["error"].is_null());
    assert_eq!(repo.git(&["show", ":f.txt"]), "one\ntwo\nthree\n");
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn file_and_hunk_header_selections_keep_imm2_precedence() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\ntwo\nthree\n")])
        .build();
    fixtures::write(&repo.path().join("f.txt"), "ONE\ntwo\nTHREE\n");
    let (mut server, mut editor, uri) = open_plain(&repo);
    let text = editor.text(&uri).to_owned();
    let lines: Vec<_> = text.lines().collect();
    let file_header = lines
        .iter()
        .position(|line| line.contains("f.txt"))
        .unwrap() as u32;
    let changed: Vec<u32> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            line.ends_with("-one")
                || line.ends_with("+ONE")
                || line.ends_with("-three")
                || line.ends_with("+THREE")
        })
        .map(|(i, _)| i as u32)
        .collect();
    assert_eq!(changed.len(), 4, "{text}");
    let whole_file = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":file_header,"character":0},"end":{"line":file_header,"character":1}}),
        "status.stage",
        101,
    );
    assert!(!whole_file.is_null());
    assert_eq!(whole_file["target"]["selection"]["startLine"], file_header);
    assert!(execute_offered(&mut server, &mut editor, whole_file, 102)["error"].is_null());
    assert_eq!(repo.git(&["show", ":f.txt"]), "ONE\ntwo\nTHREE\n");

    let staged = editor.text(&uri).to_owned();
    let hunk = staged
        .lines()
        .position(|line| line.trim_start().starts_with("@@"))
        .unwrap() as u32;
    let selected_hunk = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":hunk,"character":0},"end":{"line":hunk,"character":1}}),
        "status.unstage",
        103,
    );
    assert!(!selected_hunk.is_null());
    assert!(execute_offered(&mut server, &mut editor, selected_hunk, 104)["error"].is_null());
    assert_eq!(repo.git(&["show", ":f.txt"]), "one\ntwo\nthree\n");
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn multi_file_selection_refuses_one_stale_member_without_mutation() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\n"), ("g.txt", "two\n")])
        .build();
    fixtures::write(&repo.path().join("f.txt"), "ONE\n");
    fixtures::write(&repo.path().join("g.txt"), "TWO\n");
    let (mut server, mut editor, uri) = open_plain(&repo);
    let text = editor.text(&uri).to_owned();
    let rows: Vec<u32> = text
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            line.ends_with("-one")
                || line.ends_with("+ONE")
                || line.ends_with("-two")
                || line.ends_with("+TWO")
        })
        .map(|(i, _)| i as u32)
        .collect();
    assert_eq!(rows.len(), 4, "{text}");
    let args = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":rows[0],"character":0},"end":{"line":rows[3]+1,"character":0}}),
        "status.stage",
        111,
    );
    assert!(!args.is_null());
    let head = repo.git(&["rev-parse", "HEAD"]);
    let index_before = repo.git(&["write-tree"]);
    fixtures::write(&repo.path().join("g.txt"), "changed-after-offer\n");
    // Keep the old synced/rendered document: do not dispatch watcher applyEdits before this stale action.
    let response = execute_offered(&mut server, &mut editor, args, 112);
    assert_refused(&response);
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(repo.git(&["write-tree"]), index_before);
    assert_eq!(fixtures::read(&repo.path().join("f.txt")), "ONE\n");
    assert_eq!(
        fixtures::read(&repo.path().join("g.txt")),
        "changed-after-offer\n"
    );
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn selection_endpoints_support_utf16_unicode_end_column_zero_and_crlf_content() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f-😀.txt", "first\r\nemoji 😀\r\nlast\r\n")])
        .build();
    fixtures::write(
        &repo.path().join("f-😀.txt"),
        "FIRST\r\nemoji 😀\r\nlast\r\n",
    );
    let (mut server, mut editor, uri) = open_plain(&repo);
    let text = editor.text(&uri).to_owned();
    let lines: Vec<_> = text.lines().collect();
    let minus = lines
        .iter()
        .position(|line| line.ends_with("-first"))
        .unwrap() as u32;
    let plus = lines
        .iter()
        .position(|line| line.ends_with("+FIRST"))
        .unwrap() as u32;
    // LSP's end position at column zero excludes the following rendered row.
    let args = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":minus,"character":0},"end":{"line":plus+1,"character":0}}),
        "status.stage",
        121,
    );
    assert!(!args.is_null());
    assert_eq!(args["target"]["selection"]["startLine"], minus);
    assert_eq!(args["target"]["selection"]["endLine"], plus);
    // Validate a UTF-16 column after the supplementary-plane character in the diff row.
    let emoji = lines
        .iter()
        .position(|line| line.ends_with(" emoji 😀"))
        .unwrap() as u32;
    let unicode = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":emoji,"character":0},"end":{"line":emoji,"character":9}}),
        "status.stage",
        122,
    );
    assert!(!unicode.is_null(), "UTF-16 range rejected for {text}");
    assert!(execute_offered(&mut server, &mut editor, args, 123)["error"].is_null());
    assert_eq!(
        repo.git(&["show", ":f-😀.txt"]),
        "FIRST\r\nemoji 😀\r\nlast\r\n"
    );
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn selection_survives_independent_fresh_hunk_shift() {
    let original = (1..=45).map(|n| format!("line-{n}\n")).collect::<String>();
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", original.as_str())])
        .build();
    let changed = original
        .replace("line-5\n", "changed-five\n")
        .replace("line-35\n", "changed-thirty-five\n");
    fixtures::write(&repo.path().join("f.txt"), &changed);
    let (mut server, mut editor, uri) = open_plain(&repo);
    let text = editor.text(&uri).to_owned();
    let rows: Vec<u32> = text
        .lines()
        .enumerate()
        .filter(|(_, line)| line.ends_with("-line-5") || line.ends_with("+changed-five"))
        .map(|(i, _)| i as u32)
        .collect();
    assert_eq!(rows.len(), 2, "{text}");
    let args = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":rows[0],"character":0},"end":{"line":rows[1]+1,"character":0}}),
        "status.stage",
        131,
    );
    assert!(!args.is_null());
    // Add an earlier independent change: Git shifts the later hunk's coordinates but
    // the selected hunk's content identity remains unique. Keep the retained document.
    let shifted = changed.replace("line-20\n", "new-independent-line\nline-20\n");
    fixtures::write(&repo.path().join("f.txt"), &shifted);
    let response = execute_offered(&mut server, &mut editor, args, 132);
    assert!(
        response["error"].is_null(),
        "independent shift refused: {response}"
    );
    assert!(repo.git(&["show", ":f.txt"]).contains("changed-five\n"));
    assert!(!repo
        .git(&["show", ":f.txt"])
        .contains("changed-thirty-five\n"));
    assert_eq!(fixtures::read(&repo.path().join("f.txt")), shifted);
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn cursor_actions_target_file_or_owning_hunk_once() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("a.txt", "before\nkeep\nafter\n")])
        .build();
    fixtures::write(&repo.path().join("a.txt"), "changed\nkeep\nafter\n");
    let uri = uri_from_path(&repo.path().join(".magit"))
        .expect("native path has a file URI")
        .as_str()
        .to_owned();
    let mut server = stdio::TestServer::spawn(&[]);
    let mut init = stdio::initialize_message();
    init["params"]["capabilities"] = json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},"textDocument":{"codeAction":{}}});
    init["params"]["workspaceFolders"] = json!([{"uri":uri_from_path(repo.path()).expect("native path has a file URI").as_str(),"name":"repo"}]);
    server.send(init);
    assert!(server.response_with_id(1)["result"].is_object());
    server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    let mut editor = FakeEditor::new(Profile::Standard);
    server.send(editor.open(&uri, "old"));
    let edit = next_apply(&mut server);
    for frame in editor.dispatch(&edit) {
        server.send(frame);
    }
    let text = editor.text(&uri);
    let rows: Vec<_> = text.lines().enumerate().collect();
    let file = rows
        .iter()
        .find(|(_, line)| line.contains("a.txt"))
        .unwrap()
        .0 as u32;
    let changed = rows
        .iter()
        .find(|(_, line)| line.contains("changed") || line.contains("before"))
        .unwrap()
        .0 as u32;
    let context = rows
        .iter()
        .find(|(_, line)| line.contains(" keep"))
        .unwrap()
        .0 as u32;
    let file_actions = action_targets(&mut server, &uri, file, 91);
    let changed_actions = action_targets(&mut server, &uri, changed, 92);
    let context_actions = action_targets(&mut server, &uri, context, 93);
    for actions in [&file_actions, &changed_actions, &context_actions] {
        assert!(
            actions
                .iter()
                .any(|action| action.command == "open.commit" && action.semantic_view.is_some()),
            "semantic menu action missing: {actions:?}"
        );
    }
    let file_target = file_actions
        .iter()
        .find(|action| action.command == "status.stage")
        .unwrap()
        .content_node
        .as_ref()
        .unwrap()
        .clone();
    for (actions, label) in [
        (&changed_actions, "changed line"),
        (&context_actions, "context line"),
    ] {
        let stage: Vec<_> = actions
            .iter()
            .filter(|action| action.command == "status.stage")
            .collect();
        assert_eq!(stage.len(), 1, "{label} must offer stage once");
        assert_ne!(
            stage[0].content_node.as_ref().unwrap(),
            &file_target,
            "{label} must target hunk, not whole file"
        );
    }
    assert_eq!(
        changed_actions
            .iter()
            .filter(|action| action.command == "status.stage")
            .count(),
        1
    );
    assert_eq!(
        context_actions
            .iter()
            .filter(|action| action.command == "status.stage")
            .count(),
        1
    );
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn binary_code_actions_are_snapshot_matched_and_reject_invalid_positions() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f-😀.txt", "one\ntwo\n")])
        .build();
    fixtures::write(&repo.path().join("f-😀.txt"), "changed\ntwo\n");
    let uri = uri_from_path(&repo.path().join(".magit"))
        .expect("native path has a file URI")
        .as_str()
        .to_owned();
    let mut server = stdio::TestServer::spawn(&[]);
    let mut init = stdio::initialize_message();
    init["params"]["capabilities"] = json!({
        "workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},
        "textDocument":{"codeAction":{"dynamicRegistration":true},"general":{"positionEncodings":["utf-16"]}}
    });
    init["params"]["workspaceFolders"] = json!([{"uri":uri_from_path(repo.path()).expect("native path has a file URI").as_str(),"name":"repo"}]);
    server.send(init);
    assert!(server.response_with_id(1)["result"].is_object());
    server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    let mut editor = FakeEditor::new(Profile::Standard);
    server.send(editor.open(&uri, "old"));
    let edit = next_apply(&mut server);
    for frame in editor.dispatch(&edit) {
        server.send(frame);
    }
    let file_row = editor
        .text(&uri)
        .lines()
        .position(|line| line.contains("f-😀.txt"))
        .unwrap() as u32;
    let request = editor.code_action(
        &uri,
        json!({"start":{"line":file_row,"character":0},"end":{"line":file_row,"character":0}}),
    );
    let request_id = request["id"].as_i64().expect("code action request ID");
    server.send(request);
    let response = server.response_with_id(request_id);
    let execute = editor.execute_command("magit.command", json!([]));
    let execute_id = execute["id"].as_i64().expect("execute request ID");
    assert_ne!(
        request_id, execute_id,
        "client request IDs must be distinct"
    );
    server.send(execute);
    assert!(server.response_with_id(execute_id)["error"].is_object());
    let actions = response["result"].as_array().expect("action array");
    assert!(
        !actions.is_empty(),
        "status row should offer registered content actions"
    );
    assert!(actions.iter().all(|action| action["disabled"].is_null()));
    assert!(actions.iter().any(
        |action| action["command"]["arguments"][0]["target"]["nodeId"].is_string()
            && action["command"]["arguments"][0]["viewGeneration"].is_number()
            && action["command"]["arguments"][0]["documentUri"] == uri
    ));
    let emoji_line = editor
        .text(&uri)
        .lines()
        .position(|line| line.contains("😀"))
        .unwrap() as u32;
    let before_emoji = editor
        .text(&uri)
        .lines()
        .nth(emoji_line as usize)
        .unwrap()
        .split("😀")
        .next()
        .unwrap()
        .encode_utf16()
        .count() as u32;
    server.send(json!({"jsonrpc":"2.0","id":44,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":emoji_line,"character":before_emoji+1},"end":{"line":emoji_line,"character":before_emoji+1}},"context":{"diagnostics":[]}}}));
    assert!(
        server.response_with_id(44)["result"]
            .as_array()
            .unwrap()
            .is_empty(),
        "UTF-16 surrogate interior is invalid"
    );
    server.send(json!({"jsonrpc":"2.0","id":42,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":9999,"character":0},"end":{"line":9999,"character":0}},"context":{"diagnostics":[]}}}));
    assert!(server.response_with_id(42)["result"]
        .as_array()
        .unwrap()
        .is_empty());
    let mut changed = editor.edit(&uri, "desynchronized");
    changed["params"]["contentChanges"] = json!([{"text":"desynchronized"}]);
    server.send(changed);
    server.send(json!({"jsonrpc":"2.0","id":43,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"context":{"diagnostics":[]}}}));
    assert!(server.response_with_id(43)["result"]
        .as_array()
        .unwrap()
        .is_empty());
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri}}}));
    server.send(json!({"jsonrpc":"2.0","id":45,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"context":{"diagnostics":[]}}}));
    assert!(server.response_with_id(45)["result"]
        .as_array()
        .unwrap()
        .is_empty());
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn large_retained_view_repeated_code_actions_report_latency_without_mutation() {
    use std::time::Instant;

    let files: Vec<_> = (0..48)
        .map(|index| {
            (
                format!("large/file-{index:03}.txt"),
                fixtures::lines(100).to_string(),
            )
        })
        .collect();
    let mut fixture = fixtures::RepoFixture::new().commit("large baseline", files.clone());
    for (name, content) in files {
        let mut lines: Vec<_> = content.lines().map(str::to_owned).collect();
        lines[10] = "changed near start".into();
        lines[85] = "changed near end".into();
        fixture = fixture.modify(name, format!("{}\n", lines.join("\n")));
    }
    fixture = fixture.commit(
        "trusted fixture config",
        [(".magit-lsp.toml", "[plain]\nmax_section_lines = 5000\n")],
    );
    let (repo, _) = fixture.build();
    // Repository configuration requires explicit trust; this isolated user
    // config opts the latency fixture into its genuinely expanded view.
    let config_dir = tempfile::tempdir().unwrap();
    let config_path = config_dir.path().join("user.toml");
    std::fs::write(&config_path, "trust_repo_config = true\n").unwrap();
    let config_value = config_path.to_str().unwrap();
    let (mut server, editor, uri) = open_plain_with_capabilities_and_env(
        &repo,
        json!({
            "workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},
            "textDocument":{"codeAction":{"dynamicRegistration":true},"general":{"positionEncodings":["utf-16"]}}
        }),
        &[(magit_lsp::config::CONFIG_ENV, config_value)],
    );
    let rendered = editor.text(&uri).to_owned();
    let rendered_bytes = rendered.len();
    let rendered_lines = rendered.lines().count();
    let policy_footer = rendered
        .lines()
        .find(|line| line.starts_with("Keys: "))
        .expect("expanded fixture renders the Plain key policy footer");
    assert_eq!(
        policy_footer, "Keys: off (built-in)",
        "fixture disables Plain keys so the toggle must offer the enable action"
    );
    let file_row = rendered
        .lines()
        .position(|line| line.contains("file-000.txt"))
        .unwrap_or_else(|| {
            panic!(
                "expanded large fixture omitted first file: {}",
                rendered.lines().take(12).collect::<Vec<_>>().join("\\n")
            )
        });
    let hunk_row = rendered
        .lines()
        .position(|line| line.contains("@@"))
        .expect("fixture renders real hunks") as u32;
    let head = repo.git(&["rev-parse", "HEAD"]);
    let index = repo.git(&["write-tree"]);
    let worktree = repo.git(&["diff", "--binary"]);
    let mut first_file_payload = None;
    for (request_id, row) in [
        (101, file_row as u32),
        (102, hunk_row),
        (103, file_row as u32),
        (104, file_row as u32),
    ] {
        let started = Instant::now();
        server.send(json!({"jsonrpc":"2.0","id":request_id,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":row,"character":0},"end":{"line":row,"character":0}},"context":{"diagnostics":[]}}}));
        let response = server.response_with_id(request_id);
        let elapsed = started.elapsed();
        let actions = response["result"].as_array().expect("code actions");
        assert!(!actions.is_empty());
        // Every row of a Plain document offers the key-policy toggle once; file
        // rows add the status-scope open.rebase view action, hunk rows the nine
        // content/hunk actions applicable to that target.
        let expected_offers = if row == file_row as u32 { 11 } else { 10 };
        assert_eq!(actions.len(), expected_offers, "status offer contract");
        let identities: Vec<_> = actions
            .iter()
            .map(|action| {
                action["data"]["command"]
                    .as_str()
                    .or_else(|| action["command"]["arguments"][0]["command"].as_str())
                    .unwrap()
                    .to_owned()
            })
            .collect();
        let mut sorted = identities.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(identities.len(), sorted.len(), "duplicate command offers");
        let toggles: Vec<_> = actions
            .iter()
            .filter(|action| action["command"]["arguments"][0]["command"] == "plain.keys.toggle")
            .collect();
        assert_eq!(toggles.len(), 1, "Plain key toggle offered exactly once");
        assert_eq!(
            toggles[0]["title"], "Keys: on",
            "toggle enables Plain keys while the footer reports them off"
        );
        assert_eq!(
            toggles[0]["command"]["arguments"][0]["target"]["kind"], "view",
            "toggle targets the retained view"
        );
        if row == file_row as u32 {
            assert!(identities.iter().any(|command| command == "open.rebase"));
            let rebase_view = actions
                .iter()
                .find(|action| action["command"]["arguments"][0]["command"] == "open.rebase")
                .map(|action| action["command"]["arguments"][0]["target"]["viewId"].clone())
                .expect("open.rebase view target");
            assert_eq!(
                toggles[0]["command"]["arguments"][0]["target"]["viewId"], rebase_view,
                "toggle targets the rendered status view"
            );
        }
        assert!(
            actions
                .iter()
                .any(|action| action["command"]["arguments"][0]["target"]["nodeId"].is_string()),
            "content target missing"
        );
        let payload: Vec<_> = actions
            .iter()
            .map(|action| {
                action
                    .get("command")
                    .cloned()
                    .unwrap_or_else(|| action["data"].clone())
            })
            .collect();
        if request_id == 101 {
            first_file_payload = Some(payload.clone());
        }
        if request_id == 103 {
            assert_eq!(
                &payload,
                first_file_payload.as_ref().unwrap(),
                "repeated file-row offers must retain identical command/target payloads"
            );
        }
        eprintln!("plain codeAction id={request_id} elapsed={elapsed:?} rendered_bytes={rendered_bytes} rendered_lines={rendered_lines} offers={}", actions.len());
    }
    let selection_started = Instant::now();
    server.send(json!({"jsonrpc":"2.0","id":105,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":file_row as u32,"character":0},"end":{"line":file_row as u32+1,"character":0}},"context":{"diagnostics":[]}}}));
    let selection = server.response_with_id(105);
    eprintln!("plain codeAction selection elapsed={:?} rendered_bytes={rendered_bytes} rendered_lines={rendered_lines}", selection_started.elapsed());
    assert!(
        selection["result"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action["command"]["arguments"][0]["target"]["selection"].is_object()),
        "selection target missing"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(repo.git(&["write-tree"]), index);
    assert_eq!(repo.git(&["diff", "--binary"]), worktree);
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn offered_stage_action_executes_and_mutates_its_target() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\ntwo\n")])
        .modify("f.txt", "changed\ntwo\n")
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo);
    let args = offered_stage(&mut server, &editor, &uri, 91);
    let before = repo_state(&repo);
    let response = execute_offered(&mut server, &mut editor, args, 92);
    assert!(response["error"].is_null(), "execute failed: {response}");
    let effects = response["result"]["effects"]
        .as_array()
        .unwrap_or_else(|| panic!("success effects: {response}"));
    assert!(
        !effects.iter().any(|effect| effect["text"] == STALE_VIEW),
        "action must not be refused: {response}"
    );
    let after = repo_state(&repo);
    assert_eq!(after.0, before.0, "HEAD must not move");
    assert_eq!(before.1, "one\ntwo\n");
    assert_eq!(
        after.1, "changed\ntwo\n",
        "index must hold the offered content"
    );
    assert_eq!(after.2, "changed\ntwo\n", "worktree must be untouched");
    assert_eq!(repo.git(&["status", "--porcelain"]), "M  f.txt\n");
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn section_stage_is_fresh_bulk_and_rejects_tampered_side() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "base\n"), ("g.txt", "base-g\n")])
        .modify("f.txt", "offered\n")
        .modify("g.txt", "staged-g\n")
        .build();
    repo.git(&["add", "g.txt"]);
    let (mut server, mut editor, uri) = open_plain(&repo);
    let args = offered_section(&mut server, &editor, &uri, "Unstaged", "status.stage", 601);
    assert!(!args.is_null());
    let mut forged = args.clone();
    let staged_section =
        offered_section(&mut server, &editor, &uri, "Staged", "status.unstage", 604);
    forged["target"]["nodeId"] = staged_section["target"]["nodeId"].clone();
    let before = repo_state(&repo);
    let index_before = repo.git(&["ls-files", "--stage"]);
    assert_refused(&execute_offered(&mut server, &mut editor, forged, 602));
    assert_eq!(repo_state(&repo), before);
    assert_eq!(repo.git(&["ls-files", "--stage"]), index_before);
    fixtures::write(&repo.path().join("f.txt"), "arrived\n");
    let result = execute_offered(&mut server, &mut editor, args, 603);
    assert!(result["error"].is_null(), "{result}");
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), before.0);
    assert_eq!(repo.git(&["show", ":f.txt"]), "arrived\n");
    assert_eq!(repo.git(&["show", ":g.txt"]), "staged-g\n");
    assert_eq!(fixtures::read(&repo.path().join("f.txt")), "arrived\n");
    assert_eq!(fixtures::read(&repo.path().join("g.txt")), "staged-g\n");
    server.shutdown_and_exit();
    assert!(server
        .finish_with_messages(std::time::Duration::from_secs(10))
        .0
        .success());
}

#[test]
fn section_untracked_stage_and_staged_unstage_are_fresh_bulk() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "base\n")])
        .build();
    fixtures::write(&repo.path().join("new.txt"), "first\n");
    let (mut server, mut editor, uri) = open_plain(&repo);
    let args = offered_section(&mut server, &editor, &uri, "Untracked", "status.stage", 611);
    assert!(!args.is_null());
    let head = repo.git(&["rev-parse", "HEAD"]);
    fixtures::write(&repo.path().join("later.txt"), "later\n");
    let result = execute_offered(&mut server, &mut editor, args, 612);
    assert!(result["error"].is_null(), "{result}");
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(repo.git(&["show", ":new.txt"]), "first\n");
    assert_eq!(repo.git(&["show", ":later.txt"]), "later\n");
    assert_eq!(fixtures::read(&repo.path().join("new.txt")), "first\n");
    assert_eq!(fixtures::read(&repo.path().join("later.txt")), "later\n");
    let staged = offered_section(&mut server, &editor, &uri, "Staged", "status.unstage", 613);
    assert!(!staged.is_null());
    let head = repo.git(&["rev-parse", "HEAD"]);
    fixtures::write(&repo.path().join("f.txt"), "changed\n");
    repo.git(&["add", "f.txt"]);
    let result = execute_offered(&mut server, &mut editor, staged, 614);
    assert!(result["error"].is_null(), "{result}");
    assert_eq!(repo.git(&["ls-files"]), "f.txt\n");
    assert_eq!(repo.git(&["show", ":f.txt"]), "base\n");
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixtures::read(&repo.path().join("f.txt")), "changed\n");
    assert_eq!(fixtures::read(&repo.path().join("new.txt")), "first\n");
    assert_eq!(fixtures::read(&repo.path().join("later.txt")), "later\n");
    server.shutdown_and_exit();
    assert!(server
        .finish_with_messages(std::time::Duration::from_secs(10))
        .0
        .success());
}

#[test]
fn execution_refuses_when_target_content_changed_after_offer() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\ntwo\n")])
        .modify("f.txt", "changed\ntwo\n")
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo);
    let args = offered_stage(&mut server, &editor, &uri, 93);
    fixtures::write(&repo.path().join("f.txt"), "other\ntwo\n");
    let before = repo_state(&repo);
    let response = execute_offered(&mut server, &mut editor, args, 94);
    assert_refused(&response);
    assert_eq!(
        repo_state(&repo),
        before,
        "refused execution must not touch Git"
    );
    assert_eq!(before.1, "one\ntwo\n");
    assert_eq!(before.2, "other\ntwo\n");
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn execution_refuses_a_wrong_view_generation() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\ntwo\n")])
        .modify("f.txt", "changed\ntwo\n")
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo);
    let mut args = offered_stage(&mut server, &editor, &uri, 95);
    let generation = args["viewGeneration"].as_u64().expect("offered generation");
    args["viewGeneration"] = json!(generation + 1);
    let before = repo_state(&repo);
    let response = execute_offered(&mut server, &mut editor, args, 96);
    assert_refused(&response);
    assert_eq!(
        repo_state(&repo),
        before,
        "refused execution must not touch Git"
    );
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn execution_refuses_when_the_document_is_closed() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\ntwo\n")])
        .modify("f.txt", "changed\ntwo\n")
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo);
    let args = offered_stage(&mut server, &editor, &uri, 97);
    let before = repo_state(&repo);
    server.send(editor.close(&uri));
    let response = execute_offered(&mut server, &mut editor, args, 98);
    assert_refused(&response);
    assert_eq!(
        repo_state(&repo),
        before,
        "refused execution must not touch Git"
    );
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn plain_menu_option_row_opens_a_document_prompt() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .modify("f.txt", "changed\n")
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo);
    let row = editor
        .text(&uri)
        .lines()
        .position(|line| line.contains("f.txt"))
        .expect("file row") as u32;
    let open = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":row,"character":0},"end":{"line":row,"character":0}}),
        "open.commit",
        10,
    );
    assert!(!open.is_null(), "open.commit not offered");
    let opened = execute_offered(&mut server, &mut editor, open, 11);
    assert!(opened["error"].is_null(), "open.commit failed: {opened}");
    let menu_text = editor.text(&uri).to_owned();
    let author = menu_text
        .lines()
        .position(|line| line.contains("Override the author"))
        .expect("author option row") as u32;
    let option = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":author,"character":0},"end":{"line":author,"character":0}}),
        "menu.run-option",
        12,
    );
    assert!(!option.is_null(), "menu.run-option not offered");
    assert_eq!(option["target"]["itemKey"], "=A");
    let executed = execute_offered(&mut server, &mut editor, option.clone(), 13);
    assert!(
        executed["error"].is_null(),
        "menu.run-option failed: {executed}"
    );
    let prompt_text = editor.text(&uri).to_owned();
    assert!(
        prompt_text.contains("Override the author (--author=)"),
        "prompt title missing: {prompt_text}"
    );
    assert!(
        prompt_text.contains("Submit"),
        "submit row missing: {prompt_text}"
    );
    assert!(
        prompt_text.contains("Cancel"),
        "cancel row missing: {prompt_text}"
    );
    assert!(
        !server
            .sent
            .iter()
            .any(|message| message["method"] == "view/prompt"),
        "Plain prompt must not request a native prompt"
    );

    let again = execute_offered(&mut server, &mut editor, option, 14);
    assert_refused(&again);
    assert_eq!(
        editor.text(&uri),
        prompt_text,
        "a repeated menu option must not render a second prompt"
    );
}

#[test]
fn repaired_plain_prompt_offers_actions_for_the_repaired_generation() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .modify("f.txt", "changed\n")
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo);
    let row = editor
        .text(&uri)
        .lines()
        .position(|line| line.contains("f.txt"))
        .expect("file row") as u32;
    let open = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":row,"character":0},"end":{"line":row,"character":0}}),
        "open.commit",
        20,
    );
    execute_offered(&mut server, &mut editor, open, 21);
    let menu_text = editor.text(&uri).to_owned();
    let author = menu_text
        .lines()
        .position(|line| line.contains("Override the author"))
        .expect("author option row") as u32;
    let option = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":author,"character":0},"end":{"line":author,"character":0}}),
        "menu.run-option",
        22,
    );
    execute_offered(&mut server, &mut editor, option, 23);

    let prompt_text = editor.text(&uri).to_owned();
    let cancel = exact_row(&prompt_text, "Cancel");
    let submit = exact_row(&prompt_text, "Submit");
    let before_repair = offered_prompt_answer(&mut server, &uri, cancel, 24);
    let old_generation = before_repair["viewGeneration"]
        .as_u64()
        .expect("prompt action generation");
    let mut lines: Vec<String> = prompt_text.lines().map(str::to_owned).collect();
    let field = (submit - 1) as usize;
    lines[field] = "Typed Author <typed@example.com>".into();
    let accepted = lines.join("\n");
    server.send(editor.edit(&uri, &accepted));
    server.send(json!({"jsonrpc":"2.0","id":25,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":cancel,"character":0},"end":{"line":cancel,"character":0}},"context":{"diagnostics":[]}}}));
    loop {
        let message = server.recv();
        assert_ne!(
            message["method"], "workspace/applyEdit",
            "a valid LF field edit must not be repaired: {message}"
        );
        if message["id"] == json!(25) {
            break;
        }
    }
    let damaged = accepted.replacen("Override the author", "Damaged title", 1);
    let damaged_change = editor.edit(&uri, &damaged);
    let damaged_version = damaged_change["params"]["textDocument"]["version"].clone();
    server.send(damaged_change);
    let repair = next_apply(&mut server);
    assert_eq!(
        repair["params"]["edit"]["documentChanges"][0]["textDocument"]["version"], damaged_version,
        "repair must target the damaged notification's document version: {repair}"
    );
    for frame in editor.dispatch(&repair) {
        server.send(frame);
    }
    assert_eq!(
        editor.text(&uri),
        accepted,
        "protected prompt text was not repaired"
    );

    let answer = offered_prompt_answer(&mut server, &uri, cancel, 26);
    assert_eq!(
        answer["viewGeneration"],
        json!(old_generation + 1),
        "offered generation must match the repaired session render"
    );
    let cancelled = execute_offered(&mut server, &mut editor, answer, 27);
    assert!(
        cancelled["error"].is_null(),
        "repaired prompt cancel failed: {cancelled}"
    );
    assert!(
        !editor
            .text(&uri)
            .lines()
            .any(|line| matches!(line.trim(), "Submit" | "Cancel")),
        "prompt stayed open after repaired action: {}",
        editor.text(&uri)
    );
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

/// The rendered row index of the first line whose trimmed text equals `label`.
fn exact_row(text: &str, label: &str) -> u32 {
    text.lines()
        .position(|line| line.trim() == label)
        .unwrap_or_else(|| panic!("missing {label:?} row: {text}")) as u32
}

/// The only offered `prompt.answer` argument on `row`.
fn offered_prompt_answer(server: &mut stdio::TestServer, uri: &str, row: u32, id: i64) -> Value {
    let args = offered_for_range(
        server,
        uri,
        json!({"start":{"line":row,"character":0},"end":{"line":row,"character":0}}),
        "prompt.answer",
        id,
    );
    assert!(!args.is_null(), "prompt.answer not offered on row {row}");
    args
}

#[test]
fn plain_text_prompt_submit_row_answers_with_the_typed_value() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .modify("f.txt", "changed\n")
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo);
    let row = editor
        .text(&uri)
        .lines()
        .position(|line| line.contains("f.txt"))
        .expect("file row") as u32;
    let open = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":row,"character":0},"end":{"line":row,"character":0}}),
        "open.commit",
        120,
    );
    execute_offered(&mut server, &mut editor, open, 121);
    let menu_text = editor.text(&uri).to_owned();
    let author = menu_text
        .lines()
        .position(|line| line.contains("Override the author"))
        .expect("author option row") as u32;
    let option = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":author,"character":0},"end":{"line":author,"character":0}}),
        "menu.run-option",
        122,
    );
    execute_offered(&mut server, &mut editor, option, 123);

    let prompt_text = editor.text(&uri).to_owned();
    let submit = exact_row(&prompt_text, "Submit");
    let mut lines: Vec<String> = prompt_text.lines().map(str::to_owned).collect();
    let field = (submit - 1) as usize;
    lines[field] = "Typed Author <typed@example.com>".into();
    let edited = lines.join("\n");
    server.send(editor.edit(&uri, &edited));

    let answer = offered_prompt_answer(&mut server, &uri, submit, 124);
    assert_eq!(answer["command"], "prompt.answer");
    let submitted = execute_offered(&mut server, &mut editor, answer, 125);
    assert!(
        submitted["error"].is_null(),
        "prompt submit failed: {submitted}"
    );

    let closed = editor.text(&uri).to_owned();
    assert!(!closed.contains("Submit"), "prompt stayed open: {closed}");
    assert!(
        closed
            .lines()
            .any(|line| line.contains("Override the author")
                && line.contains("Typed Author <typed@example.com>")),
        "menu does not reflect the author: {closed}"
    );
    assert!(
        !server
            .sent
            .iter()
            .any(|message| message["method"] == "view/prompt"),
        "Plain prompt must not request a native prompt"
    );
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn plain_ignore_topdir_uses_a_choice_document_and_submits_literal_id() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit(
            "initial",
            [("dir/a*b.txt", "one\n"), ("other.txt", "other\n")],
        )
        .modify("dir/a*b.txt", "changed\n")
        .build();
    std::fs::write(repo.path().join(".gitignore"), "# retained\n").unwrap();
    let head = repo.git(&["rev-parse", "HEAD"]);
    let (mut server, mut editor, uri) = open_plain(&repo);
    let row = editor
        .text(&uri)
        .lines()
        .position(|line| line.contains("a*b.txt"))
        .expect("file row") as u32;
    let (view_id, version) = offered_status_view(&mut server, &editor, &uri, row, 139);
    let invoked = invoke_plain(
        &mut server,
        &mut editor,
        140,
        json!({"viewId":view_id,"command":"ignore.topdir","version":version,"cursor":{"line":row,"column":0}}),
    );
    assert!(
        invoked["error"].is_null(),
        "ignore.topdir invoke failed: {invoked}"
    );

    let prompt_text = editor.text(&uri).to_owned();
    assert!(
        prompt_text.contains("Ignore in the repository root .gitignore"),
        "choice prompt title missing: {prompt_text}"
    );
    assert!(
        prompt_text.contains("Literal selected path"),
        "literal choice item missing: {prompt_text}"
    );
    assert!(
        prompt_text.contains("Enter custom pattern"),
        "pattern choice item missing: {prompt_text}"
    );
    assert!(
        prompt_text.lines().any(|line| line.trim() == "Cancel"),
        "choice document must render its cancel row: {prompt_text}"
    );
    assert!(
        !server
            .sent
            .iter()
            .any(|message| message["method"] == "view/pick"),
        "a Plain pick must be a document, not view/pick"
    );

    let literal = prompt_text
        .lines()
        .position(|line| line.contains("Literal selected path"))
        .expect("literal item row") as u32;
    let answer = offered_prompt_answer(&mut server, &uri, literal, 144);
    let submitted = execute_offered(&mut server, &mut editor, answer, 145);
    assert!(
        submitted["error"].is_null(),
        "literal pick submit failed: {submitted}"
    );

    let closed = editor.text(&uri).to_owned();
    assert!(
        !closed.contains("Literal selected path"),
        "choice document did not close: {closed}"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join(".gitignore")).unwrap(),
        "# retained\n/dir/a\\*b.txt\n",
        "literal root ignore rule must be appended verbatim"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head, "HEAD must not move");
    assert_eq!(fixtures::read(&repo.path().join("other.txt")), "other\n");
    assert_eq!(
        repo.git(&["status", "--short"]),
        " M dir/a*b.txt\n?? .gitignore\n",
        "only the modified file and the newly written ignore file may change"
    );
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn plain_choice_item_row_answers_the_ignore_kind_pick() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .modify("f.txt", "changed\n")
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo);
    let row = editor
        .text(&uri)
        .lines()
        .position(|line| line.contains("f.txt"))
        .expect("file row") as u32;
    let (view_id, version) = offered_status_view(&mut server, &editor, &uri, row, 149);
    let invoked = invoke_plain(
        &mut server,
        &mut editor,
        150,
        json!({"viewId":view_id,"command":"ignore.topdir","version":version,"cursor":{"line":row,"column":0}}),
    );
    assert!(
        invoked["error"].is_null(),
        "ignore.topdir invoke failed: {invoked}"
    );

    let prompt_text = editor.text(&uri).to_owned();
    let item = prompt_text
        .lines()
        .position(|line| line.contains("Literal selected path"))
        .expect("literal item row") as u32;
    server.send(json!({"jsonrpc":"2.0","id":154,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":item,"character":0},"end":{"line":item,"character":0}},"context":{"diagnostics":[]}}}));
    let actions = server.response_with_id(154)["result"].clone();
    let answers: Vec<&Value> = actions
        .as_array()
        .expect("code actions")
        .iter()
        .filter(|action| action["command"]["arguments"][0]["command"] == "prompt.answer")
        .collect();
    assert_eq!(
        answers.len(),
        1,
        "expected one prompt.answer on the literal item row: {prompt_text}"
    );
    let answer = answers[0]["command"]["arguments"][0].clone();
    let answered = execute_offered(&mut server, &mut editor, answer, 155);
    assert!(
        answered["error"].is_null(),
        "literal item-row answer failed: {answered}"
    );

    let closed = editor.text(&uri).to_owned();
    assert!(
        !closed.contains("Literal selected path"),
        "choice document did not close: {closed}"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join(".gitignore")).unwrap(),
        "/f.txt\n",
        "the literal item-row code action must write exactly the selected path"
    );
    assert!(
        !server
            .sent
            .iter()
            .any(|message| message["method"] == "view/pick"),
        "a Plain pick must be a document, not view/pick"
    );
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn plain_confirm_prompt_cancel_row_closes_without_touching_git() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .modify("f.txt", "changed\n")
        .build();
    let (mut server, mut editor, uri) = open_plain(&repo);
    let row = editor
        .text(&uri)
        .lines()
        .position(|line| line.contains("f.txt"))
        .expect("file row") as u32;
    let open = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":row,"character":0},"end":{"line":row,"character":0}}),
        "open.commit",
        130,
    );
    execute_offered(&mut server, &mut editor, open, 131);
    let menu_text = editor.text(&uri).to_owned();
    let create = menu_text
        .lines()
        .position(|line| line.trim_start().starts_with("c  ") && line.contains("Commit"))
        .expect("create action row") as u32;
    let action = offered_for_range(
        &mut server,
        &uri,
        json!({"start":{"line":create,"character":0},"end":{"line":create,"character":0}}),
        "menu.run-action",
        132,
    );
    execute_offered(&mut server, &mut editor, action, 133);

    let prompt_text = editor.text(&uri).to_owned();
    assert!(
        prompt_text.contains("Nothing staged. Stage all changes and commit?"),
        "confirm prompt missing: {prompt_text}"
    );
    let no = exact_row(&prompt_text, "No");
    let before = repo_state(&repo);
    let answer = offered_prompt_answer(&mut server, &uri, no, 134);
    let cancelled = execute_offered(&mut server, &mut editor, answer, 135);
    assert!(
        cancelled["error"].is_null(),
        "confirm cancel failed: {cancelled}"
    );

    let closed = editor.text(&uri).to_owned();
    assert!(
        !closed.contains("Nothing staged. Stage all changes and commit?"),
        "prompt stayed open: {closed}"
    );
    assert_eq!(repo_state(&repo), before, "cancel must not touch Git");
    assert!(
        !server
            .sent
            .iter()
            .any(|message| message["method"] == "view/prompt"),
        "Plain prompt must not request a native prompt"
    );
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}
