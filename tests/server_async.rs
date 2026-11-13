//! The real server over real pipes: framing, pipelining, concurrency, ordering.

mod common;

use common::fake_git::GatedGit;
use common::stdio::*;
use common::*;
use serde_json::{json, Value};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

fn repo_param(path: &std::path::Path) -> Value {
    json!(path.to_string_lossy())
}

#[test]
fn opening_multirow_rebase_todo_batches_object_resolution_once() {
    for rows in [2, 5] {
        let mut fixture = RepoFixture::new().commit("root", [("f.txt", "0\n")]);
        for index in 1..=rows + 1 {
            fixture = fixture.commit(&format!("commit-{index}"), [("f.txt", format!("{index}\n"))]);
        }
        let (tmp, _) = fixture.build();
        let pause = Command::new("git")
            .args(["rebase", "-i", &format!("HEAD~{}", rows + 1)])
            .current_dir(tmp.path())
            .env("GIT_SEQUENCE_EDITOR", "sed -i '1s/^pick /edit /'")
            .output()
            .unwrap();
        assert!(pause.status.success(), "{}", String::from_utf8_lossy(&pause.stderr));
        let trace = tmp.path().join("trace.jsonl");
        let trace_path = trace.to_str().unwrap();
        let mut server = TestServer::spawn(&[("GIT_TRACE2_EVENT", trace_path)]);
        assert!(server.initialize(json!({})) ["error"].is_null());
        server.send(json!({"jsonrpc":"2.0","id":2,"method":"view/open",
            "params":{"kind":"rebase-todo","args":{"repo_path":repo_param(tmp.path())},
            "viewport":{"cols":80,"rows":24}}}));
        let opened = server.response_with_id(2);
        assert!(opened["error"].is_null(), "{opened}");
        let render = server.recv();
        assert_eq!(render["method"], "view/render");
        server.recv();
        server.shutdown_and_exit();
        assert!(server.finish(Duration::from_secs(10)).success());
        let events = read(&trace);
        let starts = events.lines().filter_map(|line| {
            let event: Value = serde_json::from_str(line).unwrap();
            (event["event"] == "start").then_some(event)
        });
        let count = starts.filter(|event| {
            event["argv"].as_array().is_some_and(|argv| {
                argv.iter().any(|arg| arg == "cat-file")
                    && argv.iter().any(|arg| arg.as_str().is_some_and(|arg| arg.starts_with("--batch-check")))
            })
        }).count();
        assert_eq!(count, 1, "{rows} rows: {events}");
    }
}

