//! Plain-mode document synchronization store and real stdio coverage.

mod common;

use common::stdio::TestServer;
use magit_lsp::docstore::{Change, DocStore, Encoding};
use serde_json::{json, Value};

/// Receive server frames until `method` arrives, skipping unrelated messages.
fn next_method(server: &mut TestServer, method: &str) -> Value {
    loop {
        let frame = server.recv_raw();
        if frame["method"] == method {
            return frame;
        }
    }
}

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
    let (repo, _) = common::RepoFixture::new()
        .commit("initial", [("f.txt", common::TEN_LINES)])
        .modify_standard()
        .build();
    let path = repo.path().join(".magit");
    common::write(&path, "stale disk text\n");
    let disk_before = std::fs::read(&path).unwrap();
    let uri = format!("file://{}", path.display());
    let root_uri = format!("file://{}", repo.path().display());
    let mut server = TestServer::spawn(&[]);
    let initialized = server.initialize_params(json!({
        "processId": null,
        "rootUri": null,
        "capabilities": {
            "workspace": {"applyEdit": true},
            "general": {"positionEncodings": ["utf-8", "utf-16"]},
            "textDocument": {"codeAction": {"dynamicRegistration": true}}
        },
        "workspaceFolders": [{"uri": root_uri, "name": "repo"}]
    }));
    let caps = &initialized["result"]["capabilities"];
    assert_eq!(caps["textDocumentSync"]["openClose"], true);
    assert_eq!(caps["textDocumentSync"]["change"], 2);
    assert_eq!(caps["textDocumentSync"]["save"]["includeText"], true);
    assert_eq!(caps["positionEncoding"], "utf-8");

    // Opening a presentation document must reach the portable render path
    // rather than the managed-draft rejection.
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"text","version":1,"text":"stale editor text\n"}}}));
    let apply = next_method(&mut server, "workspace/applyEdit");
    assert_eq!(std::fs::read(&path).unwrap(), disk_before);
    let edit = &apply["params"]["edit"];
    let edits = edit["documentChanges"]
        .as_array()
        .map(|documents| documents[0]["edits"].as_array().expect("edits"))
        .unwrap_or_else(|| edit["changes"][&uri].as_array().expect("edits"));
    let authoritative = edits[0]["newText"]
        .as_str()
        .expect("whole-document authoritative text")
        .to_owned();
    assert!(authoritative.contains("f.txt"));
    server.send(json!({"jsonrpc":"2.0","id":apply["id"],"result":{"applied":true}}));
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":2},"contentChanges":[{"text":authoritative}]}}));

    // Normal save and close synchronization applies to the accepted document;
    // the portable path must not reject the lifecycle after the open.
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didSave","params":{"textDocument":{"uri":uri},"text":authoritative}}));
    server.send(json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri}}}));
    server.shutdown_and_exit();
    let (status, messages) = server.finish_with_messages(std::time::Duration::from_secs(10));
    assert!(status.success(), "{messages:?}");
    assert!(messages
        .iter()
        .all(|frame| frame["method"] != "view/render"));
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

#[test]
fn adopting_client_text_stops_the_next_change_from_reverting_it() {
    use magit_lsp::presentation::plain::{DocumentFormat, PlainDocument};

    let mut doc = PlainDocument::new(
        "file:///prompt",
        Encoding::Utf8,
        true,
        DocumentFormat::from_text("server\n"),
    );
    doc.open(1, "server\n");
    assert!(doc.render("server\n").is_none());
    assert!(doc.change(2, "edited\n").is_some());

    doc.adopt(3, "adopted\n".to_string());
    assert!(doc.change(4, "adopted\n").is_none());
}

#[test]
fn observing_confirms_echoes_and_responses_in_either_order() {
    use magit_lsp::presentation::plain::{DocumentFormat, PlainDocument};

    fn controller() -> PlainDocument {
        let mut doc = PlainDocument::new(
            "file:///prompt",
            Encoding::Utf8,
            true,
            DocumentFormat::from_text("server\n"),
        );
        doc.open(1, "server\n");
        assert!(doc.render("server\n").is_none());
        doc
    }

    // Echo first, then response.
    let mut doc = controller();
    let flight = doc.change(2, "edited\n").expect("first repair edit");
    doc.observe(3, "server\n");
    assert!(
        doc.change(4, "server\n").is_none(),
        "an echoed flight with no response must not prepare an edit"
    );
    assert!(
        doc.response(flight.id, true).is_none(),
        "a confirmed echo retires silently"
    );
    assert!(
        doc.change(5, "edited\n").is_some(),
        "a confirmed flight must release the next edit"
    );

    // Response first, then echo.
    let mut doc = controller();
    let flight = doc.change(2, "edited\n").expect("first repair edit");
    assert!(
        doc.response(flight.id, true).is_none(),
        "an unconfirmed response keeps the flight"
    );
    doc.observe(3, "server\n");
    assert!(
        doc.change(4, "edited\n").is_some(),
        "a late echo must retire the flight"
    );
}

