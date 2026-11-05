use lsp_server::Message;
use magit_lsp::{transport, wirelog::WireLog};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[path = "common/stdio.rs"]
mod stdio;

#[test]
fn committed_trace_fixtures_parse_and_match_initialize_presets() {
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lsp_traces");
    let mut traces = HashMap::new();
    let mut presets = Vec::new();
    for entry in std::fs::read_dir(&directory).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path
            .extension()
            .is_some_and(|extension| extension == "jsonl")
        {
            let text = std::fs::read_to_string(&path).unwrap();
            let mut rows = Vec::new();
            for (line_number, line) in text
                .lines()
                .enumerate()
                .filter(|(_, line)| !line.trim().is_empty())
            {
                rows.push(
                    serde_json::from_str::<serde_json::Value>(line).unwrap_or_else(|error| {
                        panic!(
                            "{}:{}: invalid JSON: {error}",
                            path.display(),
                            line_number + 1
                        )
                    }),
                );
            }
            traces.insert(name, rows);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
            && name.starts_with("initialize-")
        {
            let preset: serde_json::Value = serde_json::from_slice(
                &std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display())),
            )
            .unwrap_or_else(|error| panic!("{}: invalid JSON: {error}", path.display()));
            presets.push((
                path,
                name.trim_start_matches("initialize-")
                    .trim_end_matches(".json")
                    .to_owned(),
                preset,
            ));
        }
    }
    assert!(
        !traces.is_empty(),
        "no JSONL traces in {}",
        directory.display()
    );
    let preset_count = presets.len();
    for (preset_path, trace_name, preset) in presets {
        let trace_name = format!("{trace_name}.jsonl");
        let rows = traces.get(&trace_name).unwrap_or_else(|| {
            panic!(
                "{}: matching trace {} is missing",
                preset_path.display(),
                directory.join(&trace_name).display()
            )
        });
        let params: Vec<_> = rows
            .iter()
            .filter(|row| row.get("raw").unwrap_or(row).get("method") == Some(&json!("initialize")))
            .map(|row| &row.get("raw").unwrap_or(row)["params"])
            .collect();
        assert_eq!(
            params.len(),
            1,
            "{}: expected one retained initialize request",
            preset_path.display()
        );
        assert_eq!(
            params[0],
            &preset,
            "{}: initialize params differ from {}",
            preset_path.display(),
            trace_name
        );
    }
    assert_eq!(
        preset_count, 4,
        "expected all four current-client initialize presets"
    );
}

