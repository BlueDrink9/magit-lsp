//! The command registry: every key action is a [`Command`].
//!
//! Handlers synchronously prepare outcomes, including typed prompt actions.
//! The session owns continuation state; only its driver waits for input.
//! [`CommandCtx`] carries the view, repository, positional input and effects.

use crate::dispatch;
use crate::editor::{self, Reply};
use crate::errors;
use crate::git::{GitLogOptions, GitRepo};
use crate::keymap::Keymap;
use crate::log_view::CommitSelection;
use crate::menu::Menu;
use crate::patch::{FileLayout, SelectionOp};
use crate::protocol::{Cursor, Effect, MessageLevel, PromptKind, Selection, ViewPromptParams};
use crate::session::{re_resolve, OpenView};
use crate::tree::{Node, NodeKind, Tree};
use crate::view::{Placement, ViewKind, ViewMode, VisibilityCommand};
use crate::Result;
use lsp_types::Position;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// The registry group a command belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Common,
    Status,
    Select,
    Log,
    LogSelect,
    Menu,
    CommitMessage,
    /// The read-only process log.
    ProcessLog,
    /// A command run from inside a menu. It is bound by the menu item, not by
    /// a profile, so it has no profile-scope keymap of its own.
    MenuAction,
}

impl Scope {
    /// The scope name as it appears in a command id.
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Common => "common",
            Scope::Status => "status",
            Scope::Select => "select",
            Scope::Log => "log",
            Scope::LogSelect => "log.select",
            Scope::Menu => "menu",
            Scope::CommitMessage => "commit-message",
            Scope::ProcessLog => "process-log",
            Scope::MenuAction => "menu-action",
        }
    }

    /// Whether a command in this scope applies to a view with `view_scope`.
    ///
    /// `common` commands apply everywhere; `menu.*` commands apply to every
    /// menu; the rest apply only to their own view-kind scope. Menu actions
    /// apply everywhere except menus: they run against the view the menu was
    /// opened from.
    pub fn matches(self, view_scope: &str) -> bool {
        match self {
            Scope::Common => true,
            Scope::Menu => view_scope == "menu" || view_scope.starts_with("menu."),
            Scope::Log if view_scope == "log.select" => true,
            Scope::Select if view_scope == "log.select" => true,
            Scope::LogSelect if view_scope == "log.select" => true,
            Scope::MenuAction => !view_scope.starts_with("menu"),
            other => other.as_str() == view_scope,
        }
    }
}

/// Pure availability shared by command dispatch and Plain action discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// The command handler is implemented.
    Implemented,
    /// The command is tracked by a roadmap item.
    Planned(&'static str),
}

/// Whether a command can run, and what to say when it cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandStatus {
    Implemented,
    /// Listed but not built yet; the roadmap item it will arrive with.
    Planned {
        roadmap_id: &'static str,
    },
}

/// A synchronous command handler.
pub type RunFn = fn(&mut CommandCtx<'_>) -> Result<CommandOutcome>;

/// A named server operation; see §3.5.
pub struct Command {
    pub id: &'static str,
    pub scope: Scope,
    pub doc: &'static str,
    pub status: CommandStatus,
    pub run: RunFn,
}

/// The operation requested for a commit selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionOperation {
    /// Copy the selected commits' object IDs.
    CopyCommits,
}

/// Request to open a commit-selection view for a typed operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionRequest {
    /// Filters and presentation settings for the selection log.
    pub options: GitLogOptions,
    /// Settings controlling the selection view.
    pub settings: CommitSelection,
    /// Operation that consumes the selected commits.
    pub operation: SelectionOperation,
}

/// What a handler asks the session to do after it runs.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct CommandOutcome {
    /// A prepared question and typed action; no handler waits for input.
    pub prompt: Option<(ViewPromptParams, PromptAction)>,
    /// Re-read the backing state before rendering.
    pub refresh: bool,
    /// A cursor to move to that the old node cannot describe.
    pub cursor: Option<Cursor>,
    /// Ordered stable targets from the source tree; use the first surviving node after render.
    pub cursor_nodes: Vec<String>,
    /// A mode change the session must apply (and push a keymap for).
    pub mode: Option<ViewMode>,
    /// Push a new menu view and return to the current one on `popView`.
    pub open_menu: Option<Menu>,
    /// Push a non-menu view (with its placement) above the current one.
    pub open_view: Option<(ViewKind, Placement)>,
    /// Arguments used to construct the pushed view.
    pub open_args: Option<serde_json::Value>,
    /// Open a commit-selection view for an operation.
    pub open_selection: Option<SelectionRequest>,
    /// Selected commit IDs returned by a selection operation.
    pub selected_commits: Option<Vec<String>>,
    /// Close the current view and return to the previous one.
    pub pop: bool,
    /// Spawn `git commit` on a detached worker once this command returns; the
    /// editor bridge keeps Git waiting until the client confirms.
    pub commit: Option<crate::git::CommitOperation>,
}

/// A prepared operation resumed exactly once after the user answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptAction {
    /// Stage the captured working tree before starting commit.
    Commit { fingerprint: RepositoryFingerprint, untracked: bool, operation: crate::git::CommitOperation },
    /// Confirm rewriting a published HEAD without replaying menu arguments.
    PublishedCommit { fingerprint: RepositoryFingerprint, operation: crate::git::CommitOperation },
    /// Offer tracked `--all` for Extend without adding untracked files.
    ExtendAll { fingerprint: RepositoryFingerprint, options: crate::git::CommitOptions },
    /// Apply the captured resolution against the displayed patch identities.
    Discard { fingerprint: RepositoryFingerprint, resolution: dispatch::Resolution, tree: Tree },
    /// Resolve a revision for the originating selection view.
    Revision,
}

/// Content snapshot used to reject external changes during destructive prompts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryFingerprint {
    status: crate::git::Status,
    contents: Vec<(String, Option<Vec<u8>>)>,
    index: Vec<u8>,
}

impl RepositoryFingerprint {
    /// Capture HEAD, index bytes, patches and changed file contents.
    pub fn capture(repo: &GitRepo) -> Result<Self> {
        let status = repo.status()?;
        let mut contents = Vec::new();
        for entry in status.staged.iter().chain(&status.unstaged).chain(&status.untracked) {
            let path = repo.root().join(&entry.path);
            let content = match std::fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    let mut bytes = vec![b'L'];
                    bytes.extend_from_slice(std::fs::read_link(&path)?.as_os_str().as_encoded_bytes());
                    Some(bytes)
                }
                Ok(metadata) if metadata.is_dir() => Some(vec![b'D']),
                Ok(_) => {
                    let mut bytes = vec![b'F'];
                    bytes.extend(std::fs::read(&path)?);
                    Some(bytes)
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            };
            contents.push((entry.path.clone(), content));
        }
        let dot_git = repo.root().join(".git");
        let git_dir = if dot_git.is_file() {
            let text = std::fs::read_to_string(&dot_git)?;
            let path = text.trim().strip_prefix("gitdir: ").ok_or_else(|| crate::Error::new("Invalid Git directory file"))?;
            repo.root().join(path)
        } else { dot_git };
        let index = match std::fs::read(git_dir.join("index")) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        Ok(Self { status, contents, index })
    }
}

