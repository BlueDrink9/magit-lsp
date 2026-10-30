#[path = "common/stdio.rs"]
mod stdio;
#[path = "common/mod.rs"]
mod fixtures;
#[path = "common/fake_editor.rs"]
mod fake_editor;

use fake_editor::{FakeEditor, Profile};
use magit_lsp::errors::STALE_VIEW;
use serde_json::{json, Value};

fn next_apply(server: &mut stdio::TestServer) -> Value {
    loop {
        let message = server.recv();
        if message["method"] == "workspace/applyEdit" { return message; }
    }
}

/// Open the repository's `.magit` status document as a Plain client.
fn open_plain(repo: &fixtures::RepoFixture) -> (stdio::TestServer, FakeEditor, String) {
    open_plain_with_capabilities(repo, json!({
        "workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},
        "textDocument":{"codeAction":{"dynamicRegistration":true},"general":{"positionEncodings":["utf-16"]}}
    }))
}

fn open_plain_with_capabilities(repo: &fixtures::RepoFixture, capabilities: Value) -> (stdio::TestServer, FakeEditor, String) {
    let uri = format!("file://{}/.magit", repo.path().display());
    let mut server = stdio::TestServer::spawn(&[]);
    let mut init = stdio::initialize_message();
    init["params"]["capabilities"] = capabilities;
    init["params"]["workspaceFolders"] = json!([{"uri":format!("file://{}",repo.path().display()),"name":"repo"}]);
    server.send(init);
    assert!(server.response_with_id(1)["result"].is_object());
    server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    let mut editor = FakeEditor::new(Profile::Standard);
    server.send(editor.open(&uri, "old"));
    let edit = next_apply(&mut server);
    for frame in editor.dispatch(&edit) { server.send(frame); }
    (server, editor, uri)
}

#[test]
fn discard_is_disabled_and_forged_execution_refuses_without_prompt_or_git_changes() {
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f.txt", "one\n")]).modify("f.txt", "changed\n").build();
    let caps = json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},"textDocument":{"codeAction":{"disabledSupport":true}}});
    let (mut server, mut editor, uri) = open_plain_with_capabilities(&repo, caps);
    let row = editor.text(&uri).lines().position(|line| line.contains("f.txt")).unwrap() as u32;
    server.send(json!({"jsonrpc":"2.0","id":91,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":row,"character":0},"end":{"line":row,"character":0}},"context":{"diagnostics":[]}}}));
    let actions = server.response_with_id(91)["result"].as_array().unwrap().clone();
    let discard = actions.iter().find(|action| action["data"]["command"] == "status.discard").expect("disabled discard offer");
    assert!(discard["disabled"]["reason"].as_str().unwrap().contains("confirmation"));
    assert!(discard["command"].is_null());
    let forged = discard["data"].clone();
    let before = repo_state(&repo);
    let response = execute_offered(&mut server, &mut editor, forged, 92);
    assert!(response["result"]["effects"][0]["text"].as_str().unwrap().contains("confirmation"));
    assert_eq!(repo_state(&repo), before);
    server.shutdown_and_exit(); let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10)); assert!(status.success());
}

#[test]
fn unavailable_actions_are_omitted_without_disabled_support() {
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f.txt", "one\n")]).modify("f.txt", "changed\n").build();
    let caps = json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},"textDocument":{"codeAction":{}}});
    let (mut server, editor, uri) = open_plain_with_capabilities(&repo, caps);
    let row = editor.text(&uri).lines().position(|line| line.contains("f.txt")).unwrap() as u32;
    server.send(json!({"jsonrpc":"2.0","id":93,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":row,"character":0},"end":{"line":row,"character":0}},"context":{"diagnostics":[]}}}));
    let response = server.response_with_id(93);
    let actions = response["result"].as_array().unwrap();
    assert!(actions.iter().any(|action| action["command"]["arguments"][0]["command"] == "status.stage"));
    assert!(!actions.iter().any(|action| action["data"]["command"] == "status.discard" || action["command"]["arguments"][0]["command"] == "status.discard"));
    server.shutdown_and_exit(); let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10)); assert!(status.success());
}