#[test]
fn action_sidecars_match_retained_wire_rows_and_state_hashes() {
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lsp_traces");
    let load = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(directory.join(name)).unwrap()).unwrap()
    };
    let read_rows = |name: &str| -> Vec<serde_json::Value> {
        std::fs::read_to_string(directory.join(name))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    };
    fn method(row: &serde_json::Value) -> &str {
        row["raw"]["method"].as_str().unwrap_or("")
    }
    fn changes(row: &serde_json::Value) -> &serde_json::Value {
        &row["raw"]["params"]["contentChanges"]
    }
    let selected = load("plain-neovim-0.12.6-actions.json");
    let selected_rows = read_rows("plain-neovim-0.12.6-actions.jsonl");
    assert_eq!(selected["fixture_source_rows"], json!([9, 14]));
    let selected_indices = selected["fixture_source_rows"].as_array().unwrap();
    let action_metadata = selected["actions"].as_array().unwrap();
    assert_eq!(selected_indices.len(), 2);
    assert_eq!(selected_rows.len(), 2);
    assert_eq!(action_metadata.len(), 3);
    assert_eq!(action_metadata[0]["action"], "visual replacement");
    assert_eq!(action_metadata[0]["keys"], "1G0vllrX<Esc>");
    assert_eq!(action_metadata[1]["action"], "one-character register paste");
    assert_eq!(action_metadata[1]["keys"], "gg0\"ap");
    assert_eq!(
        action_metadata[2]["action"],
        "multi-character register paste"
    );
    assert_eq!(action_metadata[2]["keys"], "gg0\"ap");
    assert_eq!(selected_indices.len(), selected_rows.len());
    for (position, (index, row)) in selected_indices.iter().zip(&selected_rows).enumerate() {
        assert_eq!(index.as_u64().unwrap() as usize, [9, 14][position]);
        assert_eq!(method(row), "textDocument/didChange");
    }
    let visual = &selected_rows[0]["raw"]["params"];
    assert_eq!(visual["textDocument"]["version"], 5);
    assert_eq!(changes(&selected_rows[0])[1]["text"], "XXX");
    assert_eq!(
        changes(&selected_rows[0])[1],
        json!({"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":3}},"rangeLength":3,"text":"XXX"})
    );
    assert_eq!(selected["wire_versions"]["visual replacement"], 5);
    assert_eq!(selected["wire_versions"]["one-character register paste"], 7);
    assert_eq!(
        selected["wire_versions"]["multi-character register paste"],
        8
    );
    assert_eq!(selected["wire_versions"]["combined didChange"], 9);
    assert_eq!(action_metadata[0]["after"]["version"], 5);
    assert_eq!(action_metadata[1]["after"]["version"], 7);
    assert_eq!(action_metadata[2]["after"]["version"], 8);
    let combined = changes(&selected_rows[1]);
    assert_eq!(
        selected_rows[1]["raw"]["params"]["textDocument"]["version"],
        9
    );
    assert!(combined
        .as_array()
        .unwrap()
        .iter()
        .any(|edit| edit["text"] == "Q"));
    assert!(combined
        .as_array()
        .unwrap()
        .iter()
        .any(|edit| edit["text"] == "MULTI"));
    assert_eq!(combined[1]["range"]["start"]["character"], 1);
    assert_eq!(combined[2]["range"]["start"]["character"], 1);
    assert_eq!(combined[3]["range"]["start"]["character"], 1);
    assert_eq!(
        combined,
        &json!([
            {"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":3}},"rangeLength":3,"text":"Hea"},
            {"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":1}},"rangeLength":0,"text":"Q"},
            {"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":1}},"rangeLength":0,"text":"MULTI"},
            {"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":6}},"rangeLength":5,"text":""}
        ])
    );

    let helix = load("helix-25.07.1-actions.json");
    let helix_rows = read_rows("helix-25.07.1-actions.jsonl");
    assert_eq!(helix["source_rows"], json!([10, 16, 22]));
    let expected = [
        (
            "selection_replace",
            10,
            "XXX",
            2,
            json!(["Escape", "g", "g", "g", "h", "v", "l", "l", "r", "X"]),
        ),
        (
            "named_register_paste_one",
            16,
            "H",
            4,
            json!([
                "Escape", "g", "g", "g", "h", "\"", "a", "y", "Escape", "g", "g", "g", "h", "\"",
                "a", "p"
            ]),
        ),
        (
            "named_register_paste_multi",
            22,
            "Hea",
            6,
            json!([
                "Escape", "g", "g", "g", "h", "v", "l", "l", "\"", "b", "y", "Escape", "g", "g",
                "g", "h", "\"", "b", "p"
            ]),
        ),
    ];
    assert_eq!(helix["actions"].as_array().unwrap().len(), 3);
    assert_eq!(helix_rows.len(), 3);
    assert_eq!(helix["source_rows"].as_array().unwrap().len(), 3);
    for (position, (action, (name, row_number, text, version, keys))) in helix["actions"]
        .as_array()
        .unwrap()
        .iter()
        .zip(expected)
        .enumerate()
    {
        assert_eq!(action["name"], name);
        assert_eq!(action["keys"], keys);
        assert_eq!(action["wire_rows"], json!([row_number, row_number + 2]));
        let row = &helix_rows[position];
        assert_eq!(method(row), "textDocument/didChange");
        assert_eq!(row["raw"]["params"]["textDocument"]["version"], version);
        assert_eq!(changes(row)[0]["text"], text);
        assert_eq!(changes(row)[0]["range"]["start"]["line"], 0);
        assert_eq!(changes(row)[0]["range"]["end"]["line"], 0);
        assert_eq!(
            changes(row)[0]["range"]["end"]["character"],
            if name == "selection_replace" { 3 } else { 1 }
        );
        assert_eq!(
            changes(row)[0]["range"]["start"]["character"],
            if name == "selection_replace" { 0 } else { 1 }
        );
        assert_eq!(action["before"], action["after"]);
    }

    let rapid = load("plain-neovim-0.12.6-rapid-repair.json");
    let rapid_rows = read_rows("plain-neovim-0.12.6-rapid-repair.jsonl");
    assert_eq!(rapid["fixture_source_rows"], json!([9, 10, 13, 14]));
    assert_eq!(rapid["fixture_source_rows"].as_array().unwrap().len(), 4);
    assert_eq!(rapid_rows.len(), 4);
    let rapid_action = &rapid["actions"]["rapid_input"];
    assert_eq!(rapid_action["keys"], "gg0iRAPID<Esc>");
    assert_eq!(rapid_action["wire_row_offset"], 8);
    let hash = |text: &str| format!("{:x}", Sha256::digest(text.as_bytes()));
    assert_eq!(
        hash(rapid_action["before"]["text"].as_str().unwrap()),
        rapid_action["before_sha256"]
    );
    assert_eq!(
        hash(rapid_action["after"]["text"].as_str().unwrap()),
        rapid_action["after_sha256"]
    );
    assert_eq!(
        hash(
            rapid["actions"]["repair"]["restored"]["text"]
                .as_str()
                .unwrap()
        ),
        rapid["actions"]["repair"]["restored_sha256"]
    );
    let before = rapid_action["before"]["text"].as_str().unwrap();
    let after = rapid_action["after"]["text"].as_str().unwrap();
    let restored = rapid["actions"]["repair"]["restored"]["text"]
        .as_str()
        .unwrap();
    let undo_before = rapid["actions"]["undo"]["before"]["text"].as_str().unwrap();
    let undo_after = rapid["actions"]["undo"]["after"]["text"].as_str().unwrap();
    assert_eq!(after, format!("RAPID{before}"));
    assert_eq!(restored, before);
    assert_eq!(undo_before, restored);
    assert_eq!(undo_after, after);
    assert_eq!(rapid_action["after"]["version"], 5);
    assert_eq!(rapid["actions"]["repair"]["restored"]["version"], 6);
    assert_eq!(rapid["actions"]["undo"]["before"]["version"], 6);
    assert_eq!(rapid["actions"]["undo"]["after"]["version"], 7);
    let source_rows = rapid["fixture_source_rows"].as_array().unwrap();
    let retained_index = |row: &serde_json::Value| {
        source_rows
            .iter()
            .position(|source_row| source_row == row)
            .unwrap()
    };
    let did = retained_index(&rapid_action["didChange_source_row"]);
    let repair = &rapid["actions"]["repair"];
    let request = retained_index(&repair["request_source_row"]);
    let reply = retained_index(&repair["reply_source_row"]);
    let undo = retained_index(&repair["editor_echo_source_row"]);
    let undo_action = &rapid["actions"]["undo"];
    assert_eq!(undo_action["keys"], "u");
    let undo_change = retained_index(&undo_action["didChange_source_row"]);
    assert_eq!(undo_action["didChange_source_row"], 14);
    assert_eq!(undo_change, 3);
    assert_eq!([did, request, reply, undo], [0, 1, 2, 3]);
    assert_eq!(undo_change, undo);
    assert!(did < request && request < reply && reply < undo);
    assert_eq!(method(&rapid_rows[did]), "textDocument/didChange");
    assert_eq!(changes(&rapid_rows[did])[1]["text"], "RAPID");
    assert_eq!(changes(&rapid_rows[did]).as_array().unwrap().len(), 2);
    assert_eq!(
        changes(&rapid_rows[did])[1],
        json!({"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"rangeLength":0,"text":"RAPID"})
    );
    let request_row = &rapid_rows[request];
    assert_eq!(request_row["direction"], "out");
    assert_eq!(method(request_row), "workspace/applyEdit");
    assert_eq!(request_row["raw"]["id"], repair["request_id"]);
    let uri = request_row["raw"]["params"]["edit"]["changes"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap();
    let repair_edit = &request_row["raw"]["params"]["edit"]["changes"][uri][0];
    assert_eq!(
        repair_edit,
        &json!({"newText":"","range":{"start":{"line":0,"character":0},"end":{"line":0,"character":5}}})
    );
    assert_eq!(repair_edit["newText"], "");
    assert_eq!(repair_edit["range"]["start"]["line"], 0);
    assert_eq!(repair_edit["range"]["start"]["character"], 0);
    assert_eq!(repair_edit["range"]["end"]["line"], 0);
    assert_eq!(repair_edit["range"]["end"]["character"], 5);
    assert_eq!(
        rapid_rows[did]["raw"]["params"]["textDocument"]["uri"],
        uri.as_str()
    );
    let reply_row = &rapid_rows[reply];
    assert_eq!(repair["reply"], *reply_row);
    assert_eq!(reply_row["direction"], "in");
    assert_eq!(reply_row["raw"]["id"], repair["request_id"]);
    assert_eq!(reply_row["raw"]["result"]["applied"], true);
    assert_eq!(
        rapid_rows[undo]["raw"]["params"]["textDocument"]["uri"],
        uri.as_str()
    );
    assert_eq!(
        rapid_rows[undo]["raw"]["params"]["textDocument"]["version"],
        rapid["actions"]["undo"]["after"]["version"]
    );
    assert_eq!(
        changes(&rapid_rows[undo]),
        &json!([
            {"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":5}},"rangeLength":5,"text":""},
            {"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"rangeLength":0,"text":"RAPID"}
        ])
    );
}

#[test]
fn records_actual_server_transport_and_drains_at_shutdown() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("wire.jsonl");
    let path_text = path.to_string_lossy().into_owned();
    let mut server = stdio::TestServer::spawn(&[("MAGIT_LSP_WIRELOG", &path_text)]);
    server.initialize(json!({"general":{"positionEncodings":["utf-16"]}}));
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
    let rows = std::fs::read_to_string(&path).unwrap();
    assert!(rows.contains("\"direction\":\"in\""));
    assert!(rows.contains("\"direction\":\"out\""));
    assert!(rows.contains("shutdown"));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn actual_server_without_wirelog_keeps_stdout_clean_and_creates_no_log() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("wire.jsonl");
    let mut server = stdio::TestServer::spawn(&[]);
    server.initialize(json!({"general":{"positionEncodings":["utf-16"]}}));
    server.shutdown_and_exit();
    let (status, _) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success());
    assert!(!path.exists());
}

