#[path = "common/stdio.rs"] mod stdio;
#[path = "common/mod.rs"] mod fixtures;
#[path = "common/fake_editor.rs"] mod fake_editor;

use fake_editor::{FakeEditor, Profile};
use serde_json::{json, Value};

fn setup_with(profile: Profile, initial: &str) -> (fixtures::RepoFixture, stdio::TestServer, FakeEditor, String, String) {
    setup_with_encoding(profile, initial, "utf-16")
}

fn setup_with_encoding(profile: Profile, initial: &str, encoding: &str) -> (fixtures::RepoFixture, stdio::TestServer, FakeEditor, String, String) {
    let (repo, _) = fixtures::RepoFixture::new().commit("initial", [("f.txt", "one\ntwo\n"), ("😀.txt", "one\ntwo\n")]).modify("f.txt", "ONE\ntwo\nthree\n").modify("😀.txt", "ONE\ntwo\nthree\n").build();
    let uri = format!("file://{}/.magit", repo.path().display());
    if profile == Profile::CrlfNormalise { fixtures::write(&repo.path().join(".magit"), "disk\r\n"); }
    let mut server = stdio::TestServer::spawn(&[]);
    let mut init = stdio::initialize_message();
    init["params"]["capabilities"] = json!({"general":{"positionEncodings":[encoding]},"workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":false}},"textDocument":{"codeAction":{"codeActionLiteralSupport":{"codeActionKind":{"valueSet":["quickfix","refactor"]}}}}});
    init["params"]["workspaceFolders"] = json!([{"uri":format!("file://{}",repo.path().display()),"name":"repo"}]);
    server.send(init); assert!(server.response_with_id(1)["error"].is_null());
    server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    let mut editor = FakeEditor::new(profile);
    server.send(editor.open(&uri, initial));
    let edit = server.recv(); assert_eq!(edit["method"], "workspace/applyEdit");
    for frame in editor.dispatch(&edit) { server.send(frame); }
    let text = editor.text(&uri).to_owned();
    (repo, server, editor, uri, text)
}

#[test]
fn plain_provider_ranges_honor_negotiated_encoding_for_unicode_and_reject_invalid_columns() {
    for (encoding, expected) in [("utf-8", 8), ("utf-16", 6), ("utf-32", 5)] {
        let (_repo, mut server, mut editor, uri, text) = setup_with_encoding(Profile::Standard, "old\n", encoding);
        let links = request(&mut server, "textDocument/documentLink", 101, json!({"textDocument":{"uri":uri}}));
        let link = links.as_array().unwrap().iter().find(|link| link["target"].as_str().is_some_and(|s| s.ends_with("/%F0%9F%98%80.txt"))).unwrap();
        assert_eq!(link["range"]["end"]["character"].as_u64().unwrap() - link["range"]["start"]["character"].as_u64().unwrap(), expected);
        let symbols = request(&mut server, "textDocument/documentSymbol", 102, json!({"textDocument":{"uri":uri}}));
        let symbol = symbols.as_array().unwrap().iter().flat_map(|s| s["children"].as_array().into_iter().flatten()).find(|s| s["name"] == "😀.txt").unwrap();
        assert_eq!(symbol["selectionRange"]["end"]["character"].as_u64().unwrap() - symbol["selectionRange"]["start"]["character"].as_u64().unwrap(), expected);
        let tokens = request(&mut server, "textDocument/semanticTokens/full", 103, json!({"textDocument":{"uri":uri}}));
        assert!(tokens["data"].as_array().is_some_and(|v| !v.is_empty()));
        let file_line = text.lines().position(|line| line.contains("😀.txt")).unwrap() as u64;
        let data = tokens["data"].as_array().unwrap();
        assert_eq!(data.len() % 5, 0, "semantic tokens use five-u32 LSP groups");
        let mut token_line = 0u64;
        let mut token_start = 0u64;
        let mut filename_token_length = None;
        for group in data.chunks_exact(5) {
            let delta_line = group[0].as_u64().unwrap();
            token_line += delta_line;
            token_start = if delta_line == 0 { token_start + group[1].as_u64().unwrap() } else { group[1].as_u64().unwrap() };
            if token_line == file_line && group[3].as_u64() == Some(2) {
                assert!(token_start <= text.lines().nth(file_line as usize).unwrap().chars().count() as u64);
                filename_token_length = group[2].as_u64();
                break;
            }
        }
        assert_eq!(filename_token_length, Some(expected), "filename semantic-token length under {encoding}");
        let def = request(&mut server, "textDocument/definition", 104, json!({"textDocument":{"uri":uri},"position":{"line":file_line,"character":0}}));
        assert!(def["uri"].as_str().unwrap().ends_with("/%F0%9F%98%80.txt"));
        assert!(request(&mut server, "textDocument/definition", 105, json!({"textDocument":{"uri":uri},"position":{"line":file_line,"character":999}})).is_null());
        let final_row = text.lines().count() as u64;
        assert!(request(&mut server, "textDocument/definition", 107, json!({"textDocument":{"uri":uri},"position":{"line":final_row,"character":0}})).is_null(), "terminal phantom row resolved under {encoding}");
        if encoding != "utf-32" {
            let row = text.lines().nth(file_line as usize).unwrap();
            let prefix = row.find('😀').unwrap();
            let column = match encoding { "utf-8" => prefix as u64 + 1, "utf-16" => row[..prefix].encode_utf16().count() as u64 + 1, _ => unreachable!() };
            assert!(request(&mut server, "textDocument/definition", 106, json!({"textDocument":{"uri":uri},"position":{"line":file_line,"character":column}})).is_null(), "interior scalar boundary accepted under {encoding}");
        }
        server.send(editor.close(&uri));
        server.shutdown_and_exit();
        let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
        assert!(status.success());
    }
}

