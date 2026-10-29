//! Plain-mode document synchronization store and real stdio coverage.

mod common;

use common::stdio::TestServer;
use magit_lsp::docstore::{Change, DocStore, Encoding};
use serde_json::json;

#[test]
fn sequential_changes_are_atomic_and_encoding_aware() {
    for (encoding, end, astral_end) in [
        (Encoding::Utf8, 3, 5),
        (Encoding::Utf16, 2, 3),
        (Encoding::Utf32, 2, 2),
    ] {
        let mut docs = DocStore::default();
        docs.open("file:///x", "aλ\r\nz😀".into(), 1);
        docs.change(
            "file:///x",
            3,
            &[
                Change {
                    start: Some((0, 1)),
                    end: Some((0, end)),
                    text: "λ!".into(),
                },
                Change {
                    start: Some((1, 1)),
                    end: Some((1, astral_end)),
                    text: "😀".into(),
                },
            ],
            encoding,
        )
        .unwrap();
        assert_eq!(docs.get("file:///x").unwrap().text, "aλ!\r\nz😀");
        assert_eq!(docs.get("file:///x").unwrap().version, 3);
    }
}

#[test]
fn ranged_and_rangeless_changes_reject_atomically_and_track_lifecycle() {
    let mut docs = DocStore::default();
    docs.open("u", "ab\ncd".into(), 1);
    let bad = Change {
        start: Some((0, 99)),
        end: Some((0, 99)),
        text: "x".into(),
    };
    assert!(docs
        .change(
            "u",
            2,
            &[
                Change {
                    start: None,
                    end: None,
                    text: "partial".into()
                },
                bad
            ],
            Encoding::Utf16
        )
        .is_err());
    assert_eq!(docs.get("u").unwrap().text, "ab\ncd");
    assert!(docs.get("u").unwrap().repair);
    assert!(docs.change("u", 1, &[], Encoding::Utf16).is_err());
    docs.change(
        "u",
        4,
        &[Change {
            start: None,
            end: None,
            text: "bundled full text\r\n".into(),
        }],
        Encoding::Utf16,
    )
    .unwrap();
    assert_eq!(docs.get("u").unwrap().text, "bundled full text\r\n");
    assert!(docs.save_text("u", "saved full text").is_err());
    assert_eq!(docs.get("u").unwrap().text, "bundled full text\r\n");
    assert!(!docs.get("u").unwrap().repair);
    assert!(docs.is_open("u"));
    docs.close("u");
    assert!(!docs.is_open("u"));
}

#[test]
fn stdio_advertises_incremental_sync_and_accepts_document_lifecycle() {
    let mut server = TestServer::spawn(&[]);
    let initialized =
        server.initialize(json!({"general":{"positionEncodings":["utf-8","utf-16"]}}));
    let caps = &initialized["result"]["capabilities"];
    assert_eq!(caps["textDocumentSync"]["openClose"], true);
    assert_eq!(caps["textDocumentSync"]["change"], 2);
    assert_eq!(caps["textDocumentSync"]["save"]["includeText"], true);
    assert_eq!(caps["positionEncoding"], "utf-8");
    let uri = "file:///tmp/plain-sync-test";
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"text","version":1,"text":"aλ\r\nz😀"}}}));
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":3},"contentChanges":[{"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":3}},"text":"!"},{"text":"bundled full replacement"}]}}));
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":4},"contentChanges":[{"range":{"start":{"line":99,"character":0},"end":{"line":99,"character":0}},"text":"bad"}]}}));
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":6},"contentChanges":[{"text":"repair full replacement"}]}}));
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didSave","params":{"textDocument":{"uri":uri},"text":"saved replacement"}}));
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri}}}));
    server.shutdown_and_exit();
    assert!(server.finish(std::time::Duration::from_secs(10)).success());
}

