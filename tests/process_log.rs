//! Process log view (R-3.3): opening, rendering real Git invocations, copying a
//! record's command, stable node ids and clearing the ring.

mod common;

use common::*;
use magit_lsp::config::Profile;
use magit_lsp::protocol::{Cursor, Effect};
use magit_lsp::runner;
use magit_lsp::semantic_tokens;
use magit_lsp::view::{Placement, ViewKind};
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn stderr_tokens_survive_semantic_token_encoding() {
    let repo = TempDir::new().unwrap();
    let cwd = repo.path().display().to_string();
    runner::global_log().record(
        Path::new(&cwd),
        &["failed"],
        1,
        Duration::ZERO,
        b"",
        b"failure text",
    );
    runner::global_log().record(
        Path::new(&cwd),
        &["warning"],
        0,
        Duration::ZERO,
        b"",
        b"warning text",
    );

    let mut test = TestSession::new(&cwd, Profile::Vim);
    test.open_view("pl", ViewKind::ProcessLog, Placement::Main, WIDE_VIEWPORT);
    let tree = test.tree_of("pl");
    let encoded = semantic_tokens::encode(tree);
    let mut line = 0;
    let mut start = 0;
    let decoded: Vec<_> = encoded
        .data
        .iter()
        .map(|token| {
            line += token.delta_line;
            start = if token.delta_line == 0 {
                start + token.delta_start
            } else {
                token.delta_start
            };
            (
                line,
                start,
                token.length,
                semantic_tokens::TOKEN_TYPES[token.token_type as usize],
            )
        })
        .collect();
    let stderr: Vec<_> = decoded
        .into_iter()
        .filter(|token| matches!(token.3, "magit.message.error" | "magit.message.warn"))
        .collect();
    assert_eq!(stderr.len(), 2, "encoded stderr tokens: {stderr:?}");
    for (message, token_type) in [
        ("failure text", "magit.message.error"),
        ("warning text", "magit.message.warn"),
    ] {
        let expected_line = tree
            .lines
            .iter()
            .position(|line| line == &format!("  stderr: {message}"))
            .unwrap() as u32;
        assert!(stderr.contains(&(
            expected_line,
            0,
            format!("  stderr: {message}").len() as u32,
            token_type
        )));
    }
    assert_eq!(
        semantic_tokens::TOKEN_GROUPS[semantic_tokens::TOKEN_TYPES
            .iter()
            .position(|t| *t == "magit.message.error")
            .unwrap()],
        "ErrorMsg"
    );
    assert_eq!(
        semantic_tokens::TOKEN_GROUPS[semantic_tokens::TOKEN_TYPES
            .iter()
            .position(|t| *t == "magit.message.warn")
            .unwrap()],
        "WarningMsg"
    );
}

#[test]
fn opens_by_kind_and_by_command() {
    let (tmp, _repo) = init_repo();
    let mut test = TestSession::new(tmp.path().to_string_lossy(), Profile::Vim);

    let (descriptor, render) =
        test.open_view("pl", ViewKind::ProcessLog, Placement::Main, WIDE_VIEWPORT);
    assert_eq!(descriptor.kind, ViewKind::ProcessLog);
    assert_eq!(descriptor.title, "Process log");
    assert_eq!(descriptor.scope.as_str(), "process-log");
    assert_eq!(descriptor.mode, magit_lsp::view::ViewMode::Interactive);
    assert!(!descriptor.capture);
    assert_eq!(descriptor.placement, Placement::Main);
    assert!(
        descriptor.uri.starts_with("magit://process-log/"),
        "unexpected uri: {}",
        descriptor.uri
    );
    assert!(render.text.expect("full text")[0].starts_with("Process log ("));
    test.session_mut().close("pl").expect("close process log");

    let kind: ViewKind = serde_json::from_str("\"process-log\"").expect("kind deserializes");
    assert_eq!(kind, ViewKind::ProcessLog);

    test.open_status_view(WIDE_VIEWPORT);
    test.ack();
    let (descriptor, render) = test.open_via_command("open.process-log");
    test.ack();
    assert_eq!(descriptor.kind, ViewKind::ProcessLog);
    assert_eq!(descriptor.placement, Placement::Main);
    assert_eq!(descriptor.scope.as_str(), "process-log");
    assert!(render.text.expect("full text")[0].starts_with("Process log ("));
    assert!(test.session().views().get(&descriptor.id).is_some());
}

