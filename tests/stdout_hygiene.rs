//! stdout is the LSP pipe: it must carry framed JSON-RPC and nothing else.

use magit_lsp_test_support as common;

use common::*;
use serde_json::json;
use std::path::Path;

type Server = common::stdio::TestServer;

/// The protocol prefix every session shares.
fn initialize(server: &mut Server) {
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "processId": null,
            "rootUri": null,
            "capabilities": {},
        },
    }));
    server.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
}

/// Open the status view and wait until the status acquisition has installed its
/// contents. This client negotiates no presentation and may receive no pushed
/// renders, so poll the server-owned render until the loading placeholder is
/// gone (the readiness idiom of tests/plain_harness.rs).
fn open_status(server: &mut Server, id: i64, repo: &Path) {
    server.send(json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "view/open",
        "params": {
            "kind": "status",
            "args": {"repo_path": repo.to_string_lossy()},
            "viewport": {"cols": 80, "rows": 24},
        },
    }));
    let opened = server.response_with_id(id);
    let view_id = opened["result"]["id"]
        .as_str()
        .expect("view id")
        .to_string();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let mut resync_id = id + 100;
    loop {
        assert!(
            std::time::Instant::now() < deadline,
            "status view {view_id} did not publish acquired contents"
        );
        server.send(json!({
            "jsonrpc": "2.0",
            "id": resync_id,
            "method": "view/resync",
            "params": {"viewId": view_id},
        }));
        let resync = server.response_with_id(resync_id);
        let lines = resync["result"]["text"]
            .as_array()
            .unwrap_or_else(|| panic!("full status resync: {resync}"));
        if !lines.is_empty()
            && !lines.iter().any(|line| {
                line.as_str()
                    .is_some_and(|line| line.contains("Loading repository status"))
            })
        {
            break;
        }
        resync_id += 1;
    }
}

/// Send `shutdown` then `exit`, and drain the responses.
fn shutdown_and_exit(server: &mut Server, id: i64) {
    server.send(json!({"jsonrpc": "2.0", "id": id, "method": "shutdown", "params": null}));
    server.response_with_id(id);
    server.send(json!({"jsonrpc": "2.0", "method": "exit", "params": null}));
}

#[test]
fn stdout_carries_only_framed_json_rpc_when_logging_to_stderr() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _repo) = init_repo();
    write(&tmp.path().join("f.txt"), "line1\nCHANGED\nline3\n");
    // A stale index lock makes the staging mutation fail without writing.
    write(&tmp.path().join(".git/index.lock"), "");

    let mut server = Server::spawn(&[]);
    initialize(&mut server);
    open_status(&mut server, 2, tmp.path());

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "view/invoke",
        "params": {"viewId": "v1", "command": "status.stage-all"},
    }));
    let stage = server.response_with_id(3);
    let effects = stage["result"]["effects"]
        .as_array()
        .expect("mutation effects");
    assert_eq!(effects[0]["level"], "error", "failing git op: {stage}");

    shutdown_and_exit(&mut server, 4);
    let (status, stderr) = server.finish_with_stderr(std::time::Duration::from_secs(10));

    assert!(status.success(), "server exited: {stderr}");
    // The failure is logged, but only on stderr.
    assert!(stderr.contains("git failed"), "stderr: {stderr}");
}

#[test]
fn log_file_receives_logs_and_stdout_stays_clean() {
    magit_lsp_test_support::cargo_server_binary!();
    let (tmp, _repo) = init_repo();
    write(&tmp.path().join("f.txt"), "line1\nCHANGED\nline3\n");
    write(&tmp.path().join(".git/index.lock"), "");
    let logs = tempfile::TempDir::new().expect("log tempdir");
    let log_file = logs.path().join("magit-lsp.log");

    let mut server = Server::spawn(&[(
        magit_lsp::logging::FILE_ENV,
        log_file.to_str().expect("log path"),
    )]);
    initialize(&mut server);
    open_status(&mut server, 2, tmp.path());
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "view/invoke",
        "params": {"viewId": "v1", "command": "status.stage-all"},
    }));
    server.response_with_id(3);

    shutdown_and_exit(&mut server, 4);
    let (status, stderr) = server.finish_with_stderr(std::time::Duration::from_secs(10));

    assert!(status.success(), "server exited: {stderr}");
    assert!(stderr.trim().is_empty(), "logs leaked to stderr: {stderr}");

    let written = std::fs::read_to_string(&log_file).expect("read log file");
    assert!(written.contains("git failed"), "log file: {written}");
    assert!(written.contains("index.lock"), "log file: {written}");
}