#[test]
fn stdio_defaults_to_utf16_when_no_supported_encoding_is_offered() {
    for capabilities in [
        json!({}),
        json!({"general":{"positionEncodings":["unknown"]}}),
    ] {
        let mut server = TestServer::spawn(&[]);
        let initialized = server.initialize(capabilities);
        assert_eq!(
            initialized["result"]["capabilities"]["positionEncoding"],
            "utf-16"
        );
        server.shutdown_and_exit();
        assert!(server.finish(std::time::Duration::from_secs(10)).success());
    }
}

#[test]
fn invalid_unicode_crlf_and_reversed_ranges_require_authoritative_repair() {
    for (encoding, start, end) in [
        (Encoding::Utf8, (0, 2), (0, 3)),  // inside λ
        (Encoding::Utf16, (0, 3), (0, 4)), // inside 😀
        (Encoding::Utf32, (0, 5), (0, 5)), // beyond CRLF boundary
        (Encoding::Utf16, (0, 4), (0, 1)), // reversed
        (Encoding::Utf16, (3, 0), (3, 0)), // nonexistent line
    ] {
        let mut docs = DocStore::default();
        docs.open("u", "aλ😀\r\nz\n".into(), 1);
        let bad = Change {
            start: Some(start),
            end: Some(end),
            text: "x".into(),
        };
        assert!(docs.change("u", 5, &[bad], encoding).is_err());
        assert_eq!(docs.get("u").unwrap().version, 1);
        assert_eq!(docs.get("u").unwrap().text, "aλ😀\r\nz\n");
        // A valid range is still relative to unknown client text after rejection.
        let relative = Change {
            start: Some((0, 0)),
            end: Some((0, 1)),
            text: "b".into(),
        };
        assert!(docs.change("u", 7, &[relative], encoding).is_err());
        assert!(docs.save_text("u", "aλ😀\r\nz\n").is_ok());
        assert!(
            docs.get("u").unwrap().repair,
            "versionless save cannot repair"
        );
        assert!(docs.save_text("u", "unversioned replacement").is_err());
        let full = Change {
            start: None,
            end: None,
            text: "fixed\r\n".into(),
        };
        assert!(docs.change("u", 6, &[full.clone()], encoding).is_err());
        docs.change(
            "u",
            9,
            &[
                full,
                Change {
                    start: Some((1, 0)),
                    end: Some((1, 0)),
                    text: "tail".into(),
                },
            ],
            encoding,
        )
        .unwrap();
        assert_eq!(docs.get("u").unwrap().text, "fixed\r\ntail");
        assert!(!docs.get("u").unwrap().repair);
    }
}

#[test]
fn changes_use_each_preceding_result_and_cannot_split_line_endings() {
    let mut docs = DocStore::default();
    docs.open("u", "a\r\nb\rc\n".into(), 1);
    docs.change(
        "u",
        8,
        &[
            Change {
                start: Some((0, 1)),
                end: Some((1, 0)),
                text: "!".into(),
            },
            Change {
                start: Some((0, 2)),
                end: Some((1, 1)),
                text: "?".into(),
            },
        ],
        Encoding::Utf16,
    )
    .unwrap();
    assert_eq!(docs.get("u").unwrap().text, "a!?\n");
    let before = docs.get("u").unwrap().text.clone();
    assert!(docs
        .change(
            "u",
            10,
            &[
                Change {
                    start: Some((0, 0)),
                    end: Some((0, 0)),
                    text: "first".into()
                },
                Change {
                    start: Some((0, 99)),
                    end: Some((0, 99)),
                    text: "invalid".into()
                },
            ],
            Encoding::Utf16
        )
        .is_err());
    assert_eq!(docs.get("u").unwrap().text, before);
    docs.close("u");
    assert!(docs.save_text("u", "closed").is_err());
    assert!(docs.change("u", 11, &[], Encoding::Utf16).is_err());
    docs.open("u", "reopened".into(), 1);
    assert!(!docs.get("u").unwrap().repair);
}