#[test]
fn missing_execute_command_capability_disables_actions_with_reason() {
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f.txt", "one\n")]).modify("f.txt", "changed\n").build();
    let caps = json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true}},"textDocument":{"codeAction":{"disabledSupport":true}}});
    let (mut server, editor, uri) = open_plain_with_capabilities(&repo, caps);
    let row = editor.text(&uri).lines().position(|line| line.contains("f.txt")).unwrap() as u32;
    server.send(json!({"jsonrpc":"2.0","id":94,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":row,"character":0},"end":{"line":row,"character":0}},"context":{"diagnostics":[]}}}));
    let response = server.response_with_id(94);
    let actions = response["result"].as_array().unwrap();
    let stage = actions.iter().find(|action| action["data"]["command"] == "status.stage").expect("disabled stage offer");
    assert!(stage["disabled"]["reason"].as_str().unwrap().contains("workspace.executeCommand"));
    assert!(stage["command"].is_null());
    server.shutdown_and_exit(); let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10)); assert!(status.success());
}

#[test]
fn scoped_planned_and_unavailable_commands_are_disabled_and_forged_requests_are_refused() {
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f.txt", "one\n")]).modify("f.txt", "changed\n").build();
    let caps = json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},"textDocument":{"codeAction":{"disabledSupport":true}}});
    let (mut server, mut editor, uri) = open_plain_with_capabilities(&repo, caps);
    let row = editor.text(&uri).lines().position(|line| line.contains("f.txt")).unwrap() as u32;
    server.send(json!({"jsonrpc":"2.0","id":95,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":row,"character":0},"end":{"line":row,"character":0}},"context":{"diagnostics":[]}}}));
    let actions = server.response_with_id(95)["result"].as_array().unwrap().clone();
    let planned = actions.iter().find(|action| action["data"]["command"] == "status.reverse").expect("scoped planned offer");
    assert!(planned["disabled"]["reason"].as_str().unwrap().contains("R-3.6"));
    assert!(planned["command"].is_null() && planned["data"]["target"].is_null());
    let ids: Vec<_> = actions.iter().filter_map(|a| a["data"]["command"].as_str().or_else(|| a["command"]["arguments"][0]["command"].as_str())).collect();
    let unique: std::collections::BTreeSet<_> = ids.iter().copied().collect();
    assert_eq!(ids.len(), unique.len(), "duplicate command offer");
    let before = repo_state(&repo);
    let refused = execute_offered(&mut server, &mut editor, planned["data"].clone(), 96);
    assert!(refused["result"]["effects"][0]["text"].as_str().unwrap().contains("planned"));
    assert_eq!(repo_state(&repo), before);
    let stage = offered_stage(&mut server, &editor, &uri, 97);
    assert!(execute_offered(&mut server, &mut editor, stage, 98)["error"].is_null());
    server.shutdown_and_exit(); let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10)); assert!(status.success());
}

#[test]
fn planned_and_unavailable_entries_are_omitted_without_disabled_support() {
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f.txt", "one\n")]).modify("f.txt", "changed\n").build();
    let caps = json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},"textDocument":{"codeAction":{}}});
    let (mut server, editor, uri) = open_plain_with_capabilities(&repo, caps);
    let row = editor.text(&uri).lines().position(|line| line.contains("f.txt")).unwrap() as u32;
    server.send(json!({"jsonrpc":"2.0","id":99,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":row,"character":0},"end":{"line":row,"character":0}},"context":{"diagnostics":[]}}}));
    let response = server.response_with_id(99);
    let actions = response["result"].as_array().unwrap();
    assert!(!actions.iter().any(|a| a["data"]["command"] == "status.reverse" || a["data"]["command"] == "status.apply"));
    server.shutdown_and_exit(); let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10)); assert!(status.success());
}

