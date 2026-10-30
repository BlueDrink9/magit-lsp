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

fn offered_switch(actions: &[Value]) -> Value {
    actions.iter().find(|action| action["command"]["arguments"][0]["command"] == "menu.toggle-switch")
        .unwrap_or_else(|| panic!("missing switch action in {actions:#?}"))["command"]["arguments"][0].clone()
}

fn state(repo: &fixtures::RepoFixture) -> (String, String, String) {
    (repo.git(&["rev-parse", "HEAD"]), repo.git(&["show", ":f.txt"]), fixtures::read(&repo.path().join("f.txt")))
}

#[test]
fn commit_menu_and_back_stay_in_plain_document_and_refuse_stale_targets() {
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f.txt", "one\ntwo\n")]).commit("second", [("f.txt", "one\ntwo\nthree\n")]).modify("f.txt", "ONE\ntwo\nthree\n").build();
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

#[test]
fn plain_menu_switches_use_retained_targets_and_restore_caller() {
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f.txt", "one\ntwo\n")]).commit("second", [("f.txt", "one\ntwo\nthree\n")]).modify("f.txt", "ONE\ntwo\nthree\n").build();
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
    for frame in editor.dispatch(&initial) { server.send(frame); }
    let caller = editor.text(&uri).to_owned();
    let before = state(&repo);
    let status_row = caller.lines().position(|line| line.contains("f.txt") || line.contains("ONE")).unwrap();
    let open = offered(&code_actions(&mut server, &uri, status_row, 20), "open.commit");
    execute(&mut server, &mut editor, open, 21);
    let menu_text = editor.text(&uri).to_owned();
    assert!(!menu_text.ends_with('\n'));
    let last_row = menu_text.lines().count() - 1;
    let on = offered_switch(&code_actions(&mut server, &uri, 0, 22));
    assert_eq!(on["target"]["itemKey"], "-a");
    let stale = on.clone();
    let toggled = execute(&mut server, &mut editor, on.clone(), 23);
    assert!(toggled["error"].is_null(), "toggle on failed: {toggled}");
    let on_text = editor.text(&uri).to_owned();
    assert!(on_text.lines().any(|line| line.contains("git commit --all")), "missing exact -a preview ({toggled}): {on_text}");
    assert!(!on_text.ends_with('\n'));
    assert!(editor.documents.contains_key(&uri));
    let stale_result = execute(&mut server, &mut editor, stale, 24);
    assert!(stale_result["result"]["effects"].as_array().unwrap().iter().any(|effect| effect["level"] == "warn"));
    assert!(editor.text(&uri).contains("git commit --all"));
    let off = offered_switch(&code_actions(&mut server, &uri, last_row.min(on_text.lines().count() - 1), 25));
    assert_eq!(off["target"]["viewId"], json!(on["target"]["viewId"]));
    assert_eq!(off["target"]["menuId"], json!(on["target"]["menuId"]));
    let mut forged = off.clone(); forged["target"]["menuId"] = json!("wrong-menu");
    let refused = execute(&mut server, &mut editor, forged, 26);
    assert!(refused["result"]["effects"].as_array().unwrap().iter().any(|effect| effect["level"] == "warn"));
    assert!(editor.text(&uri).contains("git commit --all"));
    let mut forged_view = off.clone(); forged_view["target"]["viewId"] = json!("other-view");
    let refused = execute(&mut server, &mut editor, forged_view, 261);
    assert!(refused["result"]["effects"].as_array().unwrap().iter().any(|effect| effect["level"] == "warn"));
    assert_eq!(state(&repo), before);
    let mut unsupported = off.clone(); unsupported["target"]["itemKey"] = json!("=A");
    let refused = execute(&mut server, &mut editor, unsupported, 27);
    assert!(refused["result"]["effects"].as_array().unwrap().iter().any(|effect| effect["level"] == "warn"));
    assert_eq!(state(&repo), before);
    let toggled = execute(&mut server, &mut editor, off, 28);
    assert!(toggled["error"].is_null(), "toggle off failed: {toggled}");
    let off_text = editor.text(&uri).to_owned();
    assert!(off_text.lines().any(|line| line.contains("git commit") && !line.contains("--all")), "unexpected off preview: {off_text}");
    assert!(!off_text.ends_with('\n'));
    assert_eq!(state(&repo), before);
    let back = offered(&code_actions(&mut server, &uri, 0, 29), "common.cancel");
    execute(&mut server, &mut editor, back, 30);
    assert_eq!(editor.text(&uri), caller);
    assert_eq!(state(&repo), before);
    let status_row = caller.lines().position(|line| line.contains("f.txt") || line.contains("ONE")).unwrap();
    let log_menu = offered(&code_actions(&mut server, &uri, status_row, 31), "open.log");
    let opened_log = execute(&mut server, &mut editor, log_menu, 32);
    assert!(opened_log["error"].is_null(), "open log failed: {opened_log}");
    let menu_text = editor.text(&uri).to_owned();
    assert!(!menu_text.ends_with('\n'));
    let log_actions = code_actions(&mut server, &uri, 0, 33);
    let log_switch = log_actions.iter()
        .find(|action| action["command"]["arguments"][0]["command"] == "menu.toggle-switch"
            && action["command"]["arguments"][0]["target"]["itemKey"] == "-g").unwrap_or_else(|| panic!("log menu actions: {log_actions:#?}"))["command"]["arguments"][0].clone();
    let toggled = execute(&mut server, &mut editor, log_switch, 34);
    assert!(toggled["error"].is_null(), "log switch failed: {toggled}");
    let off_menu = editor.text(&uri).to_owned();
    assert!(off_menu.lines().any(|line| line.contains("Show graph") && line.contains("[ ]")), "graph switch is not off: {off_menu}");
    assert!(off_menu.lines().any(|line| line.trim_start().starts_with("git log") && !line.contains("--graph")), "graph-off preview missing: {off_menu}");
    let log_switch = code_actions(&mut server, &uri, 0, 341).into_iter()
        .find(|action| action["command"]["arguments"][0]["command"] == "menu.toggle-switch"
            && action["command"]["arguments"][0]["target"]["itemKey"] == "-g")
        .unwrap()["command"]["arguments"][0].clone();
    execute(&mut server, &mut editor, log_switch, 342);
    let on_menu = editor.text(&uri).to_owned();
    assert!(on_menu.lines().any(|line| line.contains("Show graph") && line.contains("[x]")), "graph switch is not on: {on_menu}");
    assert!(on_menu.lines().any(|line| line.trim_start().starts_with("git log") && line.contains("--graph")), "graph-on preview missing: {on_menu}");
    let concrete = offered(&code_actions(&mut server, &uri, 0, 35), "menu.run-action");
    assert_eq!(concrete["target"]["itemKey"], "l");
    let mut action_as_switch = concrete.clone();
    action_as_switch["command"] = json!("menu.toggle-switch");
    let refused = execute(&mut server, &mut editor, action_as_switch, 353);
    assert!(refused["result"]["effects"].as_array().unwrap().iter().any(|effect| effect["level"] == "warn"));
    assert_eq!(editor.text(&uri), on_menu);
    let mut switch_as_action = concrete.clone();
    switch_as_action["command"] = json!("menu.run-action");
    switch_as_action["target"]["itemKey"] = json!("-g");
    let refused = execute(&mut server, &mut editor, switch_as_action, 354);
    assert!(refused["result"]["effects"].as_array().unwrap().iter().any(|effect| effect["level"] == "warn"));
    assert_eq!(editor.text(&uri), on_menu);
    let stale = concrete.clone();
    let mut wrong_item = concrete.clone(); wrong_item["target"]["itemKey"] = json!("o");
    let refused = execute(&mut server, &mut editor, wrong_item, 351);
    assert!(refused["result"]["effects"].as_array().unwrap().iter().any(|effect| effect["level"] == "warn"));
    let mut forged_menu = concrete.clone(); forged_menu["target"]["menuId"] = json!("commit");
    let refused = execute(&mut server, &mut editor, forged_menu, 352);
    assert!(refused["result"]["effects"].as_array().unwrap().iter().any(|effect| effect["level"] == "warn"));
    let completed = execute(&mut server, &mut editor, concrete.clone(), 36);
    assert!(completed["error"].is_null(), "log.current failed: {completed}");
    assert!(completed["result"]["effects"].as_array().unwrap().iter().any(|effect| effect["type"] == "pushView"), "log.current did not push a concrete view: {completed}; args={concrete}");
    let log_text = editor.text(&uri).to_owned();
    assert_ne!(log_text, menu_text);
    assert!(!log_text.ends_with('\n'));
    assert!(log_text.contains("initial"), "concrete log missing commit: {log_text}");
    assert!(log_text.lines().any(|line| line.contains("*")), "captured graph switch argument did not affect log output: {log_text}");
    let refused = execute(&mut server, &mut editor, stale, 361);
    assert!(refused["result"]["effects"].as_array().unwrap().iter().any(|effect| effect["level"] == "warn"));
    let back = offered(&code_actions(&mut server, &uri, 0, 37), "common.cancel");
    execute(&mut server, &mut editor, back, 38);
    assert_eq!(editor.text(&uri), caller);
    assert_eq!(state(&repo), before);
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}
