//! Process log view (R-3.3): opening, rendering real Git invocations, copying a
//! record's command, stable node ids and clearing the ring.

mod common;

use common::*;
use magit_lsp::protocol::{Cursor, Effect, ViewInvokeParams, ViewRender};
use magit_lsp::runner;
use magit_lsp::session::{InvokeResult, Session};
use magit_lsp::view::{Placement, ViewDescriptor, ViewKind, ViewMode, Viewport};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

/// The process log is process-global, so tests that inspect it run one at a time.
static LOCK: Mutex<()> = Mutex::new(());

fn viewport() -> Viewport {
    Viewport { cols: 100, rows: 30 }
}

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn invoke(session: &mut Session, view: &str, command: &str, line: Option<usize>) -> InvokeResult {
    session
        .invoke(&ViewInvokeParams {
            view_id: view.to_string(),
            command: command.to_string(),
            args: None,
            cursor: line.map(|line| Cursor { line, column: 0 }),
            selection: None,
        })
        .expect("invoke")
}

fn open_status(session: &mut Session) -> ViewRender {
    let (_descriptor, render) = session
        .open("v1", ViewKind::Status, Placement::Main, viewport())
        .expect("open status view");
    session.ack("v1", render.version).expect("ack status");
    render
}

fn open_process_log(session: &mut Session) -> (ViewDescriptor, ViewRender) {
    let outcome = invoke(session, "v1", "open.process-log", None);
    let descriptor = outcome
        .effects
        .iter()
        .find_map(|effect| match effect {
            Effect::PushView { descriptor } => Some(descriptor.clone()),
            _ => None,
        })
        .expect("a pushView effect");
    let (render, _keymap) = outcome
        .opened
        .into_iter()
        .next()
        .expect("the process log render");
    session
        .ack(&descriptor.id, render.version)
        .expect("ack process log");
    (descriptor, render)
}

#[test]
fn opens_by_kind_and_by_command() {
    let _guard = lock();
    runner::global_log().clear();

    let (tmp, _repo) = init_repo();
    let mut session = Session::new(tmp.path().to_string_lossy());

    let (descriptor, render) = session
        .open("pl", ViewKind::ProcessLog, Placement::Main, viewport())
        .expect("open process log");
    assert_eq!(descriptor.kind, ViewKind::ProcessLog);
    assert_eq!(descriptor.title, "Process log");
    assert_eq!(descriptor.scope.as_str(), "process-log");
    assert_eq!(descriptor.mode, ViewMode::Interactive);
    assert!(!descriptor.capture);
    assert_eq!(descriptor.placement, Placement::Main);
    assert!(
        descriptor.uri.starts_with("magit://process-log/"),
        "unexpected uri: {}",
        descriptor.uri
    );
    assert!(render.text.expect("full text")[0].starts_with("Process log ("));
    session.close("pl").expect("close process log");

    let kind: ViewKind = serde_json::from_str("\"process-log\"").expect("kind deserializes");
    assert_eq!(kind, ViewKind::ProcessLog);

    open_status(&mut session);
    let (descriptor, render) = open_process_log(&mut session);
    assert_eq!(descriptor.kind, ViewKind::ProcessLog);
    assert_eq!(descriptor.placement, Placement::Main);
    assert_eq!(descriptor.scope.as_str(), "process-log");
    assert!(render.text.expect("full text")[0].starts_with("Process log ("));
    assert!(session.views().get(&descriptor.id).is_some());
}

#[test]
fn a_stage_shows_its_real_git_arguments_and_copy_value_yields_the_command() {
    let _guard = lock();
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = Session::new(tmp.path().to_string_lossy());
    open_status(&mut session);
    runner::global_log().clear();

    let (descriptor, _render) = open_process_log(&mut session);
    let staged = invoke(&mut session, "v1", "status.stage-all", None);
    assert!(staged.render.is_some(), "staging re-renders the status");

    let refreshed = invoke(&mut session, &descriptor.id, "common.refresh", None);
    let render = refreshed.render.expect("refresh re-renders");
    session
        .ack(&descriptor.id, render.version)
        .expect("ack refreshed log");

    let tree = session
        .views()
        .get(&descriptor.id)
        .expect("process log view")
        .tree()
        .clone();
    let line = tree
        .lines
        .iter()
        .position(|line| line.starts_with("git add -u "))
        .unwrap_or_else(|| panic!("no `git add -u` record: {:?}", tree.lines));

    let copied = invoke(
        &mut session,
        &descriptor.id,
        "common.copy-value",
        Some(line),
    );
    assert_eq!(
        copied.effects,
        vec![Effect::Clipboard {
            text: "git add -u".to_string()
        }]
    );
}

#[test]
fn record_nodes_are_stable_and_clear_empties_the_view() {
    let _guard = lock();
    runner::global_log().clear();
    let id = runner::global_log().record(
        Path::new("/repo"),
        &["status", "--porcelain"],
        1,
        Duration::from_millis(3),
        b"",
        b"fatal: not a git repository",
    );

    let mut session = Session::new("/repo");
    let (_descriptor, _render) = session
        .open("pl", ViewKind::ProcessLog, Placement::Main, viewport())
        .expect("open process log");

    let node_id = format!("process-log:record:{id}");
    let tree = session.views().get("pl").expect("view").tree().clone();
    assert_eq!(tree.lines[0], "Process log (1)");
    assert!(tree.node(&node_id).is_some(), "record node missing");
    assert!(
        tree.tokens
            .iter()
            .any(|token| token.token_type == "magit.message.error"),
        "failed stderr has no error token"
    );
    let record_line = tree
        .node(&node_id)
        .expect("record node")
        .start_line;

    // A refresh keeps the id-derived node, even though the line moved.
    invoke(&mut session, "pl", "common.refresh", None);
    let tree = session.views().get("pl").expect("view").tree().clone();
    assert_eq!(tree.node(&node_id).expect("stable record node").start_line, record_line);

    let copied = invoke(&mut session, "pl", "common.copy-value", Some(record_line));
    assert_eq!(
        copied.effects,
        vec![Effect::Clipboard {
            text: "git status --porcelain".to_string()
        }]
    );

    let cleared = invoke(&mut session, "pl", "process-log.clear", None);
    assert!(cleared.render.is_some(), "clearing re-renders");
    assert!(runner::global_log().is_empty());
    assert_eq!(
        session.views().get("pl").expect("view").tree().lines,
        vec!["Process log (0)".to_string()]
    );
}