#[test]
fn prepared_rebase_submit_through_real_server_keys_restores_caller() {
    let (tmp, _) = RepoFixture::new()
        .commit("base", [("base", "base\n")])
        .commit("one", [("one", "1\n")])
        .commit("two", [("two", "2\n")])
        .commit("three", [("three", "3\n")])
        .build();
    let base = tmp.git(&["rev-parse", "HEAD~3"]).trim().to_owned();
    let mut server = TestServer::spawn(&[]);
    assert!(server.initialize_rich(json!({}))["error"].is_null());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));

    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{
            "viewId":view,"version":1,"key":key,"editorMode":"normal",
            "cursor":{"line":line,"column":0}}}));
        server.response_with_id(id)
    };
    let pushed = |response: &Value| {
        let descriptor = response["result"]["effects"].as_array().unwrap().iter()
            .find(|effect| effect["type"] == "pushView").expect("pushView effect")["descriptor"].clone();
        (descriptor["id"].as_str().unwrap().to_owned(), descriptor["uri"].as_str().unwrap().to_owned())
    };
    let opened = |server: &mut TestServer, view: &str| {
        let mut render = false;
        let mut keymap = false;
        let mut version = None;
        while !render || !keymap {
            let message = server.recv();
            if message["params"]["viewId"] == view {
                if message["method"] == "view/render" {
                    render = true;
                    version = message["params"]["version"].as_u64();
                }
                keymap |= message["method"] == "view/keymap";
            }
        }
        server.send(json!({"jsonrpc":"2.0","method":"view/rendered",
            "params":{"viewId":view,"version":version.unwrap()}}));
    };

    let (menu, _) = pushed(&press(&mut server, 3, &status, "r", 0));
    opened(&mut server, &menu);
    let (selector, _) = pushed(&press(&mut server, 4, &menu, "i", 0));
    opened(&mut server, &selector);
    let mut base_line = None;
    for line in 0..100 {
        let id = 50 + line as i64;
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/invoke","params":{
            "viewId":selector,"command":"common.copy-value","version":1,
            "cursor":{"line":line,"column":0}}}));
        let copied = server.response_with_id(id);
        if copied["result"]["effects"].as_array().unwrap().iter()
            .any(|effect| effect["type"] == "clipboard" && effect["text"] == base)
        {
            base_line = Some(line);
            break;
        }
    }
    let base_line = base_line.expect("server-owned base identity");
    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/invoke","params":{
        "viewId":selector,"command":"common.confirm","version":1,
        "cursor":{"line":base_line,"column":0}}}));
    let (todo, _) = pushed(&server.response_with_id(6));
    opened(&mut server, &todo);

    assert!(press(&mut server, 8, &todo, "J", 0)["result"]["effects"].as_array().unwrap().is_empty());
    assert_eq!(server.recv()["method"], "view/render");
    assert!(press(&mut server, 9, &todo, "d", 2)["result"]["effects"].as_array().unwrap().is_empty());
    assert_eq!(server.recv()["method"], "view/render");
    let submitted = press(&mut server, 10, &todo, "ctrl+s", 0);
    let submit_effects = submitted["result"]["effects"].as_array().unwrap();
    assert!(submit_effects.iter().any(|effect| effect["type"] == "popView" && effect["viewId"] == todo), "{submitted}");
    assert!(submit_effects.iter().any(|effect| effect["type"] == "focusView" && effect["viewId"] == status), "{submitted}");
    assert_eq!(tmp.git(&["log", "--format=%s", "--reverse", &format!("{base}..HEAD")]), "two\none\n");
    assert_eq!(tmp.git(&["show", "HEAD:one"]), "1\n");
    assert_eq!(tmp.git(&["show", "HEAD~1:two"]), "2\n");
    assert!(!tmp.path().join("three").exists());

    server.send(json!({"jsonrpc":"2.0","id":11,"method":"view/invoke",
        "params":{"viewId":status,"command":"common.refresh"}}));
    let refreshed = server.response_with_id(11);
    assert!(refreshed["result"]["effects"].as_array().unwrap().is_empty(), "{refreshed}");
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn instant_fixup_selection_autosquashes_through_real_server_and_restores_caller() {
    let (tmp, _) = RepoFixture::new()
        .commit("root subject", [("root.txt", "root\n")])
        .commit("base subject", [("base.txt", "base\n")])
        .commit("selected target subject", [("target.txt", "base\n")])
        .commit("newer head subject", [("newer.txt", "head\n")])
        .staged_file("fix.txt", "fixed\n")
        .build();
    let target = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let base = tmp.git(&["rev-parse", "HEAD~2"]).trim().to_owned();
    let root = tmp.git(&["rev-parse", "HEAD~3"]).trim().to_owned();
    let prior_head = tmp.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let mut server = TestServer::spawn(&[]);
    assert!(server.initialize_rich(json!({}))["error"].is_null());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{
            "viewId":view,"version":1,"key":key,"editorMode":"normal",
            "cursor":{"line":line,"column":0}}}));
        server.response_with_id(id)
    };
    let pushed = |response: &Value| response["result"]["effects"].as_array().unwrap()
        .iter().find(|effect| effect["type"] == "pushView").expect("pushView")
        ["descriptor"]["id"].as_str().unwrap().to_owned();
    let opened = |server: &mut TestServer, view: &str| {
        let mut render = false;
        let mut keymap = false;
        let mut version = None;
        while !render || !keymap {
            let message = server.recv();
            if message["params"]["viewId"] == view {
                if message["method"] == "view/render" {
                    render = true;
                    version = message["params"]["version"].as_u64();
                }
                keymap |= message["method"] == "view/keymap";
            }
        }
        server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{
            "viewId":view,"version":version.unwrap()}}));
    };

    let menu = pushed(&press(&mut server, 3, &status, "c", 0));
    opened(&mut server, &menu);
    let selector = pushed(&press(&mut server, 4, &menu, "F", 0));
    opened(&mut server, &selector);
    let mut target_line = None;
    for line in 0..100 {
        server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/invoke","params":{
            "viewId":selector,"command":"common.copy-value","version":1,
            "cursor":{"line":line,"column":0}}}));
        let copied = server.response_with_id(5);
        if copied["result"]["effects"].as_array().unwrap().iter()
            .any(|effect| effect["type"] == "clipboard" && effect["text"] == target)
        {
            target_line = Some(line);
            break;
        }
    }
    let target_line = target_line.expect("structured full-OID target");
    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/invoke","params":{
        "viewId":selector,"command":"common.confirm","version":1,
        "cursor":{"line":target_line,"column":0}}}));
    let selected = server.response_with_id(6);
    assert!(selected["error"].is_null(), "{selected}");
    let mut caller_rendered = false;
    while !caller_rendered {
        let message = server.recv();
        if message["method"] == "view/render" && message["params"]["viewId"] == status {
            server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{
                "viewId":status,"version":message["params"]["version"]}}));
            caller_rendered = true;
        }
    }
    let history = tmp.git(&["log", "--format=%s", "--reverse"]);
    assert_eq!(history, "root subject\nbase subject\nselected target subject\nnewer head subject\n");
    assert_eq!(tmp.git(&["rev-parse", "HEAD~2"]).trim(), base);
    assert_eq!(tmp.git(&["rev-parse", "HEAD~3"]).trim(), root);
    assert_eq!(tmp.git(&["show", "HEAD:target.txt"]), "base\n");
    assert_eq!(tmp.git(&["show", "HEAD:newer.txt"]), "head\n");
    assert_eq!(tmp.git(&["show", "HEAD:fix.txt"]), "fixed\n");
    assert_eq!(tmp.git(&["write-tree"]), tmp.git(&["rev-parse", "HEAD^{tree}"]));
    assert!(!tmp.path().join(".git/rebase-merge").exists());
    assert!(!tmp.path().join(".git/rebase-apply").exists());
    assert_ne!(tmp.git(&["rev-parse", "HEAD"]), prior_head);
    server.send(json!({"jsonrpc":"2.0","id":7,"method":"view/invoke","params":{
        "viewId":status,"command":"common.refresh"}}));
    assert!(server.response_with_id(7)["error"].is_null());
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn dispatched_instant_fixup_conflict_is_left_for_explicit_recovery() {
    let (tmp, _) = RepoFixture::new()
        .commit("root", [("file", "root\n")])
        .commit("selected target", [("file", "target\n")])
        .commit("newer head", [("file", "head\n")])
        .staged_file("file", "resolved\n")
        .build();
    let prior_head = tmp.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let marker_parent = prior_head.clone();
    let target = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(10);
    assert!(server.initialize_rich(json!({}))["error"].is_null());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{"viewId":view,"version":1,"key":key,"editorMode":"normal","cursor":{"line":line,"column":0}}}));
        server.response_with_id(id)
    };
    let pushed = |response: &Value| response["result"]["effects"].as_array().unwrap().iter()
        .find(|effect| effect["type"] == "pushView").expect("pushView")["descriptor"]["id"].as_str().unwrap().to_owned();
    let opened = |server: &mut TestServer, view: &str| {
        let mut render = false; let mut keymap = false; let mut version = None;
        while !render || !keymap { let message = server.recv(); if message["params"]["viewId"] == view {
            if message["method"] == "view/render" { render = true; version = message["params"]["version"].as_u64(); }
            keymap |= message["method"] == "view/keymap";
        }}
        let version = version.unwrap();
        server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view,"version":version}}));
        version
    };
    let menu = pushed(&press(&mut server, 3, &status, "c", 0)); opened(&mut server, &menu);
    let selector = pushed(&press(&mut server, 4, &menu, "F", 0));
    let selector_version = opened(&mut server, &selector);
    let mut target_line = None;
    for line in 0..100 { server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/invoke","params":{"viewId":selector,"command":"common.copy-value","version":1,"cursor":{"line":line,"column":0}}}));
        if server.response_with_id(5)["result"]["effects"].as_array().unwrap().iter().any(|e| e["type"] == "clipboard" && e["text"] == target) { target_line = Some(line); break; }
    }
    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/invoke","params":{"viewId":selector,"command":"common.confirm","version":selector_version,"cursor":{"line":target_line.expect("target oid"),"column":0}}}));
    let selected = server.response_with_id(6);
    assert!(selected["error"].is_null(), "{selected}");
    assert_eq!(selected["result"]["effects"], json!([
        {"type":"popView","viewId":selector},
        {"type":"focusView","viewId":status}
    ]), "{selected}");
    let mut pending = std::mem::take(&mut server.notifications).into_iter();
    let mut caller = None;
    let mut stopped = None;
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut actual = Vec::new();
    let mut render_versions = Vec::new();
    let readiness_diagnostics = |actual: &[Value], render_versions: &[u64]| format!(
        "confirm={selected}; selected_oid={target}; selector_ack_version={selector_version}; actual={actual:?}; render_versions={render_versions:?}; HEAD={}; status={}; unmerged={}; rebase_merge={}; rebase_apply={}",
        tmp.git(&["rev-parse", "HEAD"]).trim(),
        tmp.git(&["status", "--porcelain"]),
        tmp.git(&["diff", "--name-only", "--diff-filter=U"]),
        tmp.path().join(".git/rebase-merge").exists(),
        tmp.path().join(".git/rebase-apply").exists(),
    );
    while caller.is_none() || stopped.is_none() {
        assert!(Instant::now() < deadline, "{}", readiness_diagnostics(&actual, &render_versions));
        let message = if let Some(message) = pending.next() {
            message
        } else {
            server.recv_timeout = deadline.saturating_duration_since(Instant::now());
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| server.recv()))
                .unwrap_or_else(|_| panic!("{}", readiness_diagnostics(&actual, &render_versions)))
        };
        actual.push(message.clone());
        if message["method"] == "view/render" {
            if let Some(version) = message["params"]["version"].as_u64() { render_versions.push(version); }
        }
        if message["method"] == "view/render"
            && message["params"]["viewId"] == status
        {
            caller = Some(message.clone());
        }
        if message["method"] == "view/effects"
            && message["params"]["effects"].as_array().is_some_and(|effects| effects.iter().any(|effect|
                effect["type"] == "message"
                    && effect["text"].as_str().is_some_and(|text| text.starts_with("Instant autosquash stopped during autosquash; marker "))))
        {
            stopped = Some(message);
        }
    }
    let caller = caller.unwrap();
    let stopped = stopped.unwrap();
    assert!(stopped["params"]["effects"].as_array().unwrap().iter().any(|effect|
        effect["type"] == "message" && effect["level"] == "error"), "{stopped}");
    let marker = tmp.git(&["rev-parse", "refs/heads/main"]).trim().to_owned();
    let stopped_text = stopped["params"]["effects"].as_array().unwrap().iter()
        .find(|effect| effect["type"] == "message" && effect["level"] == "error").unwrap()["text"].as_str().unwrap();
    assert!(stopped_text.starts_with(&format!("Instant autosquash stopped during autosquash; marker {marker} retained: ")), "{stopped_text}");
    let marker_tree = tmp.git(&["rev-parse", &format!("{marker}^{{tree}}")]).trim().to_owned();
    assert_eq!(tmp.git(&["rev-parse", &format!("{marker}^")]).trim(), marker_parent);
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]).trim(), target);
    assert_eq!(tmp.git(&["status", "--porcelain"]), "UU file\n");
    assert!(std::fs::read_to_string(tmp.path().join("file")).unwrap().contains("<<<<<<<"));
    assert!(tmp.git(&["rebase", "--show-current-patch"]).contains(&format!("fixup! {target}")));
    assert_ne!(marker, prior_head);
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":status,"version":caller["params"]["version"]}}));
    server.send(json!({"jsonrpc":"2.0","id":7,"method":"view/invoke","params":{"viewId":status,"command":"rebase.abort","version":caller["params"]["version"]}}));
    let aborted = server.response_with_id(7);
    assert!(aborted["error"].is_null(), "{aborted}");
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]).trim(), marker);
    assert_eq!(tmp.git(&["rev-parse", "refs/heads/main"]).trim(), marker);
    assert_eq!(tmp.git(&["rev-parse", "HEAD^{tree}"]).trim(), marker_tree);
    assert_eq!(tmp.git(&["status", "--porcelain"]), "");
    assert_eq!(std::fs::read_to_string(tmp.path().join("file")).unwrap(), "resolved\n");
    assert!(!tmp.path().join(".git/rebase-merge").exists());
    server.send(json!({"jsonrpc":"2.0","id":8,"method":"view/invoke","params":{"viewId":status,"command":"common.refresh"}}));
    assert!(server.response_with_id(8)["error"].is_null());
    shutdown_and_exit(&mut server); assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn dispatched_instant_fixup_conflict_can_be_resolved_and_continued() {
    let (tmp, _) = RepoFixture::new()
        .commit("root", [("file", "root\n")])
        .commit("selected target", [("file", "target\n")])
        .commit("newer head", [("file", "head\n")])
        .staged_file("file", "resolved\n")
        .build();
    let target = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let mut server = TestServer::spawn(&[]);
    server.recv_timeout = Duration::from_secs(10);
    assert!(server.initialize_rich(json!({}))["error"].is_null());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{"viewId":view,"version":1,"key":key,"editorMode":"normal","cursor":{"line":line,"column":0}}}));
        server.response_with_id(id)
    };
    let pushed = |response: &Value| response["result"]["effects"].as_array().unwrap().iter()
        .find(|effect| effect["type"] == "pushView").unwrap()["descriptor"]["id"].as_str().unwrap().to_owned();
    let opened = |server: &mut TestServer, view: &str| {
        let mut render = false; let mut keymap = false; let mut version = None;
        while !render || !keymap { let message = server.recv(); if message["params"]["viewId"] == view {
            if message["method"] == "view/render" { render = true; version = message["params"]["version"].as_u64(); }
            keymap |= message["method"] == "view/keymap";
        }}
        server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view,"version":version.unwrap()}}));
    };
    let menu = pushed(&press(&mut server, 3, &status, "c", 0)); opened(&mut server, &menu);
    let selector = pushed(&press(&mut server, 4, &menu, "F", 0)); opened(&mut server, &selector);
    let mut target_line = None;
    for line in 0..100 { server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/invoke","params":{"viewId":selector,"command":"common.copy-value","version":1,"cursor":{"line":line,"column":0}}}));
        if server.response_with_id(5)["result"]["effects"].as_array().unwrap().iter().any(|e| e["type"] == "clipboard" && e["text"] == target) { target_line = Some(line); break; }
    }
    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/invoke","params":{"viewId":selector,"command":"common.confirm","version":1,"cursor":{"line":target_line.expect("target oid"),"column":0}}}));
    assert!(server.response_with_id(6)["error"].is_null());
    let deadline = Instant::now() + Duration::from_secs(20);
    while tmp.git(&["status", "--porcelain"]) != "UU file\n" {
        assert!(Instant::now() < deadline, "instant rebase did not stop in bounded wait");
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(tmp.git(&["status", "--porcelain"]), "UU file\n");
    assert!(std::fs::read_to_string(tmp.path().join("file")).unwrap().contains("<<<<<<<"));
    let caller = loop { let message = server.recv(); if message["method"] == "view/render" && message["params"]["viewId"] == status { break message; } };
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":status,"version":caller["params"]["version"]}}));
    std::fs::write(tmp.path().join("file"), "resolved\n").unwrap();
    tmp.git(&["add", "file"]);
    server.send(json!({"jsonrpc":"2.0","id":20,"method":"view/invoke","params":{"viewId":status,"command":"common.refresh"}}));
    assert!(server.response_with_id(20)["error"].is_null());
    let caller = loop { let message = server.recv(); if message["method"] == "view/render" && message["params"]["viewId"] == status { break message; } };
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":status,"version":caller["params"]["version"]}}));
    server.send(json!({"jsonrpc":"2.0","id":7,"method":"view/invoke","params":{"viewId":status,"command":"rebase.continue","version":caller["params"]["version"]}}));
    let continued = server.response_with_id(7);
    let effects = continued["result"]["effects"].as_array().expect("continue effects");
    assert_eq!(effects.len(), 1, "{continued}");
    assert_eq!(effects[0]["type"], "message");
    assert_eq!(effects[0]["level"], "warn");
    let message = effects[0]["text"].as_str().unwrap();
    assert!(message.starts_with("git rebase --continue failed: Rebasing (3/3)\rerror: could not apply "), "{message}");
    assert!(message.contains("... newer head\nhint: Resolve all conflicts manually"), "{message}");
    assert!(message.ends_with("# newer head"), "{message}");
    assert_eq!(tmp.git(&["status", "--porcelain"]), "UU file\n");
    assert_eq!(std::fs::read_to_string(tmp.path().join("file")).unwrap(),
        format!("<<<<<<< HEAD\nresolved\n=======\nhead\n>>>>>>> {} (newer head)\n", &tmp.git(&["rev-parse", "REBASE_HEAD"])[..7]));
    assert_eq!(std::fs::read_to_string(tmp.path().join(".git/rebase-merge/git-rebase-todo")).unwrap(), "");
    let done = std::fs::read_to_string(tmp.path().join(".git/rebase-merge/done")).unwrap();
    assert!(done.lines().map(|line| line.split_once(' ').unwrap().1.split_once(' ').unwrap().1).eq([
        "# selected target", &format!("# fixup! {target}"), "# newer head"]), "{done}");
    std::fs::write(tmp.path().join("file"), "resolved\n").unwrap();
    tmp.git(&["add", "file"]);
    server.send(json!({"jsonrpc":"2.0","id":8,"method":"view/invoke","params":{"viewId":status,"command":"rebase.continue","version":caller["params"]["version"]}}));
    let finished = server.response_with_id(8);
    assert!(finished["error"].is_null(), "{finished}");
    assert!(finished["result"]["effects"].as_array().unwrap().is_empty(), "{finished}");
    assert!(!tmp.path().join(".git/rebase-merge").exists());
    assert!(!tmp.path().join(".git/rebase-apply").exists());
    assert_eq!(tmp.git(&["log", "-1", "--format=%B"]), "selected target\n\n");
    assert_eq!(tmp.git(&["log", "--format=%s", "--reverse"]), "root\nselected target\n");
    assert_eq!(tmp.git(&["show", "HEAD:file"]), "resolved\n");
    assert_eq!(std::fs::read_to_string(tmp.path().join("file")).unwrap(), "resolved\n");
    assert_eq!(tmp.git(&["status", "--porcelain"]), "");
    assert_eq!(tmp.git(&["write-tree"]).trim(), tmp.git(&["rev-parse", "HEAD^{tree}"]).trim());
    server.send(json!({"jsonrpc":"2.0","id":9,"method":"view/invoke","params":{"viewId":status,"command":"common.refresh"}}));
    assert!(server.response_with_id(9)["error"].is_null());
    shutdown_and_exit(&mut server); assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn instant_squash_selection_edits_message_through_registered_server_callback() {
    let (tmp, _) = RepoFixture::new()
        .commit("root subject", [("root.txt", "root\n")])
        .commit("base subject", [("base.txt", "base\n")])
        .commit("selected target subject", [("target.txt", "base\n")])
        .commit("newer head subject", [("newer.txt", "head\n")])
        .staged_file("fix.txt", "fixed\n")
        .build();
    let target = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let mut server = TestServer::spawn(&[]);
    assert!(server.initialize_rich(json!({})) ["error"].is_null());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{
            "viewId":view,"version":1,"key":key,"editorMode":"normal",
            "cursor":{"line":line,"column":0}}}));
        server.response_with_id(id)
    };
    let pushed = |response: &Value| response["result"]["effects"].as_array().unwrap()
        .iter().find(|effect| effect["type"] == "pushView").expect("pushView")
        ["descriptor"]["id"].as_str().unwrap().to_owned();
    let opened = |server: &mut TestServer, view: &str| {
        let mut render = false;
        let mut keymap = false;
        let mut version = None;
        while !render || !keymap {
            let message = server.recv();
            if message["params"]["viewId"] == view {
                if message["method"] == "view/render" {
                    render = true;
                    version = message["params"]["version"].as_u64();
                }
                keymap |= message["method"] == "view/keymap";
            }
        }
        server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{
            "viewId":view,"version":version.unwrap()}}));
    };
    let menu = pushed(&press(&mut server, 3, &status, "c", 0));
    opened(&mut server, &menu);
    let selector = pushed(&press(&mut server, 4, &menu, "S", 0));
    opened(&mut server, &selector);
    let mut target_line = None;
    for line in 0..100 {
        server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/invoke","params":{
            "viewId":selector,"command":"common.copy-value","version":1,
            "cursor":{"line":line,"column":0}}}));
        if server.response_with_id(5)["result"]["effects"].as_array().unwrap().iter()
            .any(|effect| effect["type"] == "clipboard" && effect["text"] == target)
        {
            target_line = Some(line);
            break;
        }
    }
    let target_line = target_line.expect("structured full-OID squash target");
    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/invoke","params":{
        "viewId":selector,"command":"common.confirm","version":1,
        "cursor":{"line":target_line,"column":0}}}));
    assert!(server.response_with_id(6)["error"].is_null());
    let edit = loop {
        let message = server.recv();
        if message["method"] == "view/effects" {
            if let Some(edit) = message["params"]["effects"].as_array().unwrap().iter()
                .find(|effect| effect["type"] == "editDocument")
            {
                break edit.clone();
            }
        }
    };
    let paused_head = tmp.git(&["rev-parse", "HEAD"]);
    let paused_branch = tmp.git(&["rev-parse", "refs/heads/main"]);
    let paused_index = std::fs::read(tmp.path().join(".git/index")).unwrap();
    server.send(json!({"jsonrpc":"2.0","id":12,"method":"view/invoke","params":{
        "viewId":status,"command":"common.refresh"}}));
    assert!(server.response_with_id(12)["error"].is_null());
    let (other_tmp, _) = init_repo();
    std::fs::write(other_tmp.path().join("f.txt"), "other\n").unwrap();
    let other = open_view(&mut server, 13, &repo_param(other_tmp.path()));
    server.send(json!({"jsonrpc":"2.0","id":14,"method":"view/invoke","params":{
        "viewId":other,"command":"status.stage-all"}}));
    let other_staged = server.response_with_id(14);
    assert!(other_staged["error"].is_null(), "{other_staged}");
    assert_eq!(other_tmp.git(&["show", ":f.txt"]), "other\n");
    server.send(json!({"jsonrpc":"2.0","id":15,"method":"view/resync","params":{
        "viewId":status}}));
    let status_version = server.response_with_id(15)["result"]["version"].as_u64().unwrap();
    server.send(json!({"jsonrpc":"2.0","id":16,"method":"view/key","params":{
        "viewId":status,"version":status_version,"key":"F5","editorMode":"normal",
        "cursor":{"line":0,"column":0}}}));
    let read_key = server.response_with_id(16);
    assert!(read_key["result"]["effects"].as_array().unwrap().iter().all(|effect|
        effect["text"] != "Wait for the current Git operation to finish"), "{read_key}");
    server.send(json!({"jsonrpc":"2.0","id":17,"method":"view/key","params":{
        "viewId":status,"version":status_version,"key":"S","editorMode":"normal",
        "cursor":{"line":0,"column":0}}}));
    let key_refused = server.response_with_id(17);
    assert!(key_refused["result"]["effects"].as_array().unwrap().iter().any(|effect|
        effect["text"] == "Wait for the current Git operation to finish"), "{key_refused}");
    server.send(json!({"jsonrpc":"2.0","id":9,"method":"view/invoke","params":{
        "viewId":status,"command":"rebase.abort"}}));
    let refused = server.response_with_id(9);
    assert!(refused["result"]["effects"].as_array().unwrap().iter().any(|effect|
        effect["type"] == "message"
            && effect["text"] == "Wait for the current Git operation to finish"), "{refused}");
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), paused_head);
    assert_eq!(tmp.git(&["rev-parse", "refs/heads/main"]), paused_branch);
    assert_eq!(std::fs::read(tmp.path().join(".git/index")).unwrap(), paused_index);
    let edited = "selected target subject\n\nnew squash paragraph\n";
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":edit["uri"],"languageId":"gitcommit","version":1,"text":edit["text"]}}}));
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":edit["uri"],"version":2},"contentChanges":[{"text":edited}]}}));
    server.send(json!({"jsonrpc":"2.0","id":7,"method":"workspace/executeCommand","params":{
        "command":"magit.command","arguments":[{"command":"common.confirm","runId":edit["runId"],
            "uri":edit["uri"],"version":2,"source":"synchronized"}]}}));
    let confirmed = server.response_with_id(7);
    assert!(confirmed["error"].is_null(), "{confirmed}");
    let mut caller_rendered = false;
    let mut completed = false;
    while !caller_rendered || !completed {
        let message = server.recv();
        if message["method"] == "view/render" && message["params"]["viewId"] == status {
            server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{
                "viewId":status,"version":message["params"]["version"]}}));
            caller_rendered = true;
        }
        if message["method"] == "view/effects" {
            completed |= message["params"]["effects"].as_array().is_some_and(|effects| {
                effects.iter().any(|effect| {
                    effect["text"].as_str().is_some_and(|text| {
                        text.starts_with("Instant autosquash completed")
                    })
                })
            });
        }
    }
    assert_eq!(tmp.git(&["log", "--format=%s", "--reverse"]),
        "root subject\nbase subject\nselected target subject\nnewer head subject\n");
    assert_eq!(tmp.git(&["show", "-s", "--format=%B", "HEAD~1"]),
        "selected target subject\n\nnew squash paragraph\n\n");
    assert_eq!(tmp.git(&["show", "HEAD:target.txt"]), "base\n");
    assert_eq!(tmp.git(&["show", "HEAD:fix.txt"]), "fixed\n");
    assert_eq!(tmp.git(&["write-tree"]), tmp.git(&["rev-parse", "HEAD^{tree}"]));
    assert!(!tmp.path().join(".git/rebase-merge").exists());
    assert!(!tmp.path().join(".git/rebase-apply").exists());
    std::fs::write(tmp.path().join("fix.txt"), "after\n").unwrap();
    server.send(json!({"jsonrpc":"2.0","id":10,"method":"view/invoke","params":{
        "viewId":status,"command":"common.refresh"}}));
    assert!(server.response_with_id(10)["error"].is_null());
    server.send(json!({"jsonrpc":"2.0","id":11,"method":"view/invoke","params":{
        "viewId":status,"command":"status.stage-all"}}));
    let staged = server.response_with_id(11);
    assert!(staged["error"].is_null(), "{staged}");
    assert!(staged["result"]["effects"].as_array().unwrap().iter().all(|effect|
        effect["text"] != "Wait for the current Git operation to finish"), "{staged}");
    assert_eq!(tmp.git(&["show", ":fix.txt"]), "after\n");
    server.send(json!({"jsonrpc":"2.0","id":8,"method":"view/invoke","params":{
        "viewId":status,"command":"common.refresh"}}));
    assert!(server.response_with_id(8)["error"].is_null());
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn instant_squash_registered_editor_abort_retains_marker_and_releases_worker() {
    let (tmp, _) = RepoFixture::new()
        .commit("root subject", [("root.txt", "root\n")])
        .commit("base subject", [("base.txt", "base\n")])
        .commit("selected target subject", [("target.txt", "base\n")])
        .commit("newer head subject", [("newer.txt", "head\n")])
        .staged_file("fix.txt", "fixed\n")
        .build();
    let target = tmp.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let original_head = tmp.git(&["rev-parse", "HEAD"]);
    let mut server = TestServer::spawn(&[]);
    server.auto_respond_to_progress_create();
    assert!(server.initialize_rich(json!({"window":{"workDoneProgress":true}}))["error"].is_null());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{"viewId":view,"version":1,"key":key,"editorMode":"normal","cursor":{"line":line,"column":0}}}));
        server.response_with_id(id)
    };
    let pushed = |response: &Value| response["result"]["effects"].as_array().unwrap().iter()
        .find(|effect| effect["type"] == "pushView").unwrap()["descriptor"]["id"].as_str().unwrap().to_owned();
    let opened = |server: &mut TestServer, view: &str| {
        let mut rendered = false; let mut keymap = false; let mut version = None;
        while !rendered || !keymap { let message = server.recv(); if message["params"]["viewId"] == view {
            if message["method"] == "view/render" { rendered = true; version = message["params"]["version"].as_u64(); }
            keymap |= message["method"] == "view/keymap";
        }}
        server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view,"version":version.unwrap()}}));
    };
    let menu = pushed(&press(&mut server, 3, &status, "c", 0)); opened(&mut server, &menu);
    let selector = pushed(&press(&mut server, 4, &menu, "S", 0)); opened(&mut server, &selector);
    let mut line = None;
    for candidate in 0..100 { server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/invoke","params":{"viewId":selector,"command":"common.copy-value","version":1,"cursor":{"line":candidate,"column":0}}}));
        if server.response_with_id(5)["result"]["effects"].as_array().unwrap().iter().any(|e| e["type"] == "clipboard" && e["text"] == target) { line = Some(candidate); break; }
    }
    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/invoke","params":{"viewId":selector,"command":"common.confirm","version":1,"cursor":{"line":line.expect("selected target"),"column":0}}}));
    assert!(server.response_with_id(6)["error"].is_null());
    let edit = loop {
        let message = server.recv();
        if message["method"] == "view/effects" {
            if let Some(edit) = message["params"]["effects"].as_array().unwrap().iter().find(|e| e["type"] == "editDocument") {
                break edit.clone();
            }
        }
    };
    let marker = tmp.git(&["rev-parse", "refs/heads/main"]);
    let stopped_head = tmp.git(&["rev-parse", "HEAD"]);
    let stopped_index = std::fs::read(tmp.path().join(".git/index")).unwrap();
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":edit["uri"],"languageId":"gitcommit","version":1,"text":edit["text"]}}}));
    server.send(json!({"jsonrpc":"2.0","id":7,"method":"view/invoke","params":{"viewId":status,"command":"common.cancel","args":{"runId":edit["runId"]}}}));
    let mut saw_end = false;
    let mut cancel_response = None;
    let mut stopped_effect = None;
    while cancel_response.is_none() || stopped_effect.is_none() {
        let message = server.recv();
        if message["method"] == "$/progress" && message["params"]["value"]["kind"] == "end" { saw_end = true; }
        if message["id"] == 7 { cancel_response = Some(message.clone()); }
        if message["method"] == "view/effects" { stopped_effect = Some(message); }
    }
    assert!(cancel_response.unwrap()["result"]["effects"].as_array().unwrap().is_empty());
    assert!(stopped_effect.unwrap()["params"]["effects"].as_array().unwrap().iter().any(|e| e["type"] == "message"));
    while !saw_end {
        let message = server.recv();
        if message["method"] == "$/progress" && message["params"]["value"]["kind"] == "end" { saw_end = true; }
    }
    assert!(saw_end, "progress ended after registered editor abort");
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), stopped_head);
    assert_eq!(tmp.git(&["rev-parse", "refs/heads/main"]).trim(), marker.trim());
    assert_eq!(tmp.git(&["rev-parse", &format!("{}^", marker.trim())]).trim(), original_head.trim());
    assert!(tmp.path().join(".git/rebase-merge").exists());
    assert_eq!(std::fs::read(tmp.path().join(".git/index")).unwrap(), stopped_index);
    assert_eq!(std::fs::read_to_string(tmp.path().join("fix.txt")).unwrap(), "fixed\n");
    server.send(json!({"jsonrpc":"2.0","id":9,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[{"command":"common.confirm","runId":edit["runId"],"uri":edit["uri"],"version":1,"source":"synchronized"}]}}));
    assert!(server.response_with_id(9)["error"].is_object(), "aborted editor run cannot complete");
    server.send(json!({"jsonrpc":"2.0","id":10,"method":"view/invoke","params":{"viewId":status,"command":"rebase.abort"}}));
    assert!(server.response_with_id(10)["error"].is_null(), "normal rebase abort works after editor cancellation");
    assert!(!tmp.path().join(".git/rebase-merge").exists());
    server.send(json!({"jsonrpc":"2.0","id":8,"method":"view/invoke","params":{"viewId":status,"command":"common.refresh"}}));
    assert!(server.response_with_id(8)["error"].is_null(), "caller refresh after cancellation and abort");
    shutdown_and_exit(&mut server); assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn prepared_rebase_message_actions_refuse_through_real_server_submit() {
    for (key, label) in [("r", "reword"), ("s", "squash")] {
        let (tmp, _) = RepoFixture::new().commit("base", [("base", "base\n")])
            .commit("one", [("one", "1\n")]).commit("two", [("two", "2\n")]).build();
        let base = tmp.git(&["rev-parse", "HEAD~2"]).trim().to_owned();
        let mut server = TestServer::spawn(&[]);
        assert!(server.initialize_rich(json!({}))["error"].is_null());
        let status = open_view(&mut server, 2, &repo_param(tmp.path()));
        let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
            server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{"viewId":view,"version":1,"key":key,"editorMode":"normal","cursor":{"line":line,"column":0}}}));
            server.response_with_id(id)
        };
        let pushed = |response: &Value| response["result"]["effects"].as_array().unwrap().iter()
            .find(|effect| effect["type"] == "pushView").unwrap()["descriptor"]["id"].as_str().unwrap().to_owned();
        let opened = |server: &mut TestServer, view: &str| {
            let mut render = false; let mut keymap = false; let mut version = None;
            while !render || !keymap { let message = server.recv(); if message["params"]["viewId"] == view {
                if message["method"] == "view/render" { render = true; version = message["params"]["version"].as_u64(); }
                keymap |= message["method"] == "view/keymap";
            }}
            server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view,"version":version.unwrap()}}));
        };
        let menu = pushed(&press(&mut server, 3, &status, "r", 0)); opened(&mut server, &menu);
        let selector = pushed(&press(&mut server, 4, &menu, "i", 0)); opened(&mut server, &selector);
        let mut base_line = None;
        for line in 0..100 { server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/invoke","params":{"viewId":selector,"command":"common.copy-value","version":1,"cursor":{"line":line,"column":0}}}));
            if server.response_with_id(5)["result"]["effects"].as_array().unwrap().iter().any(|e| e["type"] == "clipboard" && e["text"] == base) { base_line = Some(line); break; }
        }
        server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/invoke","params":{"viewId":selector,"command":"common.confirm","version":1,"cursor":{"line":base_line.unwrap(),"column":0}}}));
        let todo = pushed(&server.response_with_id(6)); opened(&mut server, &todo);
        let action_line = u64::from(label == "squash");
        let changed = press(&mut server, 7, &todo, key, action_line);
        assert!(changed["result"]["effects"].as_array().unwrap().is_empty(), "{changed}");
        server.recv();
        let before = (tmp.git(&["rev-parse", "HEAD"]), tmp.git(&["for-each-ref", "--format=%(refname) %(objectname)"]), std::fs::read(tmp.path().join(".git/index")).unwrap(), std::fs::read(tmp.path().join("one")).ok(), std::fs::read(tmp.path().join("two")).ok(), tmp.path().join(".git/rebase-merge").exists(), tmp.path().join(".git/rebase-apply").exists());
        let submitted = press(&mut server, 8, &todo, "ctrl+s", 0);
        assert!(submitted["result"]["effects"].as_array().unwrap().iter().any(|e| e["type"] == "message" && e["text"].as_str().unwrap().contains("prepared rebase message editing is not supported yet")), "{submitted}");
        assert_eq!((tmp.git(&["rev-parse", "HEAD"]), tmp.git(&["for-each-ref", "--format=%(refname) %(objectname)"]), std::fs::read(tmp.path().join(".git/index")).unwrap(), std::fs::read(tmp.path().join("one")).ok(), std::fs::read(tmp.path().join("two")).ok(), tmp.path().join(".git/rebase-merge").exists(), tmp.path().join(".git/rebase-apply").exists()), before);
        let retained = server.recv();
        assert_eq!(retained["method"], "view/render"); assert_eq!(retained["params"]["viewId"], todo);
        assert!(retained["params"]["text"][action_line as usize].as_str().unwrap().contains(label), "{retained}");
        assert!(retained["params"]["text"][1 - action_line as usize].as_str().unwrap().contains("pick"), "{retained}");
        shutdown_and_exit(&mut server); assert!(server.finish(Duration::from_secs(10)).success());
    }
}