#[test]
fn concurrent_servers_append_complete_rows_without_truncating() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("shared.jsonl");
    let path_text = path.to_string_lossy().into_owned();
    let mut first = stdio::TestServer::spawn(&[("MAGIT_LSP_WIRELOG", &path_text)]);
    let mut second = stdio::TestServer::spawn(&[("MAGIT_LSP_WIRELOG", &path_text)]);
    first.initialize(json!({"general":{"positionEncodings":["utf-16"]}}));
    second.initialize(json!({"general":{"positionEncodings":["utf-16"]}}));
    let first_pid = first.pid();
    let second_pid = second.pid();
    assert_ne!(first_pid, second_pid);
    let first_messages: Vec<_> = (0..12)
        .map(|i| format!("first-{i:02}-{}", "x".repeat(4096)))
        .collect();
    let second_messages: Vec<_> = (0..12)
        .map(|i| format!("second-{i:02}-{}", "y".repeat(4096)))
        .collect();
    for (first_message, second_message) in first_messages.iter().zip(&second_messages) {
        first.send(json!({"jsonrpc":"2.0","method":"window/logMessage","params":{"type":3,"message":first_message}}));
        second.send(json!({"jsonrpc":"2.0","method":"window/logMessage","params":{"type":3,"message":second_message}}));
    }
    first.shutdown_and_exit();
    second.shutdown_and_exit();
    let first_sent = first.sent.clone();
    let second_sent = second.sent.clone();
    assert!(first
        .finish_with_messages(std::time::Duration::from_secs(10))
        .0
        .success());
    assert!(second
        .finish_with_messages(std::time::Duration::from_secs(10))
        .0
        .success());
    let text = std::fs::read_to_string(path).unwrap();
    assert!(!text.as_bytes().contains(&0));
    let rows: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), first_sent.len() + second_sent.len() + 4);
    assert!(rows.iter().all(|row| {
        [json!(first_pid), json!(second_pid)].contains(&row["pid"])
            && ["in", "out"].contains(&row["direction"].as_str().unwrap())
    }));
    for (pid, sent, expected) in [
        (first_pid, first_sent, first_messages),
        (second_pid, second_sent, second_messages),
    ] {
        let actual: Vec<_> = rows.iter().filter(|row| row["pid"] == pid).collect();
        assert_eq!(actual.len(), sent.len() + 2);
        let expected_in: Vec<_> = sent.iter().filter(|m| m.get("method").is_some()).collect();
        let actual_in: Vec<_> = actual
            .iter()
            .filter(|row| row["direction"] == "in")
            .map(|row| &row["raw"])
            .collect();
        assert_eq!(actual_in.len(), expected_in.len());
        for (actual, sent) in actual_in.iter().zip(expected_in) {
            if sent["method"] == "window/logMessage" {
                assert_eq!(actual["params"]["message"], sent["params"]["message"]);
            } else {
                assert_eq!(actual["method"], sent["method"]);
            }
        }
        let recorded: Vec<_> = actual_in
            .iter()
            .filter_map(|message| message["params"]["message"].as_str())
            .collect();
        assert_eq!(
            recorded,
            expected.iter().map(String::as_str).collect::<Vec<_>>()
        );
        let actual_out: Vec<_> = actual
            .iter()
            .filter(|row| row["direction"] == "out")
            .map(|row| &row["raw"])
            .collect();
        assert_eq!(actual_out.len(), 2);
        assert_eq!(actual_out[0]["id"], 1);
        assert_eq!(actual_out[1]["id"], 99);
        assert!(actual_out
            .iter()
            .all(|message| message.get("result").is_some()));
    }
}

