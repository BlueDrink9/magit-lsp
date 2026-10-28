//! Text mode end to end (R-2.6): mode switching, keymap narrowing, queued
//! refreshes, positional-request suppression and `common.copy-value`.

mod common;

use common::stdio::*;
use common::*;
use magit_lsp::config::Profile;
use magit_lsp::protocol::{Cursor, Effect, Selection};
use magit_lsp::tree::NodeKind;
use magit_lsp::view::ViewMode;
use serde_json::json;
use std::time::Duration;

fn message(effect: &Effect) -> &str {
    match effect {
        Effect::Message { text, .. } => text,
        other => panic!("expected a message, got {other:?}"),
    }
}

fn copy_invoke(
    test: &mut TestSession,
    line: Option<usize>,
    selection: Option<(usize, usize)>,
) -> magit_lsp::session::InvokeResult {
    let cursor = line.map(|line| Cursor { line, column: 0 });
    let selection = selection.map(|(start_line, end_line)| Selection {
        start_line,
        end_line,
    });
    test.invoke_selection("common.copy-value", cursor, selection)
}

fn copy_at(test: &mut TestSession, line: usize, selection: Option<(usize, usize)>) -> String {
    let outcome = copy_invoke(test, Some(line), selection);
    match outcome.effects.as_slice() {
        [Effect::Clipboard { text }] => text.clone(),
        other => panic!("expected one clipboard effect, got {other:?}"),
    }
}

#[test]
fn text_mode_narrows_the_keymap_and_refuses_every_other_command() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    test.goto_first_marker('+');

    let entered = test.invoke("common.toggle-text-mode");
    assert!(entered.effects.is_empty(), "{:?}", entered.effects);
    assert!(
        entered
            .render
            .as_ref()
            .is_some_and(|render| render.text.is_none()
                && render.edits.is_none()
                && render.cursor.is_none()
                && render.mode == ViewMode::Text),
        "entering text mode sends metadata without content"
    );
    let keymap = entered.keymap.expect("a keymap notice");
    assert_eq!(keymap.mode, ViewMode::Text);
    assert_eq!(keymap.bindings.len(), 1, "only the toggle stays bound");
    assert_eq!(keymap.bindings[0].command, "common.toggle-text-mode");
    assert!(!keymap.capture);

    // The toggle binding in the text-mode keymap replaces the full one.
    let notice = test.keymap_notice().expect("keymap");
    assert_eq!(notice.mode, ViewMode::Text);
    assert_eq!(notice.bindings.len(), 1);

    for outcome in [test.press("s"), test.invoke("status.stage")] {
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
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();
    test.goto_first_marker('+');

    test.set_mode(ViewMode::Text);

    // An external change arrives while editing as text.
    git(tmp.path(), &["add", "f.txt"]);
    let queued = test.refresh();
    assert!(queued.is_none(), "text mode queues refreshes");
    let current = test.text();
    assert!(
        current.iter().any(|line| line == "Unstaged changes (1)"),
        "the queued refresh must not render yet: {current:?}"
    );

    let exited = test.press("ctrl+t");
    let render = exited.render.expect("exiting text mode renders");
    assert!(
        render.text.is_some(),
        "the client's text diverged, so the exit is a full replace"
    );
    assert!(render.base_version.is_none());
    assert_eq!(render.mode, ViewMode::Interactive);
    let text = render.text.expect("full text");
    assert!(
        text.iter().any(|line| line == "Staged changes (1)"),
        "the exit render reflects the external stage: {text:?}"
    );
    assert!(render.cursor.is_some(), "the cursor follows its node");

    let keymap = exited.keymap.expect("the full keymap is pushed back");
    assert_eq!(keymap.mode, ViewMode::Interactive);
    assert!(keymap.bindings.len() > 1, "the toggle is no longer alone");
}