#[test]
fn prepared_rebase_edit_pause_is_abortable_from_usable_caller() {
    let (tmp, _) = RepoFixture::new().commit("base", [("base", "base\n")]).commit("one", [("one", "1\n")]).commit("two", [("two", "2\n")]).build();
    let base = tmp.git(&["rev-parse", "HEAD~2"]).trim().to_owned();
    let original = (tmp.git(&["rev-parse", "HEAD"]), tmp.git(&["for-each-ref", "--format=%(refname) %(objectname)"]), tmp.git(&["write-tree"]), ["base", "one", "two"].map(|name| std::fs::read(tmp.path().join(name)).ok()));
    let mut server = TestServer::spawn(&[("GIT_EDITOR", "true")]);
    assert!(server.initialize_rich(json!({}))["error"].is_null());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{"viewId":view,"version":1,"key":key,"editorMode":"normal","cursor":{"line":line,"column":0}}}));
        server.response_with_id(id)
    };
    let pushed = |response: &Value| response["result"]["effects"].as_array().unwrap().iter().find(|e| e["type"] == "pushView").unwrap()["descriptor"]["id"].as_str().unwrap().to_owned();
    let opened = |server: &mut TestServer, view: &str| {
        let mut render = false; let mut keymap = false; let mut version = None;
        while !render || !keymap { let message = server.recv(); if message["params"]["viewId"] == view { if message["method"] == "view/render" { render = true; version = message["params"]["version"].as_u64(); } keymap |= message["method"] == "view/keymap"; } }
        server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view,"version":version.unwrap()}}));
    };
    let menu = pushed(&press(&mut server, 3, &status, "r", 0)); opened(&mut server, &menu);
    let selector = pushed(&press(&mut server, 4, &menu, "i", 0)); opened(&mut server, &selector);
    let mut base_line = None;
    for line in 0..100 { server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/invoke","params":{"viewId":selector,"command":"common.copy-value","version":1,"cursor":{"line":line,"column":0}}})); if server.response_with_id(5)["result"]["effects"].as_array().unwrap().iter().any(|e| e["type"] == "clipboard" && e["text"] == base) { base_line = Some(line); break; } }
    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/invoke","params":{"viewId":selector,"command":"common.confirm","version":1,"cursor":{"line":base_line.unwrap(),"column":0}}}));
    let todo = pushed(&server.response_with_id(6)); opened(&mut server, &todo);
    let edited = press(&mut server, 7, &todo, "e", 0); assert!(edited["result"]["effects"].as_array().unwrap().is_empty()); server.recv();
    let submitted = press(&mut server, 8, &todo, "ctrl+s", 0);
    assert!(submitted["result"]["effects"].as_array().unwrap().iter().any(|e| e["type"] == "popView" && e["viewId"] == todo), "{submitted}");
    assert!(tmp.path().join(".git/rebase-merge").exists());
    let caller_render = loop { let message = server.recv(); if message["method"] == "view/render" && message["params"]["viewId"] == status { break message; } };
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":status,"version":caller_render["params"]["version"]}}));
    server.send(json!({"jsonrpc":"2.0","id":9,"method":"view/invoke","params":{"viewId":status,"command":"rebase.abort","version":caller_render["params"]["version"]}}));
    let aborted = server.response_with_id(9); assert!(aborted["error"].is_null(), "{aborted}");
    assert_eq!((tmp.git(&["rev-parse", "HEAD"]), tmp.git(&["for-each-ref", "--format=%(refname) %(objectname)"]), tmp.git(&["write-tree"]), ["base", "one", "two"].map(|name| std::fs::read(tmp.path().join(name)).ok())), original);
    assert!(!tmp.path().join(".git/rebase-merge").exists());
    shutdown_and_exit(&mut server); assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn prepared_rebase_conflict_can_be_resolved_and_continued_from_caller() {
    let (tmp, _) = RepoFixture::new()
        .commit("base", [("file", "base\n")])
        .commit("first", [("file", "first\n")])
        .commit("second", [("file", "second\n")])
        .build();
    let base = tmp.git(&["rev-parse", "HEAD~2"]).trim().to_owned();
    let mut server = TestServer::spawn(&[("GIT_EDITOR", "true")]);
    assert!(server.initialize_rich(json!({}))["error"].is_null());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{"viewId":view,"version":1,"key":key,"editorMode":"normal","cursor":{"line":line,"column":0}}}));
        server.response_with_id(id)
    };
    let pushed = |response: &Value| response["result"]["effects"].as_array().unwrap().iter()
        .find(|effect| effect["type"] == "pushView").unwrap()["descriptor"]["id"].as_str().unwrap().to_owned();
    let opened = |server: &mut TestServer, view: &str| {
        let mut render = false; let mut keymap = false; let mut version = None;
        while !render || !keymap { let message = server.recv(); if message["params"]["viewId"] == view {
            if message["method"] == "view/render" { render = true; version = message["params"]["version"].as_u64(); }
            keymap |= message["method"] == "view/keymap";
        }}
        server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view,"version":version.unwrap()}}));
    };
    let menu = pushed(&press(&mut server, 3, &status, "r", 0)); opened(&mut server, &menu);
    let selector = pushed(&press(&mut server, 4, &menu, "i", 0)); opened(&mut server, &selector);
    let mut base_line = None;
    for line in 0..100 { server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/invoke","params":{"viewId":selector,"command":"common.copy-value","version":1,"cursor":{"line":line,"column":0}}})); if server.response_with_id(5)["result"]["effects"].as_array().unwrap().iter().any(|e| e["type"] == "clipboard" && e["text"] == base) { base_line = Some(line); break; } }
    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/invoke","params":{"viewId":selector,"command":"common.confirm","version":1,"cursor":{"line":base_line.unwrap(),"column":0}}}));
    let todo = pushed(&server.response_with_id(6)); opened(&mut server, &todo);
    let moved = press(&mut server, 7, &todo, "J", 0); assert!(moved["error"].is_null(), "{moved}"); server.recv();
    let submitted = press(&mut server, 8, &todo, "ctrl+s", 0);
    assert!(submitted["result"]["effects"].as_array().unwrap().iter().any(|e| e["type"] == "popView" && e["viewId"] == todo), "{submitted}");
    assert!(submitted["result"]["effects"].as_array().unwrap().iter().any(|e| e["type"] == "message" && !e["text"].as_str().unwrap_or_default().trim().is_empty()), "{submitted}");
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]).trim(), base);
    assert_eq!(tmp.git(&["status", "--porcelain"]), "UU file\n");
    let conflict = std::fs::read_to_string(tmp.path().join("file")).unwrap();
    assert!(conflict.starts_with("<<<<<<< HEAD\nbase\n=======\nsecond\n>>>>>>> "), "{conflict:?}");
    assert!(tmp.path().join(".git/rebase-merge").exists());
    let caller = loop { let message = server.recv(); if message["method"] == "view/render" && message["params"]["viewId"] == status { break message; } };
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":status,"version":caller["params"]["version"]}}));
    server.send(json!({"jsonrpc":"2.0","id":10,"method":"view/invoke","params":{"viewId":status,"command":"common.refresh"}}));
    let refreshed = server.response_with_id(10); assert!(refreshed["error"].is_null(), "{refreshed}");
    let caller = loop { let message = server.recv(); if message["method"] == "view/render" && message["params"]["viewId"] == status { break message; } };
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":status,"version":caller["params"]["version"]}}));
    for attempt in 0..3 {
        if !tmp.path().join(".git/rebase-merge").exists() { break; }
        std::fs::write(tmp.path().join("file"), "resolved\n").unwrap();
        tmp.git(&["add", "file"]);
        let id = 11 + attempt;
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/invoke","params":{"viewId":status,"command":"rebase.continue","version":caller["params"]["version"]}}));
        let continued = server.response_with_id(id); assert!(continued["error"].is_null(), "{continued}");
    }
    assert_eq!(tmp.git(&["log", "--format=%s", "--reverse"]), "base\nsecond\n");
    assert_eq!(tmp.git(&["show", "HEAD:file"]), "resolved\n");
    assert_eq!(tmp.git(&["status", "--porcelain"]), "");
    assert!(!tmp.path().join(".git/rebase-merge").exists());
    assert_eq!(tmp.git(&["write-tree"]).trim(), tmp.git(&["rev-parse", "HEAD^{tree}"]).trim());
    shutdown_and_exit(&mut server); assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn stale_prepared_rebase_submit_retains_editor_and_edits() {
    let (tmp, _) = RepoFixture::new()
        .commit("base", [("base", "base\n")])
        .commit("one", [("one", "1\n")])
        .commit("two", [("two", "2\n")])
        .build();
    let base = tmp.git(&["rev-parse", "HEAD~2"]).trim().to_owned();
    let mut server = TestServer::spawn(&[]);
    assert!(server.initialize_rich(json!({}))["error"].is_null());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let press = |server: &mut TestServer, id: i64, view: &str, key: &str, line: u64| {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key","params":{"viewId":view,"version":1,"key":key,"editorMode":"normal","cursor":{"line":line,"column":0}}}));
        server.response_with_id(id)
    };
    let pushed = |response: &Value| response["result"]["effects"].as_array().unwrap().iter()
        .find(|effect| effect["type"] == "pushView").unwrap()["descriptor"]["id"].as_str().unwrap().to_owned();
    let opened = |server: &mut TestServer, view: &str| {
        let mut render = false;
        let mut keymap = false;
        let mut version = None;
        while !render || !keymap { let message = server.recv(); if message["params"]["viewId"] == view {
            if message["method"] == "view/render" { render = true; version = message["params"]["version"].as_u64(); }
            keymap |= message["method"] == "view/keymap";
        }}
        server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view,"version":version.unwrap()}}));
    };
    let menu = pushed(&press(&mut server, 3, &status, "r", 0)); opened(&mut server, &menu);
    let selector = pushed(&press(&mut server, 4, &menu, "i", 0)); opened(&mut server, &selector);
    let mut base_line = None;
    for line in 0..100 {
        server.send(json!({"jsonrpc":"2.0","id":50,"method":"view/invoke","params":{"viewId":selector,"command":"common.copy-value","version":1,"cursor":{"line":line,"column":0}}}));
        if server.response_with_id(50)["result"]["effects"].as_array().unwrap().iter().any(|e| e["type"] == "clipboard" && e["text"] == base) { base_line = Some(line); break; }
    }
    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/invoke","params":{"viewId":selector,"command":"common.confirm","version":1,"cursor":{"line":base_line.unwrap(),"column":0}}}));
    let todo = pushed(&server.response_with_id(6)); opened(&mut server, &todo);
    press(&mut server, 7, &todo, "e", 0); server.recv();
    press(&mut server, 8, &todo, "J", 0); server.recv();
    press(&mut server, 9, &todo, "d", 2); server.recv();
    tmp.git(&["commit", "--allow-empty", "-m", "external"]);
    let external = tmp.git(&["rev-parse", "HEAD"]);
    let pre_dispatch = (tmp.git(&["rev-parse", "HEAD"]), tmp.git(&["status", "--porcelain=v2"]), std::fs::read(tmp.path().join(".git/index")).unwrap(), tmp.git(&["for-each-ref", "--format=%(refname) %(objectname)"]), ["base", "one", "two"].map(|name| std::fs::read(tmp.path().join(name)).ok()), tmp.path().join(".git/rebase-merge").exists(), tmp.path().join(".git/rebase-apply").exists());
    let stale = press(&mut server, 10, &todo, "ctrl+s", 0);
    assert!(stale["result"]["effects"].as_array().unwrap().iter().any(|e| e["type"] == "message" && e["text"].as_str().unwrap().contains("HEAD changed")), "{stale}");
    assert_eq!((tmp.git(&["rev-parse", "HEAD"]), tmp.git(&["status", "--porcelain=v2"]), std::fs::read(tmp.path().join(".git/index")).unwrap(), tmp.git(&["for-each-ref", "--format=%(refname) %(objectname)"]), ["base", "one", "two"].map(|name| std::fs::read(tmp.path().join(name)).ok()), tmp.path().join(".git/rebase-merge").exists(), tmp.path().join(".git/rebase-apply").exists()), pre_dispatch);
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), external);
    let retained = server.recv();
    assert_eq!(retained["method"], "view/render");
    assert_eq!(retained["params"]["viewId"], todo);
    assert_eq!(retained["params"]["text"].as_array().unwrap().len(), 2, "{retained}");
    assert!(retained["params"]["text"][1].as_str().unwrap().contains("edit"), "{retained}");
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":todo,"version":retained["params"]["version"]}}));
    for (line, revision) in [(0, "HEAD~1"), (1, "HEAD~2")] {
        let id = 20 + line as i64;
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/invoke","params":{"viewId":todo,"command":"common.copy-value","version":5,"cursor":{"line":line,"column":0}}}));
        let copied = server.response_with_id(id);
        let oid = tmp.git(&["rev-parse", revision]);
        assert!(copied["result"]["effects"].as_array().unwrap().iter().any(|effect| effect["type"] == "clipboard" && effect["text"] == oid.trim()), "{copied}");
    }
    let cancel = press(&mut server, 11, &todo, "q", 0);
    assert!(cancel["result"]["effects"].as_array().unwrap().iter().any(|e| e["type"] == "popView"));
    assert!(cancel["result"]["effects"].as_array().unwrap().iter().any(|e| e["type"] == "focusView" && e["viewId"] == status));
    server.send(json!({"jsonrpc":"2.0","id":12,"method":"view/invoke","params":{"viewId":status,"command":"common.refresh"}}));
    let refreshed = server.response_with_id(12);
    assert!(refreshed["result"]["effects"].as_array().unwrap().is_empty(), "{refreshed}");
    let caller_render = server.recv();
    assert_eq!(caller_render["method"], "view/render");
    assert_eq!(caller_render["params"]["viewId"], status);
    shutdown_and_exit(&mut server); assert!(server.finish(Duration::from_secs(10)).success());
}

fn limited_rich(effects: Value, prompts: Value) -> Value {
    json!({"experimental":{"magit":{
        "v":1,"keys":"dynamic","modifiers":"full",
        "layout":["main"],"effects":effects,"prompts":prompts
    }}})
}

fn invoke_rich(server: &mut TestServer, id: i64, view: &str, command: &str) -> Value {
    server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/invoke",
        "params":{"viewId":view,"command":command}}));
    server.response_with_id(id)
}

#[test]
fn rich_acknowledged_edits_reflow_real_menu() {
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
    let mut version = 1;
    // Acknowledged versions are the base for subsequent edits.
    for cols in [60, 140] {
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
            include_str!("snapshots/menu-60.txt")
        } else {
            include_str!("snapshots/menu-140.txt")
        };
        assert_eq!(text.join("\n"), expected.trim_end());
        version += 1;
    }
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn rich_server_folds_and_acknowledged_provider_across_visibility_and_refresh() {
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
    let initial = server.recv();
    assert_eq!(initial["method"], "view/render");
    assert_eq!(server.recv()["method"], "view/keymap");
    let initial_text = initial["params"]["text"].clone();
    let mut text: Vec<String> = serde_json::from_value(initial_text.clone()).unwrap();
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
    let mut version = 1;
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
        version += 1;
    }
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn rich_basic_keys_filter_delivery_and_matching_without_disabling_explicit_menu_actions() {
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
        assert_eq!(server.recv()["method"], "view/render");
        let keymap = server.recv();
        assert_eq!(keymap["method"], "view/keymap");
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
        server.send(
            json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view,"version":1}}),
        );
        server.send(json!({"jsonrpc":"2.0","id":7,"method":"view/key",
            "params":{"viewId":view,"version":1,"key":"ctrl+alt+r","editorMode":"normal","cursor":{"line":0,"column":0}}}));
        assert_eq!(server.response_with_id(7)["result"]["effects"], json!([]));
        let refreshed = server.recv();
        assert_eq!(refreshed["method"], "view/render");
        assert_eq!(
            refreshed["params"]["version"], 2,
            "basic retains ctrl/alt resolution"
        );
        server.send(
            json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view,"version":2}}),
        );
        if modifiers == "basic" {
            for (id, key) in [(8, "ctrl+shift+Tab"), (9, "k")] {
                server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/key",
                    "params":{"viewId":view,"version":2,"key":key,"editorMode":"normal","cursor":{"line":0,"column":0}}}));
                let response = server.response_with_id(id);
                assert_eq!(
                    response["result"]["effects"],
                    json!([{"type":"passThrough","key":key}]),
                    "filtered chords must not leave pending prefixes: {response}"
                );
            }
            server.send(json!({"jsonrpc":"2.0","id":10,"method":"view/key",
                "params":{"viewId":view,"version":2,"key":"shift+Tab","editorMode":"normal","cursor":{"line":0,"column":0}}}));
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
        if message["method"] == json!("view/render")
            && message["params"]["viewId"] != json!(menu)
        {
            assert!(message["params"]["text"].is_array(), "full prompt text");
            let view_id = message["params"]["viewId"].as_str().unwrap().to_owned();
            break (view_id.clone(), format!("magit://prompt/{view_id}"), message);
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
    assert_eq!(initial["params"]["text"][0], json!("Override the author (--author=)"));
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
        !quiet.iter().any(|message| message["params"]["effects"]
            .as_array()
            .is_some_and(|effects| effects
                .iter()
                .any(|effect| effect["type"] == json!("editDocument")))),
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

    assert!(!server.notifications.iter().any(|message| {
        message["method"] == json!("view/prompt")
    }));
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn rich_missing_confirm_opens_a_document_prompt_without_mutation() {
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
        if message["method"] == json!("view/render")
            && message["params"]["viewId"] != json!(view)
        {
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
        if message["method"] == json!("view/render")
            && message["params"]["viewId"] != json!(menu)
        {
            let view_id = message["params"]["viewId"].as_str().unwrap().to_owned();
            break (view_id.clone(), format!("magit://prompt/{view_id}"), message);
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
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", "initial\n")])
        .modify("f.txt", "changed\n")
        .build();
    let mut server = TestServer::spawn(&[]);
    server.initialize_rich(json!({}));
    server.send(open_view_message(2, &repo_param(tmp.path())));
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"].as_str().unwrap();
    let uri = opened["result"]["uri"].as_str().unwrap();
    server.send(
        json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view,"version":1}}),
    );
    // Resolve the request position from the server's structured outline.
    let line = first_hunk_line(&mut server, uri);
    for (id, command, effect) in [
        (3, "common.copy-value", "clipboard"),
        (4, "common.visit", "visitFile"),
    ] {
        server.send(json!({"jsonrpc":"2.0","id":id,"method":"view/invoke",
            "params":{"viewId":view,"version":1,"command":command,"cursor":{"line":line,"column":0}}}));
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

/// Open a status view and consume its initial render and keymap pushes.
fn open_view(server: &mut TestServer, id: i64, repo: &Value) -> String {
    server.send(open_view_message(id, repo));
    let opened = server.response_with_id(id);
    let view_id = opened["result"]["id"]
        .as_str()
        .expect("view id")
        .to_string();
    let render = server.recv();
    assert_eq!(render["method"], json!("view/render"), "{render}");
    let keymap = server.recv();
    assert_eq!(keymap["method"], json!("view/keymap"), "{keymap}");
    view_id
}

/// A `view/open` request for a status view of `repo`.
fn open_view_message(id: i64, repo: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "view/open",
        "params": {
            "kind": "status",
            "args": {"repo_path": repo},
            "viewport": {"cols": 80, "rows": 24},
        },
    })
}

