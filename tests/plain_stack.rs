#[path = "common/stdio.rs"]
mod stdio;
#[path = "common/mod.rs"]
mod fixtures;
#[path = "common/fake_editor.rs"]
mod fake_editor;

use fake_editor::{FakeEditor, Profile};
use serde_json::{json, Value};

fn execute(server: &mut stdio::TestServer, editor: &mut FakeEditor, args: Value, id: i64) -> Value {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[args]}}));
    loop {
        let message = server.recv();
        if message["method"] == "workspace/applyEdit" {
            for frame in editor.dispatch(&message) { server.send(frame); }
        } else if message["id"] == json!(id) {
            return message;
        }
    }
}

fn code_actions(server: &mut stdio::TestServer, uri: &str, row: usize, id: i64) -> Vec<Value> {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":row,"character":0},"end":{"line":row,"character":0}},"context":{"diagnostics":[]}}}));
    server.response_with_id(id)["result"].as_array().unwrap().clone()
}

fn offered(actions: &[Value], command: &str) -> Value {
    actions.iter().find(|action| action["command"]["arguments"][0]["command"] == command)
        .unwrap_or_else(|| panic!("missing {command} action in {actions:#?}"))["command"]["arguments"][0].clone()
}

fn state(repo: &fixtures::RepoFixture) -> (String, String, String) {
    (repo.git(&["rev-parse", "HEAD"]), repo.git(&["show", ":f.txt"]), fixtures::read(&repo.path().join("f.txt")))
}

#[test]
fn commit_menu_and_back_stay_in_plain_document_and_refuse_stale_targets() {
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f.txt", "one\ntwo\n")]).modify("f.txt", "ONE\ntwo\n").build();
    let uri = format!("file://{}/.magit", repo.path().display());
    let mut server = stdio::TestServer::spawn(&[]);
    let mut init = stdio::initialize_message();
    init["params"]["capabilities"] = json!({"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},"textDocument":{"codeAction":{"dynamicRegistration":true},"general":{"positionEncodings":["utf-16"]}}});
    init["params"]["workspaceFolders"] = json!([{"uri":format!("file://{}",repo.path().display()),"name":"repo"}]);
    server.send(init);
    assert!(server.response_with_id(1)["error"].is_null());
    server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    let mut editor = FakeEditor::new(Profile::Standard);
    server.send(editor.open(&uri, "old"));
    let initial = server.recv();
    assert_eq!(initial["method"], "workspace/applyEdit");
    for frame in editor.dispatch(&initial) { server.send(frame); }
    let original = editor.text(&uri).to_owned();
    assert!(!original.ends_with('\n'), "Plain serializer must retain its no-final-newline convention");
    let rows: Vec<usize> = original.lines().enumerate().filter(|(_, line)| line.contains("f.txt") || line.contains("ONE")).map(|(row, _)| row).collect();
    assert!(rows.len() >= 2, "expected distinct status rows: {original}");
    let git_before = state(&repo);

    for (index, row) in rows.into_iter().take(2).enumerate() {
        let status_actions = code_actions(&mut server, &uri, row, 20 + index as i64);
        let open = offered(&status_actions, "open.commit");
        let stale = open.clone();
        let opened = execute(&mut server, &mut editor, open, 30 + index as i64);
        assert!(opened["error"].is_null(), "open action failed: {opened}");
        assert!(!opened["result"]["effects"].as_array().unwrap().is_empty(), "missing stack effect: {opened}");
        assert_eq!(state(&repo), git_before);
        assert!(editor.documents.contains_key(&uri));
        let menu_text = editor.text(&uri).to_owned();
        assert_ne!(menu_text, original, "open outcome: {opened:#}");
        assert!(!menu_text.ends_with('\n'));

        let forged = { let mut bad = stale; bad["target"]["viewId"] = json!("forged-view"); bad };
        let refused = execute(&mut server, &mut editor, forged, 40 + index as i64);
        assert!(refused["result"]["effects"].as_array().unwrap().iter().any(|effect| effect["level"] == "warn"));
        assert_eq!(state(&repo), git_before);

        let menu_actions = code_actions(&mut server, &uri, 0, 50 + index as i64);
        let back = offered(&menu_actions, "common.cancel");
        let returned = execute(&mut server, &mut editor, back, 60 + index as i64);
        assert!(returned["error"].is_null(), "Back failed: {returned}");
        assert_eq!(editor.text(&uri), original);
        assert!(!editor.text(&uri).ends_with('\n'));
        assert_eq!(state(&repo), git_before);
    }

    let methods: Vec<_> = server.notifications.iter().filter_map(|message| message["method"].as_str()).collect();
    assert!(!methods.iter().any(|method| method.starts_with("magit/") || method.starts_with("view/")), "rich notification leaked: {methods:?}");
    let after = code_actions(&mut server, &uri, 0, 70);
    assert!(after.iter().any(|action| action["command"]["arguments"][0]["command"] == "open.commit"));
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}