fn request(server: &mut stdio::TestServer, method: &str, id: i64, params: Value) -> Value {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
    server.response_with_id(id)["result"].clone()
}

fn state(repo: &fixtures::RepoFixture) -> (String, String, String) {
    (repo.git(&["rev-parse", "HEAD"]), repo.git(&["show", ":f.txt"]), fixtures::read(&repo.path().join("f.txt")))
}

fn assert_all_null(server: &mut stdio::TestServer, uri: &str, id: &mut i64) {
    for method in ["textDocument/documentSymbol", "textDocument/definition", "textDocument/documentLink", "textDocument/foldingRange", "textDocument/semanticTokens/full"] {
        let result = request(server, method, *id, json!({"textDocument":{"uri":uri},"position":{"line":0,"character":0}}));
        assert!(result.is_null(), "{method} for {uri} should return null, got {result}");
        *id += 1;
    }
}

#[test]
fn providers_read_exact_plain_tree_and_active_menu_snapshot() {
    let (repo, mut server, mut editor, uri, text) = setup_with(Profile::Standard, "old\n");
    let git = state(&repo);
    let symbols = request(&mut server, "textDocument/documentSymbol", 10, json!({"textDocument":{"uri":uri}}));
    assert!(symbols.as_array().is_some_and(|v| !v.is_empty()), "symbols: {symbols}");
    let links = request(&mut server, "textDocument/documentLink", 11, json!({"textDocument":{"uri":uri}}));
    assert!(links.as_array().is_some_and(|v| v.iter().any(|l| l["target"].as_str().is_some_and(|s| s.ends_with("/f.txt")))), "links: {links}");
    let folds = request(&mut server, "textDocument/foldingRange", 12, json!({"textDocument":{"uri":uri}}));
    let rows = text.lines().count() as u64;
    let phantom = rows;
    assert!(folds.as_array().is_some_and(|v| !v.is_empty() && v.iter().all(|r| r["startLine"].as_u64().unwrap() <= r["endLine"].as_u64().unwrap() && r["endLine"].as_u64().unwrap() < rows)), "folds {folds}, rows {rows}");
    let tokens = request(&mut server, "textDocument/semanticTokens/full", 13, json!({"textDocument":{"uri":uri}}));
    assert!(tokens["data"].as_array().is_some_and(|v| !v.is_empty()), "tokens {tokens}");
    let row = text.lines().position(|line| line.contains("f.txt")).unwrap() as u64;
    let definition = request(&mut server, "textDocument/definition", 14, json!({"textDocument":{"uri":uri},"position":{"line":row,"character":0}}));
    assert!(definition["uri"].as_str().is_some_and(|s| s.ends_with("/f.txt")), "definition {definition}");
    assert_eq!(state(&repo), git);
    let utf16_position = json!({"textDocument":{"uri":uri},"position":{"line":row,"character":999}});
    assert!(request(&mut server,"textDocument/definition",22,utf16_position).is_null(), "invalid UTF-16 character coordinate must not resolve");
    assert!(request(&mut server,"textDocument/definition",24,json!({"textDocument":{"uri":uri},"position":{"line":phantom,"character":0}})).is_null(), "terminal phantom row must not resolve");

    let actions = request(&mut server, "textDocument/codeAction", 15, json!({"textDocument":{"uri":uri},"range":{"start":{"line":row,"character":0},"end":{"line":row,"character":0}},"context":{"diagnostics":[]}}));
    let open = actions.as_array().unwrap().iter().find(|a| a["command"]["arguments"][0]["command"] == "open.commit").unwrap()["command"]["arguments"][0].clone();
    server.send(json!({"jsonrpc":"2.0","id":16,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[open]}}));
    loop { let m=server.recv(); if m["method"]=="workspace/applyEdit" { for f in editor.dispatch(&m) { server.send(f); } } else if m["id"]==16 { assert!(m["error"].is_null()); break; } }
    let menu_text = editor.text(&uri).to_owned();
    assert_ne!(menu_text, text, "the command must transition to a distinct menu document");
    assert!(menu_text.contains("Commit"), "active menu text: {menu_text}");
    let menu_tokens=request(&mut server,"textDocument/semanticTokens/full",17,json!({"textDocument":{"uri":uri}}));
    assert!(menu_tokens["data"].as_array().is_some_and(|v| !v.is_empty()), "menu tokens {menu_tokens}");
    assert_ne!(menu_tokens["data"], tokens["data"], "provider token snapshot must follow active commit menu text");
    assert_eq!(state(&repo), git);
    let mut id=30;
    let version = editor.documents[&uri].version + 1;
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":version},"contentChanges":[{"range":{"start":{"line":0,"character":999},"end":{"line":0,"character":1000}},"text":"x"}]}}));
    assert_all_null(&mut server,&uri,&mut id);
    let mut unmatched=editor.text(&uri).to_owned();
    unmatched.push_str("user edit\n");
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":editor.documents[&uri].version+1},"contentChanges":[{"text":unmatched}]}}));
    assert_all_null(&mut server,&uri,&mut id);
    server.send(editor.close(&uri));
    assert_all_null(&mut server,&uri,&mut id);
    assert_all_null(&mut server,"file:///missing/.magit",&mut id);
    assert_eq!(state(&repo), git);
    server.shutdown_and_exit(); let (status,_) = server.finish_with_messages(std::time::Duration::from_secs(10)); assert!(status.success());
}

#[test]
fn providers_are_null_while_plain_snapshot_is_format_suspended() {
    let (_repo, mut server, mut editor, uri, _) = setup_with(Profile::CrlfNormalise, "stale\r\n");
    let warning = loop {
        let frame = server.recv();
        if frame["method"] == "window/showMessage" { break frame; }
        assert_ne!(frame["method"], "workspace/applyEdit");
    };
    assert!(warning["params"]["message"].as_str().unwrap().contains("line endings or final newline"));
    let mut id = 100;
    assert_all_null(&mut server, &uri, &mut id);
    server.send(editor.close(&uri));
    assert_all_null(&mut server, &uri, &mut id);
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
}