#[test]
fn malformed_plain_envelope_is_refused_without_claiming_rich_commands() {
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f.txt", "one\n")]).modify("f.txt", "changed\n").build();
    let caps = json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},"textDocument":{"codeAction":{"disabledSupport":true}}});
    let (mut server, _editor, uri) = open_plain_with_capabilities(&repo, caps);
    let before = repo_state(&repo);
    server.send(json!({"jsonrpc":"2.0","id":101,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[{"command":"status.reverse","documentUri":uri}]}}));
    let response = server.response_with_id(101);
    assert!(response["result"]["effects"][0]["text"].as_str().unwrap().contains("planned"));
    assert_eq!(repo_state(&repo), before);
    server.shutdown_and_exit(); let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10)); assert!(status.success());
}

/// Request code actions on the `f.txt` row and return the offered stage argument.
fn offered_stage(server: &mut stdio::TestServer, editor: &FakeEditor, uri: &str, id: i64) -> Value {
    let row = editor.text(uri).lines().position(|line| line.contains("f.txt")).expect("file row") as u32;
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":row,"character":0},"end":{"line":row,"character":0}},"context":{"diagnostics":[]}}}));
    let response = server.response_with_id(id);
    response["result"].as_array().expect("actions").iter()
        .find(|action| action["command"]["arguments"][0]["command"] == "status.stage")
        .map(|action| action["command"]["arguments"][0].clone())
        .unwrap_or_else(|| panic!("no stage action offered: {response}"))
}

fn offered_section(server: &mut stdio::TestServer, editor: &FakeEditor, uri: &str, section: &str, command: &str, id: i64) -> Value {
    let row = editor.text(uri).lines().position(|line| line.starts_with(section)).expect("section row") as u32;
    offered_for_range(server, uri, json!({"start":{"line":row,"character":0},"end":{"line":row,"character":0}}), command, id)
}

/// Execute an offered Plain action, acknowledging any render edits it causes.
fn execute_offered(server: &mut stdio::TestServer, editor: &mut FakeEditor, args: Value, id: i64) -> Value {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[args]}}));
    loop {
        let message = server.recv();
        if message["method"] == "workspace/applyEdit" {
            for frame in editor.dispatch(&message) { server.send(frame); }
            continue;
        }
        if message["id"] == json!(id) { return message; }
    }
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
    assert!(response["error"].is_null(), "refusal must be a normal result: {response}");
    let effects = response["result"]["effects"].as_array().unwrap_or_else(|| panic!("refusal effects: {response}"));
    assert!(effects.iter().any(|effect| effect["type"] == "message"
        && effect["level"] == "warn"
        && effect["text"] == STALE_VIEW), "expected readable refusal: {response}");
}

fn action_ids(server: &mut stdio::TestServer, uri: &str, range: Value, id: i64) -> Vec<String> {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":range,"context":{"diagnostics":[]}}}));
    server.response_with_id(id)["result"].as_array().unwrap().iter()
        .filter_map(|a| a["command"]["arguments"][0]["target"]["node_id"].as_str().map(str::to_owned)).collect()
}

fn action_targets(server: &mut stdio::TestServer, uri: &str, line: u32, id: i64) -> Vec<(String, String)> {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":line,"character":0},"end":{"line":line,"character":0}},"context":{"diagnostics":[]}}}));
    server.response_with_id(id)["result"].as_array().unwrap().iter().map(|action| (
        action["command"]["arguments"][0]["command"].as_str().unwrap().to_owned(),
        action["command"]["arguments"][0]["target"]["node_id"].as_str().unwrap().to_owned(),
    )).collect()
}

fn offered_for_range(server: &mut stdio::TestServer, uri: &str, range: Value, command: &str, id: i64) -> Value {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":range,"context":{"diagnostics":[]}}}));
    server.response_with_id(id)["result"].as_array().unwrap().iter()
        .find(|action| action["command"]["arguments"][0]["command"] == command)
        .map(|action| action["command"]["arguments"][0].clone()).unwrap_or(Value::Null)
}

