//! stdout is the LSP pipe: it must carry framed JSON-RPC and nothing else.

mod common;

use common::*;
use serde_json::{json, Value};
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Run the real server against `repo`, returning its captured streams.
fn run_server(repo: &Path, log_file: Option<&Path>) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_magit-lsp"));
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env(magit_lsp::logging::LEVEL_ENV, "trace")
        .env_remove(magit_lsp::logging::FILE_ENV);
    if let Some(path) = log_file {
        command.env(magit_lsp::logging::FILE_ENV, path);
    }

    let mut child = command.spawn().expect("spawn magit-lsp");
    {
        let stdin = child.stdin.as_mut().expect("child stdin");
        for message in session(repo) {
            stdin.write_all(&frame(message)).expect("write request");
        }
    }
    drop(child.stdin.take());
    child.wait_with_output().expect("wait for magit-lsp")
}

/// A minimal LSP session ending in a failing Git operation.
fn session(repo: &Path) -> Vec<Value> {
    vec![
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": null,
                "rootUri": null,
                "capabilities": {},
            },
        }),
        json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "vcs/stageFile",
            "params": {
                "repo_path": repo.to_string_lossy(),
                "path": "missing.txt",
            },
        }),
        json!({"jsonrpc": "2.0", "id": 3, "method": "shutdown", "params": null}),
        json!({"jsonrpc": "2.0", "method": "exit", "params": null}),
    ]
}

/// Wrap `message` in an LSP `Content-Length` frame.
fn frame(message: Value) -> Vec<u8> {
    let body = serde_json::to_vec(&message).expect("serialise message");
    let mut bytes = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    bytes.extend_from_slice(&body);
    bytes
}

/// Parse `bytes` as a stream of LSP frames, panicking on any stray text.
fn parse_frames(mut bytes: &[u8]) -> Vec<Value> {
    let mut messages = Vec::new();
    while !bytes.is_empty() {
        let header_end = find(bytes, b"\r\n\r\n").expect("stray bytes before a frame header");
        let header = std::str::from_utf8(&bytes[..header_end]).expect("utf8 frame header");
        let length = header
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.trim()
                    .eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .expect("Content-Length header");

        let body_start = header_end + 4;
        let body_end = body_start + length;
        assert!(body_end <= bytes.len(), "truncated frame body");
        let message: Value =
            serde_json::from_slice(&bytes[body_start..body_end]).expect("well-formed JSON-RPC");
        messages.push(message);
        bytes = &bytes[body_end..];
    }
    messages
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// The response carrying `id`, which must have been answered.
fn response_with_id(messages: &[Value], id: i64) -> &Value {
    messages
        .iter()
        .find(|message| message["id"] == id)
        .unwrap_or_else(|| panic!("no response with id {id} in {messages:?}"))
}

#[test]
fn stdout_carries_only_framed_json_rpc_when_logging_to_stderr() {
    let (tmp, _repo) = init_repo();
    let out = run_server(tmp.path(), None);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(out.status.success(), "server exited: {stderr}");
    let messages = parse_frames(&out.stdout);
    assert_eq!(
        response_with_id(&messages, 1)["result"]["capabilities"]["foldingRangeProvider"],
        json!(true)
    );

    let stage = response_with_id(&messages, 2);
    let effects = stage["result"]["effects"]
        .as_array()
        .expect("mutation effects");
    assert_eq!(effects[0]["level"], "error", "failing git op: {stage}");

    // The failure is logged, but only on stderr.
    assert!(stderr.contains("git failed"), "stderr: {stderr}");
}

#[test]
fn log_file_receives_logs_and_stdout_stays_clean() {
    let (tmp, _repo) = init_repo();
    let logs = tempfile::TempDir::new().expect("log tempdir");
    let log_file = logs.path().join("magit-lsp.log");

    let out = run_server(tmp.path(), Some(&log_file));
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(out.status.success(), "server exited: {stderr}");
    parse_frames(&out.stdout);
    assert!(stderr.trim().is_empty(), "logs leaked to stderr: {stderr}");

    let written = std::fs::read_to_string(&log_file).expect("read log file");
    assert!(written.contains("git failed"), "log file: {written}");
    assert!(written.contains("missing.txt"), "log file: {written}");
}

#[test]
fn src_never_writes_to_stdout_directly() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rust_files(&src, &mut files);
    assert!(!files.is_empty(), "no Rust sources under {}", src.display());

    for file in files {
        let source = std::fs::read_to_string(&file).expect("read source");
        for name in ["println!", "print!", "dbg!"] {
            if let Some(offset) = first_macro_hit(&source, name) {
                let line = source[..offset].lines().count();
                panic!(
                    "{}:{line} uses {name}; stdout is the LSP pipe",
                    file.display()
                );
            }
        }
    }
}

fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read src dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension() == Some(OsStr::new("rs")) {
            files.push(path);
        }
    }
}

/// Byte offset of a real `name!` invocation, ignoring `eprintln!` and friends.
fn first_macro_hit(source: &str, name: &str) -> Option<usize> {
    let mut start = 0;
    while let Some(offset) = source[start..].find(name) {
        let index = start + offset;
        let qualified = source[..index]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_');
        if !qualified {
            return Some(index);
        }
        start = index + name.len();
    }
    None
}