/// Apply a prepared action without replaying command or coordinate resolution.
pub fn resume_prompt(ctx: &mut CommandCtx<'_>, action: PromptAction, answer: Option<crate::protocol::PromptValue>) -> Result<CommandOutcome> {
    use crate::protocol::PromptValue;
    let Some(answer) = answer else { return Ok(CommandOutcome::default()); };
    match action {
        PromptAction::ExtendAll { fingerprint, mut options } => {
            if !matches!(answer, PromptValue::Bool(true)) { return Ok(CommandOutcome::default()); }
            let repo = GitRepo::open(ctx.repo_path)?;
            if RepositoryFingerprint::capture(&repo)? != fingerprint {
                ctx.info("Repository changed while the prompt was open; try again");
                return Ok(CommandOutcome::default());
            }
            options.all = true;
            prepare_head_commit(ctx, &repo, crate::git::CommitOperation::Extend(options))
        }
        PromptAction::PublishedCommit { fingerprint, operation } => {
            if !matches!(answer, PromptValue::Bool(true)) { return Ok(CommandOutcome::default()); }
            let repo = GitRepo::open(ctx.repo_path)?;
            if RepositoryFingerprint::capture(&repo)? != fingerprint {
                ctx.info("Repository changed while the prompt was open; try again");
                return Ok(CommandOutcome::default());
            }
            ctx.info("Committing…");
            Ok(CommandOutcome { commit: Some(operation), ..Default::default() })
        }
        PromptAction::Commit { fingerprint, untracked, operation } => {
            if !matches!(answer, PromptValue::Bool(true)) { return Ok(CommandOutcome::default()); }
            let repo = GitRepo::open(ctx.repo_path)?;
            if RepositoryFingerprint::capture(&repo)? != fingerprint {
                ctx.info("Repository changed while the prompt was open; try again");
                return Ok(CommandOutcome::default());
            }
            ctx.extend(run_resolution(ctx.repo_path, dispatch::Resolution::StageAllUnstaged)?);
            if untracked { ctx.extend(run_resolution(ctx.repo_path, dispatch::Resolution::StageAllUntracked)?); }
            ctx.info("Committing…");
            Ok(CommandOutcome { commit: Some(operation), ..Default::default() })
        }
        PromptAction::Discard { fingerprint, resolution, tree } => {
            if !matches!(answer, PromptValue::Bool(true)) { return Ok(CommandOutcome::default()); }
            let repo = GitRepo::open(ctx.repo_path)?;
            if RepositoryFingerprint::capture(&repo)? != fingerprint {
                ctx.info("Repository changed while the prompt was open; nothing discarded");
                return Ok(CommandOutcome::default());
            }
            if let dispatch::Resolution::DiscardFile { path } = &resolution {
                let section = file_worktree_section(&tree, path);
                if !displayed_file_is_current(&repo, &tree, path, section)? {
                    ctx.info(errors::HUNK_CHANGED);
                    return Ok(CommandOutcome { refresh: true, ..Default::default() });
                }
            }
            let effects = match resolution {
                dispatch::Resolution::DiscardSelection { start, end } => execute_displayed_selection(&repo, &tree, start, end, false, true)?,
                _ => dispatch::execute(&repo, &resolution, &[])?,
            };
            ctx.extend(effects);
            Ok(CommandOutcome { refresh: true, ..Default::default() })
        }
        PromptAction::Revision => {
            let PromptValue::Text(revision) = answer else { return Err(crate::Error::new("Expected a revision")); };
            let oid = match GitRepo::open(ctx.repo_path).and_then(|repo| repo.resolve_commit(revision.trim())) {
                Ok(oid) if !revision.trim().is_empty() => oid,
                _ => { ctx.info(format!("Invalid commit revision: {}", revision.trim())); return Ok(CommandOutcome::default()); }
            };
            Ok(CommandOutcome { selected_commits: Some(vec![oid]), ..Default::default() })
        }
    }
}

/// Everything a command handler may touch, plus the effects it collected.
pub struct CommandCtx<'a> {
    pub view: &'a mut OpenView,
    pub repo_path: &'a str,
    /// The resolved keymaps, so generated views can show the active keys.
    pub keymaps: &'a BTreeMap<String, Keymap>,
    pub cursor: Option<Cursor>,
    pub selection: Option<Selection>,
    pub args: Option<&'a Value>,
    /// The canonical chord that triggered the command through `view/key`.
    pub key: Option<String>,
    /// Ask the user a question; `NullUi` when no interactive client is attached.
    pub ui: &'a dyn crate::ui::PromptUi,
    effects: Vec<Effect>,
}

impl<'a> CommandCtx<'a> {
    /// Build a context for one `view/invoke` or `view/key`.
    pub fn new(
        view: &'a mut OpenView,
        repo_path: &'a str,
        keymaps: &'a BTreeMap<String, Keymap>,
        cursor: Option<Cursor>,
        selection: Option<Selection>,
        args: Option<&'a Value>,
        key: Option<String>,
        ui: &'a dyn crate::ui::PromptUi,
    ) -> Self {
        Self {
            view,
            repo_path,
            keymaps,
            cursor,
            selection,
            args,
            key,
            ui,
            effects: Vec::new(),
        }
    }

    /// Queue an info-level message effect.
    pub fn info(&mut self, text: impl Into<String>) {
        self.effects.push(Effect::message(MessageLevel::Info, text));
    }

    /// Append effects produced by a lower layer.
    pub fn extend(&mut self, effects: Vec<Effect>) {
        self.effects.extend(effects);
    }