#[test]
fn titles_use_repository_style_configured_chord_and_structured_path() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .modify("f.txt", "changed\n")
        .build();
    let config_dir = tempfile::tempdir().unwrap();
    let config_path = config_dir.path().join("user.toml");
    let uri = format!("file://{}/.magit", repo.path().display());
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
        if repo_override { std::fs::write(&repo_config, "[actions]\ntitle_style = \"path\"\n").unwrap(); }
        else { let _ = std::fs::remove_file(&repo_config); }
        let mut server = stdio::TestServer::spawn(&[(magit_lsp::config::CONFIG_ENV, config_path.to_str().unwrap())]);
        let mut init = stdio::initialize_message();
        init["params"]["capabilities"] = json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},"textDocument":{"codeAction":{"dynamicRegistration":true},"general":{"positionEncodings":["utf-16"]}}});
        init["params"]["workspaceFolders"] = json!([{"uri":format!("file://{}",repo.path().display()),"name":"repo"}]);
        server.send(init); assert!(server.response_with_id(1)["result"].is_object());
        server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
        let mut editor = FakeEditor::new(Profile::Standard);
        server.send(editor.open(&uri, "old"));
        let edit = next_apply(&mut server); for frame in editor.dispatch(&edit) { server.send(frame); }
        let row = editor.text(&uri).lines().position(|line| line.contains("f.txt")).unwrap() as u32;
        server.send(json!({"jsonrpc":"2.0","id":71,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":row,"character":0},"end":{"line":row,"character":0}},"context":{"diagnostics":[]}}}));
        let response = server.response_with_id(71);
        let action = response["result"].as_array().unwrap().iter().find(|a| a["command"]["arguments"][0]["command"] == "status.stage").unwrap();
        assert_eq!(action["title"], expected);
        assert_eq!(action["command"]["title"], expected);
        assert_eq!(action["command"]["command"], "magit.command");
        let args = action["command"]["arguments"][0].clone();
        if let Some(previous) = &identity {
            assert_eq!(args["command"], previous["command"]);
            assert_eq!(args["target"], previous["target"]);
        } else { identity = Some(args); }
        server.shutdown_and_exit(); let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10)); assert!(status.success());
    }
}

