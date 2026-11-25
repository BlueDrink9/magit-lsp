use fixtures::stdio;
use magit_lsp_test_support as fixtures;
use magit_lsp_test_support::fake_editor;

use fake_editor::{FakeEditor, Profile};
use magit_lsp::file_uri::uri_from_path;
use serde_json::{json, Value};

fn apply(server: &mut stdio::TestServer) -> Value {
    loop {
        let message = server.recv();
        if message["method"] == "workspace/applyEdit" {
            return message;
        }
    }
}

fn actions(server: &mut stdio::TestServer, uri: &str, id: i64) -> Vec<Value> {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"context":{"diagnostics":[]}}}));
    server.response_with_id(id)["result"]
        .as_array()
        .unwrap()
        .clone()
}

fn execute(server: &mut stdio::TestServer, editor: &mut FakeEditor, args: Value, id: u64) {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[args]}}));
    loop {
        let message = server.recv();
        if message["id"] == id {
            break;
        }
        if message["method"] == "workspace/applyEdit" {
            for frame in editor.dispatch(&message) {
                server.send(frame);
            }
        }
    }
}

fn open(
    repo: &fixtures::RepoFixture,
    client_name: &str,
) -> (stdio::TestServer, FakeEditor, String) {
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
    init["params"]["clientInfo"] = json!({"name":client_name,"version":"1"});
    init["params"]["initializationOptions"] =
        json!({"plain":{"keys":{"default":true,"clients":{"Plain Test Client":false}}}});
    init["params"]["workspaceFolders"] = json!([{"uri":uri_from_path(repo.path()).expect("native path has a file URI").as_str(),"name":"repo"}]);
    server.send(init);
    server.response_with_id(1);
    server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    let mut editor = FakeEditor::new(Profile::Standard);
    server.send(editor.open(&uri, "old"));
    let edit = apply(&mut server);
    for frame in editor.dispatch(&edit) {
        server.send(frame);
    }
    (server, editor, uri)
}

#[test]
fn plain_key_header_toggle_and_user_edit_are_repaired() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .build();
    let (mut server, mut editor, uri) = open(&repo, "Plain Test Client");
    assert!(editor.text(&uri).contains("Keys: off (client)"));
    server.send(json!({"jsonrpc":"2.0","id":2,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"context":{"diagnostics":[]}}}));
    let actions = server.response_with_id(2)["result"]
        .as_array()
        .unwrap()
        .clone();
    let action = actions
        .iter()
        .find(|a| a["title"] == "Keys: on")
        .expect("key toggle action");
    let args = action["command"]["arguments"][0].clone();
    server.send(json!({"jsonrpc":"2.0","id":3,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[args]}}));
    loop {
        let message = server.recv();
        if message["id"] == 3 {
            break;
        }
        if message["method"] == "workspace/applyEdit" {
            for frame in editor.dispatch(&message) {
                server.send(frame);
            }
        }
    }
    assert!(editor.text(&uri).contains("Keys: on (document)"));
    let text = editor
        .text(&uri)
        .replace("Keys: on (document)", "spoofed header");
    server.send(editor.edit(&uri, &text));
    let repair = apply(&mut server);
    for frame in editor.dispatch(&repair) {
        server.send(frame);
    }
    assert!(editor.text(&uri).contains("Keys: on (document)"));
    server.send(editor.close(&uri));
    server.send(editor.open(&uri, "old"));
    let reopened = apply(&mut server);
    for frame in editor.dispatch(&reopened) {
        server.send(frame);
    }
    assert!(editor.text(&uri).contains("Keys: off (client)"));
}

#[test]
fn plain_key_header_reports_configured_user_default() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .build();
    let (_server, editor, uri) = open(&repo, "Other Client");
    assert!(editor.text(&uri).contains("Keys: on (default)"));
}

