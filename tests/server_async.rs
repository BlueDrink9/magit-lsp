//! The real server over real pipes: framing, pipelining, concurrency, ordering.

mod common;

use common::stdio::*;
use common::*;
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

fn repo_param(path: &std::path::Path) -> Value {
    json!(path.to_string_lossy())
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

fn shutdown_and_exit(server: &mut TestServer) {
    server.send(json!({"jsonrpc": "2.0", "id": 99, "method": "shutdown", "params": null}));
    let response = server.response_with_id(99);
    assert_eq!(response["result"], Value::Null);
    server.send(json!({"jsonrpc": "2.0", "method": "exit", "params": null}));
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

#[test]
#[cfg(debug_assertions)]
fn a_slow_mutation_does_not_delay_a_later_read() {
    let (tmp, _repo) = init_repo();
    write(&tmp.path().join("f.txt"), "line1\nCHANGED\nline3\n");
    let repo = repo_param(tmp.path());

    // The delay hook is compiled only into debug builds; see `server::maybe_test_delay`.
    let mut server = TestServer::spawn(&[("MAGIT_LSP_TEST_DELAY_MS", "500")]);
    let response = server.initialize(json!({}));
    assert_eq!(
        response["result"]["capabilities"]["foldingRangeProvider"],
        json!(true),
        "initialize response: {response}"
    );

    let view_id = open_view(&mut server, 2, &repo);

    let mut batch = Vec::new();
    batch.extend(frame(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "view/invoke",
        "params": {"viewId": view_id, "command": "status.stage-all"},
    })));
    batch.extend(frame(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "view/resync",
        "params": {"viewId": view_id},
    })));
    server.send_raw(&batch);

    let first_response = loop {
        let message = server.recv();
        if message["id"] == json!(3) || message["id"] == json!(4) {
            break message;
        }
    };
    assert_eq!(
        first_response["id"],
        json!(4),
        "the slow mutation blocked a later read: {first_response}"
    );
    let second = server.response_with_id(3);
    assert_eq!(second["id"], json!(3));

    shutdown_and_exit(&mut server);
    let status = server.finish(Duration::from_secs(10));
    assert!(status.success(), "server exited: {status:?}");
}

#[test]
fn mutations_for_one_repository_apply_in_request_order() {
    let (tmp, _repo) = init_repo();
    write(&tmp.path().join("f.txt"), "line1\nCHANGED\nline3\n");
    let repo = repo_param(tmp.path());

    let mut server = TestServer::spawn(&[]);
    let response = server.initialize(json!({}));
    assert_eq!(
        response["result"]["capabilities"]["foldingRangeProvider"],
        json!(true),
        "initialize response: {response}"
    );

    let view_id = open_view(&mut server, 2, &repo);

    let mut batch = Vec::new();
    batch.extend(frame(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "view/invoke",
        "params": {"viewId": view_id, "command": "status.stage-all"},
    })));
    batch.extend(frame(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "view/invoke",
        "params": {"viewId": view_id, "command": "status.unstage-all"},
    })));
    server.send_raw(&batch);

    let staged = server.response_with_id(3);
    assert_eq!(staged["result"]["effects"], json!([]), "{staged}");
    let unstaged = server.response_with_id(4);
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
    let text = initial["params"]["text"]
        .as_array()
        .expect("initial full text");
    let line = text
        .iter()
        .position(|line| {
            line.as_str()
                .is_some_and(|line| line.trim_start().starts_with('+'))
        })
        .expect("a '+' line in the rendered status");

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

    let line = render["params"]["text"]
        .as_array()
        .expect("initial full text")
        .iter()
        .position(|line| {
            line.as_str()
                .is_some_and(|line| line.trim_start().starts_with('+'))
        })
        .expect("a '+' line in the rendered status");

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

    let option_render = option_render.expect("the option render");
    let text = option_render["params"]["text"]
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

/// Put a fake `git` first on the server's `PATH`; it delegates everything to
/// the real Git except `add`, which signals its start and then sleeps.
fn fake_git_env(marker: &Path) -> (tempfile::TempDir, Vec<(String, String)>) {
    let output = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .expect("locate git");
    assert!(output.status.success(), "git is not on PATH");
    let real_git = String::from_utf8_lossy(&output.stdout).trim().to_string();

    let dir = tempfile::TempDir::new().expect("fake git tempdir");
    let script = dir.path().join("git");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\n\
             if [ \"$1\" = \"add\" ]; then\n\
               printf 'started\\n' > '{}'\n\
               exec sleep 30\n\
             fi\n\
             exec '{}' \"$@\"\n",
            marker.display(),
            real_git
        ),
    )
    .expect("write fake git");
    let mut permissions = std::fs::metadata(&script)
        .expect("fake git metadata")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).expect("chmod fake git");

    let path = format!(
        "{}:{}",
        dir.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    (dir, vec![("PATH".to_string(), path)])
}

fn spawn_with_env(env: &[(String, String)]) -> TestServer {
    let refs: Vec<(&str, &str)> = env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    TestServer::spawn(&refs)
}

fn wait_for_file(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "{} never appeared",
            path.display()
        );
        thread::sleep(Duration::from_millis(10));
    }
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

#[test]
fn a_cancel_notification_aborts_a_slow_mutation() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let marker = tmp.path().join("fake-git-started");
    let (_bin, env) = fake_git_env(&marker);
    let repo = repo_param(tmp.path());
    let mut server = spawn_with_env(&env);
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

    wait_for_file(&marker);
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