/// The 0-based cursor line of the first hunk, resolved from the server's own
/// outline. Tests never recover structure by matching rendered line shapes.
fn first_hunk_line(server: &mut TestServer, uri: &str) -> u64 {
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 90,
        "method": "textDocument/documentSymbol",
        "params": {"textDocument": {"uri": uri}},
    }));
    let response = server.response_with_id(90);
    let symbols: Vec<lsp_types::DocumentSymbol> =
        serde_json::from_value(response["result"].clone())
            .unwrap_or_else(|error| panic!("documentSymbol returns symbols: {error}: {response}"));
    hunk_symbol_line(&symbols).unwrap_or_else(|| panic!("no hunk symbol: {response}"))
}

/// The selection line of the first hunk in an outline, at any depth.
fn hunk_symbol_line(symbols: &[lsp_types::DocumentSymbol]) -> Option<u64> {
    symbols.iter().find_map(|symbol| {
        if symbol.kind == lsp_types::SymbolKind::METHOD {
            return Some(u64::from(symbol.selection_range.start.line));
        }
        symbol.children.as_deref().and_then(hunk_symbol_line)
    })
}

fn shutdown_and_exit(server: &mut TestServer) {
    server.send(json!({"jsonrpc": "2.0", "id": 99, "method": "shutdown", "params": null}));
    let response = server.response_with_id(99);
    assert_eq!(response["result"], Value::Null);
    server.send(json!({"jsonrpc": "2.0", "method": "exit", "params": null}));
}

#[test]
fn repository_configuration_is_trusted_isolated_canonical_and_reloaded() {
    let (first, _) = init_repo();
    let (second, _) = init_repo();
    std::fs::create_dir(first.path().join("subdir")).unwrap();
    for (root, key) in [(first.path(), "z"), (second.path(), "Z")] {
        std::fs::write(root.join(".magit-lsp.toml"), format!("profile = \"standard\"\ntrust_repo_config = false\n[diff]\ncontext = 11\n[keys.status]\nstage = [\"{key}\"]\n")).unwrap();
    }
    let user = first.path().join("user.toml");
    std::fs::write(&user, "trust_repo_config = true\n").unwrap();
    for trusted in [false, true] {
        let mut server = TestServer::spawn(&[("MAGIT_LSP_CONFIG", user.to_str().unwrap())]);
        let mut init = initialize_message();
        init["params"]["initializationOptions"] = json!({
            "trust_repo_config": trusted, "keys": {"status": {"stage": ["Q"]}}
        });
        server.send(init);
        assert!(server.response_with_id(1).get("error").is_none());
        let mut views = Vec::new();
        for (id, root, expected) in [
            (2, first.path().to_path_buf(), "z"),
            (3, second.path().to_path_buf(), "Z"),
            (4, first.path().join("subdir"), "z"),
        ] {
            server.send(open_view_message(id, &repo_param(&root)));
            let response = server.response_with_id(id);
            let view = response["result"]["id"].as_str().unwrap().to_string();
            assert_eq!(server.recv()["method"], "view/render");
            let keymap = server.recv();
            assert_stage_binding(&keymap, if trusted { expected } else { "Q" });
            views.push(view);
        }
        if trusted {
            // A forced reload must update both aliases of the first root,
            // without leaking that root's bindings into the second session.
            std::fs::write(
                first.path().join(".magit-lsp.toml"),
                "[keys.status]\nstage = [\"X\"]\n",
            )
            .unwrap();
            server.send(
                json!({"jsonrpc":"2.0", "method":"workspace/didChangeConfiguration", "params":{}}),
            );
            let mut seen = std::collections::HashSet::new();
            while seen.len() < 3 {
                let notice = server.recv();
                if notice["method"] != "view/keymap" {
                    continue;
                }
                let view = notice["params"]["viewId"].as_str().unwrap();
                let expected = if view == views[1] { "Z" } else { "X" };
                assert_stage_binding(&notice, expected);
                seen.insert(view.to_string());
            }
            server.notifications.clear();
            std::fs::write(first.path().join(".magit-lsp.toml"), "not valid toml").unwrap();
            std::fs::write(
                second.path().join(".magit-lsp.toml"),
                "[keys.status]\nstage = [\"Y\"]\n",
            )
            .unwrap();
            server.send(
                json!({"jsonrpc":"2.0", "method":"workspace/didChangeConfiguration", "params":{}}),
            );
            server.send(json!({"jsonrpc":"2.0", "id":10, "method":"view/resync", "params":{"viewId":views[0]}}));
            assert!(server.response_with_id(10).get("error").is_none());
            server.drain_notifications();
            assert!(
                server.notifications.iter().any(|notice| {
                    notice["method"] == "window/showMessage"
                        && notice["params"]["message"]
                            .as_str()
                            .is_some_and(|text| text.contains(".magit-lsp.toml"))
                }),
                "missing readable configuration error: {:?}",
                server.notifications
            );
            for view in &views {
                let notice = server
                    .notifications
                    .iter()
                    .rev()
                    .find(|notice| {
                        notice["method"] == "view/keymap" && notice["params"]["viewId"] == *view
                    })
                    .expect("reloaded root keymap");
                assert_stage_binding(notice, if *view == views[1] { "Y" } else { "X" });
            }
        }
        shutdown_and_exit(&mut server);
    }
}

fn assert_stage_binding(notice: &Value, expected: &str) {
    assert_eq!(notice["method"], "view/keymap", "{notice}");
    let binding = notice["params"]["bindings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|binding| binding["command"] == "status.stage")
        .expect("stage binding");
    assert_eq!(binding["keys"], json!([expected]), "{notice}");
}

/// User-close requests must remove nested views from routing and parent order.
#[test]
fn nested_user_close_and_reopen_does_not_retain_views_or_parents() {
    let (tmp, _repo) = init_repo();
    let (other_tmp, _other_repo) = init_repo();
    let mut server = TestServer::spawn(&[]);
    server.initialize(json!({}));
    let root = open_view(&mut server, 2, &repo_param(tmp.path()));
    let other = open_view(&mut server, 3, &repo_param(other_tmp.path()));
    let mut request_id = 10;
    let mut previous_process = None;
    let mut previous_menu = None;
    for _ in 0..3 {
        let mut parent = root.clone();
        let mut nested = Vec::new();
        for command in ["open.process-log", "open.dispatch"] {
            request_id += 1;
            server.send(json!({"jsonrpc": "2.0", "id": request_id,
                "method": "view/invoke", "params": {"viewId": parent, "command": command}}));
            let response = server.response_with_id(request_id);
            let descriptor = response["result"]["effects"]
                .as_array()
                .expect("effects")
                .iter()
                .find(|effect| effect["type"] == "pushView")
                .expect("nested push")["descriptor"]
                .clone();
            parent = descriptor["id"].as_str().expect("nested id").to_string();
            nested.push(parent.clone());
            assert_eq!(server.recv()["method"], "view/render");
            assert_eq!(server.recv()["method"], "view/keymap");
        }
        if let Some(previous) = &previous_process {
            assert_eq!(&nested[0], previous, "closed process id must be reusable");
            assert_eq!(
                Some(&nested[1]),
                previous_menu.as_ref(),
                "closed menu id must be reusable"
            );
        }
        previous_process = Some(nested[0].clone());
        previous_menu = Some(nested[1].clone());
        // User closes the deepest view; reopening it must return to the live
        // process parent, not an orphan left in the server's stack.
        for id in nested.iter().rev() {
            request_id += 1;
            server.send(json!({"jsonrpc": "2.0", "id": request_id,
                "method": "view/close", "params": {"viewId": id}}));
            let closed = server.response_with_id(request_id);
            assert_eq!(closed["result"], Value::Null, "{closed}");
            assert!(closed.get("error").is_none(), "{closed}");
            request_id += 1;
            server.send(json!({"jsonrpc": "2.0", "id": request_id,
                "method": "view/resync", "params": {"viewId": id}}));
            let stale = server.response_with_id(request_id);
            assert!(
                stale["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("unknown view"),
                "{stale}"
            );
        }
        // A normal server-driven pop after reopening must focus the root,
        // proving the removed nested entries no longer affect parent routing.
        request_id += 1;
        server.send(json!({"jsonrpc": "2.0", "id": request_id,
            "method": "view/invoke", "params": {"viewId": root, "command": "open.process-log"}}));
        let opened = server.response_with_id(request_id);
        let id = opened["result"]["effects"][0]["descriptor"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(id, nested[0]);
        assert_eq!(server.recv()["method"], "view/render");
        assert_eq!(server.recv()["method"], "view/keymap");
        request_id += 1;
        server.send(json!({"jsonrpc": "2.0", "id": request_id,
            "method": "view/invoke", "params": {"viewId": id, "command": "common.quit"}}));
        let popped = server.response_with_id(request_id);
        assert!(
            popped["result"]["effects"]
                .as_array()
                .unwrap()
                .iter()
                .any(|effect| effect["type"] == "focusView" && effect["viewId"] == root),
            "{popped}"
        );
        for live in [&root, &other] {
            request_id += 1;
            server.send(json!({"jsonrpc": "2.0", "id": request_id,
                "method": "view/resync", "params": {"viewId": live}}));
            let response = server.response_with_id(request_id);
            assert_eq!(response["result"]["viewId"], *live, "{response}");
        }
    }
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

fn process_exists(pid: u32) -> bool {
    let output = Command::new("ps")
        .args(["-o", "pid=", "-p", &pid.to_string()])
        .output()
        .expect("run ps");
    !String::from_utf8_lossy(&output.stdout).trim().is_empty()
}

#[test]
fn pipelined_and_split_messages_are_framed_and_matched_by_id() {
    let (tmp, _repo) = init_repo();
    let repo = repo_param(tmp.path());
    let mut server = TestServer::spawn(&[]);

    // Several messages in one write: initialize, initialized and a request.
    let mut batch = Vec::new();
    batch.extend(frame(initialize_message()));
    batch.extend(frame(
        json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}),
    ));
    batch.extend(frame(open_view_message(2, &repo)));
    server.send_raw(&batch);

    let initialize = server.response_with_id(1);
    assert_eq!(
        initialize["result"]["capabilities"]["foldingRangeProvider"],
        json!(true)
    );
    let opened = server.response_with_id(2);
    assert_eq!(opened["result"]["kind"], json!("status"));

    // One request split across two writes.
    let split = frame(open_view_message(3, &repo));
    let midpoint = split.len() / 2;
    server.send_raw(&split[..midpoint]);
    thread::sleep(Duration::from_millis(50));
    server.send_raw(&split[midpoint..]);

    let opened = server.response_with_id(3);
    assert_eq!(opened["result"]["kind"], json!("status"));

    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
}

/// Reads must stay responsive while a mutation is blocked inside real Git.
///
/// `view/resync` answers from the session's published snapshot, so it returns
/// the latest pre-mutation text without waiting for the session lock that the
/// blocked command holds.
#[test]
fn a_mutation_blocked_in_git_does_not_delay_a_later_read() {
    let (tmp, _repo) = init_repo();
    write(&tmp.path().join("f.txt"), "line1\nCHANGED\nline3\n");
    let repo = repo_param(tmp.path());

    let gate = GatedGit::arm("add");
    let mut server = spawn_with_env(&gate.child_env());
    server.auto_respond_to_progress_create();
    server.initialize(json!({"window":{"workDoneProgress":true}}));
    let view_id = open_view(&mut server, 2, &repo);

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "view/invoke",
        "params": {"viewId": view_id, "command": "status.stage-all"},
    }));
    // The mutation owns its token and has announced it before entering Git.
    loop {
        let message = server.recv();
        assert!(
            message["id"] != json!(3),
            "the gated mutation finished without the gate: {message}"
        );
        if message["method"] == json!("$/progress")
            && message["params"]["value"]["kind"] == json!("begin")
        {
            break;
        }
    }
    gate.wait_entered();

    // Submitted while the first mutation is verifiably blocked inside Git.
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "view/resync",
        "params": {"viewId": view_id},
    }));

    // The read must answer within a bounded window while the gate is closed.
    let started = Instant::now();
    server.recv_timeout = Duration::from_secs(3);
    let mut read = None;
    while read.is_none() {
        let message = server.recv();
        if message["id"] == json!(4) {
            read = Some(message);
        }
    }
    let read = read.expect("the read answered within the window");
    let answered_at = started.elapsed();
    assert!(
        !gate.is_released(),
        "the read answered only after the gate was released, after {answered_at:?}"
    );
    assert_eq!(
        read["result"]["viewId"],
        json!(view_id),
        "the read response must carry the resynced view: {read}"
    );
    assert!(
        read["result"]["text"]
            .as_array()
            .is_some_and(|text| !text.is_empty()),
        "the resync must carry the full text: {read}"
    );

    // Release and let the mutation finish so the test always cleans up.
    gate.release();
    let staged = server.response_with_id(3);
    assert_eq!(staged["result"]["effects"], json!([]), "{staged}");
    assert!(hunk_count(&cached_diff(tmp.path())) >= 1, "staged");

    server.send(json!({"jsonrpc":"2.0","id":8,"method":"view/close","params":{"viewId":view_id}}));
    let _ = server.response_with_id(8);
    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
}

#[test]
fn blocked_repository_does_not_serialize_unrelated_repository_operations() {
    let (repo_a_tmp, _) = init_repo();
    let (repo_b_tmp, _) = init_repo();
    write(&repo_a_tmp.path().join("f.txt"), "changed A\n");
    write(&repo_b_tmp.path().join("f.txt"), "changed B\n");
    assert!(std::process::Command::new("git")
        .args(["add", "-u"])
        .current_dir(repo_b_tmp.path())
        .status()
        .unwrap()
        .success());
    let repo_a = repo_param(repo_a_tmp.path());
    let repo_b = repo_param(repo_b_tmp.path());
    let gate = GatedGit::arm("add");
    let mut server = spawn_with_env(&gate.child_env());
    server.auto_respond_to_progress_create();
    server.initialize(json!({"window":{"workDoneProgress":true}}));
    let a = open_view(&mut server, 2, &repo_a);
    let b = open_view(&mut server, 3, &repo_b);

    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/invoke",
        "params":{"viewId":a,"command":"status.stage-all"}}));
    loop {
        let message = server.recv();
        assert_ne!(
            message["id"],
            json!(4),
            "A finished before entering the Git gate: {message}"
        );
        if message["method"] == json!("$/progress")
            && message["params"]["value"]["kind"] == json!("begin")
        {
            break;
        }
    }
    gate.wait_entered();

    // A read touching B's session must not wait on the registry or A's Git.
    server.send(json!({"jsonrpc":"2.0","method":"view/resize",
        "params":{"viewId":b,"cols":90,"rows":30}}));
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/resync","params":{"viewId":b}}));
    server.recv_timeout = Duration::from_secs(3);
    let result_b = server.response_with_id(5);
    assert_eq!(result_b["result"]["viewId"], json!(b), "{result_b}");
    assert!(
        !gate.is_released(),
        "B responded only after A's gate was released"
    );

    // Positional reads must not scan and lock A's session before reaching B.
    let uri_b = magit_lsp::session::status_uri(&repo_b_tmp.path().to_string_lossy());
    let _ = first_hunk_line(&mut server, &uri_b);
    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/invoke",
        "params":{"viewId":b,"command":"status.unstage-all"}}));
    let staged_b = server.response_with_id(6);
    assert_eq!(staged_b["result"]["effects"], json!([]), "{staged_b}");
    assert!(cached_diff(repo_b_tmp.path()).is_empty());
    assert!(!gate.is_released(), "B's mutation waited for A");

    gate.release();
    let result_a = server.response_with_id(4);
    assert_eq!(result_a["result"]["effects"], json!([]), "{result_a}");
    server.send(json!({"jsonrpc":"2.0","id":8,"method":"view/close","params":{"viewId":a}}));
    let _ = server.response_with_id(8);
    server.send(json!({"jsonrpc":"2.0","id":9,"method":"view/close","params":{"viewId":b}}));
    let _ = server.response_with_id(9);
    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
}

