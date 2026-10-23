//! Text mode end to end (R-2.6): mode switching, keymap narrowing, queued
//! refreshes, positional-request suppression and `common.copy-value`.

mod common;

use common::stdio::*;
use common::*;
use magit_lsp::config::{Config, Profile};
use magit_lsp::keymap::{self, Keymap};
use magit_lsp::protocol::{
    Cursor, Effect, Selection, ViewInvokeParams, ViewKeyParams, ViewSetModeParams,
};
use magit_lsp::session::Session;
use magit_lsp::tree::NodeKind;
use magit_lsp::view::{Placement, ViewKind, Viewport};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

fn keymaps(config: &Config) -> Arc<BTreeMap<String, Keymap>> {
    Arc::new(keymap::validate(config).expect("resolve keymaps"))
}

fn open_status(session: &mut Session) -> u64 {
    let (_descriptor, render) = session
        .open(
            "v1",
            ViewKind::Status,
            Placement::Main,
            Viewport { cols: 80, rows: 24 },
        )
        .expect("open status view");
    session.ack("v1", render.version).expect("ack");
    render.version
}

fn session_with(profile: Profile, repo: &std::path::Path) -> Session {
    let config = Config {
        profile,
        ..Default::default()
    };
    let mut session = Session::new(repo.to_string_lossy());
    session.set_keymaps(keymaps(&config));
    session
}

fn invoke(session: &mut Session, command: &str) -> magit_lsp::session::InvokeResult {
    session
        .invoke(&ViewInvokeParams {
            view_id: "v1".to_string(),
            command: command.to_string(),
            args: None,
            cursor: None,
            selection: None,
        })
        .expect("invoke")
}

fn press(session: &mut Session, key: &str, cursor: Cursor) -> magit_lsp::session::InvokeResult {
    session
        .key(&ViewKeyParams {
            view_id: "v1".to_string(),
            version: 1,
            key: key.to_string(),
            editor_mode: magit_lsp::keymap::EditorMode::Normal,
            cursor,
            selection: None,
        })
        .expect("view/key")
}

fn first_plus_line(session: &Session) -> Cursor {
    let tree = session.views().get("v1").expect("view").tree();
    let line = tree
        .line_nodes
        .iter()
        .position(|line| line.kind == NodeKind::Line && line.marker == Some('+'))
        .expect("a '+' line");
    Cursor { line, column: 0 }
}

fn message(effect: &Effect) -> &str {
    match effect {
        Effect::Message { text, .. } => text,
        other => panic!("expected a message, got {other:?}"),
    }
}

#[test]
fn text_mode_narrows_the_keymap_and_refuses_every_other_command() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = session_with(Profile::Vim, tmp.path());
    open_status(&mut session);
    let cursor = first_plus_line(&session);

    let entered = invoke(&mut session, "common.toggle-text-mode");
    assert!(entered.effects.is_empty(), "{:?}", entered.effects);
    assert!(
        entered.render.is_none(),
        "entering text mode renders nothing"
    );
    let keymap = entered.keymap.expect("a keymap notice");
    assert_eq!(keymap.mode, magit_lsp::view::ViewMode::Text);
    assert_eq!(keymap.bindings.len(), 1, "only the toggle stays bound");
    assert_eq!(keymap.bindings[0].command, "common.toggle-text-mode");
    assert!(!keymap.capture);

    // The toggle binding in the text-mode keymap replaces the full one.
    let notice = session.keymap_notice("v1").expect("keymap");
    assert_eq!(notice.mode, magit_lsp::view::ViewMode::Text);
    assert_eq!(notice.bindings.len(), 1);

    for outcome in [
        press(&mut session, "s", cursor),
        invoke(&mut session, "status.stage"),
    ] {
        assert_eq!(outcome.effects.len(), 1, "{:?}", outcome.effects);
        assert_eq!(
            message(&outcome.effects[0]),
            "Text mode: press ctrl+t to return"
        );
        assert!(outcome.render.is_none());
    }
    assert_eq!(cached_diff(tmp.path()), "", "no Git operation may run");
}

