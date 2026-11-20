#[path = "common/fake_editor.rs"]
mod fake_editor;
#[path = "common/editor_model/mod.rs"]
mod editor_model;
#[path = "common/mod.rs"]
mod fixtures;
#[path = "common/stdio.rs"]
mod stdio;

use fake_editor::{FakeEditor, Profile};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

fn open(
    repo: &fixtures::RepoFixture,
    client_name: &str,
    keys: bool,
) -> (stdio::TestServer, FakeEditor, String) {
    let uri = format!("file://{}/.magit", repo.path().display());
    let mut server = stdio::TestServer::spawn(&[]);
    let mut init = stdio::initialize_message();
    init["params"]["capabilities"] = json!({
        "window":{"showDocument":{"support":true}},
        "workspace":{"applyEdit":true,"workspaceEdit":{"documentChanges":true},"executeCommand":{"dynamicRegistration":true}},
        "textDocument":{"codeAction":{"dynamicRegistration":true},"general":{"positionEncodings":["utf-16"]}}
    });
    init["params"]["clientInfo"] = json!({"name":client_name,"version":"1"});
    init["params"]["initializationOptions"] =
        json!({"plain":{"keys":{"default":keys}}});
    init["params"]["workspaceFolders"] =
        json!([{"uri":format!("file://{}",repo.path().display()),"name":"repo"}]);
    server.send(init);
    server.response_with_id(1);
    server.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    let mut editor = FakeEditor::new(Profile::Standard);
    server.send(editor.open(&uri, "old"));
    apply(&mut server, &mut editor);
    (server, editor, uri)
}

/// Read until the next applyEdit, returning the request and any messages seen first.
fn next_apply(server: &mut stdio::TestServer) -> (Value, Vec<String>) {
    let mut messages = Vec::new();
    loop {
        let message = server.recv();
        match message["method"].as_str() {
            Some("workspace/applyEdit") => return (message, messages),
            Some("window/showMessage") => messages.push(
                message["params"]["message"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            ),
            _ => {}
        }
    }
}

/// Apply one authoritative edit and send its echo, returning messages seen first.
fn apply(server: &mut stdio::TestServer, editor: &mut FakeEditor) -> Vec<String> {
    let (edit, mut messages) = next_apply(server);
    for frame in editor.dispatch(&edit) {
        server.send(frame);
    }
    messages.extend(settle(server, editor));
    messages
}

/// Process all edits preceding a reader barrier, including their repair echoes.
fn settle(server: &mut stdio::TestServer, editor: &mut FakeEditor) -> Vec<String> {
    let uri = editor.documents.keys().next().unwrap().clone();
    let mut messages = Vec::new();
    for _ in 0..8 {
        server.send(json!({"jsonrpc":"2.0","id":9001,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"context":{"diagnostics":[]}}}));
        let mut edited = false;
        loop {
            let message = server.recv();
            if message["id"] == 9001 { break; }
            match message["method"].as_str() {
                Some("workspace/applyEdit") => {
                    edited = true;
                    for frame in editor.dispatch(&message) { server.send(frame); }
                }
                Some("window/showMessage") => messages.push(message["params"]["message"].as_str().unwrap_or_default().to_owned()),
                _ => {}
            }
        }
        if !edited { return messages; }
    }
    panic!("authoritative repairs did not converge");
}

fn line_offset(text: &str, line: u32) -> usize {
    text.split_inclusive('\n')
        .take(line as usize)
        .map(str::len)
        .sum()
}

/// Replace one exact byte range and send the ranged didChange for it.
fn replace(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    uri: &str,
    line: u32,
    character: u32,
    length: usize,
    text: &str,
) -> Value {
    let document = editor.documents.get_mut(uri).expect("open document");
    let start = line_offset(&document.text, line) + character as usize;
    let end = start + length;
    let mut next = document.text.clone();
    next.replace_range(start..end, text);
    document.text = next;
    document.version += 1;
    let message = json!({
        "jsonrpc":"2.0","method":"textDocument/didChange",
        "params":{
            "textDocument":{"uri":uri,"version":document.version},
            "contentChanges":[{
                "range":{
                    "start":{"line":line,"character":character},
                    "end":{"line":line,"character":character + length as u32}
                },
                "text":text
            }]
        }
    });
    server.send(message.clone());
    message
}

