use magit_lsp::commands::{registry, CommandStatus};
use magit_lsp::config::{Config, Profile};
use magit_lsp::keymap::Keymap;
use magit_lsp::presentation::keys::eligible;

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
    ] {
        assert!(commands.get(id).unwrap().destructive, "{id} must be destructive");
    }
    for id in ["status.stage", "status.unstage", "common.refresh", "log.current"] {
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