#[test]
fn a_refresh_in_text_mode_is_queued_and_applied_on_exit() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = session_with(Profile::Vim, tmp.path());
    open_status(&mut session);
    let cursor = first_plus_line(&session);

    session
        .set_mode(&ViewSetModeParams {
            view_id: "v1".to_string(),
            mode: magit_lsp::view::ViewMode::Text,
        })
        .expect("enter text mode");

    // An external change arrives while editing as text.
    git(tmp.path(), &["add", "f.txt"]);
    let queued = session.refresh("v1").expect("refresh");
    assert!(queued.is_none(), "text mode queues refreshes");
    let current = session.views().get("v1").expect("view").tree();
    assert!(
        current
            .lines
            .iter()
            .any(|line| line == "Unstaged changes (1)"),
        "the queued refresh must not render yet: {:?}",
        current.lines
    );

    let exited = press(&mut session, "ctrl+t", cursor);
    let render = exited.render.expect("exiting text mode renders");
    assert!(
        render.text.is_some(),
        "the client's text diverged, so the exit is a full replace"
    );
    assert!(render.base_version.is_none());
    assert_eq!(render.mode, magit_lsp::view::ViewMode::Interactive);
    let text = render.text.expect("full text");
    assert!(
        text.iter().any(|line| line == "Staged changes (1)"),
        "the exit render reflects the external stage: {text:?}"
    );
    assert!(render.cursor.is_some(), "the cursor follows its node");

    let keymap = exited.keymap.expect("the full keymap is pushed back");
    assert_eq!(keymap.mode, magit_lsp::view::ViewMode::Interactive);
    assert!(keymap.bindings.len() > 1, "the toggle is no longer alone");
}

#[test]
fn copy_value_returns_the_right_text_per_node_kind() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = session_with(Profile::Vim, tmp.path());
    open_status(&mut session);

    let tree = session.views().get("v1").expect("view").tree().clone();
    let line_of = |kind: NodeKind, n: usize| -> usize {
        tree.line_nodes
            .iter()
            .enumerate()
            .filter(|(_, line)| line.kind == kind)
            .nth(n)
            .map(|(index, _)| index)
            .expect("line of kind")
    };
    let node = |line: usize| tree.node(&tree.line_nodes[line].node).expect("node");

    // File node copies the repo-relative path.
    let file = line_of(NodeKind::File, 0);
    assert_eq!(copy_at(&mut session, file, None), "f.txt");

    // A hunk header copies the patch hunk text (header + marked lines).
    let hunk = line_of(NodeKind::Hunk, 0);
    let hunk_text = copy_at(&mut session, hunk, None);
    assert!(hunk_text.starts_with("@@ -1,10 +1,10 @@"), "{hunk_text:?}");
    assert!(hunk_text.contains("\n+NEW line"), "{hunk_text:?}");
    assert!(
        !hunk_text.contains("    @@"),
        "the patch header is not indented"
    );

    // A diff line copies its content without the marker.
    let added = tree
        .line_nodes
        .iter()
        .position(|line| line.text.as_deref() == Some("NEW line"))
        .expect("the added line");
    assert_eq!(copy_at(&mut session, added, None), "NEW line");

    // A selection copies the selected lines as diff text, markers included.
    let selected = copy_at(&mut session, added, Some((added, added + 1)));
    assert!(selected.starts_with("+NEW line\n"), "{selected:?}");

    // A section header copies its title.
    let section = line_of(NodeKind::Section, 1);
    assert_eq!(copy_at(&mut session, section, None), "Unstaged changes");
    assert!(node(section).label == "Unstaged changes");

    // Nothing copyable under the cursor is an info message, not a clipboard.
    let header = line_of(NodeKind::Header, 0);
    let outcome = copy_invoke(&mut session, Some(header), None);
    assert_eq!(outcome.effects.len(), 1);
    assert_eq!(message(&outcome.effects[0]), "Nothing to copy here");
}

fn copy_invoke(
    session: &mut Session,
    line: Option<usize>,
    selection: Option<(usize, usize)>,
) -> magit_lsp::session::InvokeResult {
    session
        .invoke(&ViewInvokeParams {
            view_id: "v1".to_string(),
            command: "common.copy-value".to_string(),
            args: None,
            cursor: line.map(|line| Cursor { line, column: 0 }),
            selection: selection.map(|(start_line, end_line)| Selection {
                start_line,
                end_line,
            }),
        })
        .expect("invoke copy-value")
}

fn copy_at(session: &mut Session, line: usize, selection: Option<(usize, usize)>) -> String {
    let outcome = copy_invoke(session, Some(line), selection);
    match outcome.effects.as_slice() {
        [Effect::Clipboard { text }] => text.clone(),
        other => panic!("expected one clipboard effect, got {other:?}"),
    }
}