/// Replace the whole document with a rangeless change.
fn replace_all(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    uri: &str,
    text: &str,
) -> Value {
    let document = editor.documents.get_mut(uri).expect("open document");
    document.text = text.to_owned();
    document.version += 1;
    let message = json!({
        "jsonrpc":"2.0","method":"textDocument/didChange",
        "params":{
            "textDocument":{"uri":uri,"version":document.version},
            "contentChanges":[{"text":text}]
        }
    });
    server.send(message.clone());
    message
}

fn line_of(editor: &FakeEditor, uri: &str, needle: &str) -> u32 {
    editor
        .text(uri)
        .lines()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("missing line containing {needle}")) as u32
}

fn actions(server: &mut stdio::TestServer, uri: &str, id: i64) -> Vec<Value> {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"context":{"diagnostics":[]}}}));
    server.response_with_id(id)["result"]
        .as_array()
        .unwrap()
        .clone()
}

fn plain_args(actions: &[Value], command: &str) -> Value {
    actions
        .iter()
        .find(|action| action["command"]["arguments"][0]["command"] == command)
        .unwrap_or_else(|| panic!("missing Plain action {command}"))["command"]["arguments"][0]
        .clone()
}

/// Pump one frame: answer server requests and let the editor apply their edits.
fn pump(server: &mut stdio::TestServer, editor: &mut FakeEditor, seen: &mut Vec<Value>) -> Value {
    let message = server.recv();
    seen.push(message.clone());
    if message["method"].is_string() && !message["id"].is_null() {
        for frame in editor.dispatch(&message) {
            server.send(frame);
        }
    }
    message
}

/// Execute one Plain action, pumping frames until its own reply.
fn execute_action(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    seen: &mut Vec<Value>,
    args: Value,
    id: i64,
) -> Value {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[args]}}));
    loop {
        let message = pump(server, editor, seen);
        if message["id"] == json!(id) && message["method"].is_null() {
            return message;
        }
    }
}

/// Query the actions offered for one line, pumping until the reply.
fn offered_actions(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    seen: &mut Vec<Value>,
    uri: &str,
    line: u32,
    id: i64,
) -> Vec<Value> {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"textDocument/codeAction","params":{"textDocument":{"uri":uri},"range":{"start":{"line":line,"character":0},"end":{"line":line,"character":0}},"context":{"diagnostics":[]}}}));
    loop {
        let message = pump(server, editor, seen);
        if message["id"] == json!(id) && message["method"].is_null() {
            return message["result"].as_array().unwrap().clone();
        }
    }
}

/// Poll for the Plain action `command` offered on `needle`'s row.
fn wait_for_action(
    server: &mut stdio::TestServer,
    editor: &mut FakeEditor,
    seen: &mut Vec<Value>,
    uri: &str,
    needle: &str,
    command: &str,
    mut id: i64,
) -> (Value, i64) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        assert!(Instant::now() < deadline, "action {command} was not offered for {needle}");
        if editor.text(uri).contains(needle) {
            let line = line_of(editor, uri, needle);
            let offered = offered_actions(server, editor, seen, uri, line, id);
            id += 1;
            if let Some(found) =
                offered.iter().find(|action| action["command"]["arguments"][0]["command"] == command)
            {
                return (found["command"]["arguments"][0].clone(), id);
            }
        } else {
            pump(server, editor, seen);
        }
    }
}

#[test]
fn eligible_key_stages_exactly_and_repairs_authoritative_text() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("init", [("f.txt", "one\n")])
        .modify("f.txt", "two\n")
        .build();
    let (mut server, mut editor, uri) = open(&repo, "Key Exec Client", true);
    let line = line_of(&editor, &uri, "f.txt");
    replace(&mut server, &mut editor, &uri, line, 0, 0, "s");
    apply(&mut server, &mut editor);
    assert_eq!(
        fixtures::git(repo.path(), &["diff", "--cached", "--name-only"]),
        "f.txt\n",
        "the key staged exactly the edited file: {}", server.stderr_text()
    );
    assert_eq!(
        fixtures::git(repo.path(), &["diff", "--name-only"]),
        "",
        "the worktree is clean after staging"
    );
    assert!(
        !editor.text(&uri).starts_with('s'),
        "the typed key was removed by the authoritative repair"
    );
    assert!(
        editor.text(&uri).contains("Staged changes"),
        "the repair shows the refreshed status view"
    );
    assert_eq!(editor.text(&uri), "Head: main\n\nStaged changes (1)\n  M f.txt\n    @@ -1,1 +1,1 @@\n    -one\n    +two\n\nUnstaged changes (0)\n\nUntracked files (0)\nKeys: on (default)");
    let repaired_version = editor.documents[&uri].version;
    settle(&mut server, &mut editor);
    assert_eq!(editor.documents[&uri].version, repaired_version, "repair echo never triggers another edit");
    assert_eq!(fixtures::git(repo.path(), &["diff", "--cached", "--name-only"]), "f.txt\n", "repair echo never repeats staging");
}

