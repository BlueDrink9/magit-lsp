#[path = "common/stdio.rs"]
mod stdio;
#[path = "common/mod.rs"]
mod fixtures;
#[path = "common/fake_editor.rs"]
mod fake_editor;

use fake_editor::{FakeEditor, Profile};
use serde_json::{json, Value};

fn next_apply(server: &mut stdio::TestServer) -> Value {
    loop {
        let message = server.recv();
        if message["method"] == "workspace/applyEdit" { return message; }
    }
}

fn action_ids(server: &mut stdio::TestServer, uri: &str, range: Value, id: i64) -> Vec<String> {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":range,"context":{"diagnostics":[]}}}));
    server.response_with_id(id)["result"].as_array().unwrap().iter()
        .filter_map(|a| a["command"]["arguments"][0]["target"]["node_id"].as_str().map(str::to_owned)).collect()
}

#[test]
fn half_open_range_uses_previous_row_and_distinct_node_targets() {
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("a.txt", "one\n"), ("b.txt", "two\n")]).build();
    fixtures::write(&repo.path().join("a.txt"), "changed-a\n");
    fixtures::write(&repo.path().join("b.txt"), "changed-b\n");
    let uri = format!("file://{}/.magit", repo.path().display());
    let mut server = stdio::TestServer::spawn(&[]);
    let mut init = stdio::initialize_message();
    init["params"]["capabilities"] = json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true}}});
    init["params"]["workspaceFolders"] = json!([{"uri":format!("file://{}",repo.path().display()),"name":"repo"}]);
    server.send(init); assert!(server.response_with_id(1)["result"].is_object());
    server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    let mut editor=FakeEditor::new(Profile::Standard); server.send(editor.open(&uri,"old"));
    let edit=next_apply(&mut server); for frame in editor.dispatch(&edit) { server.send(frame); }
    let text=editor.text(&uri).to_owned();
    let first_row=text.lines().position(|line| line.contains("a.txt")).unwrap() as u32;
    let second_row=text.lines().position(|line| line.contains("b.txt")).unwrap() as u32;
    let first=action_ids(&mut server,&uri,json!({"start":{"line":first_row,"character":0},"end":{"line":second_row,"character":0}}),81);
    let first_and_second=action_ids(&mut server,&uri,json!({"start":{"line":first_row,"character":0},"end":{"line":second_row,"character":1}}),82);
    let second=action_ids(&mut server,&uri,json!({"start":{"line":second_row,"character":0},"end":{"line":second_row+1,"character":0}}),83);
    assert!(!first.is_empty() && !second.is_empty());
    assert!(!first.iter().any(|id| second.contains(id)), "end at column zero excludes the next row target");
    assert!(first_and_second.iter().any(|id| second.contains(id)), "non-zero end character includes the next row target");
    assert!(first_and_second.iter().any(|id| first.contains(id)));
    assert_ne!(first[0],second[0],"adjacent ranges must bind distinct node targets");
    server.shutdown_and_exit(); let (status,_)=server.finish_with_messages(std::time::Duration::from_secs(10)); assert!(status.success());
}

#[test]
fn binary_code_actions_are_snapshot_matched_and_disabled_until_execution_exists() {
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
    let last = editor.text(&uri).lines().count().saturating_sub(1) as u32;
    let end_character = editor.text(&uri).lines().last().unwrap_or("").encode_utf16().count() as u32;
    let request = editor.code_action(&uri, json!({"start":{"line":0,"character":0},"end":{"line":last,"character":end_character}}));
    server.send(request);
    let response = server.response_with_id(71);
    let actions = response["result"].as_array().expect("action array");
    assert!(!actions.is_empty(), "status row should offer registered content actions");
    assert!(actions.iter().all(|action| action["disabled"]["reason"].is_string()));
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
