use lsp_server::Message;
use magit_lsp::{transport, wirelog::WireLog};
use serde_json::json;

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
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"token":"password-literal","home":"/home/alice"}}),
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
    let (tx, rx) = tokio::io::duplex(64);
    drop(tx);
    assert!(transport::read_message(&mut tokio::io::BufReader::new(rx))
        .await
        .unwrap()
        .is_none());
    std::fs::remove_file(path).unwrap();
}