#[test]
fn destructive_paste_planned_and_off_keys_never_mutate() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("init", [("f.txt", "one\n")])
        .modify("f.txt", "two\n")
        .build();
    let (mut server, mut editor, uri) = open(&repo, "Key Exec Client", true);
    let line = line_of(&editor, &uri, "f.txt");
    let before = fixtures::git(repo.path(), &["status", "--porcelain"]);
    let authoritative = editor.text(&uri).to_owned();
    let messages = {
        replace(&mut server, &mut editor, &uri, line, 0, 0, "x");
        apply(&mut server, &mut editor)
    };
    assert!(
        messages.iter().any(|message| message.contains("destructive")),
        "the destructive key points to the code action: {messages:?}"
    );
    assert_eq!(fixtures::git(repo.path(), &["status", "--porcelain"]), before);
    assert_eq!(editor.text(&uri), authoritative);
    replace(&mut server, &mut editor, &uri, line, 0, 0, "ss");
    apply(&mut server, &mut editor);
    assert_eq!(fixtures::git(repo.path(), &["status", "--porcelain"]), before);
    assert_eq!(editor.text(&uri), authoritative);
    replace(&mut server, &mut editor, &uri, line, 0, 0, "R");
    let messages = apply(&mut server, &mut editor);
    assert!(messages.iter().any(|message| message.contains("planned")), "planned key refused: {messages:?}");
    assert_eq!(fixtures::git(repo.path(), &["status", "--porcelain"]), before);
    assert_eq!(editor.text(&uri), authoritative);
    assert!(!editor.text(&uri).starts_with('x'), "destructive edit repaired");
    assert!(!editor.text(&uri).starts_with("ss"), "paste repaired");

    let (off_repo, _) = fixtures::RepoFixture::new()
        .commit("init", [("f.txt", "one\n")])
        .modify("f.txt", "two\n")
        .build();
    let (mut off_server, mut off_editor, off_uri) = open(&off_repo, "Keys Off Client", false);
    let off_line = line_of(&off_editor, &off_uri, "f.txt");
    let off_text = off_editor.text(&off_uri).to_owned();
    replace(&mut off_server, &mut off_editor, &off_uri, off_line, 0, 0, "s");
    apply(&mut off_server, &mut off_editor);
    assert_eq!(off_editor.text(&off_uri), off_text);
    assert_eq!(
        fixtures::git(off_repo.path(), &["diff", "--cached", "--name-only"]),
        "",
        "keys off never stages"
    );
}

#[test]
fn stale_key_edit_reports_and_changes_nothing() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("init", [("f.txt", "one\n")])
        .modify("f.txt", "two\n")
        .build();
    let (mut server, mut editor, uri) = open(&repo, "Key Exec Client", true);
    let stale_text = editor.text(&uri).to_owned();
    let stale_line = line_of(&editor, &uri, "f.txt");
    let mut written = actions(&mut server, &uri, 10);
    let open_commit = plain_args(&written, "open.commit");
    server.send(json!({"jsonrpc":"2.0","id":11,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[open_commit]}}));
    let mut menu_text = None;
    loop {
        let message = server.recv();
        if message["method"] == "workspace/applyEdit" {
            for frame in editor.dispatch(&message) {
                server.send(frame);
            }
            menu_text = Some(editor.text(&uri).to_owned());
        }
        if message["id"] == 11 {
            break;
        }
    }
    assert!(
        menu_text.is_some_and(|text| text != stale_text),
        "the commit menu advanced the session"
    );
    // Return to a text the document still matches but the session has moved past.
    replace_all(&mut server, &mut editor, &uri, &stale_text);
    let before = fixtures::git(repo.path(), &["status", "--porcelain"]);
    replace(&mut server, &mut editor, &uri, stale_line, 0, 0, "s");
    let mut reported = false;
    for _ in 0..40 {
        let message = server.recv();
        match message["method"].as_str() {
            Some("window/showMessage") => {
                let text = message["params"]["message"].as_str().unwrap_or_default();
                if text.contains("changed since displayed") {
                    reported = true;
                    break;
                }
            }
            Some("workspace/applyEdit") => {
                server.send(json!({"jsonrpc":"2.0","id":message["id"],"result":{"applied":false}}));
            }
            _ => {}
        }
    }
    assert!(reported, "the stale edit reported its refresh");
    assert_eq!(
        fixtures::git(repo.path(), &["status", "--porcelain"]),
        before,
        "a stale key edit never mutates Git"
    );
}