#[test]
fn a_stage_shows_its_real_git_arguments_and_copy_value_yields_the_command() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let (mut test, _initial) = TestSession::open_status_at(&tmp, Profile::Vim, WIDE_VIEWPORT);
    test.ack();

    let (descriptor, _render) = test.open_via_command("open.process-log");
    test.ack();
    test.focus("v1");
    let staged = test.invoke("status.stage-all");
    assert!(staged.render.is_some(), "staging re-renders the status");

    test.focus(&descriptor.id);
    let refreshed = test.invoke("common.refresh");
    assert!(refreshed.render.is_some(), "refresh re-renders");
    test.ack();

    // The record node records its exact command; the cursor target comes from
    // the node, never from parsing the rendered record line. The log is shared
    // with other tests in this binary, so the record is identified by its
    // unique cwd and arguments rather than by position.
    let cwd = tmp.path().display().to_string();
    let record = runner::global_log()
        .iter()
        .into_iter()
        .rev()
        .find(|record| {
            record.cwd == cwd && record.args == vec!["add".to_string(), "-u".to_string()]
        })
        .expect("the stage is recorded");
    let tree = test.tree_of(&descriptor.id).clone();
    let line = tree
        .node(&format!("process-log:record:{}", record.id))
        .expect("the stage has a record node")
        .start_line;
    let copied = test.invoke_at("common.copy-value", Cursor { line, column: 0 });
    assert_eq!(
        copied.effects,
        vec![Effect::Clipboard {
            text: "git add -u".to_string()
        }]
    );
}

#[test]
fn record_nodes_are_stable_and_clear_empties_the_view() {
    // Clearing the ring is process-global and destructive, so this case runs
    // alone in a child: the other process-log cases share the same log.
    const CHILD: &str = "MAGIT_LSP_PROCESS_LOG_CLEAR_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "record_nodes_are_stable_and_clear_empties_the_view",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "child output: {}{}",
            String::from_utf8_lossy(&status.stdout),
            String::from_utf8_lossy(&status.stderr)
        );
        return;
    }

    let repo = TempDir::new().unwrap();
    let cwd = repo.path().display().to_string();
    let id = runner::global_log().record(
        Path::new(&cwd),
        &["status", "--porcelain"],
        1,
        Duration::from_millis(3),
        b"",
        b"fatal: not a git repository",
    );

    let mut test = TestSession::new(&cwd, Profile::Vim);
    test.open_view("pl", ViewKind::ProcessLog, Placement::Main, WIDE_VIEWPORT);

    let node_id = format!("process-log:record:{id}");
    let tree = test.tree_of("pl").clone();
    assert_eq!(tree.lines[0], "Process log (1)");
    assert!(tree.node(&node_id).is_some(), "record node missing");
    assert!(
        tree.tokens
            .iter()
            .any(|token| token.token_type == "magit.message.error"),
        "failed stderr has no error token"
    );
    let record_line = tree.node(&node_id).expect("record node").start_line;

    // A refresh keeps the id-derived node, even though the line moved.
    test.invoke("common.refresh");
    let tree = test.tree_of("pl").clone();
    assert_eq!(
        tree.node(&node_id).expect("stable record node").start_line,
        record_line
    );

    let copied = test.invoke_at(
        "common.copy-value",
        Cursor {
            line: record_line,
            column: 0,
        },
    );
    assert_eq!(
        copied.effects,
        vec![Effect::Clipboard {
            text: "git status --porcelain".to_string()
        }]
    );

    let cleared = test.invoke("process-log.clear");
    assert!(cleared.render.is_some(), "clearing re-renders");
    assert!(runner::global_log().is_empty());
    assert_eq!(test.text_of("pl"), vec!["Process log (0)".to_string()]);
}
