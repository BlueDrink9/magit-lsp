//! `commit.create` pre-flight (R-UI-5, U1.10).
//!
//! A clean tree never starts Git, an unstaged-only tree asks before staging,
//! and a staged tree commits without a prompt. These are L1 assertions on the
//! effects and the repository state; the real screen is covered by
//! `tests/ui/scenarios/commit-*.scn`.

mod common;

use common::*;
use magit_lsp::config::{Config, Profile};
use magit_lsp::keymap::{self, Keymap};
use magit_lsp::protocol::{Effect, MessageLevel, PromptKind, ViewInvokeParams};
use magit_lsp::session::{InvokeResult, Session};
use magit_lsp::ui::ScriptedUi;
use magit_lsp::view::{Placement, ViewKind, Viewport};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

fn keymaps() -> Arc<BTreeMap<String, Keymap>> {
    let config = Config {
        profile: Profile::Vim,
        ..Default::default()
    };
    Arc::new(keymap::validate(&config).expect("resolve keymaps"))
}

fn open_session(repo: &Path) -> Session {
    let mut session = Session::new(repo.to_string_lossy());
    session.set_keymaps(keymaps());
    let (_descriptor, render) = session
        .open(
            "v1",
            ViewKind::Status,
            Placement::Main,
            Viewport { cols: 100, rows: 30 },
        )
        .expect("open status view");
    session.ack("v1", render.version).expect("ack");
    session
}

fn commit(session: &mut Session, ui: &ScriptedUi) -> InvokeResult {
    session
        .invoke_ui(
            &ViewInvokeParams {
                view_id: "v1".to_string(),
                command: "commit.create".to_string(),
                args: None,
                cursor: None,
                selection: None,
            },
            ui,
        )
        .expect("commit.create")
}

fn message_effects(outcome: &InvokeResult) -> Vec<(&MessageLevel, &str, Option<&str>)> {
    outcome
        .effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Message { level, text, hint, .. } => {
                Some((level, text.as_str(), hint.as_deref()))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_clean_tree_reports_nothing_to_commit_without_git() {
    let (_tmp, _repo) = init_repo();
    let mut session = open_session(_tmp.path());
    let ui = ScriptedUi::new();

    let outcome = commit(&mut session, &ui);

    assert!(outcome.commit.is_none(), "no commit worker is started");
    assert!(ui.requests().is_empty(), "the clean tree never prompts");
    let messages = message_effects(&outcome);
    assert_eq!(messages.len(), 1, "{messages:?}");
    let (level, text, hint) = messages[0];
    assert_eq!(*level, MessageLevel::Info);
    assert_eq!(text, "Nothing to commit (working tree clean)");
    assert!(
        hint.is_some_and(|hint| hint.contains("--allow-empty")),
        "the hint mentions --allow-empty: {hint:?}"
    );
    assert!(
        !messages.iter().any(|(_, text, _)| text.contains("Committing")),
        "a clean tree shows no progress: {messages:?}"
    );
}

#[test]
fn unstaged_changes_prompt_then_stage_and_commit() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = open_session(tmp.path());
    let ui = ScriptedUi::new();
    ui.reply_confirm(true);

    let outcome = commit(&mut session, &ui);

    let request = ui.last_request().expect("a confirm prompt");
    assert_eq!(request.kind, PromptKind::Confirm);
    assert_eq!(
        request.title,
        "Nothing staged. Stage all changes and commit?"
    );
    assert_eq!(
        outcome.commit.as_deref(),
        Some(tmp.path().to_string_lossy().as_ref()),
        "the commit continues after staging"
    );
    assert_eq!(
        tmp.git(&["status", "--porcelain"]),
        "M  f.txt\n",
        "the tracked change is staged"
    );
}

#[test]
fn declining_the_prompt_cancels_quietly() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = open_session(tmp.path());
    let ui = ScriptedUi::new();
    ui.reply_confirm(false);

    let outcome = commit(&mut session, &ui);

    assert!(outcome.commit.is_none(), "no commit worker is started");
    assert!(outcome.effects.is_empty(), "{:?}", outcome.effects);
    assert_eq!(
        tmp.git(&["status", "--porcelain"]),
        " M f.txt\n",
        "nothing was staged"
    );
}

#[test]
fn a_dismissed_prompt_cancels_quietly() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .modify_standard()
        .build();
    let mut session = open_session(tmp.path());
    let ui = ScriptedUi::new();
    ui.reply_cancel();

    let outcome = commit(&mut session, &ui);

    assert!(outcome.commit.is_none(), "no commit worker is started");
    assert!(outcome.effects.is_empty(), "{:?}", outcome.effects);
    assert_eq!(tmp.git(&["status", "--porcelain"]), " M f.txt\n");
}

#[test]
fn staged_changes_commit_without_a_prompt() {
    let (tmp, _repo) = RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .staged_file("f.txt", standard_changes())
        .build();
    let mut session = open_session(tmp.path());
    let ui = ScriptedUi::new();

    let outcome = commit(&mut session, &ui);

    assert!(ui.requests().is_empty(), "a staged tree needs no prompt");
    assert_eq!(
        outcome.commit.as_deref(),
        Some(tmp.path().to_string_lossy().as_ref()),
        "the commit worker is started"
    );
}