#[test]
fn recorder_restart_preserves_existing_rows() {
    let path = std::env::temp_dir().join(format!("wirelog-restart-{}.jsonl", std::process::id()));
    for method in ["first", "second"] {
        let log = WireLog::new(&path, "").unwrap();
        let message: Message =
            serde_json::from_value(json!({"jsonrpc":"2.0","method":method})).unwrap();
        log.record("in", &message).unwrap();
        log.finish().unwrap();
    }
    let text = std::fs::read_to_string(&path).unwrap();
    let rows: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["raw"]["method"], "first");
    assert_eq!(rows[1]["raw"]["method"], "second");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn unavailable_output_warns_but_server_still_speaks_valid_lsp() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("missing").join("wire.jsonl");
    let path_text = path.to_string_lossy().into_owned();
    let mut server = stdio::TestServer::spawn(&[("MAGIT_LSP_WIRELOG", &path_text)]);
    server.initialize(json!({"general":{"positionEncodings":["utf-16"]}}));
    server.shutdown_and_exit();
    let (status, warning) = server.finish_with_stderr(std::time::Duration::from_secs(10));
    assert!(status.success());
    assert!(
        warning.contains("wirelog") && warning.contains("failed:"),
        "{warning}"
    );
    assert!(!path.exists());
}