#[test]
fn prompt_and_commit_draft_never_offer_plain_key_toggle() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .modify("f.txt", "two\n")
        .build();
    let (mut server, mut editor, uri) = open(&repo, "Other Client");
    let file_line = editor
        .text(&uri)
        .lines()
        .position(|line| line.contains("f.txt"))
        .unwrap() as u32;
    server.send(json!({"jsonrpc":"2.0","id":10,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":file_line,"character":0},"end":{"line":file_line,"character":0}},"context":{"diagnostics":[]}}}));
    let open_commit = server.response_with_id(10)["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["command"]["arguments"][0]["command"] == "open.commit")
        .unwrap()["command"]["arguments"][0]
        .clone();
    execute(&mut server, &mut editor, open_commit, 11);
    let menu = editor.text(&uri).to_owned();
    let author_line = menu
        .lines()
        .position(|line| line.contains("Override the author"))
        .unwrap() as u32;
    server.send(json!({"jsonrpc":"2.0","id":12,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":author_line,"character":0},"end":{"line":author_line,"character":0}},"context":{"diagnostics":[]}}}));
    let author_action = server.response_with_id(12)["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["command"]["arguments"][0]["command"] == "menu.run-option")
        .unwrap()["command"]["arguments"][0]
        .clone();
    execute(&mut server, &mut editor, author_action, 13);
    assert!(
        editor.text(&uri).contains("Submit"),
        "document prompt opened"
    );
    assert!(
        actions(&mut server, &uri, 14)
            .iter()
            .all(|action| !action["title"].as_str().unwrap_or("").starts_with("Keys: ")),
        "prompt must not offer key toggle"
    );

    let (draft_repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\n")])
        .modify("f.txt", "two\n")
        .build();
    let (mut draft_server, mut draft_editor, status_uri) = open(&draft_repo, "Other Client");
    let file_line = draft_editor
        .text(&status_uri)
        .lines()
        .position(|line| line.contains("f.txt"))
        .unwrap() as u32;
    draft_server.send(json!({"jsonrpc":"2.0","id":20,"method":"textDocument/codeAction","params":{"textDocument":{"uri":status_uri},"range":{"start":{"line":file_line,"character":0},"end":{"line":file_line,"character":0}},"context":{"diagnostics":[]}}}));
    let stage = draft_server.response_with_id(20)["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["command"]["arguments"][0]["command"] == "status.stage")
        .unwrap()["command"]["arguments"][0]
        .clone();
    execute(&mut draft_server, &mut draft_editor, stage, 21);
    let file_line = draft_editor
        .text(&status_uri)
        .lines()
        .position(|line| line.contains("f.txt"))
        .unwrap() as u32;
    draft_server.send(json!({"jsonrpc":"2.0","id":22,"method":"textDocument/codeAction","params":{"textDocument":{"uri":status_uri},"range":{"start":{"line":file_line,"character":0},"end":{"line":file_line,"character":0}},"context":{"diagnostics":[]}}}));
    let open_commit = draft_server.response_with_id(22)["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["command"]["arguments"][0]["command"] == "open.commit")
        .unwrap()["command"]["arguments"][0]
        .clone();
    execute(&mut draft_server, &mut draft_editor, open_commit, 23);
    let menu = draft_editor.text(&status_uri).to_owned();
    let commit_line = menu
        .lines()
        .position(|line| line.contains("Commit"))
        .unwrap() as u32;
    draft_server.send(json!({"jsonrpc":"2.0","id":24,"method":"textDocument/codeAction","params":{"textDocument":{"uri":status_uri},"range":{"start":{"line":commit_line,"character":0},"end":{"line":commit_line,"character":0}},"context":{"diagnostics":[]}}}));
    let create = draft_server.response_with_id(24)["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["command"]["arguments"][0]["command"] == "menu.run-action")
        .unwrap()["command"]["arguments"][0]
        .clone();
    draft_server.send(json!({"jsonrpc":"2.0","id":25,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[create]}}));
    let draft_uri = uri_from_path(&draft_repo.path().join(".git/COMMIT_EDITMSG"))
        .expect("native path has a file URI")
        .as_str()
        .to_owned();
    let mut attached = false;
    loop {
        let message = draft_server.recv();
        if message["method"] == "window/showDocument" && !attached {
            draft_server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":draft_uri,"languageId":"plaintext","version":1,"text":"draft\n"}}}));
            attached = true;
        }
        if message["id"] == 25 {
            break;
        }
    }
    assert!(
        actions(&mut draft_server, &draft_uri, 26)
            .iter()
            .all(|action| !action["title"].as_str().unwrap_or("").starts_with("Keys: ")),
        "commit draft must not offer key toggle"
    );
}