#[test]
fn observing_a_rejected_edit_does_not_retry_before_a_new_change() {
    use magit_lsp::presentation::plain::{DocumentFormat, PlainDocument};

    let mut doc = PlainDocument::new(
        "file:///prompt",
        Encoding::Utf8,
        true,
        DocumentFormat::from_text("server\n"),
    );
    doc.open(1, "server\n");
    assert!(doc.render("server\n").is_none());
    let flight = doc.change(2, "edited\n").expect("first repair edit");
    assert!(
        doc.response(flight.id, false).is_none(),
        "a rejected edit is not retried in place"
    );
    doc.observe(2, "edited\n");
    let retry = doc
        .change(2, "edited\n")
        .expect("a new observation retries the rejected edit");
    assert_eq!(retry.id, flight.id + 1);
}

#[test]
fn queued_render_survives_echo_and_response_in_either_order() {
    use magit_lsp::presentation::plain::{text_edits, DocumentFormat, PlainDocument};
    use magit_lsp::tree::Tree;

    fn tree(version: u64, row: &str) -> Tree {
        Tree {
            version,
            lines: vec![row.into()],
            nodes: vec![],
            line_nodes: vec![],
            tokens: vec![],
            symbols: vec![],
            links: vec![],
        }
    }

    // Open on `seed`, start an edit toward the older tree, then queue the newer
    // authoritative render while that edit is still in flight.
    fn controller() -> (PlainDocument, u64) {
        let format = DocumentFormat::from_text("seed\n");
        let mut doc = PlainDocument::new("file:///prompt", Encoding::Utf16, true, format);
        doc.open(1, "seed\n");
        let older = doc
            .render_actions(
                tree(1, "alpha"),
                "view-old".into(),
                1,
                "prompt".into(),
                None,
            )
            .expect("the older tree starts a flight");
        assert!(
            doc.render_actions(tree(2, "beta"), "view-new".into(), 2, "prompt".into(), None)
                .is_none(),
            "a queued render must wait for the in-flight edit"
        );
        (doc, older.id)
    }

    // The queued edit must target the latest observed client version and
    // replace the echoed older text with the newer authoritative text.
    fn expected_edit() -> serde_json::Value {
        json!({"documentChanges":[{
            "textDocument":{"uri":"file:///prompt","version":2},
            "edits": text_edits("alpha\n", "beta\n", Encoding::Utf16)
        }]})
    }

    fn assert_converged(doc: &PlainDocument) {
        let (retained, view_id, generation, scope, _) = doc
            .action_snapshot("beta\n")
            .expect("the newer rendered text resolves its retained actions");
        assert_eq!(retained.lines, vec!["beta"]);
        assert_eq!((view_id, generation, scope), ("view-new", 2, "prompt"));
    }

    // Echo before response: the confirmed echo releases the queued render.
    let (mut doc, older_id) = controller();
    doc.observe(2, "alpha\n");
    let newer = doc
        .response(older_id, true)
        .expect("the confirmed echo releases the queued render");
    assert_eq!(newer.edit, expected_edit());
    doc.observe(3, "beta\n");
    assert_converged(&doc);

    // Response before echo: the late echo releases the queued render.
    let (mut doc, older_id) = controller();
    assert!(
        doc.response(older_id, true).is_none(),
        "an unconfirmed response keeps the flight"
    );
    let newer = doc
        .change(2, "alpha\n")
        .expect("the late echo releases the queued render");
    assert_eq!(newer.edit, expected_edit());
    doc.change(3, "beta\n");
    assert_converged(&doc);
}

#[test]
fn retained_actions_resolve_the_exact_rendered_text() {
    use magit_lsp::presentation::plain::{DocumentFormat, PlainDocument};
    use magit_lsp::tree::Tree;

    let format = DocumentFormat::from_text("alpha\r\nbeta\r\n");
    let tree = Tree {
        version: 5,
        lines: vec!["alpha".into(), "beta".into()],
        nodes: vec![],
        line_nodes: vec![],
        tokens: vec![],
        symbols: vec![],
        links: vec![],
    };
    let text = format.serialize(&tree);
    let mut doc = PlainDocument::new("file:///prompt", Encoding::Utf16, true, format);
    doc.retain_actions(tree, "view", 5, "prompt", None);

    let (retained, view_id, generation, scope, _) =
        doc.action_snapshot(&text).expect("the exact rendered text");
    assert_eq!(retained.lines, vec!["alpha", "beta"]);
    assert_eq!((view_id, generation, scope), ("view", 5, "prompt"));
    assert!(doc.action_snapshot("alpha\r\nbeta").is_none());
}