#[test]
fn selection_range_offers_exact_target_and_stages_only_selected_change() {
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f.txt", "one\ntwo\nthree\n")]).build();
    fixtures::write(&repo.path().join("f.txt"), "ONE\ntwo\nTHREE\n");
    let uri = format!("file://{}/.magit", repo.path().display());
    let mut server = stdio::TestServer::spawn(&[]);
    let mut init = stdio::initialize_message();
    init["params"]["capabilities"] = json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},"textDocument":{"codeAction":{}}});
    init["params"]["workspaceFolders"] = json!([{"uri":format!("file://{}",repo.path().display()),"name":"repo"}]);
    server.send(init); assert!(server.response_with_id(1)["result"].is_object());
    server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    let mut editor=FakeEditor::new(Profile::Standard); server.send(editor.open(&uri,"old"));
    let edit=next_apply(&mut server); for frame in editor.dispatch(&edit) { server.send(frame); }
    let text=editor.text(&uri).to_owned();
    let changed_rows: Vec<u32> = text.lines().enumerate().filter(|(_,line)| line.contains("one") || line.contains("ONE") || line.contains("three") || line.contains("THREE"))
        .map(|(row,_)| row as u32).collect();
    assert_eq!(changed_rows.len(),4,"{text}");
    let args=offered_for_range(&mut server,&uri,json!({"start":{"line":changed_rows[0],"character":0},"end":{"line":changed_rows[1]+1,"character":0}}),"status.stage",81);
    assert!(!args.is_null(), "selection action missing");
    assert_eq!(args["target"]["selection"],json!({"startLine":changed_rows[0],"endLine":changed_rows[1]}));
    assert!(repo_state(&repo).1.contains("one\ntwo\nthree\n"));
    let mut forged=args.clone(); forged["target"]["selection"]["endLine"]=json!(changed_rows[3]);
    let refused=execute_offered(&mut server,&mut editor,forged,84); assert_refused(&refused);
    assert_eq!(repo.git(&["show",":f.txt"]),"one\ntwo\nthree\n");
    let response=execute_offered(&mut server,&mut editor,args,82); assert!(response["error"].is_null());
    assert_eq!(repo.git(&["show",":f.txt"]),"ONE\ntwo\nthree\n");
    assert_eq!(fixtures::read(&repo.path().join("f.txt")),"ONE\ntwo\nTHREE\n");
    let staged_text=editor.text(&uri).to_owned();
    let staged_rows: Vec<u32> = staged_text.lines().enumerate().filter(|(_,line)| line.ends_with("-one") || line.ends_with("+ONE"))
        .map(|(row,_)| row as u32).collect();
    assert_eq!(staged_rows.len(),2,"{staged_text}");
    let unstage=offered_for_range(&mut server,&uri,json!({"start":{"line":staged_rows[0],"character":0},"end":{"line":staged_rows[1]+1,"character":0}}),"status.unstage",85);
    assert!(!unstage.is_null());
    let response=execute_offered(&mut server,&mut editor,unstage,86); assert!(response["error"].is_null());
    assert_eq!(repo.git(&["show",":f.txt"]),"one\ntwo\nthree\n");
    let refreshed=editor.text(&uri).to_owned();
    let context_row=refreshed.lines().position(|line| line.trim_end().ends_with("two")).unwrap() as u32;
    let context=offered_for_range(&mut server,&uri,json!({"start":{"line":context_row,"character":0},"end":{"line":context_row+1,"character":0}}),"status.stage",87);
    assert!(!context.is_null());
    let response=execute_offered(&mut server,&mut editor,context,88); assert!(response["error"].is_null());
    assert_eq!(repo.git(&["show",":f.txt"]),"one\ntwo\nthree\n");
    server.shutdown_and_exit(); let (status,_)=server.finish_with_messages(std::time::Duration::from_secs(10)); assert!(status.success());
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
    let file_header = lines.iter().position(|line| line.contains("f.txt")).unwrap() as u32;
    let changed: Vec<u32> = lines.iter().enumerate().filter(|(_, line)| line.ends_with("-one") || line.ends_with("+ONE") || line.ends_with("-three") || line.ends_with("+THREE")).map(|(i, _)| i as u32).collect();
    assert_eq!(changed.len(), 4, "{text}");
    let whole_file = offered_for_range(&mut server, &uri, json!({"start":{"line":file_header,"character":0},"end":{"line":file_header,"character":1}}), "status.stage", 101);
    assert!(!whole_file.is_null());
    assert_eq!(whole_file["target"]["selection"]["startLine"], file_header);
    assert!(execute_offered(&mut server, &mut editor, whole_file, 102)["error"].is_null());
    assert_eq!(repo.git(&["show", ":f.txt"]), "ONE\ntwo\nTHREE\n");

    let staged = editor.text(&uri).to_owned();
    let hunk = staged.lines().position(|line| line.trim_start().starts_with("@@")).unwrap() as u32;
    let selected_hunk = offered_for_range(&mut server, &uri, json!({"start":{"line":hunk,"character":0},"end":{"line":hunk,"character":1}}), "status.unstage", 103);
    assert!(!selected_hunk.is_null());
    assert!(execute_offered(&mut server, &mut editor, selected_hunk, 104)["error"].is_null());
    assert_eq!(repo.git(&["show", ":f.txt"]), "one\ntwo\nthree\n");
    server.shutdown_and_exit(); let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10)); assert!(status.success());
}