#[test]
fn outbound_request_responses_are_correlated_without_blocking_intake() {
    let (tmp, _repo) = init_repo();
    write(&tmp.path().join("f.txt"), "line1\nCHANGED\nline3\n");
    let mut server = spawn_with_env(&[]);
    server.initialize(json!({"window":{"workDoneProgress":true}}));
    let view = open_view(&mut server, 2, &repo_param(tmp.path()));
    server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/invoke","params":{"viewId":view,"command":"status.stage-all"}}));

    let create = loop {
        let message = server.recv();
        if message["method"] == "window/workDoneProgress/create" {
            break message;
        }
    };
    let outbound_id = create["id"].clone();
    assert!(outbound_id.is_string() || outbound_id.is_number());

    // The reader continues serving ordinary requests while the client holds
    // the server request open.
    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/resync","params":{"viewId":view}}));
    assert_eq!(server.response_with_id(4)["result"]["viewId"], view);
    // The delayed response exercises correlation and the remote-error path.
    server.send(
        json!({"jsonrpc":"2.0","id":outbound_id,"error":{"code":-32000,"message":"declined"}}),
    );

    // A duplicate/unknown response is ignored rather than completing another
    // request or being misrouted as a client request.
    server.send(json!({"jsonrpc":"2.0","id":outbound_id,"result":null}));
    server.send(json!({"jsonrpc":"2.0","id":"unknown-outbound-id","result":null}));
    assert_eq!(server.response_with_id(3)["result"]["effects"], json!([]));
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn absent_or_false_progress_capability_emits_no_create_or_progress() {
    for capabilities in [json!({}), json!({"window":{"workDoneProgress":false}})] {
        let (tmp, _) = init_repo();
        write(&tmp.path().join("f.txt"), "line1\nCHANGED\nline3\n");
        let gate = GatedGit::arm("add");
        let mut server = spawn_with_env(&gate.child_env());
        server.initialize(capabilities);
        let view = open_view(&mut server, 2, &repo_param(tmp.path()));
        server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/invoke","params":{"viewId":view,"command":"status.stage-all"}}));
        gate.wait_entered();
        server
            .send(json!({"jsonrpc":"2.0","id":4,"method":"view/resync","params":{"viewId":view}}));
        assert_eq!(server.response_with_id(4)["result"]["viewId"], view);
        assert!(!gate.is_released());
        gate.release();
        assert_eq!(server.response_with_id(3)["result"]["effects"], json!([]));
        shutdown_and_exit(&mut server);
        assert!(
            !server
                .notifications
                .iter()
                .any(|m| progress_event(m).is_some()),
            "{:?}",
            server.notifications
        );
        assert!(hunk_count(&cached_diff(tmp.path())) >= 1);
        assert!(server.finish(Duration::from_secs(10)).success());
    }
}

#[test]
fn delayed_failed_unanswered_and_completed_progress_create_keep_intake_responsive() {
    for reply in ["success", "error", "never", "after-completion"] {
        let (tmp, _) = init_repo();
        write(&tmp.path().join("f.txt"), "line1\nCHANGED\nline3\n");
        let gate = GatedGit::arm("add");
        let mut server = spawn_with_env(&gate.child_env());
        // Raw receive must permit withholding even with the convenience pump on.
        server.auto_respond_to_progress_create();
        server.initialize(json!({"window":{"workDoneProgress":true}}));
        let view = open_view(&mut server, 2, &repo_param(tmp.path()));
        server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/invoke","params":{"viewId":view,"command":"status.stage-all"}}));
        let create = loop {
            let message = server.recv_raw();
            assert!(
                message["method"] != "$/progress",
                "begin before create success: {message}"
            );
            if message["method"] == "window/workDoneProgress/create" {
                break message;
            }
            server.notifications.push(message);
        };
        gate.wait_entered();
        server
            .send(json!({"jsonrpc":"2.0","id":4,"method":"view/resync","params":{"viewId":view}}));
        assert_eq!(server.response_with_id(4)["result"]["viewId"], view);
        assert!(!gate.is_released());
        assert!(
            !server.sent.iter().any(|m| m["id"] == create["id"]),
            "raw receive replied"
        );
        assert!(!server
            .notifications
            .iter()
            .any(|m| m["method"] == "$/progress"));
        match reply {
            "success" => {
                server.send(json!({"jsonrpc":"2.0","id":create["id"],"result":null}));
                let begin = server.recv();
                assert_eq!(progress_event(&begin), Some((create["params"]["token"].as_str().unwrap().into(), "begin".into())));
                server.notifications.push(begin);
            }
            "error" => server.send(json!({"jsonrpc":"2.0","id":create["id"],"error":{"code":-32000,"message":"declined"}})),
            _ => {}
        }
        gate.release();
        assert_eq!(server.response_with_id(3)["result"]["effects"], json!([]));
        if reply == "after-completion" {
            server.send(json!({"jsonrpc":"2.0","id":create["id"],"result":null}));
        }
        shutdown_and_exit(&mut server);
        let phases: Vec<_> = server
            .notifications
            .iter()
            .filter_map(progress_event)
            .map(|(_, phase)| phase)
            .collect();
        assert_eq!(
            phases,
            if reply == "success" {
                vec!["begin", "end"]
            } else {
                vec![]
            },
            "{reply}"
        );
        assert!(hunk_count(&cached_diff(tmp.path())) >= 1, "{reply}");
        assert!(server.finish(Duration::from_secs(10)).success());
    }
}

/// The `(token, phase)` of a work-done progress message, if it is one.
///
/// Records every phase of the LSP work-done handshake: `create`, then the
/// `$/progress` kinds `begin` and `end`, each with its token.
fn progress_event(message: &Value) -> Option<(String, String)> {
    let token = message["params"]["token"].as_str()?.to_string();
    match message["method"].as_str()? {
        "window/workDoneProgress/create" => Some((token, "create".to_string())),
        "$/progress" => Some((
            token,
            message["params"]["value"]["kind"].as_str()?.to_string(),
        )),
        _ => None,
    }
}

#[test]
fn mutations_for_one_repository_apply_in_request_order() {
    let (tmp, _repo) = init_repo();
    write(&tmp.path().join("f.txt"), "line1\nCHANGED\nline3\n");
    let repo = repo_param(tmp.path());

    // The first `git add` blocks inside real Git until the test releases it,
    // so the second mutation is provably submitted while the first one runs.
    let gate = GatedGit::arm("add");
    let mut server = spawn_with_env(&gate.child_env());
    server.auto_respond_to_progress_create();
    let response = server.initialize(json!({"window":{"workDoneProgress":true}}));
    assert_eq!(
        response["result"]["capabilities"]["foldingRangeProvider"],
        json!(true),
        "initialize response: {response}"
    );

    let view_id = open_view(&mut server, 2, &repo);

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "view/invoke",
        "params": {"viewId": view_id, "command": "status.stage-all"},
    }));

    // Every progress token and phase, in stream order.
    let mut progress: Vec<(String, String)> = Vec::new();
    let first_token = loop {
        let message = server.recv();
        assert!(
            message["id"] != json!(3),
            "the gated mutation finished without the gate: {message}"
        );
        match progress_event(&message) {
            Some(event) => {
                progress.push(event.clone());
                if event.1 == "begin" {
                    break event.0;
                }
            }
            None => {}
        }
    };
    // The first mutation is now verifiably blocked inside real Git.
    gate.wait_entered();

    // Submit the second mutation while the first one is still inside Git.
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "view/invoke",
        "params": {"viewId": view_id, "command": "status.unstage-all"},
    }));

    // The gate is the only thing holding the first mutation back, so release
    // it before reading further: the second mutation may not run until the
    // first one has finished.
    gate.release();

    let mut staged = None;
    let mut unstaged = None;
    while staged.is_none() || unstaged.is_none() {
        let message = server.recv();
        match message["id"].as_i64() {
            Some(3) => staged = Some(message),
            Some(4) => unstaged = Some(message),
            _ => {
                if let Some(event) = progress_event(&message) {
                    progress.push(event);
                }
            }
        }
    }

    // Ordering: the whole first handshake, then the whole second one. In
    // particular the second mutation's `begin` follows the first's `end`.
    let phases: Vec<String> = progress.iter().map(|(_, phase)| phase.clone()).collect();
    assert_eq!(
        phases,
        vec![
            "create".to_string(),
            "begin".to_string(),
            "end".to_string(),
            "create".to_string(),
            "begin".to_string(),
            "end".to_string(),
        ],
        "the first mutation's progress must end before the second begins: {progress:?}"
    );
    let first_end = &progress[2].0;
    let second_begin = &progress[4].0;
    assert_eq!(progress[0].0, first_token, "{progress:?}");
    assert_eq!(progress[1].0, first_token, "{progress:?}");
    assert_eq!(first_end, &first_token, "{progress:?}");
    assert_ne!(
        second_begin, &first_token,
        "the second mutation must carry its own token: {progress:?}"
    );
    assert_eq!(progress[3].0, *second_begin, "{progress:?}");
    assert_eq!(progress[5].0, *second_begin, "{progress:?}");

    let staged = staged.expect("stage response");
    assert_eq!(staged["result"]["effects"], json!([]), "{staged}");
    let unstaged = unstaged.expect("unstage response");
    assert_eq!(unstaged["result"]["effects"], json!([]), "{unstaged}");

    // The unstage ran after the stage, so the index is empty again.
    assert_eq!(cached_diff(tmp.path()), "");
    assert!(read(&tmp.path().join("f.txt")).contains("CHANGED"));

    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
}

#[test]
fn shutdown_then_exit_terminates_zero_without_an_orphan() {
    let mut server = TestServer::spawn(&[]);
    let response = server.initialize(json!({}));
    assert_eq!(
        response["result"]["capabilities"]["foldingRangeProvider"],
        json!(true),
        "initialize response: {response}"
    );
    let pid = server.pid();

    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert_eq!(status.code(), Some(0), "server exited: {status:?}");
    assert!(!process_exists(pid), "magit-lsp {pid} is still running");
}

#[test]
fn exit_without_shutdown_terminates_with_code_one() {
    let mut server = TestServer::spawn(&[]);
    let response = server.initialize(json!({}));
    assert_eq!(
        response["result"]["capabilities"]["foldingRangeProvider"],
        json!(true),
        "initialize response: {response}"
    );

    server.send(json!({"jsonrpc": "2.0", "method": "exit", "params": null}));
    let status = server.finish(Duration::from_secs(10));
    assert_eq!(status.code(), Some(1), "server exited: {status:?}");
}

#[test]
fn unknown_requests_reply_method_not_found() {
    let mut server = TestServer::spawn(&[]);
    let response = server.initialize(json!({}));
    assert_eq!(
        response["result"]["capabilities"]["foldingRangeProvider"],
        json!(true),
        "initialize response: {response}"
    );

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "magit/nope",
        "params": {},
    }));
    let response = server.response_with_id(7);
    assert_eq!(response["error"]["code"], json!(-32601), "{response}");

    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
}

#[test]
fn view_open_then_invoke_stage_round_trips_over_stdio() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let repo = repo_param(tmp.path());
    let mut server = TestServer::spawn(&[]);
    let response = server.initialize(json!({}));
    assert_eq!(
        response["result"]["capabilities"]["foldingRangeProvider"],
        json!(true),
        "initialize response: {response}"
    );

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "view/open",
        "params": {
            "kind": "status",
            "args": {"repo_path": repo},
            "viewport": {"cols": 80, "rows": 24},
        },
    }));
    let opened = server.response_with_id(2);
    let view_id = opened["result"]["id"]
        .as_str()
        .expect("view id")
        .to_string();
    assert_eq!(opened["result"]["kind"], json!("status"));
    let uri = opened["result"]["uri"]
        .as_str()
        .expect("view uri")
        .to_string();
    assert_eq!(
        opened["result"]["uri"],
        json!(format!(
            "magit://status/{}",
            tmp.path().to_string_lossy().trim_start_matches('/')
        ))
    );

    let initial = server.recv();
    assert_eq!(initial["method"], json!("view/render"), "{initial}");
    assert_eq!(initial["params"]["viewId"], json!(view_id));
    assert_eq!(initial["params"]["version"], json!(1));
    assert!(initial["params"]["baseVersion"].is_null());
    assert!(initial["params"]["text"].is_array());
    let line = first_hunk_line(&mut server, &uri);

    server.send(json!({
        "jsonrpc": "2.0",
        "method": "view/rendered",
        "params": {"viewId": view_id, "version": 1},
    }));
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "view/invoke",
        "params": {
            "viewId": view_id,
            "command": "status.stage",
            "version": 1,
            "cursor": {"line": line, "column": 0},
        },
    }));

    let mut response = None;
    let mut render = None;
    while response.is_none() || render.is_none() {
        let message = server.recv();
        if message["id"] == json!(3) {
            response = Some(message);
        } else if message["method"] == json!("view/render") {
            render = Some(message);
        }
    }
    let response = response.expect("invoke response");
    let render = render.expect("render notification");
    assert_eq!(response["result"]["effects"], json!([]));
    assert_eq!(render["params"]["viewId"], json!(view_id));
    assert_eq!(render["params"]["version"], json!(2));
    assert_eq!(render["params"]["baseVersion"], json!(1));
    assert!(render["params"]["text"].is_null());
    assert!(!render["params"]["edits"]
        .as_array()
        .expect("edits")
        .is_empty());
    assert!(!render["params"]["cursor"].is_null());
    assert!(hunk_count(&cached_diff(tmp.path())) >= 1);

    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
}

#[test]
fn view_resize_notification_reflows_an_acked_menu_incrementally() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let repo = repo_param(tmp.path());
    let mut server = TestServer::spawn(&[]);
    let response = server.initialize(json!({}));
    assert_eq!(
        response["result"]["capabilities"]["foldingRangeProvider"],
        json!(true),
        "initialize response: {response}"
    );

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "view/open",
        "params": {
            "kind": "status",
            "args": {"repo_path": repo},
            "viewport": {"cols": 140, "rows": 30},
        },
    }));
    let opened = server.response_with_id(2);
    let status_id = opened["result"]["id"].as_str().expect("status view id");
    let initial_status = server.recv();
    assert_eq!(initial_status["method"], json!("view/render"));
    let status_keymap = server.recv();
    assert_eq!(status_keymap["method"], json!("view/keymap"));
    server.send(json!({
        "jsonrpc": "2.0",
        "method": "view/rendered",
        "params": {"viewId": status_id, "version": 1},
    }));

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "view/invoke",
        "params": {"viewId": status_id, "command": "open.commit"},
    }));
    let invoke = server.response_with_id(3);
    let menu_id = invoke["result"]["effects"]
        .as_array()
        .expect("invoke effects")
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .and_then(|effect| effect["descriptor"]["id"].as_str())
        .expect("pushed commit menu")
        .to_string();
    let initial_menu = server.recv();
    assert_eq!(initial_menu["method"], json!("view/render"));
    assert_eq!(initial_menu["params"]["viewId"], json!(menu_id));
    assert_eq!(initial_menu["params"]["version"], json!(1));
    assert!(initial_menu["params"]["baseVersion"].is_null());
    let initial_text: Vec<String> =
        serde_json::from_value(initial_menu["params"]["text"].clone()).expect("initial menu text");
    assert_eq!(
        initial_text.join("\n"),
        include_str!("snapshots/menu-140.txt").trim_end()
    );
    let menu_keymap = server.recv();
    assert_eq!(menu_keymap["method"], json!("view/keymap"));
    server.send(json!({
        "jsonrpc": "2.0",
        "method": "view/rendered",
        "params": {"viewId": menu_id, "version": 1},
    }));

    server.send(json!({
        "jsonrpc": "2.0",
        "method": "view/resize",
        "params": {"viewId": menu_id, "cols": 60, "rows": 30},
    }));
    let resized = server.recv();
    assert_eq!(resized["method"], json!("view/render"), "{resized}");
    assert_eq!(resized["params"]["viewId"], json!(menu_id));
    assert_eq!(resized["params"]["version"], json!(2));
    assert_eq!(resized["params"]["baseVersion"], json!(1));
    assert!(resized["params"]["text"].is_null());
    let edits: Vec<magit_lsp::protocol::LineEdit> =
        serde_json::from_value(resized["params"]["edits"].clone())
            .expect("incremental resize edits");
    let reconstructed = magit_lsp::diff_text::apply_edits(&initial_text, &edits);
    assert_eq!(
        reconstructed.join("\n"),
        include_str!("snapshots/menu-60.txt").trim_end()
    );
    let wide = initial_text.join("\n");
    let narrow = reconstructed.join("\n");
    assert_ne!(wide, narrow, "the resize changes the menu's layout");

    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
}

/// The keys bound to `command` in a `view/keymap` notification.
fn keymap_keys(notification: &Value, command: &str) -> Option<Vec<String>> {
    notification["params"]["bindings"]
        .as_array()?
        .iter()
        .find(|binding| binding["command"] == json!(command))?
        .get("keys")?
        .as_array()
        .map(|keys| {
            keys.iter()
                .filter_map(|key| key.as_str().map(str::to_string))
                .collect()
        })
}

#[test]
fn static_keymap_delivery_suppresses_open_and_reload_pushes() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let config_dir = tempfile::TempDir::new().expect("config tempdir");
    let config_path = config_dir.path().join("config.toml");
    std::fs::write(&config_path, "profile = \"vim\"\n").expect("write config");
    let config = config_path.to_string_lossy().to_string();
    let repo = repo_param(tmp.path());
    let mut server = TestServer::spawn(&[("MAGIT_LSP_CONFIG", config.as_str())]);
    server.initialize(json!({"experimental":{"magit":{"v":1,"keys":"static","modifiers":"full","layout":["main"],"effects":[],"prompts":[]}}}));

    server.send(open_view_message(2, &repo));
    let opened = server.response_with_id(2);
    assert!(opened["result"]["id"].is_string());

    // The shutdown response is an ordered barrier: the preceding configuration
    // notification has been handled before the request stream can terminate.
    std::fs::write(
        &config_path,
        "profile = \"vim\"\n[keys.status]\nstage = [\"q\"]\n",
    )
    .expect("rewrite config");
    server.send(json!({"jsonrpc":"2.0","method":"workspace/didChangeConfiguration","params":{"settings":{}}}));
    server.send(json!({"jsonrpc":"2.0","id":99,"method":"shutdown","params":null}));
    let shutdown = server.response_with_id(99);
    assert_eq!(shutdown["result"], Value::Null);
    let observed = server.notifications.clone();
    assert!(
        observed
            .iter()
            .any(|message| message["method"] == json!("view/render")),
        "open should still deliver its render: {observed:?}"
    );
    assert!(
        observed
            .iter()
            .all(|message| message["method"] != json!("view/keymap")),
        "static clients must not receive keymap pushes on open or reload: {observed:?}"
    );
    assert!(
        server
            .stderr_text()
            .contains("configuration reloaded on client request"),
        "reload completion barrier was not observed"
    );
    server.send(json!({"jsonrpc":"2.0","method":"exit","params":null}));
    let status = server.finish(Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
}

#[test]
fn server_rejects_unsupported_presentation_modes() {
    for (field, value) in [
        ("keys", "forwardAll"),
        ("render", "full"),
        ("folds", "native"),
    ] {
        let mut server = TestServer::spawn(&[]);
        let mut caps = limited_rich(json!([]), json!([]));
        caps["experimental"]["magit"][field] = json!(value);
        server.send(
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":caps}}),
        );
        let response = server.response_with_id(1);
        assert!(
            response.get("error").is_some(),
            "{field}={value}: {response}"
        );
        server.send(json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{"capabilities":limited_rich(json!([]), json!([]))}}));
        let response = server.response_with_id(2);
        assert!(
            response.get("result").is_some(),
            "supported capabilities: {response}"
        );
        server.send(json!({"jsonrpc":"2.0","id":3,"method":"shutdown","params":null}));
        assert_eq!(server.response_with_id(3)["result"], Value::Null);
        server.send(json!({"jsonrpc":"2.0","method":"exit","params":null}));
        assert!(server.finish(Duration::from_secs(10)).success());
    }
}