#[test]
fn chord_and_menu_steps_keep_server_state() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("init", [("f.txt", "one\n")])
        .modify("f.txt", "two\n")
        .build();
    let (mut server, mut editor, uri) = open(&repo, "Key Exec Client", true);
    let line = line_of(&editor, &uri, "f.txt");
    let before = fixtures::git(repo.path(), &["status", "--porcelain"]);
    replace(&mut server, &mut editor, &uri, line, 0, 0, "g");
    assert!(apply(&mut server, &mut editor).is_empty(), "safe chord prefix accepted");
    assert!(!editor.text(&uri).starts_with('g'), "chord prefix repaired");
    replace(&mut server, &mut editor, &uri, line, 0, 0, "r");
    assert!(apply(&mut server, &mut editor).is_empty(), "pending chord completed rather than refusing an unbound r");
    assert_eq!(
        fixtures::git(repo.path(), &["status", "--porcelain"]),
        before,
        "the chord only refreshed"
    );
    let line = line_of(&editor, &uri, "f.txt");
    replace(&mut server, &mut editor, &uri, line, 0, 0, "c");
    apply(&mut server, &mut editor);
    assert!(
        editor.text(&uri).contains("Commit"),
        "the commit menu replaced the status document: {:?}\n{}",
        editor.text(&uri),
        server.stderr_text()
    );
    let menu = editor.text(&uri).to_owned();
    replace(&mut server, &mut editor, &uri, 0, 0, 0, "-");
    apply(&mut server, &mut editor);
    replace(&mut server, &mut editor, &uri, 0, 0, 0, "a");
    apply(&mut server, &mut editor);
    assert!(
        editor.text(&uri).contains("Commit"),
        "menu progression kept the menu view"
    );
    assert_ne!(editor.text(&uri), menu, "-a toggled the commit menu switch");
    assert!(editor.text(&uri).contains("git commit --all"), "the authoritative menu reflects --all");
    assert_eq!(
        fixtures::git(repo.path(), &["status", "--porcelain"]),
        before,
        "menu keys never mutated Git"
    );
}