#[test]
fn multi_file_selection_refuses_one_stale_member_without_mutation() {
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f.txt", "one\n"), ("g.txt", "two\n")]).build();
    fixtures::write(&repo.path().join("f.txt"), "ONE\n");
    fixtures::write(&repo.path().join("g.txt"), "TWO\n");
    let (mut server, mut editor, uri) = open_plain(&repo);
    let text = editor.text(&uri).to_owned();
    let rows: Vec<u32> = text.lines().enumerate().filter(|(_, line)| line.ends_with("-one") || line.ends_with("+ONE") || line.ends_with("-two") || line.ends_with("+TWO")).map(|(i, _)| i as u32).collect();
    assert_eq!(rows.len(), 4, "{text}");
    let args = offered_for_range(&mut server, &uri, json!({"start":{"line":rows[0],"character":0},"end":{"line":rows[3]+1,"character":0}}), "status.stage", 111);
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
    assert_eq!(fixtures::read(&repo.path().join("g.txt")), "changed-after-offer\n");
    server.shutdown_and_exit(); let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10)); assert!(status.success());
}

#[test]
fn selection_endpoints_support_utf16_unicode_end_column_zero_and_crlf_content() {
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f-😀.txt", "first\r\nemoji 😀\r\nlast\r\n")]).build();
    fixtures::write(&repo.path().join("f-😀.txt"), "FIRST\r\nemoji 😀\r\nlast\r\n");
    let (mut server, mut editor, uri) = open_plain(&repo);
    let text = editor.text(&uri).to_owned();
    let lines: Vec<_> = text.lines().collect();
    let minus = lines.iter().position(|line| line.ends_with("-first")).unwrap() as u32;
    let plus = lines.iter().position(|line| line.ends_with("+FIRST")).unwrap() as u32;
    // LSP's end position at column zero excludes the following rendered row.
    let args = offered_for_range(&mut server, &uri, json!({"start":{"line":minus,"character":0},"end":{"line":plus+1,"character":0}}), "status.stage", 121);
    assert!(!args.is_null());
    assert_eq!(args["target"]["selection"]["startLine"], minus);
    assert_eq!(args["target"]["selection"]["endLine"], plus);
    // Validate a UTF-16 column after the supplementary-plane character in the diff row.
    let emoji = lines.iter().position(|line| line.ends_with(" emoji 😀")).unwrap() as u32;
    let unicode = offered_for_range(&mut server, &uri, json!({"start":{"line":emoji,"character":0},"end":{"line":emoji,"character":9}}), "status.stage", 122);
    assert!(!unicode.is_null(), "UTF-16 range rejected for {text}");
    assert!(execute_offered(&mut server, &mut editor, args, 123)["error"].is_null());
    assert_eq!(repo.git(&["show", ":f-😀.txt"]), "FIRST\r\nemoji 😀\r\nlast\r\n");
    server.shutdown_and_exit(); let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10)); assert!(status.success());
}

#[test]
fn selection_survives_independent_fresh_hunk_shift() {
    let original = (1..=45).map(|n| format!("line-{n}\n")).collect::<String>();
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f.txt", original.as_str())]).build();
    let changed = original.replace("line-5\n", "changed-five\n").replace("line-35\n", "changed-thirty-five\n");
    fixtures::write(&repo.path().join("f.txt"), &changed);
    let (mut server, mut editor, uri) = open_plain(&repo);
    let text = editor.text(&uri).to_owned();
    let rows: Vec<u32> = text.lines().enumerate().filter(|(_, line)| line.ends_with("-line-5") || line.ends_with("+changed-five")).map(|(i, _)| i as u32).collect();
    assert_eq!(rows.len(), 2, "{text}");
    let args = offered_for_range(&mut server, &uri, json!({"start":{"line":rows[0],"character":0},"end":{"line":rows[1]+1,"character":0}}), "status.stage", 131);
    assert!(!args.is_null());
    // Add an earlier independent change: Git shifts the later hunk's coordinates but
    // the selected hunk's content identity remains unique. Keep the retained document.
    let shifted = changed.replace("line-20\n", "new-independent-line\nline-20\n");
    fixtures::write(&repo.path().join("f.txt"), &shifted);
    let response = execute_offered(&mut server, &mut editor, args, 132);
    assert!(response["error"].is_null(), "independent shift refused: {response}");
    assert!(repo.git(&["show", ":f.txt"]).contains("changed-five\n"));
    assert!(!repo.git(&["show", ":f.txt"]).contains("changed-thirty-five\n"));
    assert_eq!(fixtures::read(&repo.path().join("f.txt")), shifted);
    server.shutdown_and_exit(); let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10)); assert!(status.success());
}