#[test]
fn view_open_pushes_a_keymap_and_view_key_round_trips_over_stdio() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let repo = repo_param(tmp.path());
    let mut server = TestServer::spawn(&[]);
    let response = server.initialize(json!({}));
    assert_eq!(
        response["result"]["capabilities"]["foldingRangeProvider"],
        json!(true),
        "initialize response: {response}"
    );

    server.send(open_view_message(2, &repo));
    let opened = server.response_with_id(2);
    let view_id = opened["result"]["id"]
        .as_str()
        .expect("view id")
        .to_string();
    let uri = opened["result"]["uri"]
        .as_str()
        .expect("view uri")
        .to_string();

    let mut render = None;
    let mut keymap = None;
    while render.is_none() || keymap.is_none() {
        let message = server.recv();
        match message["method"].as_str() {
            Some("view/render") => render = Some(message),
            Some("view/keymap") => keymap = Some(message),
            other => panic!("unexpected message {other:?}: {message}"),
        }
    }
    let render = render.expect("initial render");
    let keymap = keymap.expect("keymap push on view/open");
    assert_eq!(keymap["params"]["viewId"], json!(view_id));
    assert_eq!(keymap["params"]["scope"], json!("status"));
    assert_eq!(keymap["params"]["capture"], json!(false));
    assert_eq!(
        keymap_keys(&keymap, "status.stage"),
        Some(vec!["s".to_string()])
    );

    assert!(render["params"]["text"].is_array());
    let line = first_hunk_line(&mut server, &uri);

    server.send(json!({
        "jsonrpc": "2.0",
        "method": "view/rendered",
        "params": {"viewId": view_id, "version": 1},
    }));
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "view/key",
        "params": {
            "viewId": view_id,
            "version": 1,
            "key": "s",
            "editorMode": "normal",
            "cursor": {"line": line, "column": 0},
        },
    }));

    let mut response = None;
    let mut staged_render = None;
    while response.is_none() || staged_render.is_none() {
        let message = server.recv();
        if message["id"] == json!(3) {
            response = Some(message);
        } else if message["method"] == json!("view/render") {
            staged_render = Some(message);
        }
    }
    let response = response.expect("view/key response");
    let staged_render = staged_render.expect("render after view/key");
    assert_eq!(response["result"]["effects"], json!([]), "{response}");
    assert_eq!(staged_render["params"]["version"], json!(2));
    assert!(hunk_count(&cached_diff(tmp.path())) >= 1);

    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
}

#[test]
fn a_menu_option_prompt_round_trips_over_stdio() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let repo = repo_param(tmp.path());
    let mut server = TestServer::spawn(&[]);
    let response = server.initialize(limited_rich(json!([]), json!(["text"])));
    assert_eq!(
        response["result"]["capabilities"]["foldingRangeProvider"],
        json!(true),
        "initialize response: {response}"
    );

    let view_id = open_view(&mut server, 2, &repo);

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "view/invoke",
        "params": {"viewId": view_id, "command": "open.commit"},
    }));
    let opened = server.response_with_id(3);
    let menu_id = opened["result"]["effects"]
        .as_array()
        .expect("effects")
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .and_then(|effect| effect["descriptor"]["id"].as_str())
        .expect("the pushed menu")
        .to_string();
    let menu_render = server.recv();
    assert_eq!(menu_render["method"], json!("view/render"), "{menu_render}");
    let menu_keymap = server.recv();
    assert_eq!(menu_keymap["method"], json!("view/keymap"), "{menu_keymap}");

    // The `=` prefix waits for the option letter.
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "view/key",
        "params": {
            "viewId": menu_id,
            "version": 1,
            "key": "=",
            "editorMode": "normal",
            "cursor": {"line": 0, "column": 0},
        },
    }));
    let prefix = server.response_with_id(4);
    assert_eq!(prefix["result"]["effects"], json!([]), "{prefix}");

    // The letter asks the question; the response is held until it is answered.
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "view/key",
        "params": {
            "viewId": menu_id,
            "version": 1,
            "key": "A",
            "editorMode": "normal",
            "cursor": {"line": 0, "column": 0},
        },
    }));
    let prompt = loop {
        let message = server.recv();
        if message["method"] == json!("view/prompt") {
            break message;
        }
        assert!(
            message["id"] != json!(5),
            "view/key must not answer before the prompt: {message}"
        );
    };
    let prompt_id = prompt["params"]["promptId"]
        .as_str()
        .expect("prompt id")
        .to_string();
    assert_eq!(prompt["params"]["kind"], json!("text"), "{prompt}");
    assert_eq!(
        prompt["params"]["title"],
        json!("Override the author (--author=)"),
        "{prompt}"
    );

    // Both notifications are read BEFORE promptRespond. Previously rendered
    // blocked the reader on session ownership and the answer was never read.
    server.send(json!({"jsonrpc":"2.0", "method":"view/rendered",
        "params":{"viewId":menu_id, "version":2}}));
    server.send(json!({"jsonrpc":"2.0", "method":"view/resize",
        "params":{"viewId":menu_id, "cols":60, "rows":24}}));
    server.send(json!({"jsonrpc":"2.0", "id":40, "method":"view/resync",
        "params":{"viewId":menu_id}}));
    let resized = server.response_with_id(40);
    assert!(
        resized["result"]["version"].as_u64().unwrap() > 2,
        "{resized}"
    );

    let (other_tmp, _other_repo) = init_repo();
    let other = open_view(&mut server, 41, &repo_param(other_tmp.path()));
    server.send(json!({"jsonrpc":"2.0", "id":42, "method":"view/invoke",
        "params":{"viewId":other, "command":"status.stage-all"}}));
    assert!(server.response_with_id(42).get("error").is_none());
    server.send(json!({"jsonrpc":"2.0", "id":43, "method":"view/invoke",
        "params":{"viewId":view_id, "command":"status.stage-all"}}));
    let refused = server.response_with_id(43);
    assert!(
        refused["result"]["effects"][0]["text"]
            .as_str()
            .unwrap()
            .contains("pending prompt"),
        "{refused}"
    );

    server.send(json!({"jsonrpc":"2.0", "id":44, "method":"view/invoke",
        "params":{"viewId":menu_id, "command":"common.refresh"}}));
    assert!(server.response_with_id(44).get("error").is_none());
    let render = server.recv();
    assert_eq!(render["method"], "view/render");

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "view/promptRespond",
        "params": {
            "promptId": prompt_id,
            "cancelled": false,
            "value": "Alice",
        },
    }));

    let mut key_response = None;
    let mut respond_response = None;
    let mut option_render = None;
    while key_response.is_none() || respond_response.is_none() || option_render.is_none() {
        let message = server.recv();
        match message["id"].as_i64() {
            Some(5) => key_response = Some(message),
            Some(6) => respond_response = Some(message),
            _ if message["method"] == json!("view/render") => {
                if message["params"]["viewId"] == json!(menu_id) {
                    option_render = Some(message);
                }
            }
            _ => {}
        }
    }

    let key_response = key_response.expect("view/key response");
    assert_eq!(
        key_response["result"]["effects"],
        json!([]),
        "{key_response}"
    );
    let respond_response = respond_response.expect("view/promptRespond response");
    assert_eq!(respond_response["result"], Value::Null);

    option_render.expect("the option render");
    server.send(json!({"jsonrpc":"2.0", "id":45, "method":"view/resync",
        "params":{"viewId":menu_id}}));
    let option_render = server.response_with_id(45);
    let text = option_render["result"]["text"]
        .as_array()
        .expect("full text after a fresh option");
    assert!(
        text.iter().any(|line| line
            .as_str()
            .is_some_and(|line| line.contains("--author=Alice"))),
        "the option and its preview show the answer: {option_render}"
    );

    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
}

/// A Plain client gets the prompt as a document view, never over `view/prompt`.
#[test]
fn plain_menu_option_prompt_opens_a_document_without_a_native_request() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let repo = repo_param(tmp.path());
    let mut server = TestServer::spawn(&[]);
    server.initialize(json!({}));

    let view_id = open_view(&mut server, 2, &repo);

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "view/invoke",
        "params": {"viewId": view_id, "command": "open.commit"},
    }));
    let opened = server.response_with_id(3);
    let menu_id = opened["result"]["effects"]
        .as_array()
        .expect("effects")
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .and_then(|effect| effect["descriptor"]["id"].as_str())
        .expect("the pushed menu")
        .to_string();
    let menu_render = server.recv();
    assert_eq!(menu_render["method"], json!("view/render"), "{menu_render}");
    let menu_keymap = server.recv();
    assert_eq!(menu_keymap["method"], json!("view/keymap"), "{menu_keymap}");

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "view/key",
        "params": {
            "viewId": menu_id,
            "version": 1,
            "key": "=",
            "editorMode": "normal",
            "cursor": {"line": 0, "column": 0},
        },
    }));
    let prefix = server.response_with_id(4);
    assert_eq!(prefix["result"]["effects"], json!([]), "{prefix}");

    // A Plain client declares no prompts: the answer must be a pushed document,
    // and the key request must not block waiting for a native answer.
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "view/key",
        "params": {
            "viewId": menu_id,
            "version": 1,
            "key": "A",
            "editorMode": "normal",
            "cursor": {"line": 0, "column": 0},
        },
    }));
    let answer = server.response_with_id(5);
    let descriptor = answer["result"]["effects"]
        .as_array()
        .expect("effects")
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .unwrap_or_else(|| panic!("the prompt view: {answer}"))["descriptor"]
        .clone();
    assert_eq!(descriptor["kind"], json!("prompt"), "{answer}");
    let prompt_view = descriptor["id"].as_str().expect("prompt view id").to_string();
    assert_eq!(
        descriptor["uri"],
        json!(format!("magit://prompt/{prompt_view}")),
        "{answer}"
    );

    let mut observed = server.notifications.clone();
    let prompt_render = loop {
        let message = server.recv();
        if message["method"] == json!("view/keymap") {
            observed.push(message);
            continue;
        }
        break message;
    };
    assert_eq!(prompt_render["method"], json!("view/render"), "{prompt_render}");
    assert_eq!(
        prompt_render["params"]["viewId"],
        json!(prompt_view),
        "{prompt_render}"
    );

    // The pending document prompt refuses the next command without answering it.
    let refused = invoke_rich(&mut server, 6, &view_id, "status.stage-all");
    assert!(
        refused["result"]["effects"][0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("Answer or cancel the pending prompt")),
        "{refused}"
    );

    observed.extend(server.notifications.clone());
    assert!(
        !observed
            .iter()
            .any(|message| message["method"] == json!("view/prompt")),
        "a Plain client must never receive a native prompt: {observed:?}"
    );

    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
}

#[test]
fn a_config_reload_repushes_the_view_keymap() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let config_dir = tempfile::TempDir::new().expect("config tempdir");
    let config_path = config_dir.path().join("config.toml");
    std::fs::write(&config_path, "profile = \"vim\"\n").expect("write config");

    let repo = repo_param(tmp.path());
    let config = config_path.to_string_lossy().to_string();
    let mut server = TestServer::spawn(&[("MAGIT_LSP_CONFIG", config.as_str())]);
    let response = server.initialize(json!({}));
    assert_eq!(
        response["result"]["capabilities"]["foldingRangeProvider"],
        json!(true),
        "initialize response: {response}"
    );

    server.send(open_view_message(2, &repo));
    let _opened = server.response_with_id(2);

    let mut keymap = None;
    while keymap.is_none() {
        let message = server.recv();
        if message["method"] == json!("view/keymap") {
            keymap = Some(message);
        }
    }
    let keymap = keymap.expect("initial keymap");
    assert_eq!(
        keymap_keys(&keymap, "status.stage"),
        Some(vec!["s".to_string()])
    );

    std::fs::write(
        &config_path,
        "profile = \"vim\"\n[keys.status]\nstage = [\"q\"]\n",
    )
    .expect("rewrite config");
    server.send(json!({
        "jsonrpc": "2.0",
        "method": "workspace/didChangeConfiguration",
        "params": {"settings": {}},
    }));

    loop {
        let message = server.recv();
        if message["method"] == json!("view/keymap")
            && keymap_keys(&message, "status.stage") == Some(vec!["q".to_string()])
        {
            break;
        }
    }

    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
}

#[test]
fn discard_prompt_cancellation_changed_content_and_closed_view_never_mutate() {
    for mode in ["cancel", "changed", "closed"] {
        let (tmp, _repo) = RepoFixture::new()
            .commit("init", [("f.txt", TEN_LINES)])
            .modify_standard()
            .build();
        let mut server = TestServer::spawn(&[]);
        server.initialize(limited_rich(json!([]), json!(["confirm"])));
        server.send(open_view_message(2, &repo_param(tmp.path())));
        let opened = server.response_with_id(2);
        let view = opened["result"]["id"].as_str().unwrap().to_string();
        let uri = opened["result"]["uri"].as_str().unwrap().to_string();
        assert_eq!(server.recv()["method"], "view/render");
        assert_eq!(server.recv()["method"], "view/keymap");
        let line = first_hunk_line(&mut server, &uri);
        server.send(json!({"jsonrpc":"2.0", "id":3, "method":"view/invoke",
            "params":{"viewId":view,"command":"status.discard","version":1,"cursor":{"line":line,"column":0}}}));
        let prompt = loop {
            let message = server.recv();
            if message["method"] == "view/prompt" {
                break message;
            }
            assert_ne!(
                message["id"], 3,
                "discard finished without prompting: {message}"
            );
        };
        server.send(json!({"jsonrpc":"2.0", "method":"view/rendered", "params":{"viewId":view,"version":1}}));
        server.send(json!({"jsonrpc":"2.0", "method":"view/resize", "params":{"viewId":view,"cols":60,"rows":24}}));
        server.send(
            json!({"jsonrpc":"2.0", "id":4, "method":"view/resync", "params":{"viewId":view}}),
        );
        assert!(
            server.response_with_id(4)["result"]["version"]
                .as_u64()
                .unwrap()
                >= 1
        );
        server.send(json!({"jsonrpc":"2.0", "id":5, "method":"view/key",
            "params":{"viewId":view,"version":1,"key":"s","editorMode":"normal","cursor":{"line":line,"column":0}}}));
        let refused = server.response_with_id(5);
        assert!(refused["result"]["effects"][0]["text"]
            .as_str()
            .unwrap()
            .contains("pending prompt"));
        if mode == "changed" {
            std::fs::write(tmp.path().join("f.txt"), "external replacement\n").unwrap();
        }
        if mode == "closed" {
            server.send(
                json!({"jsonrpc":"2.0", "id":6, "method":"view/close", "params":{"viewId":view}}),
            );
            assert!(server.response_with_id(6).get("error").is_none());
        }
        let before = std::fs::read(tmp.path().join("f.txt")).unwrap();
        server.send(json!({"jsonrpc":"2.0", "id":7, "method":"view/promptRespond",
            "params":{"promptId":prompt["params"]["promptId"],"cancelled":mode == "cancel","value":true}}));
        let mut answered = false;
        let mut completed = false;
        while !answered || !completed {
            let message = server.recv();
            if message["id"] == 7 {
                answered = true;
                assert!(message.get("error").is_none(), "{message}");
            }
            if message["id"] == 3 {
                completed = true;
                assert!(message.get("error").is_none(), "{message}");
            }
        }
        assert_eq!(
            std::fs::read(tmp.path().join("f.txt")).unwrap(),
            before,
            "{mode}"
        );
        assert!(tmp.git(&["diff", "--cached"]).is_empty(), "{mode}");
        shutdown_and_exit(&mut server);
        assert!(server.finish(Duration::from_secs(10)).success());
    }
}

#[test]
fn disconnect_and_shutdown_cancel_pending_preflight_without_staging() {
    for mode in ["disconnect", "shutdown", "exit"] {
        let (tmp, _) = RepoFixture::new()
            .commit("init", [("f.txt", TEN_LINES)])
            .modify_standard()
            .build();
        let mut server = TestServer::spawn(&[]);
        server.initialize(limited_rich(
            json!(["editDocument"]),
            json!(["confirm"]),
        ));
        let view = open_view(&mut server, 2, &repo_param(tmp.path()));
        server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/invoke",
            "params":{"viewId":view,"command":"commit.create"}}));
        loop {
            let message = server.recv();
            if message["method"] == "view/prompt" {
                break;
            }
            assert_ne!(message["id"], 3, "{message}");
        }
        match mode {
            "disconnect" => server.close_stdin(),
            "shutdown" => shutdown_and_exit(&mut server),
            "exit" => server.send(json!({"jsonrpc":"2.0","method":"exit","params":null})),
            _ => unreachable!(),
        }
        let status = server.finish(Duration::from_secs(10));
        assert_eq!(
            status.code(),
            Some(if mode == "exit" { 1 } else { 0 }),
            "{mode}"
        );
        assert_eq!(tmp.git(&["status", "--porcelain"]), " M f.txt\n", "{mode}");
    }
}

#[test]
fn a12_selection_stage_reports_index_lock_as_successful_error_effect() {
    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let file = tmp.path().join("f.txt");
    let before_worktree = std::fs::read(&file).unwrap();
    let before_index = std::fs::read(tmp.path().join(".git/index")).unwrap();
    let mut server = TestServer::spawn(&[]);
    server.initialize(json!({}));
    server.send(open_view_message(2, &repo_param(tmp.path())));
    let opened = server.response_with_id(2);
    let view = opened["result"]["id"].as_str().unwrap().to_string();
    let uri = opened["result"]["uri"].as_str().unwrap().to_string();
    let render = server.recv();
    assert_eq!(render["method"], "view/render");
    let version = render["params"]["version"]
        .as_u64()
        .expect("initial render version");
    assert_eq!(server.recv()["method"], "view/keymap");
    // The changed-line symbol range comes from the server's structured outline,
    // not by interpreting rendered text.
    let (start_line, end_line) = first_hunk_range(&mut server, &uri);
    write(&tmp.path().join(".git/index.lock"), "");

    server.send(json!({"jsonrpc":"2.0", "id":3, "method":"view/invoke",
        "params":{"viewId":view,"command":"status.stage","version":version,
        "cursor":{"line":start_line,"column":0},
        "selection":{"startLine":start_line,"endLine":end_line}}}));
    let response = server.response_with_id(3);
    assert!(
        response.get("error").is_none(),
        "Git failure is a result: {response}"
    );
    let effect = &response["result"]["effects"][0];
    assert_eq!(effect["level"], "error", "{response}");
    let message = effect["text"].as_str().expect("readable error message");
    assert!(message.contains("Another Git process"), "{response}");
    assert!(
        effect["hint"]
            .as_str()
            .is_some_and(|hint| hint.contains("index.lock")),
        "{response}"
    );
    assert_eq!(std::fs::read(&file).unwrap(), before_worktree);
    assert_eq!(
        std::fs::read(tmp.path().join(".git/index")).unwrap(),
        before_index
    );

    server.send(json!({"jsonrpc":"2.0", "id":4, "method":"view/invoke",
        "params":{"command":"status.stage"}}));
    let malformed = server.response_with_id(4);
    assert!(
        malformed.get("error").is_some(),
        "invalid protocol request must error: {malformed}"
    );
    shutdown_and_exit(&mut server);
    let (status, stderr) = server.finish_with_stderr(Duration::from_secs(10));
    assert!(status.success(), "server exited: {stderr}");
    assert!(stderr.contains("git failed"), "stderr: {stderr}");
}

