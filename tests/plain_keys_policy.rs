use magit_lsp::commands::{registry, CommandStatus};
use magit_lsp::config::{Config, Profile};
use magit_lsp::keymap::Keymap;
use magit_lsp::presentation::keys::eligible;
use magit_lsp::dispatch::{DispatchRequest, DispatchTarget};
use magit_lsp::protocol::{Cursor, PromptChoiceAnswer, ViewPromptParams};
use magit_lsp::ui::PromptUi;
use magit_lsp::Result;
use std::cell::RefCell;

mod common;

#[derive(Default)]
struct RecordingCancelUi(RefCell<Vec<ViewPromptParams>>);

impl PromptUi for RecordingCancelUi {
    fn confirm(&self, request: &ViewPromptParams) -> Result<Option<bool>> {
        self.0.borrow_mut().push(request.clone());
        Ok(None)
    }
    fn text(&self, request: &ViewPromptParams) -> Result<Option<String>> {
        self.0.borrow_mut().push(request.clone());
        Ok(None)
    }
    fn choice(&self, request: &ViewPromptParams) -> Result<Option<PromptChoiceAnswer>> {
        self.0.borrow_mut().push(request.clone());
        Ok(None)
    }
    fn multi_choice(&self, request: &ViewPromptParams) -> Result<Option<Vec<String>>> {
        self.0.borrow_mut().push(request.clone());
        Ok(None)
    }
}

#[test]
fn registry_marks_destructive_commands_and_keeps_safe_representatives_unflagged() {
    let commands = registry();
    for id in [
        "status.discard",
        "reset.hard",
        "branch.delete",
        "tag.delete",
        "worktree.delete",
        "push.to-pushremote",
        "push.to-upstream",
        "commit.extend",
    ] {
        assert!(commands.get(id).unwrap().destructive, "{id} must be destructive");
    }
    for id in ["status.stage", "status.unstage", "common.refresh", "log.current", "commit.fixup"] {
        assert!(!commands.get(id).unwrap().destructive, "{id} must be safe");
    }
}

#[test]
fn eligibility_requires_implemented_safe_command_bound_in_profile() {
    let keymap = Keymap::resolve("status", Profile::Vim, &Config::default()).unwrap();
    let commands = registry();
    assert!(eligible(commands.get("status.stage").unwrap(), &keymap));
    assert!(!eligible(commands.get("status.discard").unwrap(), &keymap));
    assert!(!eligible(commands.get("status.reverse").unwrap(), &keymap));
    assert!(eligible(commands.get("common.refresh").unwrap(), &keymap));

    let unbound = Keymap::resolve("commit", Profile::Vim, &Config::default()).unwrap();
    assert!(!eligible(commands.get("status.stage").unwrap(), &unbound));
}

#[test]
fn registered_command_status_is_explicit() {
    assert!(registry().iter().all(|command| matches!(
        command.status,
        CommandStatus::Implemented | CommandStatus::Planned { .. }
    )));
}

fn assert_repository_unchanged(
    fixture: &common::RepoFixture,
    command: &str,
    moment: &str,
    before_head: &str,
    before_index: &[u8],
    before_worktree: &[u8],
    before_refs: &str,
) {
    assert_eq!(common::git(fixture.path(), &["rev-parse", "HEAD"]), before_head,
        "{command} changed HEAD {moment}");
    assert_eq!(std::fs::read(fixture.path().join(".git/index")).unwrap(), before_index,
        "{command} changed the index {moment}");
    assert_eq!(std::fs::read(fixture.path().join("f.txt")).unwrap(), before_worktree,
        "{command} changed the worktree {moment}");
    assert_eq!(common::git(fixture.path(), &["for-each-ref", "--format=%(refname) %(objectname)"]), before_refs,
        "{command} changed refs {moment}");
}