#[test]
fn armed_stage_key_is_refused_while_a_commit_draft_owns_the_repository() {
    let (repo, _) = fixtures::RepoFixture::new()
        .commit("initial", [("f.txt", "one\n"), ("g.txt", "unchanged\n")])
        .modify("f.txt", "two\n")
        .modify("g.txt", "changed\n")
        .build();
    let (mut server, mut editor, uri) = open(&repo, "Key Exec Client", true);
    let mut seen = Vec::new();

    // Stage f.txt so commit creation has something to commit.
    let (stage, id) = wait_for_action(
        &mut server, &mut editor, &mut seen, &uri, "f.txt", "status.stage", 20,
    );
    let staged = execute_action(&mut server, &mut editor, &mut seen, stage, id);
    assert!(staged["error"].is_null(), "{staged}");
    let mut id = id + 1;
    let _ = settle(&mut server, &mut editor);

    // Open the commit menu and start a commit draft that owns the repository.
    let (open_commit, next) = wait_for_action(
        &mut server, &mut editor, &mut seen, &uri, "f.txt", "open.commit", id,
    );
    id = next;
    let opened = execute_action(&mut server, &mut editor, &mut seen, open_commit, id);
    assert!(opened["error"].is_null(), "{opened}");
    id += 1;
    let _ = settle(&mut server, &mut editor);
    replace(&mut server, &mut editor, &uri, 0, 0, 0, "c");
    let draft_uri = format!("file://{}/.git/COMMIT_EDITMSG", repo.path().display());
    let deadline = Instant::now() + Duration::from_secs(20);
    while !seen.iter().any(|message| {
        message["method"] == "window/showDocument" && message["params"]["uri"] == draft_uri
    }) {
        assert!(Instant::now() < deadline, "commit draft was not opened: {seen:#?}");
        pump(&mut server, &mut editor, &mut seen);
    }
    let template =
        std::fs::read_to_string(repo.path().join(".git/COMMIT_EDITMSG")).expect("commit template");
    server.send(editor.open(&draft_uri, &template));

    // The draft holds mutation authority while the status document stays usable.
    let deadline = Instant::now() + Duration::from_secs(20);
    while !(editor.text(&uri).contains("g.txt") && editor.text(&uri).contains(&draft_uri)) {
        assert!(
            Instant::now() < deadline,
            "status document did not return: {:#?}",
            editor.text(&uri)
        );
        pump(&mut server, &mut editor, &mut seen);
    }
    let _ = settle(&mut server, &mut editor);

    let head = repo.git(&["rev-parse", "HEAD"]);
    let index = repo.git(&["write-tree"]);
    let worktree = repo.git(&["status", "--porcelain=v1"]);
    let diff = repo.git(&["diff", "--binary"]);
    let before = editor.text(&uri).to_owned();
    let line = line_of(&editor, &uri, "g.txt");
    replace(&mut server, &mut editor, &uri, line, 0, 0, "s");
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut refusal = None;
    while refusal.is_none() {
        assert!(Instant::now() < deadline, "deferred key was not refused: {seen:#?}");
        let message = pump(&mut server, &mut editor, &mut seen);
        if message["method"] == "window/showMessage" {
            let text = message["params"]["message"].as_str().unwrap_or_default();
            if text == "Wait for the current Git operation to finish" {
                refusal = Some(text.to_owned());
            }
        }
    }
    let _ = settle(&mut server, &mut editor);
    assert_eq!(
        editor.text(&uri),
        before,
        "the refused key was repaired to the authoritative status text"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head, "the refused key never committed");
    assert_eq!(repo.git(&["write-tree"]), index, "the refused key never touched the index");
    assert_eq!(
        repo.git(&["status", "--porcelain=v1"]),
        worktree,
        "the refused key never touched the worktree"
    );
    assert_eq!(repo.git(&["diff", "--binary"]), diff, "the refused key left the diff unchanged");

    // Abort the draft, releasing repository mutation authority.
    for _ in 0..2 {
        let offered = offered_actions(&mut server, &mut editor, &mut seen, &draft_uri, 0, id);
        id += 1;
        let Some(cancel) = offered
            .iter()
            .find(|action| action["command"]["arguments"][0]["command"] == "common.cancel")
        else {
            break;
        };
        let cancelled =
            execute_action(&mut server, &mut editor, &mut seen, cancel["command"]["arguments"][0].clone(), id);
        id += 1;
        assert!(cancelled["error"].is_null(), "{cancelled}");
    }
    server.send(editor.close(&draft_uri));
    let deadline = Instant::now() + Duration::from_secs(20);
    while editor.text(&uri).contains(&draft_uri) || !editor.text(&uri).contains("g.txt") {
        assert!(
            Instant::now() < deadline,
            "status document did not return after abort: {:#?}",
            editor.text(&uri)
        );
        pump(&mut server, &mut editor, &mut seen);
    }
    let _ = settle(&mut server, &mut editor);

    // The same armed key stages once the deferred operation has released ownership.
    let line = line_of(&editor, &uri, "g.txt");
    replace(&mut server, &mut editor, &uri, line, 0, 0, "s");
    apply(&mut server, &mut editor);
    assert_eq!(
        fixtures::git(repo.path(), &["diff", "--cached", "--name-only"]),
        "f.txt\ng.txt\n",
        "the same key stages after the draft is aborted: {}",
        server.stderr_text()
    );
    assert_eq!(
        fixtures::git(repo.path(), &["diff", "--name-only"]),
        "",
        "the worktree is clean after the released key staged"
    );
    assert!(
        !editor.text(&uri).lines().any(|line| line.starts_with('s')),
        "the typed key was removed by the authoritative repair"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head, "staging after release never commits");
}