/// Resolve the first hunk's complete range from the server-provided outline.
fn first_hunk_range(server: &mut TestServer, uri: &str) -> (u64, u64) {
    server.send(json!({"jsonrpc":"2.0", "id":91,
        "method":"textDocument/documentSymbol",
        "params":{"textDocument":{"uri":uri}}}));
    let response = server.response_with_id(91);
    let symbols: Vec<lsp_types::DocumentSymbol> =
        serde_json::from_value(response["result"].clone()).expect("document symbols");
    fn search(symbols: &[lsp_types::DocumentSymbol]) -> Option<(u64, u64)> {
        symbols.iter().find_map(|symbol| {
            if symbol.kind == lsp_types::SymbolKind::METHOD {
                Some((
                    u64::from(symbol.range.start.line),
                    u64::from(symbol.range.end.line),
                ))
            } else {
                symbol.children.as_deref().and_then(search)
            }
        })
    }
    search(&symbols).expect("changed-line symbol")
}

fn spawn_with_env(env: &[(String, String)]) -> TestServer {
    let refs: Vec<(&str, &str)> = env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    TestServer::spawn(&refs)
}

#[test]
fn a_mutation_reports_work_done_progress() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let repo = repo_param(tmp.path());
    let mut server = TestServer::spawn(&[]);
    server.auto_respond_to_progress_create();
    let response = server.initialize(json!({"window":{"workDoneProgress":true}}));
    assert_eq!(
        response["result"]["capabilities"]["foldingRangeProvider"],
        json!(true),
        "initialize response: {response}"
    );
    let view_id = open_view(&mut server, 2, &repo);

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "view/invoke",
        "params": {"viewId": view_id, "command": "status.stage-all"},
    }));

    let mut phases: Vec<String> = Vec::new();
    let mut tokens: Vec<String> = Vec::new();
    loop {
        let message = server.recv();
        if message["id"] == json!(3) {
            break;
        }
        match message["method"].as_str().unwrap_or_default() {
            "window/workDoneProgress/create" => {
                phases.push("create".to_string());
                tokens.push(
                    message["params"]["token"]
                        .as_str()
                        .expect("a create token")
                        .to_string(),
                );
            }
            "$/progress" => {
                phases.push(
                    message["params"]["value"]["kind"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                );
                tokens.push(
                    message["params"]["token"]
                        .as_str()
                        .expect("a progress token")
                        .to_string(),
                );
            }
            _ => {}
        }
    }

    assert_eq!(
        phases,
        vec!["create".to_string(), "begin".to_string(), "end".to_string()],
        "the work-done handshake must precede the response"
    );
    assert!(
        tokens.windows(2).all(|pair| pair[0] == pair[1]),
        "create, begin and end must share a token: {tokens:?}"
    );

    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
}

/// Make the next `git commit` fail in its pre-commit hook.
#[cfg(unix)]
fn install_failing_pre_commit_hook(repo: &RepoFixture) {
    use std::os::unix::fs::PermissionsExt;

    let hook = repo.path().join(".git/hooks/pre-commit");
    write(
        &hook,
        "#!/bin/sh\necho \"pre-commit refused\" >&2\nexit 1\n",
    );
    let mut permissions = std::fs::metadata(&hook)
        .expect("read the hook metadata")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&hook, permissions).expect("make the hook executable");
}

#[cfg(unix)]
#[test]
fn a_failed_commit_ends_its_progress_with_an_error() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .staged_file("f.txt", standard_changes())
        .build();
    install_failing_pre_commit_hook(&tmp);
    let repo = repo_param(tmp.path());
    let mut server = TestServer::spawn(&[]);
    server.auto_respond_to_progress_create();
    let response = server.initialize(json!({"window":{"workDoneProgress":true}}));
    assert_eq!(
        response["result"]["capabilities"]["foldingRangeProvider"],
        json!(true),
        "initialize response: {response}"
    );
    let view_id = open_view(&mut server, 2, &repo);

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "view/invoke",
        "params": {"viewId": view_id, "command": "commit.create"},
    }));

    let mut progress: Vec<(String, String)> = Vec::new();
    let mut response = None;
    let mut failure = None;
    let saw_end = |progress: &[(String, String)]| progress.iter().any(|(_, phase)| phase == "end");
    while response.is_none() || failure.is_none() || !saw_end(&progress) {
        let message = server.recv();
        if message["id"] == json!(3) {
            response = Some(message);
            continue;
        }
        if let Some(event) = progress_event(&message) {
            progress.push(event);
        }
        if message["method"] == json!("window/showMessage")
            && message["params"]["type"] == json!(1)
            && message["params"]["message"].as_str().is_some_and(|text| {
                text.contains("`git commit` failed.") && text.contains("pre-commit refused")
            })
        {
            failure = Some(message);
        }
    }

    let response = response.expect("commit.create response");
    assert!(
        response["result"]["effects"]
            .as_array()
            .is_some_and(|effects| effects
                .iter()
                .any(|effect| effect["text"] == json!("Committing…"))),
        "the commit must announce itself before the worker starts: {response}"
    );

    let phases: Vec<String> = progress.iter().map(|(_, phase)| phase.clone()).collect();
    assert_eq!(
        phases,
        vec!["create".to_string(), "begin".to_string(), "end".to_string()],
        "a failed commit still ends its progress: {progress:?}"
    );
    assert!(
        progress.windows(2).all(|pair| pair[0].0 == pair[1].0),
        "create, begin and end must share one token: {progress:?}"
    );

    let failure = failure.expect("the failed commit effect");
    let error = failure["params"]["message"].as_str().unwrap();
    assert!(error.contains("`git commit` failed."), "{failure}");
    assert!(error.contains("pre-commit refused"), "hook output: {failure}");

    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
}

#[test]
fn a_cancel_notification_aborts_a_slow_mutation() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let gate = GatedGit::arm("add");
    let repo = repo_param(tmp.path());
    let mut server = spawn_with_env(&gate.child_env());
    server.auto_respond_to_progress_create();
    let response = server.initialize(json!({"window":{"workDoneProgress":true}}));
    assert_eq!(
        response["result"]["capabilities"]["foldingRangeProvider"],
        json!(true),
        "initialize response: {response}"
    );
    let view_id = open_view(&mut server, 2, &repo);

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "view/invoke",
        "params": {"viewId": view_id, "command": "status.stage-all"},
    }));

    let token = loop {
        let message = server.recv();
        assert!(
            message["id"] != json!(3),
            "the mutation finished before it could be cancelled: {message}"
        );
        if message["method"] == json!("$/progress")
            && message["params"]["value"]["kind"] == json!("begin")
        {
            break message["params"]["token"]
                .as_str()
                .expect("a progress token")
                .to_string();
        }
    };

    gate.wait_entered();
    let cancelled_at = Instant::now();
    server.send(json!({
        "jsonrpc": "2.0",
        "method": "window/workDoneProgress/cancel",
        "params": {"token": token},
    }));

    let mut saw_end = false;
    let response = loop {
        let message = server.recv();
        if message["id"] == json!(3) {
            break message;
        }
        if message["method"] == json!("$/progress")
            && message["params"]["value"]["kind"] == json!("end")
        {
            saw_end = true;
        }
    };

    assert!(
        cancelled_at.elapsed() < Duration::from_secs(5),
        "the cancelled mutation took {:?}",
        cancelled_at.elapsed()
    );
    assert!(saw_end, "the cancelled mutation never ended its progress");
    let effects = response["result"]["effects"]
        .as_array()
        .unwrap_or_else(|| panic!("the response is not an invoke result: {response}"));
    assert!(
        effects.iter().any(|effect| {
            effect["type"] == json!("message") && effect["level"] == json!("error")
        }),
        "a killed Git command must report an error effect: {response}"
    );

    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
}

/// Disconnect (EOF) exactly while a real commit is blocked inside its
/// commit-msg hook: the server's shutdown path must cancel the owned process
/// group, the resistant hook descendant must be TERM/grace/KILL-reaped, no
/// commit or repository state change may be recorded, and the repository must
/// stay usable for a subsequent commit. This closes the disconnect-across-phases
/// acceptance for the commit-msg phase (pre-hook/editor-active are covered
/// elsewhere; progress-cancel covers commit-msg but not the disconnect path).
#[cfg(target_os = "linux")]
#[test]
fn disconnect_during_commit_msg_hook_cancels_owned_hook_without_committing() {
    use std::os::unix::fs::PermissionsExt;

    let (tmp, _) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .staged_file("f.txt", standard_changes())
        .build();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let staged = tmp.git(&["ls-files", "--stage", "-z"]);
    let tree = tmp.git(&["write-tree"]);
    let patch = tmp.git(&["diff", "--cached", "--binary"]);
    let worktree = read(&tmp.path().join("f.txt"));
    let ready = tmp.path().join("hook.ready");
    let child_pid = tmp.path().join("hook-child.pid");
    let hook = tmp.path().join(".git/hooks/commit-msg");
    write(
        &hook,
        &format!(
            "#!/bin/sh\ntrap '' TERM\nsh -c 'echo $$ > \"$1\"; touch \"$2\"; exec sleep 60' sh '{}' '{}' </dev/null >/dev/null 2>&1 &\nwait\n",
            child_pid.display(),
            ready.display()
        ),
    );
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();

    fn start_time(pid: i32) -> Option<u64> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        stat.rsplit_once(')')?
            .1
            .split_whitespace()
            .nth(19)?
            .parse()
            .ok()
    }
    fn live(pid: i32) -> bool {
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|stat| {
                stat.rsplit_once(')')
                    .map(|(_, tail)| !matches!(tail.trim_start().chars().next(), Some('Z' | 'X')))
            })
            .unwrap_or(false)
    }

    let mut server = TestServer::spawn(&[]);
    server.auto_respond_to_progress_create();
    server.initialize_rich(json!({"window":{"workDoneProgress":true}}));
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/invoke","params":{"viewId":status,"command":"commit.create"}}));
    let mut edit = None;
    while edit.is_none() {
        let message = server.recv();
        if message["method"] == json!("view/effects") {
            edit = message["params"]["effects"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["type"] == json!("editDocument"))
                .cloned()
                .or(edit);
        }
    }
    let edit = edit.unwrap();
    // The one explicit full-sync stream, then a confirm barrier on the exact
    // saved checkpoint (never a file or a guessed version).
    server.send_raw(&frame(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":edit["uri"],"languageId":"gitcommit","version":1,"text":edit["text"]}}})));
    server.send_raw(&frame(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":edit["uri"],"version":2},"contentChanges":[{"text":"Never submitted\n"}]}})));
    server.send_raw(&frame(json!({"jsonrpc":"2.0","id":501,"method":"workspace/executeCommand","params":{"command":"magit.command","arguments":[{"command":"common.confirm","runId":edit["runId"],"uri":edit["uri"],"version":2,"source":"synchronized"}]}})));
    assert!(server.response_with_id(501).get("error").is_none());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.exists() {
        assert!(
            Instant::now() < deadline,
            "commit-msg hook did not become ready"
        );
        thread::sleep(Duration::from_millis(20));
    }
    let child: i32 = read(&child_pid).trim().parse().unwrap();
    let identity = start_time(child).expect("hook descendant identity");
    struct Cleanup(i32, u64);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            if start_time(self.0) == Some(self.1) && live(self.0) {
                unsafe {
                    libc::kill(self.0, libc::SIGKILL);
                }
            }
        }
    }
    let _owned = Cleanup(child, identity);

    // Disconnect while the hook is blocked; the shutdown path must cancel the
    // owned group rather than orphan it past the server's own exit.
    server.close_stdin();
    let exit = server.finish(Duration::from_secs(15));
    assert!(
        exit.success(),
        "server exited cleanly on disconnect: {exit:?}"
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    while live(child) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !live(child),
        "owned hook descendant survived the disconnect"
    );
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), staged);
    assert_eq!(tmp.git(&["write-tree"]), tree);
    assert_eq!(tmp.git(&["diff", "--cached", "--binary"]), patch);
    assert_eq!(read(&tmp.path().join("f.txt")), worktree);
    // The repository is not poisoned: a follow-up commit lands after removing
    // the blocking hook.
    std::fs::remove_file(&hook).unwrap();
    tmp.git(&["commit", "-m", "follow-up"]);
}

/// Full rich declaration including every native pick shape.
fn rich_pick_capabilities() -> Value {
    json!({"experimental":{"magit":{
        "v":1,"keys":"dynamic","modifiers":"full",
        "layout":["main"],"effects":[],"prompts":[],
        "pick":{"multi":false,"freeText":true,"description":true,"detail":true,
                "group":true,"preselection":true,"fallbackView":true,"maxItems":100}
    }}})
}

/// Mixed rung: native picks and document effects declared, native prompts not.
fn mixed_rung_pick_capabilities() -> Value {
    json!({"experimental":{"magit":{
        "v":1,"keys":"dynamic","modifiers":"full",
        "layout":["main"],"effects":["editDocument"],"prompts":[],
        "pick":{"multi":false,"freeText":true,"description":true,"detail":true,
                "group":true,"preselection":true,"fallbackView":true,"maxItems":100}
    }}})
}

#[test]
fn a_declared_native_pick_answers_the_staged_menu_prompt() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let mut server = TestServer::spawn(&[]);
    server.initialize(rich_pick_capabilities());
    server.send(open_view_message(2, &repo_param(tmp.path())));
    let status = server.response_with_id(2)["result"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/invoke",
        "params":{"viewId":status,"command":"open.branch"}}));
    let opened = server.response_with_id(3);
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let menu_render = server.recv();
    assert_eq!(menu_render["method"], json!("view/render"), "{menu_render}");
    assert_eq!(server.recv()["method"], json!("view/keymap"));
    let version = menu_render["params"]["version"].as_u64().unwrap();
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered",
        "params":{"viewId":menu,"version":version}}));

    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/key",
        "params":{"viewId":menu,"version":version,"key":"=","editorMode":"normal","cursor":{"line":0,"column":0}}}));
    let prefix = server.response_with_id(4);
    assert_eq!(prefix["result"]["effects"], json!([]), "{prefix}");
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/key",
        "params":{"viewId":menu,"version":version,"key":"u","editorMode":"normal","cursor":{"line":0,"column":0}}}));

    let pick = loop {
        let message = server.recv();
        server.notifications.push(message.clone());
        if message["method"] == json!("view/pick") {
            break message;
        }
    };
    let params = &pick["params"];
    assert!(
        params["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == json!("main")),
        "{pick}"
    );
    assert_eq!(params["allowFreeText"], json!(true), "{pick}");
    assert_eq!(params["multi"], json!(false), "{pick}");
    let pick_id = params["pickId"].as_str().unwrap().to_owned();

    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/pickRespond",
        "params":{"pickId":pick_id,"cancelled":false,"selected":["main"]}}));

    let mut saw_key_response = false;
    let mut saw_update = false;
    for _ in 0..30 {
        let message = server.recv();
        server.notifications.push(message.clone());
        if message["id"] == json!(5) {
            saw_key_response = true;
        }
        if message["method"] == json!("view/render")
            && message["params"].to_string().contains("--set-upstream-to=main")
        {
            saw_update = true;
        }
        if saw_key_response && saw_update {
            break;
        }
    }
    assert!(
        saw_key_response,
        "the held view/key response arrives: {:?}",
        server.notifications
    );
    assert!(
        saw_update,
        "the menu re-renders the new option: {:?}",
        server.notifications
    );
    assert!(
        server
            .notifications
            .iter()
            .all(|message| message["method"] != json!("view/prompt")),
        "a declared pick must not emit a document prompt: {:?}",
        server.notifications
    );

    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn cancelled_native_pick_on_the_mixed_rung_leaves_the_menu_and_repo_unchanged() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let index = tmp.git(&["ls-files", "--stage", "-z"]);
    let worktree = read(&tmp.path().join("f.txt"));
    let mut server = TestServer::spawn(&[]);
    // Mixed rung: native pick declared, native prompts (confirm) undeclared.
    server.initialize(rich_pick_capabilities());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let opened = invoke_rich(&mut server, 3, &status, "open.branch");
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let menu_render = server.recv();
    assert_eq!(menu_render["method"], json!("view/render"), "{menu_render}");
    assert_eq!(server.recv()["method"], json!("view/keymap"));
    let version = menu_render["params"]["version"].as_u64().unwrap();
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered",
        "params":{"viewId":menu,"version":version}}));

    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/key",
        "params":{"viewId":menu,"version":version,"key":"=","editorMode":"normal","cursor":{"line":0,"column":0}}}));
    assert_eq!(server.response_with_id(4)["result"]["effects"], json!([]));
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/key",
        "params":{"viewId":menu,"version":version,"key":"u","editorMode":"normal","cursor":{"line":0,"column":0}}}));

    let pick = loop {
        let message = server.recv();
        server.notifications.push(message.clone());
        if message["method"] == json!("view/pick") {
            break message;
        }
    };
    let params = &pick["params"];
    assert_eq!(params["multi"], json!(false), "{pick}");
    assert_eq!(params["allowFreeText"], json!(true), "{pick}");
    let pick_id = params["pickId"].as_str().unwrap().to_owned();

    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/pickRespond",
        "params":{"pickId":pick_id,"cancelled":true,"selected":[]}}));

    let mut key_response = None;
    let mut respond_response = None;
    for _ in 0..30 {
        let message = server.recv();
        server.notifications.push(message.clone());
        match message["id"].as_i64() {
            Some(5) => key_response = Some(message),
            Some(6) => respond_response = Some(message),
            _ => {}
        }
        if key_response.is_some() && respond_response.is_some() {
            break;
        }
    }
    let key_response = key_response.expect("the held view/key response returns");
    let respond_response = respond_response.expect("view/pickRespond responds");
    assert!(respond_response.get("error").is_none(), "{respond_response}");
    for response in [&key_response, &respond_response] {
        assert!(
            !response["result"]["effects"]
                .as_array()
                .is_some_and(|effects| {
                    effects
                        .iter()
                        .any(|effect| effect["type"] == json!("pushView"))
                }),
            "cancelling a pick must not push a view: {response}"
        );
    }
    assert!(
        server
            .notifications
            .iter()
            .all(|message| message["method"] != json!("view/prompt")),
        "a declared pick must not emit a document prompt: {:?}",
        server.notifications
    );
    assert!(
        server
            .notifications
            .iter()
            .all(|message| !message.to_string().contains("--set-upstream-to=main")),
        "a cancelled option pick leaves the option unset: {:?}",
        server.notifications
    );

    // The menu is still the top view and holds no pending prompt: a command
    // on it runs instead of being refused.
    let refreshed = invoke_rich(&mut server, 7, &menu, "common.refresh");
    assert!(refreshed.get("error").is_none(), "{refreshed}");
    let effects = refreshed["result"]["effects"].as_array().unwrap();
    assert!(
        effects.iter().all(|effect| effect["type"] != json!("pushView")),
        "{refreshed}"
    );
    assert!(
        effects.iter().all(|effect| !effect["text"]
            .as_str()
            .is_some_and(|text| text.contains("pending prompt"))),
        "{refreshed}"
    );

    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), index);
    assert_eq!(read(&tmp.path().join("f.txt")), worktree);
    assert!(!tmp.path().join(".git/index.lock").exists());
}