#[test]
fn session_dispatch_sweep_cancels_destructive_first_prompt_without_mutation() {
    use common::session::TestSession;
    use common::RepoFixture;
    use magit_lsp::dispatch::offer_target;
    use magit_lsp::tree::NodeKind;

    let mut reached = Vec::new();
    let mut destructive_prompted = Vec::new();
    for command in registry().iter() {
        let fixture = RepoFixture::new()
            .commit("initial", [("f.txt", common::lines(40).to_string())])
            .modify("f.txt", common::two_hunk_changes())
            .build().0;
        // Give the ref oracle real branch and tag targets for delete commands.
        fixture.git(&["branch", "topic"]);
        fixture.git(&["tag", "v1"]);
        let (mut test, render) = TestSession::open_status(&fixture, Profile::Vim);
        let ui = RecordingCancelUi::default();
        let before_head = common::git(fixture.path(), &["rev-parse", "HEAD"]);
        let before_index = std::fs::read(fixture.path().join(".git/index")).expect("index state");
        let before_worktree = std::fs::read(fixture.path().join("f.txt")).expect("worktree state");
        let before_refs = common::git(fixture.path(), &["for-each-ref", "--format=%(refname) %(objectname)"]);
        let action_target = (command.id == "status.discard").then(|| {
            let node = test.tree().nodes.iter()
                .find(|node| node.kind == NodeKind::File && node.section.as_deref() == Some("unstaged"))
                .expect("fixture has an unstaged file");
            offer_target(test.tree(), command.id, &node.id).expect("structured discard target")
        });
        let request = DispatchRequest {
            command: command.id.into(),
            target: DispatchTarget {
                view_id: "v1".into(),
                version: Some(render.version),
                cursor: Some(Cursor { line: 0, column: 0 }),
                selection: None,
                key: None,
            },
            action_target,
            args: None,
        };
        let _ = test.session.dispatch(request, &ui);
        let prompt = test.session.pending_prompt_request();
        if command.destructive && matches!(command.status, CommandStatus::Implemented) {
            assert!(prompt.is_some(), "{} did not prompt before mutation", command.id);
            assert_repository_unchanged(&fixture, command.id, "before its first prompt",
                &before_head, &before_index, &before_worktree, &before_refs);
            destructive_prompted.push(command.id);
        }
        if let Some(prompt) = prompt {
            if prompt.destructive == Some(true) {
                assert!(command.destructive, "{} emitted destructive prompt metadata without a destructive flag", command.id);
            }
            let answer = match prompt.kind {
                magit_lsp::protocol::PromptKind::Confirm => ui.confirm(&prompt).map(|v| v.map(magit_lsp::protocol::PromptValue::Bool)),
                magit_lsp::protocol::PromptKind::Text => ui.text(&prompt).map(|v| v.map(magit_lsp::protocol::PromptValue::Text)),
                magit_lsp::protocol::PromptKind::Choice => ui.choice(&prompt).map(|v| v.map(magit_lsp::protocol::PromptValue::Choice)),
                magit_lsp::protocol::PromptKind::MultiChoice => ui.multi_choice(&prompt).map(|v| v.map(|ids| magit_lsp::protocol::PromptValue::Multi { ids })),
            }.expect("recording UI cancellation");
            assert!(answer.is_none(), "recording UI must cancel");
            let _ = test.session.resume_prompt(&prompt.prompt_id, answer).expect("cancel prompt");
        }
        if command.destructive && matches!(command.status, CommandStatus::Implemented) {
            assert_repository_unchanged(&fixture, command.id, "after cancelling its first prompt",
                &before_head, &before_index, &before_worktree, &before_refs);
        }
        reached.push(command.id);
    }
    let missing: Vec<_> = registry().iter().map(|command| command.id)
        .filter(|id| !reached.contains(id)).collect();
    assert!(missing.is_empty(), "commands not dispatched: {missing:?}");
    let unprompted: Vec<_> = registry().iter()
        .filter(|command| command.destructive && matches!(command.status, CommandStatus::Implemented)
            && !destructive_prompted.contains(&command.id))
        .map(|command| command.id).collect();
    assert!(unprompted.is_empty(), "implemented destructive commands without a first prompt: {unprompted:?}");
    // The menu-generated destructive commands are explicitly Planned in the
    // production registry and dispatch rejects Planned commands before run.
    // They cannot reach PromptUi until implemented.
    for command in registry().iter().filter(|command| command.destructive && !destructive_prompted.contains(&command.id)) {
        assert!(matches!(command.status, CommandStatus::Planned { .. }),
            "{} lacks a destructive first prompt and is not marked planned", command.id);
    }
}
