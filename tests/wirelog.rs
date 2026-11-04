use lsp_server::Message;
use magit_lsp::{transport, wirelog::WireLog};
use serde_json::json;

#[path = "common/stdio.rs"]
mod stdio;

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