#[test]
fn cursor_actions_target_file_or_owning_hunk_once() {
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("a.txt", "before\nkeep\nafter\n")]).build();
    fixtures::write(&repo.path().join("a.txt"), "changed\nkeep\nafter\n");
    let uri = format!("file://{}/.magit", repo.path().display());
    let mut server = stdio::TestServer::spawn(&[]);
    let mut init = stdio::initialize_message();
    init["params"]["capabilities"] = json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},"textDocument":{"codeAction":{}}});
    init["params"]["workspaceFolders"] = json!([{"uri":format!("file://{}",repo.path().display()),"name":"repo"}]);
    server.send(init); assert!(server.response_with_id(1)["result"].is_object());
    server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    let mut editor=FakeEditor::new(Profile::Standard); server.send(editor.open(&uri,"old"));
    let edit=next_apply(&mut server); for frame in editor.dispatch(&edit) { server.send(frame); }
    let text=editor.text(&uri);
    let rows: Vec<_> = text.lines().enumerate().collect();
    let file = rows.iter().find(|(_,line)| line.contains("a.txt")).unwrap().0 as u32;
    let changed = rows.iter().find(|(_,line)| line.contains("changed") || line.contains("before")).unwrap().0 as u32;
    let context = rows.iter().find(|(_,line)| line.contains(" keep")).unwrap().0 as u32;
    let file_actions = action_targets(&mut server,&uri,file,91);
    let changed_actions = action_targets(&mut server,&uri,changed,92);
    let context_actions = action_targets(&mut server,&uri,context,93);
    let file_target = file_actions.iter().find(|(command,_)| command == "status.stage").unwrap().1.clone();
    for (actions,label) in [(&changed_actions,"changed line"),(&context_actions,"context line")] {
        let stage: Vec<_> = actions.iter().filter(|(command,_)| command == "status.stage").collect();
        assert_eq!(stage.len(),1,"{label} must offer stage once");
        assert_ne!(stage[0].1,file_target,"{label} must target hunk, not whole file");
    }
    assert_eq!(changed_actions.iter().filter(|(command,_)| command == "status.stage").count(),1);
    assert_eq!(context_actions.iter().filter(|(command,_)| command == "status.stage").count(),1);
    server.shutdown_and_exit(); let (status,_)=server.finish_with_messages(std::time::Duration::from_secs(10)); assert!(status.success());
}