#[test]
fn copy_value_returns_the_right_text_per_node_kind() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _initial) = TestSession::open_status(&tmp, Profile::Vim);
    test.ack();

    let tree = test.tree().clone();
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
    assert_eq!(copy_at(&mut test, file, None), "f.txt");

    // A hunk header copies source lines without diff decoration.
    let hunk = line_of(NodeKind::Hunk, 0);
    let hunk_text = copy_at(&mut test, hunk, None);
    assert!(hunk_text.contains("NEW line"), "{hunk_text:?}");

    // A diff line copies its content without the marker.
    let added = tree
        .line_nodes
        .iter()
        .position(|line| line.text.as_deref() == Some("NEW line"))
        .expect("the added line");
    assert_eq!(copy_at(&mut test, added, None), "NEW line");

    // A selection copies source lines without diff markers.
    let selected = copy_at(&mut test, added, Some((added, added + 1)));
    assert!(selected.starts_with("NEW line\n"), "{selected:?}");

    // A section header copies its title.
    let section = line_of(NodeKind::Section, 1);
    assert_eq!(copy_at(&mut test, section, None), "Unstaged changes");
    assert!(node(section).label == "Unstaged changes");

    // Nothing copyable under the cursor is an info message, not a clipboard.
    let header = line_of(NodeKind::Header, 0);
    let outcome = copy_invoke(&mut test, Some(header), None);
    assert_eq!(outcome.effects.len(), 1);
    assert_eq!(message(&outcome.effects[0]), "Nothing to copy here");
}