// --- The real server proves positional requests go quiet in text mode. ---

#[test]
fn positional_requests_return_null_in_text_mode() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut server = TestServer::spawn(&[(magit_lsp::logging::LEVEL_ENV, "warn")]);
    server.initialize(json!({"keymapDelivery": "dynamic"}));

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "view/open",
        "params": {
            "kind": "status",
            "args": {"repo_path": tmp.path().to_string_lossy()},
            "viewport": {"cols": 80, "rows": 24},
        },
    }));
    let opened = server.response_with_id(2);
    let view_id = opened["result"]["id"]
        .as_str()
        .expect("view id")
        .to_string();
    let uri = opened["result"]["uri"].as_str().expect("uri").to_string();

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "textDocument/documentSymbol",
        "params": {"textDocument": {"uri": uri}},
    }));
    assert!(
        server.response_with_id(3)["result"].is_array(),
        "interactive documentSymbol returns symbols"
    );

    server.notifications.clear();
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "view/setMode",
        "params": {"viewId": view_id, "mode": "text"},
    }));
    let response = server.response_with_id(4);
    assert!(response["result"]["effects"].is_array());

    let notifications = server.drain_notifications();
    assert!(
        !notifications.iter().any(|n| n["method"] == "view/render"),
        "entering text mode must not render"
    );
    let keymap = notifications
        .iter()
        .find(|n| n["method"] == "view/keymap")
        .expect("a text-mode keymap");
    assert_eq!(keymap["params"]["mode"], "text");
    let bindings = keymap["params"]["bindings"].as_array().expect("bindings");
    assert_eq!(bindings.len(), 1, "only the toggle stays bound");
    assert_eq!(bindings[0]["command"], "common.toggle-text-mode");

    // Every positional request must answer null while the buffer is editable.
    let requests = [
        (
            5,
            "textDocument/documentSymbol",
            json!({"textDocument": {"uri": uri}}),
        ),
        (
            6,
            "textDocument/foldingRange",
            json!({"textDocument": {"uri": uri}}),
        ),
        (
            7,
            "textDocument/semanticTokens/full",
            json!({"textDocument": {"uri": uri}}),
        ),
        (
            8,
            "textDocument/documentLink",
            json!({"textDocument": {"uri": uri}}),
        ),
        (
            9,
            "textDocument/definition",
            json!({
                "textDocument": {"uri": uri},
                "position": {"line": 0, "character": 0},
            }),
        ),
    ];
    for (id, method, params) in requests {
        server.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let response = server.response_with_id(id);
        assert!(
            response["result"].is_null(),
            "{method} must return null in text mode: {response}"
        );
    }

    // Keys other than the toggle are refused without touching the repository.
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "view/key",
        "params": {
            "viewId": view_id,
            "version": 1,
            "key": "s",
            "editorMode": "normal",
            "cursor": {"line": 0, "column": 0},
        },
    }));
    let refused = server.response_with_id(10);
    let effects = refused["result"]["effects"].as_array().expect("effects");
    assert_eq!(effects.len(), 1);
    assert!(
        effects[0]["text"]
            .as_str()
            .expect("message text")
            .starts_with("Text mode: press "),
        "{refused}"
    );
    assert_eq!(cached_diff(tmp.path()), "", "no Git operation may run");

    // Leaving text mode restores the full keymap and replaces the text.
    server.notifications.clear();
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "view/setMode",
        "params": {"viewId": view_id, "mode": "interactive"},
    }));
    server.response_with_id(11);
    let notifications = server.drain_notifications();
    let render = notifications
        .iter()
        .find(|n| n["method"] == "view/render")
        .expect("an exit render");
    assert!(
        render["params"]["text"].is_array(),
        "the exit render is a full replace"
    );
    assert_eq!(render["params"]["mode"], "interactive");
    let keymap = notifications
        .iter()
        .find(|n| n["method"] == "view/keymap")
        .expect("the full keymap");
    assert_eq!(keymap["params"]["mode"], "interactive");
    assert!(
        keymap["params"]["bindings"]
            .as_array()
            .expect("bindings")
            .len()
            > 1
    );

    server.send(json!({"jsonrpc":"2.0","id":99,"method":"shutdown","params":null}));
    server.response_with_id(99);
    server.send(json!({"jsonrpc":"2.0","method":"exit","params":null}));
    assert!(server.finish(Duration::from_secs(10)).success());
}