#[test]
fn cancelled_document_confirm_after_a_native_pick_on_the_mixed_rung_leaves_the_repo_unchanged() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", "initial\n")])
        .modify("f.txt", "changed\n")
        .build();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let index = tmp.git(&["ls-files", "--stage", "-z"]);
    let worktree = read(&tmp.path().join("f.txt"));
    let mut server = TestServer::spawn(&[]);
    // Mixed rung: native pick and editDocument declared, native confirm undeclared.
    server.initialize(mixed_rung_pick_capabilities());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let opened = invoke_rich(&mut server, 3, &status, "open.branch");
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let menu_render = server.recv();
    assert_eq!(menu_render["method"], json!("view/render"), "{menu_render}");
    assert_eq!(server.recv()["method"], json!("view/keymap"));
    let version = menu_render["params"]["version"].as_u64().unwrap();
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered",
        "params":{"viewId":menu,"version":version}}));

    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/key",
        "params":{"viewId":menu,"version":version,"key":"=","editorMode":"normal","cursor":{"line":0,"column":0}}}));
    assert_eq!(server.response_with_id(4)["result"]["effects"], json!([]));
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/key",
        "params":{"viewId":menu,"version":version,"key":"u","editorMode":"normal","cursor":{"line":0,"column":0}}}));

    let pick = loop {
        let message = server.recv();
        server.notifications.push(message.clone());
        if message["method"] == json!("view/pick") {
            break message;
        }
    };
    let params = &pick["params"];
    assert!(
        params["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == json!("main")),
        "{pick}"
    );
    let pick_id = params["pickId"].as_str().unwrap().to_owned();

    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/pickRespond",
        "params":{"pickId":pick_id,"cancelled":false,"selected":["main"]}}));

    let mut key_response = None;
    let mut respond_response = None;
    for _ in 0..30 {
        let message = server.recv();
        server.notifications.push(message.clone());
        match message["id"].as_i64() {
            Some(5) => key_response = Some(message),
            Some(6) => respond_response = Some(message),
            _ => {}
        }
        if key_response.is_some() && respond_response.is_some() {
            break;
        }
    }
    let key_response = key_response.expect("the held view/key response returns");
    let respond_response = respond_response.expect("view/pickRespond responds");
    assert!(respond_response.get("error").is_none(), "{respond_response}");
    assert!(
        server
            .notifications
            .iter()
            .all(|message| message["method"] != json!("view/prompt")),
        "a declared pick must not emit a document prompt: {:?}",
        server.notifications
    );

    // The unstaged worktree change makes commit.create open the confirm prompt
    // as a document because the client does not declare native confirm.
    let opened = invoke_rich(&mut server, 7, &status, "commit.create");
    let prompt_effect = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| {
            effect["type"] == json!("pushView")
                && effect["descriptor"]["kind"] == json!("prompt")
        })
        .expect("commit.create opens the document confirm");
    let prompt = prompt_effect["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut prompt_render = server
        .notifications
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("view/render")
                && message["params"]["viewId"] == json!(prompt)
        })
        .cloned();
    while prompt_render.is_none() {
        let message = server.recv();
        server.notifications.push(message.clone());
        if message["method"] == json!("view/render")
            && message["params"]["viewId"] == json!(prompt)
        {
            prompt_render = Some(message);
        }
    }
    let prompt_render = prompt_render.unwrap();
    let prompt_version = prompt_render["params"]["version"].as_u64().unwrap();
    let text: Vec<String> =
        serde_json::from_value(prompt_render["params"]["text"].clone()).unwrap();
    let no_row = text.iter().position(|line| line == "No").expect("No row");

    server.send(json!({"jsonrpc":"2.0","id":8,"method":"view/key",
        "params":{"viewId":prompt,"version":prompt_version,"key":"Enter","editorMode":"normal",
            "cursor":{"line":no_row,"column":0}}}));
    let cancelled = server.response_with_id(8);
    assert!(
        cancelled["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == json!("popView") && effect["viewId"] == json!(prompt)),
        "{cancelled}"
    );
    for response in [&key_response, &respond_response, &opened, &cancelled] {
        assert!(
            !response["result"]["effects"]
                .as_array()
                .is_some_and(|effects| {
                    effects.iter().any(|effect| effect["type"] == json!("editDocument"))
                }),
            "cancelling the confirm must not edit the document: {response}"
        );
    }
    server.drain_notifications();
    assert!(
        server
            .notifications
            .iter()
            .all(|message| message["method"] != json!("view/prompt")),
        "an undeclared confirm uses the document prompt: {:?}",
        server.notifications
    );
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), index);
    assert_eq!(read(&tmp.path().join("f.txt")), worktree);
    assert!(!tmp.path().join(".git/index.lock").exists());

    // The prompt popped, so the status view is usable instead of refusing
    // commands while a prompt is pending.
    let refreshed = invoke_rich(&mut server, 9, &status, "common.refresh");
    assert!(refreshed.get("error").is_none(), "{refreshed}");
    let effects = refreshed["result"]["effects"].as_array().unwrap();
    assert!(
        effects.iter().all(|effect| effect["type"] != json!("pushView")),
        "{refreshed}"
    );
    assert!(
        effects.iter().all(|effect| !effect["text"]
            .as_str()
            .is_some_and(|text| text.contains("pending prompt"))),
        "{refreshed}"
    );

    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn a_rich_client_without_a_pick_declaration_gets_the_document_prompt() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let mut server = TestServer::spawn(&[]);
    server.initialize(limited_rich(json!([]), json!([])));
    server.send(open_view_message(2, &repo_param(tmp.path())));
    let status = server.response_with_id(2)["result"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/invoke",
        "params":{"viewId":status,"command":"open.branch"}}));
    let opened = server.response_with_id(3);
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(server.recv()["method"], json!("view/render"));
    assert_eq!(server.recv()["method"], json!("view/keymap"));

    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/key",
        "params":{"viewId":menu,"version":1,"key":"=","editorMode":"normal","cursor":{"line":0,"column":0}}}));
    assert_eq!(server.response_with_id(4)["result"]["effects"], json!([]));
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/key",
        "params":{"viewId":menu,"version":1,"key":"u","editorMode":"normal","cursor":{"line":0,"column":0}}}));

    let answer = server.response_with_id(5);
    let descriptor = answer["result"]["effects"]
        .as_array()
        .expect("effects")
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .unwrap_or_else(|| panic!("the prompt view: {answer}"))["descriptor"]
        .clone();
    assert_eq!(descriptor["kind"], json!("prompt"), "{answer}");
    let prompt_view = descriptor["id"].as_str().expect("prompt view id").to_owned();
    let mut rendered = server.notifications.iter().any(|message| {
        message["method"] == json!("view/render") && message["params"]["viewId"] == json!(prompt_view)
    });
    while !rendered {
        let message = server.recv();
        server.notifications.push(message.clone());
        if message["method"] == json!("view/render")
            && message["params"]["viewId"] == json!(prompt_view)
        {
            rendered = true;
        }
    }
    assert!(
        server
            .notifications
            .iter()
            .all(|message| message["method"] != json!("view/pick")),
        "an undeclared pick must fall back to the document: {:?}",
        server.notifications
    );

    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn pick_preference_truncates_native_items() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    tmp.git(&["branch", "b1"]);
    tmp.git(&["branch", "b2"]);
    let config_dir = tempfile::TempDir::new().expect("config tempdir");
    let config_path = config_dir.path().join("config.toml");
    std::fs::write(&config_path, "[pick]\nmax_items = 2\n").expect("write config");
    let config = config_path.to_string_lossy().to_string();
    let mut server = TestServer::spawn(&[("MAGIT_LSP_CONFIG", config.as_str())]);
    server.initialize(rich_pick_capabilities());
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let opened = invoke_rich(&mut server, 3, &status, "open.branch");
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let menu_render = server.recv();
    assert_eq!(menu_render["method"], json!("view/render"), "{menu_render}");
    assert_eq!(server.recv()["method"], json!("view/keymap"));
    let version = menu_render["params"]["version"].as_u64().unwrap();
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered",
        "params":{"viewId":menu,"version":version}}));

    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/key",
        "params":{"viewId":menu,"version":version,"key":"=","editorMode":"normal","cursor":{"line":0,"column":0}}}));
    assert_eq!(server.response_with_id(4)["result"]["effects"], json!([]));
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/key",
        "params":{"viewId":menu,"version":version,"key":"u","editorMode":"normal","cursor":{"line":0,"column":0}}}));

    let pick = loop {
        let message = server.recv();
        server.notifications.push(message.clone());
        if message["method"] == json!("view/pick") {
            break message;
        }
    };
    let params = &pick["params"];
    assert_eq!(
        params["items"].as_array().unwrap().len(),
        2,
        "the preference truncates the native pick: {pick}"
    );
    assert_eq!(params["allowFreeText"], json!(true), "{pick}");

    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/pickRespond",
        "params":{"pickId":params["pickId"],"cancelled":true,"selected":[]}}));
    let _ = server.response_with_id(5);
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn pick_preference_beyond_the_declared_limit_uses_the_document_picker() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    tmp.git(&["branch", "b1"]);
    tmp.git(&["branch", "b2"]);
    let config_dir = tempfile::TempDir::new().expect("config tempdir");
    let config_path = config_dir.path().join("config.toml");
    std::fs::write(&config_path, "[pick]\nmax_items = 10\n").expect("write config");
    let config = config_path.to_string_lossy().to_string();
    let mut server = TestServer::spawn(&[("MAGIT_LSP_CONFIG", config.as_str())]);
    let mut capabilities = rich_pick_capabilities();
    capabilities["experimental"]["magit"]["pick"]["maxItems"] = json!(2);
    server.initialize(capabilities);
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let opened = invoke_rich(&mut server, 3, &status, "open.branch");
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(server.recv()["method"], json!("view/render"));
    assert_eq!(server.recv()["method"], json!("view/keymap"));

    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/key",
        "params":{"viewId":menu,"version":1,"key":"=","editorMode":"normal",
            "cursor":{"line":0,"column":0}}}));
    assert_eq!(server.response_with_id(4)["result"]["effects"], json!([]));
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/key",
        "params":{"viewId":menu,"version":1,"key":"u","editorMode":"normal",
            "cursor":{"line":0,"column":0}}}));

    let answer = server.response_with_id(5);
    let effects = answer["result"]["effects"].as_array().expect("effects");
    let warn = effects
        .iter()
        .find(|effect| effect["type"] == json!("message") && effect["level"] == json!("warn"))
        .unwrap_or_else(|| panic!("the over-limit warning: {answer}"));
    assert!(
        warn["text"]
            .as_str()
            .is_some_and(|text| text.contains("document picker")),
        "{warn}"
    );
    let prompt_view = effects
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .and_then(|effect| effect["descriptor"]["id"].as_str())
        .unwrap_or_else(|| panic!("the document prompt: {answer}"))
        .to_owned();

    let mut prompt_text = None;
    while prompt_text.is_none() {
        let message = server.recv();
        server.notifications.push(message.clone());
        if message["method"] == json!("view/render")
            && message["params"]["viewId"] == json!(prompt_view)
        {
            prompt_text = serde_json::from_value(message["params"]["text"].clone()).ok();
        }
    }
    let prompt_text: Vec<String> = prompt_text.expect("prompt text");
    assert!(
        prompt_text
            .iter()
            .any(|line| line.contains("Set the upstream branch (--set-upstream-to=)")),
        "{prompt_text:?}"
    );
    assert!(
        server
            .notifications
            .iter()
            .all(|message| message["method"] != json!("view/pick")),
        "the over-limit preference must not send a native pick: {:?}",
        server.notifications
    );

    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

/// Initialize with a named `clientInfo`, which `TestServer::initialize` omits.
fn initialize_named(server: &mut TestServer, capabilities: Value, name: &str) -> Value {
    let mut message = initialize_message();
    message["params"]["capabilities"] = capabilities;
    message["params"]["clientInfo"] = json!({"name": name, "version": "1"});
    server.send(message);
    server.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    server.response_with_id(1)
}

#[test]
fn pick_client_override_allows_native_beyond_the_declared_limit() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    for branch in ["b1", "b2", "b3", "b4"] {
        tmp.git(&["branch", branch]);
    }
    let config_dir = tempfile::TempDir::new().expect("config tempdir");
    let config_path = config_dir.path().join("config.toml");
    std::fs::write(
        &config_path,
        "[pick]\nmax_items = 10\n[pick.clients]\nTest = 5\n",
    )
    .expect("write config");
    let config = config_path.to_string_lossy().to_string();
    let mut server = TestServer::spawn(&[("MAGIT_LSP_CONFIG", config.as_str())]);
    let mut capabilities = rich_pick_capabilities();
    capabilities["experimental"]["magit"]["pick"]["maxItems"] = json!(2);
    initialize_named(&mut server, capabilities, "Test");
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let opened = invoke_rich(&mut server, 3, &status, "open.branch");
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let menu_render = server.recv();
    assert_eq!(menu_render["method"], json!("view/render"), "{menu_render}");
    assert_eq!(server.recv()["method"], json!("view/keymap"));
    let version = menu_render["params"]["version"].as_u64().unwrap();
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered",
        "params":{"viewId":menu,"version":version}}));

    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/key",
        "params":{"viewId":menu,"version":version,"key":"=","editorMode":"normal","cursor":{"line":0,"column":0}}}));
    assert_eq!(server.response_with_id(4)["result"]["effects"], json!([]));
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/key",
        "params":{"viewId":menu,"version":version,"key":"u","editorMode":"normal","cursor":{"line":0,"column":0}}}));

    let pick = loop {
        let message = server.recv();
        server.notifications.push(message.clone());
        if message["method"] == json!("view/pick") {
            break message;
        }
    };
    let params = &pick["params"];
    assert_eq!(
        params["items"].as_array().unwrap().len(),
        5,
        "the per-client override allows the full native pick: {pick}"
    );

    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/pickRespond",
        "params":{"pickId":params["pickId"],"cancelled":true,"selected":[]}}));
    let _ = server.response_with_id(5);
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn pick_override_absent_names_the_client_in_the_fallback() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    tmp.git(&["branch", "b1"]);
    tmp.git(&["branch", "b2"]);
    let config_dir = tempfile::TempDir::new().expect("config tempdir");
    let config_path = config_dir.path().join("config.toml");
    std::fs::write(&config_path, "[pick]\nmax_items = 10\n").expect("write config");
    let config = config_path.to_string_lossy().to_string();
    let mut server = TestServer::spawn(&[("MAGIT_LSP_CONFIG", config.as_str())]);
    let mut capabilities = rich_pick_capabilities();
    capabilities["experimental"]["magit"]["pick"]["maxItems"] = json!(2);
    initialize_named(&mut server, capabilities, "Test");
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    let opened = invoke_rich(&mut server, 3, &status, "open.branch");
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(server.recv()["method"], json!("view/render"));
    assert_eq!(server.recv()["method"], json!("view/keymap"));

    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/key",
        "params":{"viewId":menu,"version":1,"key":"=","editorMode":"normal",
            "cursor":{"line":0,"column":0}}}));
    assert_eq!(server.response_with_id(4)["result"]["effects"], json!([]));
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/key",
        "params":{"viewId":menu,"version":1,"key":"u","editorMode":"normal",
            "cursor":{"line":0,"column":0}}}));

    let answer = server.response_with_id(5);
    let effects = answer["result"]["effects"].as_array().expect("effects");
    let warn = effects
        .iter()
        .find(|effect| effect["type"] == json!("message") && effect["level"] == json!("warn"))
        .unwrap_or_else(|| panic!("the over-limit warning: {answer}"));
    assert!(
        warn["text"]
            .as_str()
            .is_some_and(|text| text.contains("pick.clients[\"Test\"]")),
        "{warn}"
    );
    assert!(
        server
            .notifications
            .iter()
            .all(|message| message["method"] != json!("view/pick")),
        "the absent override must not send a native pick: {:?}",
        server.notifications
    );

    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}

#[test]
fn rich_choice_item_row_enter_answers_the_pick() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build();
    let mut server = TestServer::spawn(&[]);
    server.initialize(limited_rich(json!(["editDocument"]), json!(["confirm"])));
    let view = open_view(&mut server, 2, &repo_param(tmp.path()));
    let opened = invoke_rich(&mut server, 3, &view, "open.branch");
    let menu = opened["result"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["type"] == json!("pushView"))
        .unwrap()["descriptor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(server.recv()["method"], json!("view/render"));
    assert_eq!(server.recv()["method"], json!("view/keymap"));

    // `=` prefixes the option; `u` picks the upstream branch and opens a
    // document prompt because the client declares no pick.
    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/key",
        "params":{"viewId":menu,"version":1,"key":"=","editorMode":"normal",
            "cursor":{"line":0,"column":0}}}));
    assert_eq!(server.response_with_id(4)["result"]["effects"], json!([]));
    server.send(json!({"jsonrpc":"2.0","id":5,"method":"view/key",
        "params":{"viewId":menu,"version":1,"key":"u","editorMode":"normal",
            "cursor":{"line":0,"column":0}}}));
    let (prompt, version, text) = loop {
        let message = server.recv();
        if message["method"] == json!("view/render")
            && message["params"]["viewId"] != json!(menu)
        {
            let prompt = message["params"]["viewId"].as_str().unwrap().to_owned();
            let version = message["params"]["version"].as_u64().unwrap();
            let text: Vec<String> =
                serde_json::from_value(message["params"]["text"].clone()).unwrap();
            break (prompt, version, text);
        }
    };
    let item = text
        .iter()
        .position(|line| line.strip_prefix("> ").unwrap_or(line).trim_end() == "main")
        .unwrap_or_else(|| panic!("main item row: {text:?}"));

    // Enter on the prompt title is not a row: the prompt stays active.
    server.send(json!({"jsonrpc":"2.0","id":6,"method":"view/key",
        "params":{"viewId":prompt,"version":version,"key":"Enter","editorMode":"normal",
            "cursor":{"line":0,"column":0}}}));
    let non_row = server.response_with_id(6);
    assert!(
        !non_row["result"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["type"] == json!("popView")),
        "{non_row}"
    );
    let refused = invoke_rich(&mut server, 7, &view, "status.stage-all");
    assert!(
        refused["result"]["effects"][0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("pending prompt")),
        "{refused}"
    );

    server.send(json!({"jsonrpc":"2.0","id":8,"method":"view/key",
        "params":{"viewId":prompt,"version":version,"key":"Enter","editorMode":"normal",
            "cursor":{"line":item,"column":0}}}));
    let answered = server.response_with_id(8);
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
                        line.as_str()
                            .is_some_and(|line| line.contains("--set-upstream-to=main"))
                    })
                })
        });
    assert!(
        menu_reflects,
        "menu does not reflect the picked upstream: {:?}",
        server.notifications
    );
    assert!(
        !server
            .notifications
            .iter()
            .chain(after.iter())
            .any(|message| message["method"] == json!("view/pick")),
        "an undeclared pick must not emit view/pick: {:?}",
        server.notifications
    );
    assert!(
        !server
            .notifications
            .iter()
            .chain(after.iter())
            .any(|message| message["method"] == json!("view/prompt")),
        "an undeclared pick must not emit view/prompt: {:?}",
        server.notifications
    );

    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
}