#[test]
fn copy_value_preserves_source_markers_whitespace_and_excludes_metadata() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", "\n+literal\n-old\n  context\n\nlast")])
        .build();
    let source = "\n+literal\n+new\n  context\n\nlast";
    std::fs::write(tmp.path().join("f.txt"), source).expect("write changed source");
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(tmp.path())
            .output()
            .unwrap();
        assert!(output.status.success());
        output.stdout
    };
    let before = (
        git(&["status", "--porcelain=v1", "-z"]),
        git(&["diff", "--binary"]),
        git(&["diff", "--cached", "--binary"]),
        git(&["write-tree"]),
        git(&["rev-parse", "HEAD"]),
    );
    let (mut test, _) = TestSession::open_status(&tmp, Profile::Vim);
    let tree = test.tree().clone();
    let hunk = tree
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::Hunk)
        .unwrap();
    let expected = "\n+literal\n-old\n+new\n  context\n\nlast";
    assert_eq!(copy_at(&mut test, hunk.start_line, None), expected);
    let lines: Vec<_> = tree
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Line)
        .collect();
    for line in &lines {
        if matches!(line.marker, Some('+' | '-' | ' ')) {
            assert_eq!(copy_at(&mut test, line.start_line, None), line.label);
            assert_eq!(
                copy_at(
                    &mut test,
                    line.start_line,
                    Some((line.start_line, line.start_line))
                ),
                line.label
            );
        } else {
            let outcome = copy_invoke(&mut test, Some(line.start_line), None);
            assert_eq!(message(&outcome.effects[0]), "Nothing to copy here");
        }
    }
    let first = lines.first().unwrap().start_line;
    let last = lines.last().unwrap().start_line;
    // Headers/annotations are excluded, even with the cursor on the hunk title.
    for range in [(0, last), (last, 0), (hunk.start_line, last)] {
        assert_eq!(copy_at(&mut test, hunk.start_line, Some(range)), expected);
    }
    assert_eq!(
        copy_at(&mut test, first, Some((first, first + 1))),
        "\n+literal"
    );
    assert_eq!(
        copy_at(&mut test, first, Some((first + 3, first + 2))),
        "-old\n+new"
    );
    let outcome = copy_invoke(&mut test, Some(hunk.start_line), Some((0, hunk.start_line)));
    assert_eq!(message(&outcome.effects[0]), "Nothing to copy here");
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
        source
    );
    assert_eq!(
        before,
        (
            git(&["status", "--porcelain=v1", "-z"]),
            git(&["diff", "--binary"]),
            git(&["diff", "--cached", "--binary"]),
            git(&["write-tree"]),
            git(&["rev-parse", "HEAD"]),
        )
    );
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

    // Ask the server which version the client really holds; never assume 1.
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 20,
        "method": "view/resync",
        "params": {"viewId": view_id},
    }));
    let version = server.response_with_id(20)["result"]["version"]
        .as_u64()
        .expect("initial view version");

    server.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "textDocument/documentSymbol",
        "params": {"textDocument": {"uri": uri}},
    }));
    let response = server.response_with_id(3);
    let symbols = response["result"]
        .as_array()
        .expect("interactive documentSymbol returns symbols");
    let names: Vec<&str> = symbols
        .iter()
        .map(|symbol| symbol["name"].as_str().expect("symbol name"))
        .collect();
    assert_eq!(
        names,
        vec!["Staged changes", "Unstaged changes", "Untracked files"]
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
    let metadata = notifications.iter().find(|n| n["method"] == "view/render").expect("metadata render");
    assert!(metadata["params"]["text"].is_null() && metadata["params"]["edits"].is_null() && metadata["params"]["cursor"].is_null());
    assert_eq!(metadata["params"]["version"], version);
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
            "version": version,
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

#[test]
fn static_clients_receive_mode_metadata_without_dynamic_keymaps() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut server = TestServer::spawn(&[(magit_lsp::logging::LEVEL_ENV, "warn")]);
    server.initialize(json!({"keymapDelivery": "static"}));
    server.send(json!({"jsonrpc":"2.0","id":2,"method":"view/open","params":{"kind":"status","args":{"repo_path":tmp.path().to_string_lossy()},"viewport":{"cols":80,"rows":24}}}));
    let opened = server.response_with_id(2);
    let view_id = opened["result"]["id"].as_str().unwrap().to_string();
    server.send(json!({"jsonrpc":"2.0","id":3,"method":"view/resync","params":{"viewId":view_id}}));
    let mut version = server.response_with_id(3)["result"]["version"].as_u64().unwrap();

    for (id, method, params) in [
        (4, "view/invoke", json!({"viewId":view_id,"command":"common.toggle-text-mode"})),
        (5, "view/key", json!({"viewId":view_id,"version":version,"key":"ctrl+t","editorMode":"normal","cursor":{"line":0,"column":0}})),
        (6, "view/setMode", json!({"viewId":view_id,"mode":"text"})),
    ] {
        server.notifications.clear();
        server.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        server.response_with_id(id);
        let notifications = server.drain_notifications();
        let render = notifications.iter().find(|n| n["method"] == "view/render").expect("mode metadata");
        assert!(render["params"]["text"].is_null() && render["params"]["edits"].is_null() && render["params"]["cursor"].is_null());
        assert_eq!(render["params"]["version"], version);
        assert_eq!(render["params"]["mode"], "text");
        assert!(!notifications.iter().any(|n| n["method"] == "view/keymap"), "static delivery must not send dynamic keymaps");
        server.send(json!({"jsonrpc":"2.0","id":id+10,"method":"view/setMode","params":{"viewId":view_id,"mode":"interactive"}}));
        server.response_with_id(id+10);
        let exited = server.drain_notifications();
        if let Some(render) = exited.iter().find(|n| n["method"] == "view/render") {
            version = render["params"]["version"].as_u64().expect("exit version");
        }
    }
    // Open the real dispatch menu, then invoke its advertised toggle action
    // through the same key path used by native menu selection.
    server.send(json!({"jsonrpc":"2.0","id":39,"method":"view/invoke","params":{"viewId":view_id,"command":"open.dispatch"}}));
    let opened_menu = server.response_with_id(39);
    let menu_id = opened_menu["result"]["effects"].as_array().unwrap().iter()
        .find_map(|effect| effect["descriptor"]["id"].as_str()).expect("dispatch menu id").to_string();
    let menu_renders = server.drain_notifications();
    let menu_render = &menu_renders.iter().find(|n| n["method"] == "view/render" && n["params"]["viewId"] == menu_id).expect("dispatch render")["params"];
    let toggle_key = menu_render["menu"]["groups"].as_array().unwrap().iter()
        .flat_map(|group| group["items"].as_array().unwrap())
        .find(|item| item["command"] == "common.toggle-text-mode").expect("advertised menu toggle")["key"].as_str().unwrap();
    server.notifications.clear();
    server.send(json!({"jsonrpc":"2.0","id":40,"method":"view/key","params":{"viewId":menu_id,"version":menu_render["version"],"key":toggle_key,"editorMode":"normal","cursor":{"line":0,"column":0}}}));
    server.response_with_id(40);
    let menu_invocation = server.drain_notifications();
    let entered = &menu_invocation.iter().find(|n| n["method"] == "view/render" && n["params"]["mode"] == "text").expect("menu action mode metadata")["params"];
    assert!(entered["text"].is_null() && entered["edits"].is_null());
    assert!(!menu_invocation.iter().any(|n| n["method"] == "view/keymap"));
    let toggled_view = entered["viewId"].as_str().unwrap();
    server.send(json!({"jsonrpc":"2.0","id":41,"method":"view/invoke","params":{"viewId":toggled_view,"command":"common.toggle-text-mode"}}));
    server.response_with_id(41);
    server.drain_notifications();
    server.shutdown_and_exit();
    assert!(server.finish(Duration::from_secs(10)).success());
}
