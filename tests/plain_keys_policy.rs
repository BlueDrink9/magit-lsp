use magit_lsp::commands::{registry, CommandStatus};
use magit_lsp::config::{Config, Profile};
use magit_lsp::dispatch::{DispatchRequest, DispatchTarget};
use magit_lsp::keymap::Keymap;
use magit_lsp::presentation::keys::eligible;
use magit_lsp::protocol::Cursor;
use magit_lsp::view::ViewMode;

mod common;

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
        assert!(
            commands.get(id).unwrap().destructive,
            "{id} must be destructive"
        );
    }
    for id in [
        "status.stage",
        "status.unstage",
        "common.refresh",
        "log.current",
        "commit.fixup",
    ] {
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

/// The sweep's repository oracle: HEAD, the index bytes, the worktree file
/// bytes and every ref, captured as one comparable value.
#[derive(PartialEq)]
struct RepositoryState {
    /// `git rev-parse HEAD`.
    head: String,
    /// `.git/index` contents.
    index: Vec<u8>,
    /// The worktree copy of the fixture's only file.
    worktree: Vec<u8>,
    /// `git for-each-ref` over `refname` and `objectname`.
    refs: String,
}

impl RepositoryState {
    /// Capture the oracle state of `fixture`'s repository.
    fn capture(fixture: &common::RepoFixture) -> Self {
        Self {
            head: common::git(fixture.path(), &["rev-parse", "HEAD"]),
            index: std::fs::read(fixture.path().join(".git/index")).expect("index state"),
            worktree: std::fs::read(fixture.path().join("f.txt")).expect("worktree state"),
            refs: common::git(
                fixture.path(),
                &["for-each-ref", "--format=%(refname) %(objectname)"],
            ),
        }
    }
}

/// Assert the repository still matches `before` for `command` at `moment`.
fn assert_repository_unchanged(
    fixture: &common::RepoFixture,
    command: &str,
    moment: &str,
    before: &RepositoryState,
) {
    let now = RepositoryState::capture(fixture);
    assert_eq!(now.head, before.head, "{command} changed HEAD {moment}");
    assert_eq!(
        now.index, before.index,
        "{command} changed the index {moment}"
    );
    assert_eq!(
        now.worktree, before.worktree,
        "{command} changed the worktree {moment}"
    );
    assert_eq!(now.refs, before.refs, "{command} changed refs {moment}");
}

/// Template name of the sweep's pristine repository copy.
const SWEEP_TEMPLATE: &str = "plain-keys-policy-sweep";

/// A status session opened on a fresh copy of the sweep repository template.
///
/// The template carries a committed forty-line file with a two-hunk change, a
/// `topic` branch and a `v1` tag so destructive commands have real targets.
fn sweep_session() -> (common::RepoFixture, common::TestSession) {
    use common::RepoFixture;

    let (fixture, _repo) = common::fork(SWEEP_TEMPLATE, |parent| {
        let (fixture, repo) = RepoFixture::new_in(parent)
            .commit("initial", [("f.txt", common::lines(40).to_string())])
            .modify("f.txt", common::two_hunk_changes())
            .build();
        fixture.git(&["branch", "topic"]);
        fixture.git(&["tag", "v1"]);
        (fixture, repo)
    });
    let test = common::TestSession::open_status(&fixture, Profile::Vim).0;
    (fixture, test)
}

/// Whether `session` can take the next sweep dispatch: exactly one open view,
/// the root status view, in interactive mode with no question awaiting an
/// answer. A quit, a text-mode toggle or an opened child view makes it false.
fn sweep_session_usable(session: &magit_lsp::session::Session) -> bool {
    let views = session.views();
    views.iter().count() == 1
        && views
            .get("v1")
            .is_some_and(|view| view.mode() == ViewMode::Interactive)
        && !session.has_pending_prompt()
}

#[test]
fn session_dispatch_sweep_cancels_destructive_first_prompt_without_mutation() {
    use common::recording_prompt_ui::RecordingPromptUi;
    use magit_lsp::dispatch::offer_target;
    use magit_lsp::tree::NodeKind;

    let mut reached = Vec::new();
    let mut destructive_prompted = Vec::new();
    // Commands whose dispatch left the session unusable; the next command ran
    // in a session rebuilt against the pristine base.
    let mut poisoned = Vec::new();
    // Commands whose dispatch dirtied the repository; the next command ran
    // against a pristine forked copy.
    let mut dirtied = Vec::new();
    let (mut fixture, mut test) = sweep_session();
    let mut pristine = RepositoryState::capture(&fixture);
    for command in registry().iter() {
        assert!(
            sweep_session_usable(&test.session),
            "{} would dispatch into an unusable session",
            command.id
        );
        let runnable =
            matches!(command.status, CommandStatus::Implemented) && command.applies_to("status");
        let destructive =
            command.destructive && matches!(command.status, CommandStatus::Implemented);
        let before = destructive.then(|| RepositoryState::capture(&fixture));
        let ui = RecordingPromptUi::default();
        // The version must be the one the session holds now: a stale version
        // makes a positional command answer STALE_VIEW instead of running.
        let version = test
            .session
            .views()
            .get("v1")
            .expect("the sweep dispatches into the root status view")
            .version();
        let action_target = (command.id == "status.discard").then(|| {
            let node = test
                .tree()
                .nodes
                .iter()
                .find(|node| {
                    node.kind == NodeKind::File && node.section.as_deref() == Some("unstaged")
                })
                .expect("fixture has an unstaged file");
            offer_target(test.tree(), command.id, &node.id).expect("structured discard target")
        });
        let request = DispatchRequest {
            command: command.id.into(),
            target: DispatchTarget {
                view_id: "v1".into(),
                version: Some(version),
                cursor: Some(Cursor { line: 0, column: 0 }),
                selection: None,
                key: None,
            },
            action_target,
            args: None,
        };
        let _ = test.dispatch(request, &ui);
        let prompt = test.session.pending_prompt_request();
        if destructive {
            assert!(
                prompt.is_some(),
                "{} did not prompt before mutation",
                command.id
            );
            assert_repository_unchanged(
                &fixture,
                command.id,
                "before its first prompt",
                before.as_ref().expect("destructive state captured"),
            );
            destructive_prompted.push(command.id);
        }
        if let Some(prompt) = prompt {
            if prompt.destructive == Some(true) {
                assert!(
                    command.destructive,
                    "{} emitted destructive prompt metadata without a destructive flag",
                    command.id
                );
            }
            let answer = magit_lsp::ui::answer(&ui, &prompt).expect("recording UI answer");
            assert!(answer.is_none(), "recording UI must cancel");
            let _ = test
                .session
                .resume_prompt(&prompt.prompt_id, answer)
                .expect("cancel prompt");
        }
        if destructive {
            assert_repository_unchanged(
                &fixture,
                command.id,
                "after cancelling its first prompt",
                before.as_ref().expect("destructive state captured"),
            );
        }
        reached.push(command.id);
        let unusable = !sweep_session_usable(&test.session);
        let dirty = runnable && RepositoryState::capture(&fixture) != pristine;
        if unusable {
            poisoned.push(command.id);
        }
        if dirty {
            dirtied.push(command.id);
        }
        if unusable || dirty {
            let (fresh_fixture, fresh_test) = sweep_session();
            fixture = fresh_fixture;
            test = fresh_test;
            pristine = RepositoryState::capture(&fixture);
        }
    }
    // The validity gate is not decorative: quitting the root view and switching
    // to text mode each force a rebuild before the next dispatch.
    for id in ["common.quit", "common.toggle-text-mode"] {
        assert!(
            poisoned.contains(&id),
            "{id} must make the session unusable and force a rebuild"
        );
    }
    // Mutating commands are what make the pristine repository gate load-bearing.
    assert!(
        !dirtied.is_empty(),
        "no dispatched command dirtied the repository, so the pristine gate never ran"
    );
    let missing: Vec<_> = registry()
        .iter()
        .map(|command| command.id)
        .filter(|id| !reached.contains(id))
        .collect();
    assert!(missing.is_empty(), "commands not dispatched: {missing:?}");
    let unprompted: Vec<_> = registry()
        .iter()
        .filter(|command| {
            command.destructive
                && matches!(command.status, CommandStatus::Implemented)
                && !destructive_prompted.contains(&command.id)
        })
        .map(|command| command.id)
        .collect();
    assert!(
        unprompted.is_empty(),
        "implemented destructive commands without a first prompt: {unprompted:?}"
    );
    // The menu-generated destructive commands are explicitly Planned in the
    // production registry and dispatch rejects Planned commands before run.
    // They cannot reach PromptUi until implemented.
    for command in registry()
        .iter()
        .filter(|command| command.destructive && !destructive_prompted.contains(&command.id))
    {
        assert!(
            matches!(command.status, CommandStatus::Planned { .. }),
            "{} lacks a destructive first prompt and is not marked planned",
            command.id
        );
    }
}