    /// The `recursive` argument of a fold command, as sent by `zA`/`zO`/`zC`.
    pub fn recursive(&self) -> bool {
        self.args
            .and_then(|args| args.get("recursive"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// The opaque managed run a confirm/cancel invocation refers to.
    pub fn draft_run_id(&self) -> Option<&str> {
        self.args?.get("runId")?.as_str()
    }

    /// Take the collected effects, consuming the context.
    pub fn into_effects(self) -> Vec<Effect> {
        self.effects
    }
}

/// The registry of every known command, built once.
pub struct Registry {
    commands: Vec<Command>,
}

impl Registry {
    /// Look a command up by id.
    pub fn get(&self, id: &str) -> Option<&Command> {
        self.commands.iter().find(|command| command.id == id)
    }

    /// Return a command's pure registry availability without invoking its handler.
    pub fn availability(&self, id: &str) -> Option<Availability> {
        self.get(id).map(|command| match command.status {
            CommandStatus::Implemented => Availability::Implemented,
            CommandStatus::Planned { roadmap_id } => Availability::Planned(roadmap_id),
        })
    }

    /// Whether a registered command is both in scope and implemented.
    pub fn is_runnable_in(&self, id: &str, view_scope: &str) -> bool {
        self.get(id).is_some_and(|command| {
            command.scope.matches(view_scope)
                && self.availability(id) == Some(Availability::Implemented)
        })
    }

    /// Iterate over every command, in registration order.
    pub fn iter(&self) -> impl Iterator<Item = &Command> {
        self.commands.iter()
    }

    /// Iterate over the commands of one scope.
    pub fn by_scope(&self, scope: Scope) -> impl Iterator<Item = &Command> {
        self.commands
            .iter()
            .filter(move |command| command.scope == scope)
    }
}

#[cfg(test)]
mod availability_tests {
    use super::{Availability, registry};

    #[test]
    fn availability_and_scope_are_pure_registry_metadata() {
        let commands = registry();
        assert!(matches!(commands.availability("status.reverse"), Some(Availability::Planned(_))));
        assert_eq!(commands.availability("common.confirm"), Some(Availability::Implemented));
        assert!(commands.is_runnable_in("common.confirm", "commit"));
        assert!(commands.is_runnable_in("common.confirm", "status"));
        assert!(!commands.is_runnable_in("status.apply", "commit"));
        assert!(!commands.is_runnable_in("status.reverse", "status"));
    }
}

/// The process-wide command registry.
pub fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(build)
}

/// A `Planned` command: does nothing until its roadmap item lands.
const fn planned(
    id: &'static str,
    scope: Scope,
    doc: &'static str,
    roadmap_id: &'static str,
) -> Command {
    Command {
        id,
        scope,
        doc,
        status: CommandStatus::Planned { roadmap_id },
        run: planned_run,
    }
}

/// An implemented command that opens a menu from the status view.
const fn menu(id: &'static str, doc: &'static str, run: RunFn) -> Command {
    Command {
        id,
        scope: Scope::Status,
        doc,
        status: CommandStatus::Implemented,
        run,
    }
}

/// Handlers never call this; `view/invoke` answers `Planned` before it runs.
fn planned_run(_ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    Ok(CommandOutcome::default())
}

fn build() -> Registry {
    let mut commands = vec![
        Command {
            id: "common.refresh",
            scope: Scope::Common,
            doc: "Refresh the view",
            status: CommandStatus::Implemented,
            run: refresh,
        },
        Command {
            id: "common.toggle-section",
            scope: Scope::Common,
            doc: "Fold or unfold the section at point",
            status: CommandStatus::Implemented,
            run: toggle_section,
        },
        Command {
            id: "common.show-section",
            scope: Scope::Common,
            doc: "Unfold the section at point",
            status: CommandStatus::Implemented,
            run: show_section,
        },
        Command {
            id: "common.hide-section",
            scope: Scope::Common,
            doc: "Fold the section at point",
            status: CommandStatus::Implemented,
            run: hide_section,
        },
        Command {
            id: "common.show-all",
            scope: Scope::Common,
            doc: "Unfold every section",
            status: CommandStatus::Implemented,
            run: show_all,
        },
        Command {
            id: "common.hide-all",
            scope: Scope::Common,
            doc: "Fold every section",
            status: CommandStatus::Implemented,
            run: hide_all,
        },
        Command {
            id: "common.more-detail",
            scope: Scope::Common,
            doc: "Show one more level of detail",
            status: CommandStatus::Implemented,
            run: more_detail,
        },
        Command {
            id: "common.less-detail",
            scope: Scope::Common,
            doc: "Show one less level of detail",
            status: CommandStatus::Implemented,
            run: less_detail,
        },
        Command {
            id: "common.detail-level-1",
            scope: Scope::Common,
            doc: "Show section headers only",
            status: CommandStatus::Implemented,
            run: detail_level_1,
        },
        Command {
            id: "common.detail-level-2",
            scope: Scope::Common,
            doc: "Show section and file headers",
            status: CommandStatus::Implemented,
            run: detail_level_2,
        },
        Command {
            id: "common.detail-level-3",
            scope: Scope::Common,
            doc: "Show hunk headers",
            status: CommandStatus::Implemented,
            run: detail_level_3,
        },
        Command {
            id: "common.detail-level-4",
            scope: Scope::Common,
            doc: "Show hunk bodies",
            status: CommandStatus::Implemented,
            run: detail_level_4,
        },
        Command {
            // keep-for: TASKS §5.1 retained binding; planned navigation implementation requires roadmap disposition.
            id: "common.next-section",
            scope: Scope::Common,
            doc: "Move to the next section",
            status: CommandStatus::Implemented,
            run: next_section,
        },
        Command {
            id: "common.prev-section",
            scope: Scope::Common,
            doc: "Move to the previous section",
            status: CommandStatus::Implemented,
            run: prev_section,
        },
        Command {
            // keep-for: TASKS §5.1 retained binding; planned navigation implementation requires roadmap disposition.
            id: "common.jump",
            scope: Scope::Common,
            doc: "Move to the next or previous visible section, file or hunk",
            status: CommandStatus::Implemented,
            run: jump,
        },
        Command {
            id: "status.stage",
            scope: Scope::Status,
            doc: "Stage the change at point",
            status: CommandStatus::Implemented,
            run: stage,
        },
        Command {
            id: "status.unstage",
            scope: Scope::Status,
            doc: "Unstage the change at point",
            status: CommandStatus::Implemented,
            run: unstage,
        },
        Command {
            id: "status.stage-all",
            scope: Scope::Status,
            doc: "Stage all unstaged changes",
            status: CommandStatus::Implemented,
            run: stage_all,
        },
        Command {
            id: "status.unstage-all",
            scope: Scope::Status,
            doc: "Unstage all staged changes",
            status: CommandStatus::Implemented,
            run: unstage_all,
        },
    ];

    commands.extend([
        Command {
            id: "common.cancel",
            scope: Scope::Common,
            doc: "Cancel the current operation",
            status: CommandStatus::Implemented,
            run: cancel,
        },
        Command {
            id: "common.confirm",
            scope: Scope::Common,
            doc: "Confirm the current selection",
            status: CommandStatus::Implemented,
            run: confirm,
        },
        Command {
            id: "common.quit",
            scope: Scope::Common,
            doc: "Close all views",
            status: CommandStatus::Implemented,
            run: quit,
        },
        Command {
            id: "common.cycle-visibility", scope: Scope::Common,
            doc: "Cycle the visibility of every section",
            status: CommandStatus::Implemented, run: cycle_visibility,
        },
        Command {
            id: "common.next-sibling", scope: Scope::Common,
            doc: "Move to the next sibling",
            status: CommandStatus::Implemented, run: next_sibling,
        },
        Command {
            id: "common.prev-sibling", scope: Scope::Common,
            doc: "Move to the previous sibling",
            status: CommandStatus::Implemented, run: prev_sibling,
        },
        Command {
            id: "common.parent-section", scope: Scope::Common,
            doc: "Move to the parent section",
            status: CommandStatus::Implemented, run: parent_section,
        },
        Command {
            id: "common.toggle-text-mode",
            scope: Scope::Common,
            doc: "Toggle text mode",
            status: CommandStatus::Implemented,
            run: toggle_text_mode,
        },
        Command {
            id: "common.copy-value",
            scope: Scope::Common,
            doc: "Copy the value at point",
            status: CommandStatus::Implemented,
            run: copy_value,
        },
        Command {
            id: "common.visit",
            scope: Scope::Common,
            doc: "Visit the thing at point",
            status: CommandStatus::Implemented,
            run: visit,
        },
        Command {
            id: "status.discard",
            scope: Scope::Status,
            doc: "Discard the change at point",
            status: CommandStatus::Implemented,
            run: discard,
        },
        planned(
            "status.reverse",
            Scope::Status,
            "Reverse-apply the change at point",
            "R-3.6",
        ),
        planned(
            "status.apply",
            Scope::Status,
            "Apply the change at point",
            "R-8.2",
        ),
        Command {
            id: "open.dispatch",
            scope: Scope::Common,
            doc: "Show the dispatch menu",
            status: CommandStatus::Implemented,
            run: open_dispatch,
        },
        Command {
            id: "open.commit",
            scope: Scope::Status,
            doc: "Open the commit menu",
            status: CommandStatus::Implemented,
            run: open_commit,
        },
        Command {
            id: "commit.create",
            scope: Scope::MenuAction,
            doc: "Commit",
            status: CommandStatus::Implemented,
            run: commit_create,
        },
        Command { id: "commit.amend", scope: Scope::MenuAction, doc: "Amend HEAD", status: CommandStatus::Implemented, run: commit_amend },
        Command { id: "commit.extend", scope: Scope::MenuAction, doc: "Extend HEAD without editing its message", status: CommandStatus::Implemented, run: commit_extend },
        Command { id: "commit.reword", scope: Scope::MenuAction, doc: "Reword HEAD", status: CommandStatus::Implemented, run: commit_reword },
        menu("open.branch", "Open the branch menu", open_branch),
        menu("open.log", "Open the log menu", open_log),
        Command {
            id: "log.current",
            scope: Scope::MenuAction,
            doc: "Show the current log",
            status: CommandStatus::Implemented,
            run: open_current_log,
        },
        Command {
            id: "log.load-more",
            scope: Scope::Log,
            doc: "Load more commits",
            status: CommandStatus::Implemented,
            run: log_load_more,
        },
        Command {
            id: "log.select-and-copy",
            scope: Scope::MenuAction,
            doc: "Select commits and copy their object IDs",
            status: CommandStatus::Implemented,
            run: select_and_copy,
        },
        menu("open.diff", "Open the diff menu", open_diff),
        menu("open.fetch", "Open the fetch menu", open_fetch),
        menu("open.pull", "Open the pull menu", open_pull),
        menu("open.push", "Open the push menu", open_push),
        menu("open.merge", "Open the merge menu", open_merge),
        menu("open.rebase", "Open the rebase menu", open_rebase),
        menu("open.stash", "Open the stash menu", open_stash),
        menu("open.tag", "Open the tag menu", open_tag),
        menu("open.reset", "Open the reset menu", open_reset),
        menu("open.revert", "Open the revert menu", open_revert),
        menu(
            "open.cherry-pick",
            "Open the cherry-pick menu",
            open_cherry_pick,
        ),
        menu("open.bisect", "Open the bisect menu", open_bisect),
        menu("open.remote", "Open the remote menu", open_remote),
        menu("open.submodule", "Open the submodule menu", open_submodule),
        menu("open.worktree", "Open the worktree menu", open_worktree),
        menu("open.ignore", "Open the ignore menu", open_ignore),
        menu(
            "open.patch-apply",
            "Open the apply-patches menu",
            open_patch_apply,
        ),
        menu(
            "open.patch-create",
            "Open the format-patches menu",
            open_patch_create,
        ),
        planned("open.refs", Scope::Status, "Open the refs view", "R-5.3"),
        planned(
            "open.cherries",
            Scope::Status,
            "Open the cherry view",
            "R-7.2",
        ),
        planned("open.run", Scope::Status, "Run a Git command", "R-8.2"),
        Command {
            id: "open.process-log",
            scope: Scope::Status,
            doc: "Open the process log",
            status: CommandStatus::Implemented,
            run: open_process_log,
        },
        Command {
            id: "process-log.clear",
            scope: Scope::ProcessLog,
            doc: "Clear the process log",
            status: CommandStatus::Implemented,
            run: clear_process_log,
        },
        Command {
            id: "select.toggle-mark",
            scope: Scope::Select,
            doc: "Toggle the mark on the commit at point",
            status: CommandStatus::Implemented,
            run: toggle_commit_mark,
        },
        Command {
            id: "select.enter-rev",
            scope: Scope::Select,
            doc: "Enter a revision by hand",
            status: CommandStatus::Implemented,
            run: enter_revision,
        },
        Command {
            id: "menu.save-args",
            scope: Scope::Menu,
            doc: "Save the menu arguments",
            status: CommandStatus::Implemented,
            run: save_menu_args,
        },
        Command {
            id: "menu.reset-args",
            scope: Scope::Menu,
            doc: "Reset the menu arguments",
            status: CommandStatus::Implemented,
            run: reset_menu_args,
        },
        planned("commit-message.history-prev", Scope::CommitMessage,
            "Recall the previous commit message", "R-EDIT-HISTORY"),
        planned("commit-message.history-next", Scope::CommitMessage,
            "Recall the next commit message", "R-EDIT-HISTORY"),
        planned(
            "commit-message.add-signoff",
            Scope::CommitMessage,
            "Add a Signed-off-by trailer",
            "R-4.2",
        ),
    ]);

    // Every menu action is a registry command too: the menu item carries the
    // label and status for rendering, the registry entry is what a future
    // implementation will run. Actions that are already registered (the
    // `open.*` openers) are not duplicated.
    for def in crate::menus::all() {
        for group in &def.groups {
            for item in &group.items {
                let crate::menus::ItemDef::Action {
                    command,
                    label,
                    roadmap,
                    ..
                } = item
                else {
                    continue;
                };
                if roadmap.is_empty() || commands.iter().any(|existing| existing.id == *command) {
                    continue;
                }
                commands.push(planned(command, Scope::MenuAction, label, roadmap));
            }
        }
    }

    Registry { commands }
}

fn refresh(_ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    Ok(CommandOutcome {
        refresh: true,
        ..Default::default()
    })
}

fn log_load_more(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    if let Err(err) = ctx.view.load_more() {
        if let Some(effect) = Effect::from_error(&err) {
            ctx.extend(vec![effect]);
        } else {
            return Err(err);
        }
    }
    Ok(CommandOutcome::default())
}

/// Open a menu by name; the session creates the view and pushes it.
fn open_menu(ctx: &mut CommandCtx<'_>, name: &str) -> Result<CommandOutcome> {
    let menu = crate::menus::by_name(name, ctx.keymaps)
        .ok_or_else(|| crate::Error::new(format!("The `{name}` menu is not implemented yet.")))?;
    Ok(CommandOutcome {
        open_menu: Some(menu),
        ..Default::default()
    })
}

/// Open the commit menu; the session creates the view and pushes it.
fn open_commit(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    open_menu(ctx, "commit")
}

/// Commit asks the server to launch `git commit` on a detached worker.
///
/// The pre-flight runs before progress: a clean tree requires allow-empty,
/// and an unstaged-only tree asks before staging unless all is selected.
/// Options are validated before mutation
/// and captured with the operation across the staging prompt.
fn commit_create(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    commit_operation(ctx, crate::git::CommitKind::Create)
}

fn commit_amend(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> { commit_operation(ctx, crate::git::CommitKind::Amend) }
fn commit_extend(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> { commit_operation(ctx, crate::git::CommitKind::Extend) }
fn commit_reword(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> { commit_operation(ctx, crate::git::CommitKind::Reword) }

fn commit_operation(ctx: &mut CommandCtx<'_>, kind: crate::git::CommitKind) -> Result<CommandOutcome> {
    let repo = GitRepo::open(ctx.repo_path)?;
    let options = match commit_options(ctx.args, &repo, kind) {
        Ok(options) => options,
        Err(error) => {
            ctx.info(format!("Invalid commit arguments: {error}"));
            return Ok(CommandOutcome::default());
        }
    };
    let status = repo.status_listing()?;
    if kind != crate::git::CommitKind::Create && status.head.is_none() {
        ctx.info("Cannot edit HEAD: this repository has no commit yet");
        return Ok(CommandOutcome::default());
    }
    if kind != crate::git::CommitKind::Create && (options.reuse_message.is_some() || options.reedit_message.is_some()) {
        ctx.info("Message reuse and reedit options conflict with HEAD edit actions");
        return Ok(CommandOutcome::default());
    }
    if kind == crate::git::CommitKind::Reword && options.all {
        ctx.info("Reword uses --only and cannot be combined with --all");
        return Ok(CommandOutcome::default());
    }
    if kind == crate::git::CommitKind::Extend && status.staged.is_empty()
        && !(options.all && !status.unstaged.is_empty())
        && !(options.allow_empty || options.reset_author || options.signoff || options.author.is_some() || options.date.is_some())
    {
        if status.unstaged.is_empty() {
            ctx.info("Nothing staged (or unstaged) to extend");
            return Ok(CommandOutcome::default());
        }
        return Ok(CommandOutcome { prompt: Some((head_commit_prompt(
            "Nothing staged. Commit all tracked changes and extend HEAD?", None,
        ), PromptAction::ExtendAll { fingerprint: RepositoryFingerprint::capture(&repo)?, options })), ..Default::default() });
    }
    let ready = options.allow_empty || (options.all && !status.unstaged.is_empty());
    let operation = match kind {
        crate::git::CommitKind::Amend => crate::git::CommitOperation::Amend(options),
        crate::git::CommitKind::Extend => crate::git::CommitOperation::Extend(options),
        crate::git::CommitKind::Reword => crate::git::CommitOperation::Reword(options),
        crate::git::CommitKind::Create => crate::git::CommitOperation::Create(options),
    };
    if kind != crate::git::CommitKind::Create {
        return prepare_head_commit(ctx, &repo, operation);
    }
    if status.staged.is_empty() && !ready {
        if status.unstaged.is_empty() && status.untracked.is_empty() {
            ctx.effects.push(Effect::Message {
                level: MessageLevel::Info,
                text: "Nothing to commit (working tree clean)".to_string(),
                hint: Some("Use `git commit --allow-empty` to record an empty commit.".to_string()),
                detail: None,
                process_log_id: None,
            });
            return Ok(CommandOutcome::default());
        }
        let prompt = ViewPromptParams {
            prompt_id: String::new(),
            kind: PromptKind::Confirm,
            title: "Nothing staged. Stage all changes and commit?".to_string(),
            detail: None,
            yes_label: None,
            no_label: None,
            destructive: None,
            default: None,
            placeholder: None,
            secret: None,
            items: None,
            allow_custom: None,
            preselected: None,
            min: None,
            max: None,
        };
        return Ok(CommandOutcome { prompt: Some((prompt, PromptAction::Commit {
            fingerprint: RepositoryFingerprint::capture(&repo)?,
            untracked: !status.untracked.is_empty(),
            operation,
        })), ..Default::default() });
    }
    ctx.info("Committing…");
    Ok(CommandOutcome {
        commit: Some(operation),
        ..Default::default()
    })
}

fn head_commit_prompt(title: &str, detail: Option<&str>) -> ViewPromptParams {
    ViewPromptParams {
        prompt_id: String::new(), kind: PromptKind::Confirm, title: title.into(),
        detail: detail.map(str::to_string), yes_label: None, no_label: None,
        destructive: Some(true), default: None, placeholder: None, secret: None,
        items: None, allow_custom: None, preselected: None, min: None, max: None,
    }
}

fn prepare_head_commit(ctx: &mut CommandCtx<'_>, repo: &GitRepo, operation: crate::git::CommitOperation) -> Result<CommandOutcome> {
    if repo.head_is_published()? {
        let prompt = head_commit_prompt("Rewrite published HEAD?", Some(
            "HEAD is reachable from a remote-tracking branch. Rewriting it may require a force push.",
        ));
        return Ok(CommandOutcome { prompt: Some((prompt, PromptAction::PublishedCommit {
            fingerprint: RepositoryFingerprint::capture(repo)?, operation,
        })), ..Default::default() });
    }
    ctx.info("Committing…");
    Ok(CommandOutcome { commit: Some(operation), ..Default::default() })
}

/// Validate structured commit-menu input once, before staging or launching Git.
fn commit_options(args: Option<&Value>, repo: &GitRepo, kind: crate::git::CommitKind) -> Result<crate::git::CommitOptions> {
    let args: crate::menu::Args = match args.filter(|value| !value.is_null()) {
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|error| crate::Error::new(format!("Expected commit switches and option values: {error}")))?,
        None => crate::menu::Args::default(),
    };
    let mut options = crate::git::CommitOptions::default();
    let mut seen = std::collections::HashSet::new();
    for key in args.switches {
        if !seen.insert(key.clone()) { return Err(crate::Error::new(format!("Duplicate commit switch {key}"))); }
        match key.as_str() {
            "-a" => options.all = true,
            "-e" => options.allow_empty = true,
            "-v" => options.verbose = true,
            "-n" => options.no_verify = true,
            "-R" => options.reset_author = true,
            "-s" => options.signoff = true,
            _ => return Err(crate::Error::new(format!("Unsupported commit switch {key}"))),
        }
    }
    for (key, value) in args.options {
        if value.contains('\0') || (value.trim().is_empty() && key != "=S") {
            return Err(crate::Error::new(format!("Missing or invalid value for {key}")));
        }
        match key.as_str() {
            "=A" => {
                let valid = value.rsplit_once('<').is_some_and(|(name, email)|
                    !name.trim().is_empty() && email.ends_with('>') && email.len() > 2);
                if !valid || value.contains(['\n', '\r']) {
                    return Err(crate::Error::new("Author must be Name <email>"));
                }
                options.author = Some(value);
            }
            "=D" => options.date = Some(value),
            "=S" => options.signing_key = Some(value),
            "=C" => options.reuse_message = Some(value),
            "=c" => options.reedit_message = Some(value),
            _ => return Err(crate::Error::new(format!("Unsupported commit option {key}"))),
        }
    }
    if options.reuse_message.is_some() && options.reedit_message.is_some() {
        return Err(crate::Error::new("Reuse and reedit message options conflict"));
    }
    if options.reset_author && options.author.is_some() {
        return Err(crate::Error::new("Reset author cannot be combined with author"));
    }
    if options.reset_author && kind == crate::git::CommitKind::Create
        && options.reuse_message.is_none() && options.reedit_message.is_none() {
        return Err(crate::Error::new("Reset author requires reuse/reedit when creating a commit"));
    }
    for revision in [&mut options.reuse_message, &mut options.reedit_message].into_iter().flatten() {
        *revision = repo.resolve_commit(revision)?;
    }
    if let Some(date) = &options.date { repo.validate_author_date(date)?; }
    Ok(options)
}

/// Open the dispatch menu, including from inside another menu.
fn open_dispatch(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    open_menu(ctx, "dispatch")
}

/// Open the process log view; the session creates and pushes it.
fn open_process_log(_ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    Ok(CommandOutcome {
        open_view: Some((ViewKind::ProcessLog, Placement::Main)),
        ..Default::default()
    })
}

/// Open the current repository's server-rendered commit log.
fn open_current_log(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    // The menu arguments are interpreted by the log view using its typed
    // options contract.
    Ok(CommandOutcome {
        open_view: Some((ViewKind::Log, Placement::Main)),
        open_args: ctx.args.cloned(),
        ..Default::default()
    })
}

/// Push a commit-selection log used to copy one or more commit IDs.
fn select_and_copy(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    let args = ctx.args.and_then(Value::as_object);
    let string_arg = |name: &str, default: &str| {
        args.and_then(|args| args.get(name)).and_then(Value::as_str).unwrap_or(default).to_string()
    };
    let bool_arg = |name: &str, default: bool| {
        args.and_then(|args| args.get(name)).and_then(Value::as_bool).unwrap_or(default)
    };
    Ok(CommandOutcome {
        open_selection: Some(SelectionRequest {
            options: crate::session::log_options_from_args(ctx.args)?,
            settings: CommitSelection {
                purpose: string_arg("purpose", "copy commits"),
                prompt: string_arg("prompt", "Select commit(s) to copy"),
                multi: bool_arg("multi", false),
                allow_free_text: bool_arg("allowFreeText", false),
            },
            operation: SelectionOperation::CopyCommits,
        }),
        ..Default::default()
    })
}

/// Toggle a commit mark using its full identity in the acknowledged tree.
fn toggle_commit_mark(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    let Some(tree) = ctx.view.command_source_tree() else {
        ctx.info("Wait for the log view to render before selecting a commit");
        return Ok(CommandOutcome::default());
    };
    let oid = ctx.cursor.and_then(|cursor| {
        tree.line_nodes.get(cursor.line).and_then(|line| tree.node(&line.node))
            .filter(|node| node.kind == NodeKind::Commit).and_then(|node| node.commit.clone())
    });
    if oid.as_deref().is_some_and(|oid| ctx.view.toggle_commit_mark(oid)) {
        return Ok(CommandOutcome { refresh: false, ..Default::default() });
    }
    ctx.info("Place the cursor on a selectable commit to mark it");
    Ok(CommandOutcome::default())
}

/// Prompt for a revision and return its validated full commit object ID.
fn enter_revision(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    if !ctx.view.commit_selection().is_some_and(|selection| selection.allow_free_text) {
        ctx.info("Free-text revisions are not available for this selection");
        return Ok(CommandOutcome::default());
    }
    let prompt = ViewPromptParams {
        prompt_id: String::new(),
        kind: PromptKind::Text,
        title: "Enter a commit revision".to_string(),
        detail: None,
        yes_label: None,
        no_label: None,
        destructive: None,
        default: None,
        placeholder: Some("Revision".to_string()),
        secret: None,
        items: None,
        allow_custom: None,
        preselected: None,
        min: None,
        max: None,
    };
    Ok(CommandOutcome { prompt: Some((prompt, PromptAction::Revision)), ..Default::default() })
}

/// Drop every record from the process log and re-render the view.
fn clear_process_log(_ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    crate::runner::global_log().clear();
    Ok(CommandOutcome {
        refresh: true,
        ..Default::default()
    })
}

macro_rules! menu_opener {
    ($name:ident, $menu:literal) => {
        /// Open the menu with this name.
        fn $name(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
            open_menu(ctx, $menu)
        }
    };
}

menu_opener!(open_branch, "branch");
menu_opener!(open_log, "log");
menu_opener!(open_diff, "diff");
menu_opener!(open_fetch, "fetch");
menu_opener!(open_pull, "pull");
menu_opener!(open_push, "push");
menu_opener!(open_merge, "merge");
menu_opener!(open_rebase, "rebase");
menu_opener!(open_stash, "stash");
menu_opener!(open_tag, "tag");
menu_opener!(open_reset, "reset");
menu_opener!(open_revert, "revert");
menu_opener!(open_cherry_pick, "cherry-pick");
menu_opener!(open_bisect, "bisect");
menu_opener!(open_remote, "remote");
menu_opener!(open_submodule, "submodule");
menu_opener!(open_worktree, "worktree");
menu_opener!(open_ignore, "ignore");
menu_opener!(open_patch_apply, "patch-apply");
menu_opener!(open_patch_create, "patch-create");

/// Remember the current switches/options as this menu's defaults.
///
/// Storage is in memory for now; persisted global/per-repo storage arrives
/// with the menu configuration work (§3.11, §3.18).
fn save_menu_args(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    match ctx.view.menu_mut() {
        Some(menu) => {
            menu.save_args();
            ctx.info("Saved menu arguments");
        }
        None => ctx.info("No menu is open"),
    }
    Ok(CommandOutcome::default())
}

/// Restore the menu's saved switches/options.
fn reset_menu_args(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    match ctx.view.menu_mut() {
        Some(menu) => {
            menu.reset_args();
            ctx.info("Reset menu arguments");
        }
        None => ctx.info("No menu is open"),
    }
    Ok(CommandOutcome::default())
}

/// Flip the view between interactive and text mode; the session applies it.
fn toggle_text_mode(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    let mode = match ctx.view.mode() {
        ViewMode::Text => ViewMode::Interactive,
        ViewMode::Interactive => ViewMode::Text,
    };
    Ok(CommandOutcome {
        mode: Some(mode),
        ..Default::default()
    })
}

/// Copy the value at point as a clipboard effect, from the request-bound tree.
fn copy_value(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    let source = ctx
        .view
        .command_source_tree()
        .cloned()
        .unwrap_or_else(|| ctx.view.tree().clone());
    let selection = ctx
        .selection
        .map(|selection| (selection.start_line, selection.end_line));
    match source.copy_value(ctx.cursor.map(|cursor| cursor.line), selection) {
        Some(text) => ctx.effects.push(Effect::Clipboard { text }),
        None => ctx.info("Nothing to copy here"),
    }
    Ok(CommandOutcome::default())
}

/// Visit the worktree file at point.
///
/// Resolution is server-owned: the request tree maps the cursor's line to a file
/// and new-side line, and the client only opens the resulting location.
fn visit(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    let Some(cursor) = ctx.cursor else {
        ctx.info("Nothing to visit here");
        return Ok(CommandOutcome::default());
    };
    let source = ctx
        .view
        .command_source_tree()
        .cloned()
        .unwrap_or_else(|| ctx.view.tree().clone());
    let Some(location) = source.definition(
        ctx.repo_path,
        Position::new(cursor.line as u32, cursor.column as u32),
    ) else {
        ctx.info("Nothing to visit here");
        return Ok(CommandOutcome::default());
    };
    ctx.effects.push(Effect::VisitFile {
        uri: location.uri.as_str().to_string(),
        line: location.range.start.line,
        column: location.range.start.character,
    });
    Ok(CommandOutcome::default())
}

/// Confirm a commit selection; managed drafts require versioned save/finish.
fn confirm(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    if ctx.view.commit_selection().is_some() {
        let Some(tree) = ctx.view.command_source_tree() else {
            ctx.info("Wait for the log view to render before selecting a commit");
            return Ok(CommandOutcome::default());
        };
        let cursor_oid = ctx.cursor.and_then(|cursor| tree.line_nodes.get(cursor.line)
            .and_then(|line| tree.node(&line.node)).filter(|node| node.kind == NodeKind::Commit)
            .and_then(|node| node.commit.as_deref()));
        let selected = ctx.view.selected_commits(cursor_oid);
        if selected.is_empty() {
            ctx.info("Place the cursor on a commit to confirm the selection");
            return Ok(CommandOutcome::default());
        }
        return Ok(CommandOutcome { selected_commits: Some(selected), ..Default::default() });
    }
    if ctx.draft_run_id().is_none() {
        ctx.info("Nothing to confirm");
        return Ok(CommandOutcome::default());
    }
    ctx.info("Use magit.command common.confirm with the exact synchronized commit message version");
    Ok(CommandOutcome::default())
}

/// Cancel closes a menu, aborts a pending editor bridge edit, falls through
/// when the user has a visual selection, and is a quiet no-op otherwise.
fn cancel(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    if let Some(edit_id) = ctx.draft_run_id() {
        if editor::registry().resolve(edit_id, Reply::Abort) {
            return Ok(CommandOutcome::default());
        }
    }
    if ctx.view.menu().is_some() {
        return Ok(CommandOutcome {
            pop: true,
            ..Default::default()
        });
    }
    if ctx.view.commit_selection().is_some() {
        return Ok(CommandOutcome { pop: true, ..Default::default() });
    }
    if ctx.selection.is_some() {
        let key = ctx.key.clone().unwrap_or_else(|| "Escape".to_string());
        ctx.extend(vec![Effect::PassThrough { key }]);
    }
    Ok(CommandOutcome::default())
}

/// Quit the current view, closing the session when it is the root view.
fn quit(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    ctx.extend(vec![Effect::Quit]);
    Ok(CommandOutcome::default())
}

fn apply_visibility(
    ctx: &mut CommandCtx<'_>,
    command: VisibilityCommand,
) -> Result<CommandOutcome> {
    // Positional commands use their bound source; global visibility uses the ack.
    let source = ctx
        .view
        .command_source_tree()
        .cloned()
        .unwrap_or_else(|| ctx.view.tree().clone());
    ctx.view
        .set_visibility(&source, ctx.cursor.map(|cursor| cursor.line), command);
    Ok(CommandOutcome::default())
}

fn toggle_section(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    let recursive = ctx.recursive();
    apply_visibility(ctx, VisibilityCommand::Toggle { recursive })
}

fn show_section(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    let recursive = ctx.recursive();
    apply_visibility(ctx, VisibilityCommand::Show { recursive })
}

fn hide_section(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    let recursive = ctx.recursive();
    apply_visibility(ctx, VisibilityCommand::Hide { recursive })
}

fn show_all(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    apply_visibility(ctx, VisibilityCommand::ShowAll)
}

fn hide_all(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    apply_visibility(ctx, VisibilityCommand::HideAll)
}

fn more_detail(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    apply_visibility(ctx, VisibilityCommand::MoreDetail)
}

fn less_detail(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    apply_visibility(ctx, VisibilityCommand::LessDetail)
}

fn detail_level_1(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    apply_visibility(ctx, VisibilityCommand::SetDetail(1))
}

fn detail_level_2(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    apply_visibility(ctx, VisibilityCommand::SetDetail(2))
}

fn detail_level_3(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    apply_visibility(ctx, VisibilityCommand::SetDetail(3))
}

fn detail_level_4(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    apply_visibility(ctx, VisibilityCommand::SetDetail(4))
}

fn next_section(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    Ok(CommandOutcome {
        cursor: section_cursor(ctx.view, ctx.cursor, true),
        ..Default::default()
    })
}

fn cycle_visibility(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    ctx.view.cycle_visibility();
    Ok(CommandOutcome::default())
}

fn hierarchy_cursor(ctx: &mut CommandCtx<'_>, direction: Option<bool>) -> Result<CommandOutcome> {
    let tree = ctx.view.command_source_tree().unwrap_or_else(|| ctx.view.tree());
    let target = ctx.cursor.and_then(|cursor| tree.navigation_node(cursor.line)).and_then(|node| {
        match direction {
            Some(forward) => tree.sibling(&node.id, forward).or(Some(node)),
            None => tree.parent(&node.id).or(Some(node)),
        }
    });
    Ok(CommandOutcome { cursor_nodes: target.map(|node| node.id.clone()).into_iter().collect(), ..Default::default() })
}

fn next_sibling(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    hierarchy_cursor(ctx, Some(true))
}

fn prev_sibling(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    hierarchy_cursor(ctx, Some(false))
}

fn parent_section(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    hierarchy_cursor(ctx, None)
}

fn prev_section(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    Ok(CommandOutcome {
        cursor: section_cursor(ctx.view, ctx.cursor, false),
        ..Default::default()
    })
}

/// Move to a visible structured header of the requested kind.
fn jump(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    let args = ctx.args.and_then(Value::as_object);
    let kind = args.and_then(|args| args.get("kind")).and_then(Value::as_str);
    let dir = args.and_then(|args| args.get("dir")).and_then(Value::as_i64);
    let Some(kind) = kind.filter(|kind| matches!(*kind, "section" | "file" | "hunk")) else {
        ctx.info("Jump kind must be section, file or hunk");
        return Ok(CommandOutcome::default());
    };
    let Some(dir) = dir.filter(|dir| *dir == 1 || *dir == -1) else {
        ctx.info("Jump direction must be 1 or -1");
        return Ok(CommandOutcome::default());
    };
    let wanted = match kind { "section" => NodeKind::Section, "file" => NodeKind::File, _ => NodeKind::Hunk };
    let Some(cursor) = ctx.cursor else { return Ok(CommandOutcome::default()); };
    let source = ctx.view.command_source_tree().unwrap_or_else(|| ctx.view.tree());
    let lines: Vec<usize> = source.line_nodes.iter().enumerate().filter_map(|(line, owner)| {
        (owner.kind == wanted && source.node(&owner.node).is_some_and(|node| node.start_line == line)).then_some(line)
    }).collect();
    let target = if dir > 0 { lines.into_iter().find(|line| *line > cursor.line) }
        else { lines.into_iter().rev().find(|line| *line < cursor.line) };
    Ok(CommandOutcome { cursor: target.map(|line| Cursor { line, column: 0 }), ..Default::default() })
}

fn stage(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    stage_or_unstage(ctx, dispatch::Command::Stage)
}

fn unstage(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    stage_or_unstage(ctx, dispatch::Command::Unstage)
}

fn discard(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    stage_or_unstage(ctx, dispatch::Command::Discard)
}

fn stage_or_unstage(
    ctx: &mut CommandCtx<'_>,
    command: dispatch::Command,
) -> Result<CommandOutcome> {
    let selection = ctx.selection.map(|selection| dispatch::LineRange {
        start: selection.start_line as u32,
        end: selection.end_line as u32,
    });
    let source = ctx.view.command_source_tree().unwrap_or_else(|| ctx.view.tree());
    // A stage request originating in the displayed staged section is never
    // retargeted to an unstaged hunk after a refresh. Visual selections retain
    // precedence and are resolved by their original section below.
    if command == dispatch::Command::Stage && selection.is_none()
        && ctx.cursor.and_then(|cursor| source.navigation_node(cursor.line))
            .is_some_and(|node| node.kind == NodeKind::Hunk && node.section.as_deref() == Some("staged"))
    {
        return Ok(CommandOutcome::default());
    }
    let destinations = if command == dispatch::Command::Stage && ctx.selection.is_none() {
        ctx.cursor.and_then(|cursor| source.navigation_node(cursor.line))
            .filter(|node| node.kind == NodeKind::Hunk && node.section.as_deref() == Some("unstaged"))
            .map(|node| {
                // Capture the successor and source ancestors before Git mutates the view.
                // Never follow the staged hunk's relocation when no sibling survives.
                let file = source.parent(&node.id);
                source.sibling(&node.id, true).into_iter()
                    .chain(file)
                    .chain(file.and_then(|file| source.parent(&file.id)))
                    .map(|node| node.id.clone()).collect()
            }).unwrap_or_default()
    } else { Vec::new() };
    let (effects, prompt) = stage_or_unstage_at(
        ctx.view,
        ctx.repo_path,
        command,
        ctx.cursor,
        selection,
    )?;
    let succeeded = effects.is_empty();
    let move_cursor = succeeded && prompt.is_none();
    ctx.extend(effects);
    Ok(CommandOutcome {
        prompt,
        refresh: true,
        cursor_nodes: if move_cursor { destinations } else { Vec::new() },
        ..Default::default()
    })
}

fn stage_all(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    let effects = run_resolution(ctx.repo_path, dispatch::Resolution::StageAllUnstaged)?;
    ctx.extend(effects);
    Ok(CommandOutcome {
        refresh: true,
        ..Default::default()
    })
}

fn unstage_all(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    let effects = run_resolution(ctx.repo_path, dispatch::Resolution::UnstageAllStaged)?;
    ctx.extend(effects);
    Ok(CommandOutcome {
        refresh: true,
        ..Default::default()
    })
}

/// The cursor line of the next or previous section header.
fn section_cursor(view: &OpenView, cursor: Option<Cursor>, forward: bool) -> Option<Cursor> {
    let cursor = cursor?;
    let source = view.command_source_tree().unwrap_or_else(|| view.tree());
    let sections: Vec<&Node> = source
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Section)
        .collect();
    let current = source
        .line_nodes
        .get(cursor.line)
        .and_then(|line| line.section.as_deref());
    let index = current.and_then(|section| {
        sections
            .iter()
            .position(|node| node.section.as_deref() == Some(section))
    });
    let target = match (index, forward) {
        (Some(index), true) => sections.get(index + 1),
        (Some(index), false) => index.checked_sub(1).and_then(|index| sections.get(index)),
        (None, true) => sections.first(),
        (None, false) => sections.last(),
    }?;
    Some(Cursor {
        line: target.start_line,
        column: 0,
    })
}

fn stage_or_unstage_at(
    view: &OpenView,
    repo_path: &str,
    command: dispatch::Command,
    cursor: Option<Cursor>,
    selection: Option<dispatch::LineRange>,
) -> Result<(Vec<Effect>, Option<(ViewPromptParams, PromptAction)>)> {
    // Resolve against the exact request tree; actions are addressed by its
    // node identities, never by index in a different retained render.
    let source = view.command_source_tree().unwrap_or_else(|| view.tree());

    let resolution = if let Some(range) = selection {
        // A visual selection wins over the node under the cursor (IMM-2).
        dispatch::resolve(command, dispatch::NodeKind::Other, Some(range))
    } else {
        let Some(cursor) = cursor else {
            return Ok((vec![Effect::message(
                MessageLevel::Info,
                "Place the cursor on a change to stage or unstage.",
            )], None));
        };
        let Some(node) = source
            .line_nodes
            .get(cursor.line)
            .and_then(|line| source.node(&line.node))
        else {
            return Ok((vec![Effect::message(
                MessageLevel::Info,
                "No change under the cursor.",
            )], None));
        };

        let current = view.tree();
        let node = if current.node(&node.id).is_some() {
            node
        } else {
            // The node moved or disappeared. Re-resolve by identity (a hunk id
            // is a content hash) before giving up; never use a bare line index.
            match re_resolve(current, node) {
                Some(node) => node,
                None => {
                    return Ok((vec![Effect::message(
                        MessageLevel::Warn,
                        errors::HUNK_CHANGED,
                    )], None))
                }
            }
        };

        // An untracked file has no index entry to remove.
        if command == dispatch::Command::Unstage && node.section.as_deref() == Some("untracked") {
            return Ok((vec![Effect::message(
                MessageLevel::Info,
                dispatch::NOTHING_TO_UNSTAGE,
            )], None));
        }

        dispatch::resolve(command, dispatch_target(node), None)
    };

    let repo = GitRepo::open(repo_path)?;
    if let Some(path) = match &resolution {
        dispatch::Resolution::StageFile { path } | dispatch::Resolution::UnstageFile { path } => Some(path.as_str()),
        _ => None,
    } {
        let section = if matches!(resolution, dispatch::Resolution::UnstageFile { .. }) {
            "staged"
        } else {
            file_worktree_section(source, path)
        };
        if !displayed_file_is_current(&repo, source, path, section)? {
            return Ok((vec![Effect::message(MessageLevel::Warn, errors::HUNK_CHANGED)], None));
        }
    }
    if matches!(
        resolution,
        dispatch::Resolution::DiscardAllUnstaged
            | dispatch::Resolution::DiscardAllUntracked
            | dispatch::Resolution::DiscardFile { .. }
            | dispatch::Resolution::DiscardHunk { .. }
            | dispatch::Resolution::DiscardSelection { .. }
    ) {
        if let dispatch::Resolution::DiscardFile { path } = &resolution {
            let section = file_worktree_section(source, path);
            if !displayed_file_is_current(&repo, source, path, section)? {
                return Ok((vec![Effect::message(MessageLevel::Warn, errors::HUNK_CHANGED)], None));
            }
        }
        let what = match &resolution {
            dispatch::Resolution::DiscardFile { path } => format!("Discard changes to {path}?"),
            dispatch::Resolution::DiscardHunk { path, .. } => {
                format!("Discard this hunk in {path}?")
            }
            dispatch::Resolution::DiscardSelection { .. } => {
                "Discard the selected changes?".to_string()
            }
            dispatch::Resolution::DiscardAllUntracked => "Delete all untracked files?".to_string(),
            _ => "Discard all unstaged changes?".to_string(),
        };
        let prompt = ViewPromptParams {
            prompt_id: String::new(),
            kind: PromptKind::Confirm,
            title: what,
            detail: None,
            yes_label: None,
            no_label: None,
            destructive: Some(true),
            default: Some(Value::Bool(false)),
            placeholder: None,
            secret: None,
            items: None,
            allow_custom: None,
            preselected: None,
            min: None,
            max: None,
        };
        return Ok((Vec::new(), Some((prompt, PromptAction::Discard {
            fingerprint: RepositoryFingerprint::capture(&repo)?, resolution, tree: source.clone(),
        }))));
    }
    let effects = match &resolution {
        dispatch::Resolution::Selection { start, end, reverse } => {
            execute_displayed_selection(&repo, source, *start, *end, *reverse, false)
        }
        dispatch::Resolution::DiscardSelection { start, end } => {
            execute_displayed_selection(&repo, source, *start, *end, false, true)
        }
        _ => dispatch::execute(&repo, &resolution, &[]),
    }?;
    Ok((effects, None))
}

/// Check the complete file state captured by the originating render.
fn displayed_file_is_current(repo: &GitRepo, tree: &Tree, path: &str, section: &str) -> Result<bool> {
    let Some(file) = tree.nodes.iter().find(|node| node.kind == NodeKind::File
        && node.path.as_deref() == Some(path) && node.section.as_deref() == Some(section)) else {
        return Ok(false);
    };
    let Some(crate::tree::MutationContent::File(captured)) = file.mutation.as_ref() else {
        return Ok(false);
    };
    let current = repo.file_mutation_identity(path, captured.original.as_ref().map(|(path, _)| path.as_str()), section != "staged")?;
    fn matches(captured: &crate::git::FileMutationIdentity, current: &crate::git::FileMutationIdentity, staged: bool) -> bool {
        captured.head == current.head && captured.index == current.index
            && (staged || captured.worktree == current.worktree)
            && match (&captured.original, &current.original) {
                (None, None) => true,
                (Some((a, before)), Some((b, now))) => a == b && matches(before, now, staged),
                _ => false,
            }
    }
    Ok(matches(&captured, &current, section == "staged"))
}

fn file_worktree_section<'a>(tree: &'a Tree, path: &str) -> &'a str {
    tree.nodes.iter().find(|node| node.kind == NodeKind::File
        && node.path.as_deref() == Some(path)
        && matches!(node.section.as_deref(), Some("unstaged" | "untracked")))
        .and_then(|node| node.section.as_deref()).unwrap_or("unstaged")
}

/// Run a whole-section operation against `repo_path`.
fn run_resolution(repo_path: &str, resolution: dispatch::Resolution) -> Result<Vec<Effect>> {
    let repo = GitRepo::open(repo_path)?;
    dispatch::execute(&repo, &resolution, &[])
}

/// Resolve displayed coordinates before reading Git; fresh diffs only supply
/// current patch coordinates for the selected content identities.
fn execute_displayed_selection(
    repo: &GitRepo, tree: &Tree, start: u32, end: u32, staged: bool, discard: bool,
) -> Result<Vec<Effect>> {
    let (start, end) = (start.min(end) as usize, start.max(end) as usize);
    let section = if staged { "staged" } else { "unstaged" };
    // Preserve the selection's originating-section rule.
    if tree.line_nodes.get(start).or_else(|| tree.line_nodes.get(end))
        .and_then(|line| line.section.as_deref()) != Some(section) {
        return Ok(vec![Effect::message(MessageLevel::Info, crate::patch::NO_CHANGED_LINES)]);
    }
    let selected = |node: &Node| start <= node.start_line && node.start_line <= end;
    let mut ops = Vec::new();
    let mut layouts = Vec::new();
    for file in tree.nodes.iter().filter(|node| node.kind == NodeKind::File
        && node.section.as_deref() == Some(section)) {
        if end < file.start_line || start > file.end_line { continue; }
        let Some(path) = file.path.as_deref() else { continue; };
        let hunks: Vec<_> = tree.nodes.iter().filter(|node| node.kind == NodeKind::Hunk
            && node.section.as_deref() == Some(section) && node.path.as_deref() == Some(path)).collect();
        let first_op = ops.len();
        if selected(file) {
            ops.push(SelectionOp::WholeFile { path: path.into() });
        } else {
            for hunk in &hunks {
                let Some(id) = hunk.hunk_id.as_deref() else { continue; };
                if selected(hunk) {
                    ops.push(SelectionOp::WholeHunk { path: path.into(), hunk_id: id.into() });
                } else {
                    let lines: Vec<_> = tree.nodes.iter().filter(|node| node.kind == NodeKind::Line
                        && node.section.as_deref() == Some(section) && node.path.as_deref() == Some(path)
                        && node.hunk_id.as_deref() == Some(id) && selected(node)
                        && matches!(node.marker, Some('+') | Some('-')))
                        .filter_map(|node| node.line_index).collect();
                    if !lines.is_empty() {
                        ops.push(SelectionOp::Lines { path: path.into(), hunk_id: id.into(), lines });
                    }
                }
            }
        }
        if ops.len() == first_op { continue; }
        let whole_file = ops[first_op..].iter().any(|op| matches!(op, SelectionOp::WholeFile { .. }));
        if whole_file && !displayed_file_is_current(repo, tree, path, section)? {
            return Ok(vec![Effect::message(MessageLevel::Warn, errors::HUNK_CHANGED)]);
        }
        let diff = if staged { repo.diff_staged(path)? } else { repo.diff_unstaged(path)? };
        // Layout rows are deliberately unused: operations already contain the
        // request tree's identities and line_index offsets, not fresh rows.
        let layout = if whole_file {
            FileLayout::from_diff(&diff, 0).unwrap_or_else(|| FileLayout {
                path: path.to_string(), header: String::new(), header_line: 0, hunks: Vec::new(),
            })
        } else if let Some(layout) = FileLayout::from_diff(&diff, 0) {
            layout
        } else {
            return Ok(vec![Effect::message(MessageLevel::Warn, errors::HUNK_CHANGED)]);
        };
        let unchanged = ops[first_op..].iter().all(|op| match op {
            SelectionOp::WholeFile { .. } => whole_file,
            SelectionOp::WholeHunk { hunk_id, .. } | SelectionOp::Lines { hunk_id, .. } => {
                // Selection execution also consumes the fresh file header.
                // Compare its semantics, not just content IDs or fresh rows.
                let captured = hunks.iter().find(|node| node.hunk_id.as_ref() == Some(hunk_id))
                    .and_then(|node| node.mutation.as_ref());
                match captured {
                    Some(crate::tree::MutationContent::Hunk { hunk, .. }) =>
                        layout.hunks.iter().any(|h| &h.hunk.id == hunk_id
                            && h.hunk.metadata == hunk.metadata),
                    _ => false,
                }
            }
        });
        if !unchanged {
            return Ok(vec![Effect::message(MessageLevel::Warn, errors::HUNK_CHANGED)]);
        }
        layouts.push(layout);
    }
    // Validate every selected identity before mutating any file.
    let result = if discard { repo.discard_selection_ops(&layouts, &ops) }
        else { repo.apply_selection_ops(&layouts, &ops, staged) };
    errors::effects_for_result(result)
}

fn dispatch_target(node: &Node) -> dispatch::NodeKind {
    match node.kind {
        NodeKind::Section => match node.section.as_deref() {
            Some("unstaged") => dispatch::NodeKind::SectionUnstaged,
            Some("staged") => dispatch::NodeKind::SectionStaged,
            Some("untracked") => dispatch::NodeKind::SectionUntracked,
            _ => dispatch::NodeKind::Other,
        },
        NodeKind::File => match &node.path {
            Some(path) => dispatch::NodeKind::FileHeader { path: path.clone() },
            None => dispatch::NodeKind::Other,
        },
        NodeKind::Hunk => match (&node.path, &node.hunk_id) {
            (Some(path), Some(hunk_id)) => dispatch::NodeKind::HunkHeader {
                path: path.clone(),
                hunk_id: hunk_id.clone(),
            },
            _ => dispatch::NodeKind::Other,
        },
        NodeKind::Line => match (&node.path, &node.hunk_id) {
            (Some(path), Some(hunk_id)) => dispatch::NodeKind::DiffLine {
                changed: node.marker.is_some_and(|marker| marker != ' '),
                path: path.clone(),
                hunk_id: hunk_id.clone(),
            },
            _ => dispatch::NodeKind::Other,
        },
        _ => dispatch::NodeKind::Other,
    }
}