#[tokio::test]
async fn records_redacted_order_and_roundtrips_real_frames() {
    let path = std::env::temp_dir().join(format!(
        "wirelog-{}-{}.jsonl",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let log = WireLog::new(&path, "/home/alice").unwrap();
    let inputs = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"token":"password-literal","home":"/home/alice","semanticTokens":{"dynamicRegistration":true}}}),
        json!({"jsonrpc":"2.0","id":1,"result":{"ok":true}}),
        json!({"jsonrpc":"2.0","method":"window/logMessage","params":{"message":"hello"}}),
        json!({"jsonrpc":"2.0","method":"view/promptRespond","params":{"promptId":"p","cancelled":false,"value":"secret-answer"}}),
        json!({"jsonrpc":"2.0","method":"view/prompt","params":{"secret":true,"default":"secret-default"}}),
    ];
    for (i, raw) in inputs.iter().enumerate() {
        let message: Message = serde_json::from_value(raw.clone()).unwrap();
        log.record(if i % 2 == 0 { "in" } else { "out" }, &message)
            .unwrap();
    }
    log.finish().unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    for forbidden in [
        "/home/alice",
        "password-literal",
        "secret-answer",
        "secret-default",
    ] {
        assert!(!text.contains(forbidden), "leaked {forbidden}");
    }
    let rows: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), inputs.len());
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(row["direction"], if i % 2 == 0 { "in" } else { "out" });
        let message: Message = serde_json::from_value(row["raw"].clone()).unwrap();
        let (mut tx, rx) = tokio::io::duplex(2048);
        transport::write_message(&mut tx, &message).await.unwrap();
        drop(tx);
        let mut reader = tokio::io::BufReader::new(rx);
        let replay = transport::read_message(&mut reader).await.unwrap().unwrap();
        let replay = serde_json::to_value(replay).unwrap();
        assert_eq!(replay, row["raw"]);
        assert_eq!(replay["id"], row["raw"]["id"]);
    }
    assert_eq!(rows[3]["raw"]["params"]["value"], "[REDACTED]");
    assert_eq!(rows[4]["raw"]["params"]["default"], "[REDACTED]");
    assert_eq!(
        rows[0]["raw"]["params"]["semanticTokens"]["dynamicRegistration"],
        true
    );
    let (tx, rx) = tokio::io::duplex(64);
    drop(tx);
    assert!(transport::read_message(&mut tokio::io::BufReader::new(rx))
        .await
        .unwrap()
        .is_none());
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn redacts_percent_encoded_home_and_username_forms() {
    let path = std::env::temp_dir().join(format!("wirelog-identity-{}.jsonl", std::process::id()));
    let log = WireLog::new(&path, "/home/alice smith/álîce").unwrap();
    let raw = json!({
        "jsonrpc":"2.0", "method":"window/logMessage", "params":{"message":
            "/home/alice smith/álîce file:///home/alice%20smith/%C3%A1l%C3%AEce file:///home/alice%20smith/%c3%a1l%c3%aece /tmp/alice%20smith semanticTokens"
        }
    });
    log.record("in", &serde_json::from_value(raw).unwrap())
        .unwrap();
    log.finish().unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    for forbidden in [
        "/home/alice smith/álîce",
        "alice smith",
        "álîce",
        "%C3%A1l%C3%AEce",
    ] {
        assert!(!text.contains(forbidden), "leaked {forbidden}");
    }
    let row: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
    let message = row["raw"]["params"]["message"].as_str().unwrap();
    assert!(message.contains("/tmp/alice%20smith"));
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn redaction_preserves_unicode_offsets_and_queued_rows() {
    let path = std::env::temp_dir().join(format!("wirelog-unicode-{}.jsonl", std::process::id()));
    let identity = "/home/alice/İdentity-álîce";
    let log = WireLog::new(&path, identity).unwrap();
    for (id, message) in [
        (1, format!("İ before {identity} İ after")),
        (2, format!("before İ {identity} after İ")),
        (3, format!("İ {identity} İ")),
    ] {
        let message: Message = serde_json::from_value(json!({"jsonrpc":"2.0","id":id,"method":"window/logMessage","params":{"message":message}})).unwrap();
        log.record("in", &message).unwrap();
    }
    log.finish().unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    for forbidden in [identity, "%C3%8Ddentity-%C3%A1l%C3%AEce", "álîce"] {
        assert!(!text.contains(forbidden), "leaked {forbidden}");
    }
    let rows: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 3);
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row["raw"]["id"], index + 1);
        let message = row["raw"]["params"]["message"].as_str().unwrap();
        assert_eq!(message.matches("[REDACTED]").count(), 1);
        assert_eq!(message.matches('İ').count(), 2);
    }
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn unrelated_request_id_does_not_consume_secret_response_correlation() {
    let path = std::env::temp_dir().join(format!("wirelog-collision-{}.jsonl", std::process::id()));
    let log = WireLog::new(&path, "").unwrap();
    for (direction, raw) in [
        (
            "out",
            json!({"jsonrpc":"2.0","id":7,"method":"view/prompt","params":{"secret":true,"default":"secret-input"}}),
        ),
        (
            "in",
            json!({"jsonrpc":"2.0","id":7,"method":"client/other","params":{"value":"unrelated"}}),
        ),
        (
            "in",
            json!({"jsonrpc":"2.0","id":7,"method":"client/other","params":{"value":"unrelated"}}),
        ),
        (
            "in",
            json!({"jsonrpc":"2.0","id":99,"error":{"code":-32001,"message":"unrelated error","data":{"detail":"untouched"}}}),
        ),
        (
            "in",
            json!({"jsonrpc":"2.0","id":7,"error":{"code":-32002,"message":"failed with secret-answer","data":{"nested":{"secret":"secret-answer"},"reason":"denied"}}}),
        ),
    ] {
        let message: Message = serde_json::from_value(raw).unwrap();
        log.record(direction, &message).unwrap();
    }
    log.finish().unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("secret-input"));
    assert!(!text.contains("secret-answer"));
    let rows: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows[1]["raw"]["params"]["value"], "unrelated");
    assert_eq!(rows[2]["raw"]["params"]["value"], "unrelated");
    assert_eq!(rows[3]["raw"]["error"]["message"], "unrelated error");
    assert_eq!(rows[3]["raw"]["error"]["data"]["detail"], "untouched");
    assert_eq!(rows[4]["raw"]["id"], 7);
    assert_eq!(rows[4]["raw"]["error"]["code"], -32002);
    assert_eq!(rows[4]["raw"]["error"]["message"], "[REDACTED]");
    assert_eq!(rows[4]["raw"]["error"]["data"], "[REDACTED]");
    std::fs::remove_file(path).unwrap();
}
