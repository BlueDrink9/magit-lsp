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
fn managed_draft_shutdown_cancels_saved_checkpoint_and_rejects_pipelined_late_input() {
    let (tmp, _) = RepoFixture::new().commit("init", [("f.txt", "initial\n")])
        .staged_file("f.txt", "staged\n").build();
    let head = tmp.git(&["rev-parse", "HEAD"]);
    let staged = tmp.git(&["ls-files", "--stage", "-z"]);
    let mut server = TestServer::spawn(&[]);
    server.initialize(json!({}));
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/invoke","params":{"viewId":status,"command":"commit.create"}}));
    let mut response = false;
    let mut edit = None;
    while !response || edit.is_none() {
        let message = server.recv();
        if message["id"] == 4 { response = true; }
        if message["method"] == "view/effects" {
            edit = message["params"]["effects"].as_array().unwrap().iter()
                .find(|e| e["type"] == "editDocument").cloned().or(edit);
        }
    }
    let edit = edit.unwrap();
    let messages = [
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":edit["uri"],"languageId":"gitcommit","version":1,"text":edit["text"]}}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":edit["uri"],"version":7},"contentChanges":[{"text":"Saved but never confirmed\n"}]}}),
        json!({"jsonrpc":"2.0","id":10,"method":"draft/history","params":{"runId":edit["runId"],"uri":edit["uri"],"version":6,"command":"commit-message.history-prev"}}),
    ];
    server.send_raw(&messages.into_iter().flat_map(frame).collect::<Vec<_>>());
    assert!(server.response_with_id(10)["error"].is_object());
    let messages = [
        json!({"jsonrpc":"2.0","id":19,"method":"shutdown","params":null}),
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":edit["uri"],"version":8},"contentChanges":[{"text":"Too late\n"}]}}),
        json!({"jsonrpc":"2.0","id":20,"method":"draft/save","params":{"runId":edit["runId"],"uri":edit["uri"],"version":8}}),
        json!({"jsonrpc":"2.0","id":21,"method":"draft/finish","params":{"runId":edit["runId"],"uri":edit["uri"],"version":7,"reason":"normal-close"}}),
    ];
    server.send_raw(&messages.into_iter().flat_map(frame).collect::<Vec<_>>());
    assert_eq!(server.response_with_id(19)["result"], Value::Null);
    assert!(server.response_with_id(20)["error"].is_object());
    assert!(server.response_with_id(21)["error"].is_object());
    // Shutdown removes the run before late file sync; unrelated files are ignored.
    assert_eq!(tmp.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(tmp.git(&["ls-files", "--stage", "-z"]), staged);
    assert_eq!(read(&tmp.path().join(".git/COMMIT_EDITMSG")), edit["text"].as_str().unwrap());
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
    server.initialize(json!({}));
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

    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
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
    let response = server.initialize(json!({}));
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
    server.initialize(json!({"keymapDelivery": "static"}));

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
fn forward_all_passes_through_unbound_keys_and_dispatches_bound_keys() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let repo = repo_param(tmp.path());
    let mut server = TestServer::spawn(&[]);
    server.initialize(json!({"keymapDelivery": "forwardAll"}));
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
    let render = server.recv();
    assert_eq!(render["method"], json!("view/render"));
    let version = render["params"]["version"]
        .as_u64()
        .expect("render version");
    assert!(render["params"]["text"].is_array());
    let line = first_hunk_line(&mut server, &uri);
    server.send(json!({"jsonrpc":"2.0","method":"view/rendered","params":{"viewId":view_id,"version":version}}));

    server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/key","params":{"viewId":view_id,"version":version,"key":"~","editorMode":"normal","cursor":{"line":line,"column":0}}}));
    let passthrough = server.response_with_id(3);
    assert_eq!(
        passthrough["result"]["effects"],
        json!([{"type":"passThrough","key":"~"}])
    );
    let before_diff = git(tmp.path(), &["diff", "--", "f.txt"]);
    assert!(
        !before_diff.is_empty(),
        "fixture starts with an unstaged change"
    );
    assert_eq!(
        git(tmp.path(), &["diff", "--", "f.txt"]),
        before_diff,
        "unbound key must not mutate the repository"
    );

    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/key","params":{"viewId":view_id,"version":version,"key":"s","editorMode":"normal","cursor":{"line":line,"column":0}}}));
    let mut bound_response = None;
    let mut staged_render = None;
    while bound_response.is_none() || staged_render.is_none() {
        let message = server.recv();
        if message["id"] == json!(4) {
            bound_response = Some(message);
        } else if message["method"] == json!("view/render") {
            staged_render = Some(message);
        }
    }
    assert_eq!(
        bound_response.expect("bound response")["result"]["effects"],
        json!([])
    );
    assert_eq!(
        staged_render.expect("staged render")["params"]["version"],
        json!(version + 1)
    );
    assert_eq!(
        hunk_count(&cached_diff(tmp.path())),
        1,
        "bound stage key stages the repository change"
    );
    shutdown_and_exit(&mut server);
    assert!(server.finish(Duration::from_secs(10)).success());
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
    let response = server.initialize(json!({}));
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
        server.initialize(json!({}));
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
        server.initialize(json!({}));
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
    let response = server.initialize(json!({}));
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
    let response = server.initialize(json!({}));
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
        if message["method"] == json!("view/effects")
            && message["params"]["effects"]
                .as_array()
                .is_some_and(|effects| {
                    effects.iter().any(|effect| {
                        effect["type"] == json!("message") && effect["level"] == json!("error")
                    })
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
    let error = failure["params"]["effects"]
        .as_array()
        .and_then(|effects| {
            effects
                .iter()
                .find(|effect| effect["level"] == json!("error"))
        })
        .unwrap_or_else(|| panic!("no error effect: {failure}"));
    assert_eq!(error["text"], json!("`git commit` failed."), "{error}");
    assert!(
        error["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("pre-commit refused")),
        "the hook output is the detail: {error}"
    );

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
    let response = server.initialize(json!({}));
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
    write(&hook, &format!(
        "#!/bin/sh\ntrap '' TERM\nsh -c 'echo $$ > \"$1\"; touch \"$2\"; exec sleep 60' sh '{}' '{}' </dev/null >/dev/null 2>&1 &\nwait\n",
        child_pid.display(), ready.display()
    ));
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();

    fn start_time(pid: i32) -> Option<u64> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        stat.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()
    }
    fn live(pid: i32) -> bool {
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok().and_then(|stat| stat.rsplit_once(')').map(|(_, tail)| {
                !matches!(tail.trim_start().chars().next(), Some('Z' | 'X'))
            })).unwrap_or(false)
    }

    let mut server = TestServer::spawn(&[]);
    server.initialize(json!({}));
    let status = open_view(&mut server, 2, &repo_param(tmp.path()));
    server.send(json!({"jsonrpc":"2.0","id":4,"method":"view/invoke","params":{"viewId":status,"command":"commit.create"}}));
    let mut edit = None;
    while edit.is_none() {
        let message = server.recv();
        if message["method"] == json!("view/effects") {
            edit = message["params"]["effects"].as_array().unwrap().iter()
                .find(|e| e["type"] == json!("editDocument")).cloned().or(edit);
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
        assert!(Instant::now() < deadline, "commit-msg hook did not become ready");
        thread::sleep(Duration::from_millis(20));
    }
    let child: i32 = read(&child_pid).trim().parse().unwrap();
    let identity = start_time(child).expect("hook descendant identity");
    struct Cleanup(i32, u64);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            if start_time(self.0) == Some(self.1) && live(self.0) {
                unsafe { libc::kill(self.0, libc::SIGKILL); }
            }
        }
    }
    let _owned = Cleanup(child, identity);

    // Disconnect while the hook is blocked; the shutdown path must cancel the
    // owned group rather than orphan it past the server's own exit.
    server.close_stdin();
    let exit = server.finish(Duration::from_secs(15));
    assert!(exit.success(), "server exited cleanly on disconnect: {exit:?}");
    let deadline = Instant::now() + Duration::from_secs(3);
    while live(child) && Instant::now() < deadline { thread::sleep(Duration::from_millis(20)); }
    assert!(!live(child), "owned hook descendant survived the disconnect");
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