#[test]
fn binary_code_actions_are_snapshot_matched_and_reject_invalid_positions() {
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f-😀.txt", "one\ntwo\n")]).build();
    fixtures::write(&repo.path().join("f-😀.txt"), "changed\ntwo\n");
    let uri = format!("file://{}/.magit", repo.path().display());
    let mut server = stdio::TestServer::spawn(&[]);
    let mut init = stdio::initialize_message();
    init["params"]["capabilities"] = json!({
        "workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},
        "textDocument":{"codeAction":{"dynamicRegistration":true},"general":{"positionEncodings":["utf-16"]}}
    });
    init["params"]["workspaceFolders"] = json!([{"uri":format!("file://{}", repo.path().display()),"name":"repo"}]);
    server.send(init);
    assert!(server.response_with_id(1)["result"].is_object());
    server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    let mut editor = FakeEditor::new(Profile::Standard);
    server.send(editor.open(&uri, "old"));
    let edit = next_apply(&mut server);
    for frame in editor.dispatch(&edit) { server.send(frame); }
    let file_row = editor.text(&uri).lines().position(|line| line.contains("f-😀.txt")).unwrap() as u32;
    let request = editor.code_action(&uri, json!({"start":{"line":file_row,"character":0},"end":{"line":file_row,"character":0}}));
    server.send(request);
    let response = server.response_with_id(71);
    let actions = response["result"].as_array().expect("action array");
    assert!(!actions.is_empty(), "status row should offer registered content actions");
    assert!(actions.iter().all(|action| action["disabled"].is_null()));
    assert!(actions.iter().any(|action| action["command"]["arguments"][0]["target"]["node_id"].is_string()
        && action["command"]["arguments"][0]["viewGeneration"].is_number()
        && action["command"]["arguments"][0]["documentUri"] == uri));
    let emoji_line = editor.text(&uri).lines().position(|line| line.contains("😀")).unwrap() as u32;
    let before_emoji = editor.text(&uri).lines().nth(emoji_line as usize).unwrap().split("😀").next().unwrap().encode_utf16().count() as u32;
    server.send(json!({"jsonrpc":"2.0","id":44,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":emoji_line,"character":before_emoji+1},"end":{"line":emoji_line,"character":before_emoji+1}},"context":{"diagnostics":[]}}}));
    assert!(server.response_with_id(44)["result"].as_array().unwrap().is_empty(), "UTF-16 surrogate interior is invalid");
    server.send(json!({"jsonrpc":"2.0","id":42,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":9999,"character":0},"end":{"line":9999,"character":0}},"context":{"diagnostics":[]}}}));
    assert!(server.response_with_id(42)["result"].as_array().unwrap().is_empty());
    let mut changed = editor.edit(&uri, "desynchronized");
    changed["params"]["contentChanges"] = json!([{"text":"desynchronized"}]);
    server.send(changed);
    server.send(json!({"jsonrpc":"2.0","id":43,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"context":{"diagnostics":[]}}}));
    assert!(server.response_with_id(43)["result"].as_array().unwrap().is_empty());
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri}}}));
    server.send(json!({"jsonrpc":"2.0","id":45,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"context":{"diagnostics":[]}}}));
    assert!(server.response_with_id(45)["result"].as_array().unwrap().is_empty());
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
    let effects = response["result"]["effects"].as_array().unwrap_or_else(|| panic!("success effects: {response}"));
    assert!(!effects.iter().any(|effect| effect["text"] == STALE_VIEW), "action must not be refused: {response}");
    let after = repo_state(&repo);
    assert_eq!(after.0, before.0, "HEAD must not move");
    assert_eq!(before.1, "one\ntwo\n");
    assert_eq!(after.1, "changed\ntwo\n", "index must hold the offered content");
    assert_eq!(after.2, "changed\ntwo\n", "worktree must be untouched");
    assert_eq!(repo.git(&["status", "--porcelain"]), "M  f.txt\n");
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}

#[test]
fn section_stage_is_fresh_bulk_and_rejects_tampered_side() {
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f.txt", "base\n"), ("g.txt", "base-g\n")]).modify("f.txt", "offered\n").modify("g.txt", "staged-g\n").build();
    repo.git(&["add", "g.txt"]);
    let (mut server, mut editor, uri) = open_plain(&repo);
    let args = offered_section(&mut server, &editor, &uri, "Unstaged", "status.stage", 601);
    assert!(!args.is_null());
    let mut forged = args.clone();
    let staged_section = offered_section(&mut server, &editor, &uri, "Staged", "status.unstage", 604);
    forged["target"]["node_id"] = staged_section["target"]["node_id"].clone();
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
    assert!(server.finish_with_messages(std::time::Duration::from_secs(10)).0.success());
}

#[test]
fn section_untracked_stage_and_staged_unstage_are_fresh_bulk() {
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f.txt", "base\n")]).build();
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
    assert!(server.finish_with_messages(std::time::Duration::from_secs(10)).0.success());
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
    assert_eq!(repo_state(&repo), before, "refused execution must not touch Git");
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
    assert_eq!(repo_state(&repo), before, "refused execution must not touch Git");
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
    assert_eq!(repo_state(&repo), before, "refused execution must not touch Git");
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}
