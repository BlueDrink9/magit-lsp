//! stdout is the LSP pipe: it must carry framed JSON-RPC and nothing else.

mod common;

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
}

/// Send `shutdown` then `exit`, and drain the responses.
fn shutdown_and_exit(server: &mut Server, id: i64) {
    server.send(json!({"jsonrpc": "2.0", "id": id, "method": "shutdown", "params": null}));
    server.response_with_id(id);
    server.send(json!({"jsonrpc": "2.0", "method": "exit", "params": null}));
}

#[test]
fn stdout_carries_only_framed_json_rpc_when_logging_to_stderr() {
    let (tmp, _repo) = init_repo();
    write(&tmp.path().join("f.txt"), "line1\nCHANGED\nline3\n");
    // A stale index lock makes the staging mutation fail without writing.
    write(&tmp.path().join(".git/index.lock"), "");

    let mut server = Server::spawn(&[]);
    initialize(&mut server);
    open_status(&mut server, 2, tmp.path());
    server.response_with_id(2);

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
    server.response_with_id(2);
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
