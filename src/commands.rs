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
use crate::protocol::{
    Cursor, Effect, MessageLevel, PromptKind, PromptValue, Selection, ViewPromptParams,
};
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
    /// Commands available only inside the paused rebase todo editor.
    RebaseTodo,
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
            Scope::RebaseTodo => "rebase-todo",
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
            Scope::RebaseTodo => view_scope == "rebase-todo",
            other => other.as_str() == view_scope,
        }
    }
}

/// Where a command acts when a menu item invokes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandTarget {
    /// The nearest view that owns a document; transient menus above it are skipped.
    Document,
    /// The view the command was invoked from, including a menu.
    View,
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

/// An optional client effect a command may need to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    /// Copy text to the client's clipboard.
    Clipboard,
    /// Apply a workspace edit to a document.
    EditDocument,
    /// Open a file at a location in the client.
    VisitFile,
}

impl EffectKind {
    /// The wire name clients declare for this effect.
    pub fn name(self) -> &'static str {
        match self {
            EffectKind::Clipboard => "clipboard",
            EffectKind::EditDocument => "editDocument",
            EffectKind::VisitFile => "visitFile",
        }
    }
}

macro_rules! command {
    (
        id: $id:expr,
        scope: $scope:expr,
        doc: $doc:expr,
        status: $status:expr,
        run: $run:expr,
        effects: $effects:expr,
        target: $target:expr $(,)?
    ) => {
        Command {
            id: $id,
            scope: $scope,
            doc: $doc,
            status: $status,
            run: $run,
            destructive: false,
            effects: $effects,
            target: $target,
        }
    };
    (
        id: $id:expr,
        scope: $scope:expr,
        doc: $doc:expr,
        status: $status:expr,
        run: $run:expr,
        destructive: $destructive:expr,
        effects: $effects:expr,
        target: $target:expr $(,)?
    ) => {
        Command {
            id: $id,
            scope: $scope,
            doc: $doc,
            status: $status,
            run: $run,
            destructive: $destructive,
            effects: $effects,
            target: $target,
        }
    };
    (
        id: $id:expr,
        scope: $scope:expr,
        doc: $doc:expr,
        status: $status:expr,
        run: $run:expr,
        effects: $effects:expr $(,)?
    ) => {
        Command {
            id: $id,
            scope: $scope,
            doc: $doc,
            status: $status,
            run: $run,
            destructive: false,
            effects: $effects,
            target: CommandTarget::Document,
        }
    };
    (
        id: $id:expr,
        scope: $scope:expr,
        doc: $doc:expr,
        status: $status:expr,
        run: $run:expr,
        destructive: $destructive:expr,
        effects: $effects:expr $(,)?
    ) => {
        Command {
            id: $id,
            scope: $scope,
            doc: $doc,
            status: $status,
            run: $run,
            destructive: $destructive,
            effects: $effects,
            target: CommandTarget::Document,
        }
    };
}

/// A named server operation; see §3.5.
pub struct Command {
    pub id: &'static str,
    pub scope: Scope,
    pub doc: &'static str,
    pub status: CommandStatus,
    pub run: RunFn,
    /// Whether this command can discard data or perform an irreversible remote operation.
    pub destructive: bool,
    /// Optional effects this command may emit, in declaration order.
    pub effects: &'static [EffectKind],
    /// Where the command acts when a menu item invokes it.
    pub target: CommandTarget,
}

/// Return the registered command's declared effects; empty when unknown.
pub fn required_effects(command: &str) -> &'static [EffectKind] {
    registry()
        .get(command)
        .map(|command| command.effects)
        .unwrap_or(&[])
}

/// The declaration kind an optional client effect requires, or `None` for
/// effects that clients always support.
pub fn effect_kind(effect: &crate::protocol::Effect) -> Option<EffectKind> {
    match effect {
        Effect::VisitFile { .. } => Some(EffectKind::VisitFile),
        Effect::EditDocument { .. } => Some(EffectKind::EditDocument),
        Effect::Clipboard { .. } => Some(EffectKind::Clipboard),
        _ => None,
    }
}

/// Enforce that every optional effect a command emits was declared by it.
///
/// Debug builds panic on an undeclared emission; release builds replace it
/// with a readable warning. Effects added by delivery after this point are not
/// subject to the check.
pub fn enforce_effects(command_id: &str, effects: &mut [crate::protocol::Effect]) {
    let declared = required_effects(command_id);
    for effect in effects {
        let Some(kind) = effect_kind(effect) else {
            continue;
        };
        if declared.contains(&kind) {
            continue;
        }
        #[cfg(debug_assertions)]
        panic!(
            "undeclared optional effect {} emitted by {command_id}",
            kind.name()
        );
        #[cfg(not(debug_assertions))]
        {
            *effect = Effect::message(
                MessageLevel::Warn,
                format!(
                    "Internal error: {command_id} emitted an undeclared {} effect",
                    kind.name()
                ),
            );
        }
    }
}

impl Command {
    /// Whether shipped profiles must explicitly bind or unbind this command.
    pub fn requires_profile_binding(&self) -> bool {
        !matches!(self.scope, Scope::MenuAction)
            && !matches!(
                self.id,
                "rebase.todo-set-action" | "rebase.todo-move-before"
            )
    }

    /// Whether this command applies to a view scope, including command-specific exceptions.
    pub fn applies_to(&self, view_scope: &str) -> bool {
        self.scope.matches(view_scope) || (self.id == "open.commit" && view_scope == "log")
    }
}

/// The operation requested for a commit selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionOperation {
    /// Copy the selected commits' object IDs.
    CopyCommits,
    /// Pick one commit as the value of the caller's menu option.
    PickCommit,
    /// Create a fixup commit for one selected commit.
    FixupCommit,
    /// Run an instant autosquash for one selected commit.
    InstantAutosquash(crate::instant_rebase::InstantAutosquashKind),
    /// Use one selected commit as the exclusive base for an initial rebase.
    InitialRebase,
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
    /// Run a repository operation on a detached worker once this command returns.
    pub commit: Option<crate::git::RepositoryOperation>,
}

/// A prepared operation resumed exactly once after the user answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptAction {
    /// Stage the captured working tree before starting commit.
    Commit {
        fingerprint: RepositoryFingerprint,
        untracked: bool,
        operation: crate::git::CommitOperation,
    },
    /// Confirm rewriting a published HEAD without replaying menu arguments.
    PublishedCommit {
        fingerprint: RepositoryFingerprint,
        operation: crate::git::CommitOperation,
    },
    /// Offer tracked `--all` for Extend without adding untracked files.
    ExtendAll {
        fingerprint: RepositoryFingerprint,
        options: crate::git::CommitOptions,
    },
    /// Apply the captured resolution against the displayed patch identities.
    Discard {
        fingerprint: RepositoryFingerprint,
        resolution: dispatch::Resolution,
        tree: Tree,
    },
    /// Resolve a revision for the originating selection view.
    Revision,
    /// New branch flow: the answered starting point, then the name.
    BranchCreateStart,
    /// Second half of a create: create at this validated startpoint.
    BranchCreateTarget {
        /// The starting point answered at the first prompt.
        startpoint: String,
    },
    /// Spin the current branch's unpushed commits onto the answered branch.
    BranchSpinoff,
    /// Spin out the current branch's unpushed commits onto the answered branch.
    BranchSpinout,
    /// Rename the answered branch to a second answered name.
    BranchRename,
    /// Second half of a rename: move this validated source to the answered name.
    BranchRenameTarget {
        /// The branch name answered at the first prompt.
        source: String,
    },
    /// Reset the answered branch to a second answered revision.
    BranchReset,
    /// Second half of a reset: point this validated branch at the answered revision.
    BranchResetTarget {
        /// The branch name answered at the first prompt.
        source: String,
    },
    /// Delete the answered branch.
    BranchDelete,
    /// Set a description on the answered branch.
    BranchDescription,
    /// Second half of a description set: store it for this validated branch.
    BranchDescriptionText {
        /// The branch name answered at the first prompt.
        name: String,
    },
    /// Create a tag with the answered name.
    TagCreate,
    /// Create the named lightweight tag at the answered revision.
    TagCreateRevision {
        /// The tag name answered at the first prompt.
        name: String,
    },
    /// Delete the answered tag.
    TagDelete,
    /// Add a remote with the answered name.
    RemoteAdd,
    /// Second half of a remote add: point this validated name at the answered URL.
    RemoteAddUrl {
        /// The remote name answered at the first prompt.
        name: String,
    },
    /// Remove the answered remote.
    RemoteRemove,
    /// Remove the named remote only after explicit confirmation.
    RemoteRemoveConfirm {
        /// The remote name answered at the first prompt.
        name: String,
    },
    /// Select the remote whose fetch and push URLs should be configured.
    RemoteConfigure,
    /// Read the fetch URL for the selected remote.
    RemoteConfigureFetch {
        /// The remote name answered at the first prompt.
        name: String,
    },
    /// Read the push URL before applying both URL changes together.
    RemoteConfigurePush {
        /// The selected remote name.
        name: String,
        /// The fetch URL answered at the previous prompt.
        fetch: String,
    },
    /// Rename a remote: the answered name is the existing remote.
    RemoteRename,
    /// Second half of a remote rename: rename this remote to the answered name.
    RemoteRenameTarget {
        /// The existing remote name answered at the first prompt.
        source: String,
    },
    /// Prune stale tracking branches of the answered remote.
    RemotePrune,
    /// Reset HEAD and the index to the answered revision.
    ResetMixed,
    /// Move HEAD to the answered revision.
    ResetSoft,
    /// Reset HEAD, index and worktree to the answered revision.
    ResetHard,
    /// Second half of a hard reset: the revision accepted after the dirty-tree warning.
    ResetHardConfirm {
        /// The revision text shown when the confirmation was presented.
        label: String,
        /// The commit `label` resolved to when the confirmation was presented.
        commit: String,
        /// Repository content captured when the confirmation was presented.
        fingerprint: RepositoryFingerprint,
    },
    /// Reset HEAD and the index to the answered revision, keeping local changes.
    ResetKeep,
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
        for entry in status
            .staged
            .iter()
            .chain(&status.unstaged)
            .chain(&status.untracked)
        {
            let path = repo.root().join(&entry.path);
            let content = match std::fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    let mut bytes = vec![b'L'];
                    bytes.extend_from_slice(
                        std::fs::read_link(&path)?.as_os_str().as_encoded_bytes(),
                    );
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
            let path = text
                .trim()
                .strip_prefix("gitdir: ")
                .ok_or_else(|| crate::Error::new("Invalid Git directory file"))?;
            repo.root().join(path)
        } else {
            dot_git
        };
        let index = match std::fs::read(git_dir.join("index")) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            status,
            contents,
            index,
        })
    }
}

/// Apply a prepared action without replaying command or coordinate resolution.
pub fn resume_prompt(
    ctx: &mut CommandCtx<'_>,
    action: PromptAction,
    answer: Option<crate::protocol::PromptValue>,
) -> Result<CommandOutcome> {
    use crate::protocol::PromptValue;
    let Some(answer) = answer else {
        return Ok(CommandOutcome::default());
    };
    match action {
        PromptAction::ExtendAll {
            fingerprint,
            mut options,
        } => {
            if !matches!(answer, PromptValue::Bool(true)) {
                return Ok(CommandOutcome::default());
            }
            let repo = GitRepo::open(ctx.repo_path)?;
            if RepositoryFingerprint::capture(&repo)? != fingerprint {
                ctx.info("Repository changed while the prompt was open; try again");
                return Ok(CommandOutcome::default());
            }
            options.all = true;
            prepare_head_commit(ctx, &repo, crate::git::CommitOperation::Extend(options))
        }
        PromptAction::PublishedCommit {
            fingerprint,
            operation,
        } => {
            if !matches!(answer, PromptValue::Bool(true)) {
                return Ok(CommandOutcome::default());
            }
            let repo = GitRepo::open(ctx.repo_path)?;
            if RepositoryFingerprint::capture(&repo)? != fingerprint {
                ctx.info("Repository changed while the prompt was open; try again");
                return Ok(CommandOutcome::default());
            }
            ctx.info("Committing…");
            Ok(CommandOutcome {
                commit: Some(crate::git::RepositoryOperation::Commit(operation)),
                ..Default::default()
            })
        }
        PromptAction::Commit {
            fingerprint,
            untracked,
            operation,
        } => {
            if !matches!(answer, PromptValue::Bool(true)) {
                return Ok(CommandOutcome::default());
            }
            let repo = GitRepo::open(ctx.repo_path)?;
            if RepositoryFingerprint::capture(&repo)? != fingerprint {
                ctx.info("Repository changed while the prompt was open; try again");
                return Ok(CommandOutcome::default());
            }
            ctx.extend(run_resolution(
                ctx.repo_path,
                dispatch::Resolution::StageAllUnstaged,
            )?);
            if untracked {
                ctx.extend(run_resolution(
                    ctx.repo_path,
                    dispatch::Resolution::StageAllUntracked,
                )?);
            }
            ctx.info("Committing…");
            Ok(CommandOutcome {
                commit: Some(crate::git::RepositoryOperation::Commit(operation)),
                ..Default::default()
            })
        }
        PromptAction::Discard {
            fingerprint,
            resolution,
            tree,
        } => {
            if !matches!(answer, PromptValue::Bool(true)) {
                return Ok(CommandOutcome::default());
            }
            let repo = GitRepo::open(ctx.repo_path)?;
            if RepositoryFingerprint::capture(&repo)? != fingerprint {
                ctx.info("Repository changed while the prompt was open; nothing discarded");
                return Ok(CommandOutcome::default());
            }
            if let dispatch::Resolution::DiscardFile { path } = &resolution {
                let section = file_worktree_section(&tree, path);
                if !displayed_file_is_current(&repo, &tree, path, section)? {
                    ctx.info(errors::HUNK_CHANGED);
                    return Ok(CommandOutcome {
                        refresh: true,
                        ..Default::default()
                    });
                }
            }
            let effects = match resolution {
                dispatch::Resolution::DiscardSelection { start, end } => {
                    execute_displayed_selection(&repo, &tree, start, end, false, true)?
                }
                _ => dispatch::execute(&repo, &resolution, &[])?,
            };
            ctx.extend(effects);
            Ok(CommandOutcome {
                refresh: true,
                ..Default::default()
            })
        }
        PromptAction::Revision => {
            let PromptValue::Text(revision) = answer else {
                return Err(crate::Error::new("Expected a revision"));
            };
            let oid = match GitRepo::open(ctx.repo_path)
                .and_then(|repo| repo.resolve_commit(revision.trim()))
            {
                Ok(oid) if !revision.trim().is_empty() => oid,
                _ => {
                    ctx.info(format!("Invalid commit revision: {}", revision.trim()));
                    return Ok(CommandOutcome::default());
                }
            };
            Ok(CommandOutcome {
                selected_commits: Some(vec![oid]),
                ..Default::default()
            })
        }
        PromptAction::BranchCreateStart => {
            let PromptValue::Text(startpoint) = answer else {
                return Err(crate::Error::new("Expected a starting point"));
            };
            let startpoint = startpoint.trim().to_owned();
            if startpoint.is_empty() {
                ctx.info("A starting point is required");
                return Ok(CommandOutcome::default());
            }
            if startpoint.starts_with('-') {
                ctx.info("A starting point cannot begin with '-'");
                return Ok(CommandOutcome::default());
            }
            Ok(CommandOutcome {
                prompt: Some((
                    branch_name_prompt("Create branch", "", "New branch name"),
                    PromptAction::BranchCreateTarget { startpoint },
                )),
                ..Default::default()
            })
        }
        PromptAction::BranchCreateTarget { startpoint } => {
            let PromptValue::Text(name) = answer else {
                return Err(crate::Error::new("Expected a branch name"));
            };
            let name = name.trim().to_owned();
            branch_action(
                ctx,
                |repo| repo.create_branch_at(&name, &startpoint),
                format!("Created branch {name}"),
            )
        }
        PromptAction::BranchSpinoff => spin_answered(ctx, answer, SpinMode::Spinoff),
        PromptAction::BranchSpinout => spin_answered(ctx, answer, SpinMode::Spinout),
        PromptAction::BranchRename => {
            let PromptValue::Text(source) = answer else {
                return Err(crate::Error::new("Expected a branch name"));
            };
            let source = source.trim().to_owned();
            if source.is_empty() {
                ctx.info("A branch name is required");
                return Ok(CommandOutcome::default());
            }
            Ok(CommandOutcome {
                prompt: Some((
                    branch_name_prompt("Rename branch to", &source, "New branch name"),
                    PromptAction::BranchRenameTarget { source },
                )),
                ..Default::default()
            })
        }
        PromptAction::BranchRenameTarget { source } => {
            let PromptValue::Text(target) = answer else {
                return Err(crate::Error::new("Expected a branch name"));
            };
            let target = target.trim().to_owned();
            branch_action(
                ctx,
                |repo| repo.rename_branch(&source, &target),
                format!("Renamed branch {source} to {target}"),
            )
        }
        PromptAction::BranchReset => {
            let PromptValue::Text(source) = answer else {
                return Err(crate::Error::new("Expected a branch name"));
            };
            let source = source.trim().to_owned();
            Ok(CommandOutcome {
                prompt: Some((
                    branch_name_prompt("Reset to", "", "Target revision"),
                    PromptAction::BranchResetTarget { source },
                )),
                ..Default::default()
            })
        }
        PromptAction::BranchResetTarget { source } => {
            let PromptValue::Text(target) = answer else {
                return Err(crate::Error::new("Expected a revision"));
            };
            let target = target.trim().to_owned();
            let repo = GitRepo::open(ctx.repo_path)?;
            if repo.current_branch().as_deref() == Some(source.as_str()) {
                return reset_answered(ctx, PromptValue::Text(target), ResetMode::Hard);
            }
            branch_action(
                ctx,
                |repo| repo.reset_branch(&source, &target),
                format!("Reset branch {source} to {target}"),
            )
        }
        PromptAction::BranchDelete => {
            let PromptValue::Text(name) = answer else {
                return Err(crate::Error::new("Expected a branch name"));
            };
            let name = name.trim().to_owned();
            branch_action(
                ctx,
                |repo| repo.delete_branch(&name),
                format!("Deleted branch {name}"),
            )
        }
        PromptAction::BranchDescription => {
            let PromptValue::Text(name) = answer else {
                return Err(crate::Error::new("Expected a branch name"));
            };
            let name = name.trim().to_owned();
            if name.is_empty() {
                ctx.info("A branch name is required");
                return Ok(CommandOutcome::default());
            }
            Ok(CommandOutcome {
                prompt: Some((
                    branch_description_prompt(&name),
                    PromptAction::BranchDescriptionText { name },
                )),
                ..Default::default()
            })
        }
        PromptAction::BranchDescriptionText { name } => {
            let PromptValue::Text(description) = answer else {
                return Err(crate::Error::new("Expected a branch description"));
            };
            let description = description.trim().to_owned();
            branch_action(
                ctx,
                |repo| repo.set_branch_description(&name, &description),
                format!("Set description for {name}"),
            )
        }
        PromptAction::TagCreate => {
            let PromptValue::Text(name) = answer else {
                return Err(crate::Error::new("Expected a tag name"));
            };
            let name = name.trim().to_owned();
            Ok(CommandOutcome {
                prompt: Some((
                    branch_name_prompt("Tag revision", "HEAD", "Revision"),
                    PromptAction::TagCreateRevision { name },
                )),
                ..Default::default()
            })
        }
        PromptAction::TagCreateRevision { name } => {
            let PromptValue::Text(revision) = answer else {
                return Err(crate::Error::new("Expected a revision"));
            };
            branch_action(
                ctx,
                |repo| repo.create_lightweight_tag(&name, revision.trim()),
                format!("Created tag {name}"),
            )
        }
        PromptAction::TagDelete => {
            let PromptValue::Text(name) = answer else {
                return Err(crate::Error::new("Expected a tag name"));
            };
            let name = name.trim().to_owned();
            branch_action(
                ctx,
                |repo| repo.delete_tag(&name),
                format!("Deleted tag {name}"),
            )
        }
        PromptAction::RemoteAdd => {
            let PromptValue::Text(name) = answer else {
                return Err(crate::Error::new("Expected a remote name"));
            };
            let name = name.trim().to_owned();
            Ok(CommandOutcome {
                prompt: Some((
                    branch_name_prompt(&format!("URL for remote {name}"), "", "Remote URL"),
                    PromptAction::RemoteAddUrl { name },
                )),
                ..Default::default()
            })
        }
        PromptAction::RemoteAddUrl { name } => {
            let PromptValue::Text(url) = answer else {
                return Err(crate::Error::new("Expected a remote URL"));
            };
            let url = url.trim().to_owned();
            branch_action(
                ctx,
                |repo| repo.add_remote(&name, &url),
                format!("Added remote {name}"),
            )
        }
        PromptAction::RemoteRemove => {
            let PromptValue::Text(name) = answer else {
                return Err(crate::Error::new("Expected a remote name"));
            };
            let name = name.trim().to_owned();
            let mut prompt = head_commit_prompt(
                &format!("Remove remote '{name}'?"),
                Some("This permanently removes the remote configuration."),
            );
            prompt.default = Some(Value::Bool(false));
            prompt.yes_label = Some("Remove".into());
            prompt.no_label = Some("Cancel".into());
            Ok(CommandOutcome {
                prompt: Some((prompt, PromptAction::RemoteRemoveConfirm { name })),
                ..Default::default()
            })
        }
        PromptAction::RemoteRemoveConfirm { name } => {
            if !matches!(answer, PromptValue::Bool(true)) {
                return Ok(CommandOutcome::default());
            }
            branch_action(
                ctx,
                |repo| repo.remove_remote(&name),
                format!("Removed remote {name}"),
            )
        }
        PromptAction::RemoteConfigure => {
            let PromptValue::Text(name) = answer else {
                return Err(crate::Error::new("Expected a remote name"));
            };
            Ok(CommandOutcome {
                prompt: Some((
                    branch_name_prompt("Remote fetch URL", "", "Fetch URL"),
                    PromptAction::RemoteConfigureFetch {
                        name: name.trim().to_owned(),
                    },
                )),
                ..Default::default()
            })
        }
        PromptAction::RemoteConfigureFetch { name } => {
            let PromptValue::Text(fetch) = answer else {
                return Err(crate::Error::new("Expected a remote URL"));
            };
            Ok(CommandOutcome {
                prompt: Some((
                    branch_name_prompt("Remote push URL", "", "Push URL"),
                    PromptAction::RemoteConfigurePush {
                        name,
                        fetch: fetch.trim().to_owned(),
                    },
                )),
                ..Default::default()
            })
        }
        PromptAction::RemoteConfigurePush { name, fetch } => {
            let PromptValue::Text(push) = answer else {
                return Err(crate::Error::new("Expected a remote URL"));
            };
            branch_action(
                ctx,
                |repo| repo.remote_configure(&name, &fetch, push.trim()),
                format!("Configured remote {name}"),
            )
        }
        PromptAction::RemoteRename => {
            let PromptValue::Text(source) = answer else {
                return Err(crate::Error::new("Expected a remote name"));
            };
            let source = source.trim().to_owned();
            Ok(CommandOutcome {
                prompt: Some((
                    branch_name_prompt(
                        &format!("Rename remote {source} to"),
                        "",
                        "New remote name",
                    ),
                    PromptAction::RemoteRenameTarget { source },
                )),
                ..Default::default()
            })
        }
        PromptAction::RemoteRenameTarget { source } => {
            let PromptValue::Text(target) = answer else {
                return Err(crate::Error::new("Expected a remote name"));
            };
            let target = target.trim().to_owned();
            branch_action(
                ctx,
                |repo| repo.rename_remote(&source, &target),
                format!("Renamed remote {source} to {target}"),
            )
        }
        PromptAction::RemotePrune => {
            let PromptValue::Text(name) = answer else {
                return Err(crate::Error::new("Expected a remote name"));
            };
            let name = name.trim().to_owned();
            branch_action(
                ctx,
                |repo| repo.prune_remote(&name),
                format!("Pruned remote {name}"),
            )
        }
        PromptAction::ResetMixed => reset_answered(ctx, answer, ResetMode::Mixed),
        PromptAction::ResetSoft => reset_answered(ctx, answer, ResetMode::Soft),
        PromptAction::ResetHard => reset_answered(ctx, answer, ResetMode::Hard),
        PromptAction::ResetKeep => reset_answered(ctx, answer, ResetMode::Keep),
        PromptAction::ResetHardConfirm {
            label,
            commit,
            fingerprint,
        } => {
            let PromptValue::Text(answer) = answer else {
                return Err(crate::Error::new("Expected a confirmation"));
            };
            if answer.trim() != "yes" {
                ctx.info(format!("Hard reset to {label} cancelled"));
                return Ok(CommandOutcome::default());
            }
            let repo = GitRepo::open(ctx.repo_path)?;
            if RepositoryFingerprint::capture(&repo)? != fingerprint {
                ctx.info("Repository changed while the prompt was open; try again");
                return Ok(CommandOutcome::default());
            }
            branch_action(
                ctx,
                |repo| repo.reset_hard(&commit),
                format!("Reset to {label} (hard)"),
            )
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
            command.applies_to(view_scope)
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
    use super::{registry, Availability};

    #[test]
    fn availability_and_scope_are_pure_registry_metadata() {
        let commands = registry();
        assert!(matches!(
            commands.availability("status.reverse"),
            Some(Availability::Planned(_))
        ));
        assert_eq!(
            commands.availability("common.confirm"),
            Some(Availability::Implemented)
        );
        assert!(commands.is_runnable_in("common.confirm", "commit"));
        assert!(commands.is_runnable_in("common.confirm", "status"));
        assert!(!commands.is_runnable_in("status.apply", "commit"));
        assert!(!commands.is_runnable_in("status.reverse", "status"));
        assert!(commands.is_runnable_in("open.commit", "log"));
        assert!(!commands.is_runnable_in("status.stage", "log"));
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
    command! {
        id: id,
        scope: scope,
        doc: doc,
        status: CommandStatus::Planned { roadmap_id },
        run: planned_run,
        effects: &[],
    }
}

/// An implemented command that opens a menu from the status view.
const fn menu(id: &'static str, doc: &'static str, run: RunFn) -> Command {
    command! {
        id: id,
        scope: Scope::Status,
        doc: doc,
        status: CommandStatus::Implemented,
        run: run,
        effects: &[],
    }
}

/// Handlers never call this; `view/invoke` answers `Planned` before it runs.
fn planned_run(_ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    Ok(CommandOutcome::default())
}

fn build() -> Registry {
    let mut commands = vec![
        command! {
            id: "common.refresh",
            scope: Scope::Common,
            doc: "Refresh the view",
            status: CommandStatus::Implemented,
            run: refresh,
            effects: &[],
        },
        command! {
            id: "common.toggle-section",
            scope: Scope::Common,
            doc: "Fold or unfold the section at point",
            status: CommandStatus::Implemented,
            run: toggle_section,
            effects: &[],
        },
        command! {
            id: "common.show-section",
            scope: Scope::Common,
            doc: "Unfold the section at point",
            status: CommandStatus::Implemented,
            run: show_section,
            effects: &[],
        },
        command! {
            id: "common.hide-section",
            scope: Scope::Common,
            doc: "Fold the section at point",
            status: CommandStatus::Implemented,
            run: hide_section,
            effects: &[],
        },
        command! {
            id: "common.show-all",
            scope: Scope::Common,
            doc: "Unfold every section",
            status: CommandStatus::Implemented,
            run: show_all,
            effects: &[],
        },
        command! {
            id: "common.hide-all",
            scope: Scope::Common,
            doc: "Fold every section",
            status: CommandStatus::Implemented,
            run: hide_all,
            effects: &[],
        },
        command! {
            id: "common.more-detail",
            scope: Scope::Common,
            doc: "Show one more level of detail",
            status: CommandStatus::Implemented,
            run: more_detail,
            effects: &[],
        },
        command! {
            id: "common.less-detail",
            scope: Scope::Common,
            doc: "Show one less level of detail",
            status: CommandStatus::Implemented,
            run: less_detail,
            effects: &[],
        },
        command! {
            id: "common.detail-level-1",
            scope: Scope::Common,
            doc: "Show section headers only",
            status: CommandStatus::Implemented,
            run: detail_level_1,
            effects: &[],
        },
        command! {
            id: "common.detail-level-2",
            scope: Scope::Common,
            doc: "Show section and file headers",
            status: CommandStatus::Implemented,
            run: detail_level_2,
            effects: &[],
        },
        command! {
            id: "common.detail-level-3",
            scope: Scope::Common,
            doc: "Show hunk headers",
            status: CommandStatus::Implemented,
            run: detail_level_3,
            effects: &[],
        },
        command! {
            id: "common.detail-level-4",
            scope: Scope::Common,
            doc: "Show hunk bodies",
            status: CommandStatus::Implemented,
            run: detail_level_4,
            effects: &[],
        },
        command! {
            id: "common.next-section",
            scope: Scope::Common,
            doc: "Move to the next section",
            status: CommandStatus::Implemented,
            run: next_section,
            effects: &[],
        },
        command! {
            id: "common.prev-section",
            scope: Scope::Common,
            doc: "Move to the previous section",
            status: CommandStatus::Implemented,
            run: prev_section,
            effects: &[],
        },
        command! {
            id: "common.jump",
            scope: Scope::Common,
            doc: "Move to the next or previous visible section, file or hunk",
            status: CommandStatus::Implemented,
            run: jump,
            effects: &[],
        },
        command! {
            id: "status.stage",
            scope: Scope::Status,
            doc: "Stage the change at point",
            status: CommandStatus::Implemented,
            run: stage,
            effects: &[],
        },
        command! { id: "branch.create", scope: Scope::MenuAction, doc: "Create a branch from a starting point", status: CommandStatus::Implemented, run: branch_create, effects: &[] },
        command! { id: "branch.rename", scope: Scope::MenuAction, doc: "Rename a branch", status: CommandStatus::Implemented, run: branch_rename, effects: &[] },
        command! { id: "branch.delete", scope: Scope::MenuAction, doc: "Delete a branch", status: CommandStatus::Implemented, run: branch_delete, destructive: true, effects: &[] },
        command! { id: "branch.reset", scope: Scope::MenuAction, doc: "Reset a branch to a revision", status: CommandStatus::Implemented, run: branch_reset, effects: &[] },
        command! { id: "branch.configure-description", scope: Scope::MenuAction, doc: "Set the branch description", status: CommandStatus::Implemented, run: branch_configure_description, effects: &[] },
        command! { id: "tag.create", scope: Scope::MenuAction, doc: "Create a lightweight tag at a revision", status: CommandStatus::Implemented, run: tag_create, effects: &[] },
        command! { id: "tag.delete", scope: Scope::MenuAction, doc: "Delete a tag", status: CommandStatus::Implemented, run: tag_delete, destructive: true, effects: &[] },
        command! { id: "remote.add", scope: Scope::MenuAction, doc: "Add a remote", status: CommandStatus::Implemented, run: remote_add, effects: &[] },
        command! { id: "remote.remove", scope: Scope::MenuAction, doc: "Remove a remote", status: CommandStatus::Implemented, run: remote_remove, effects: &[] },
        command! { id: "remote.rename", scope: Scope::MenuAction, doc: "Rename a remote", status: CommandStatus::Implemented, run: remote_rename, effects: &[] },
        command! { id: "remote.configure", scope: Scope::MenuAction, doc: "Configure remote URLs", status: CommandStatus::Implemented, run: remote_configure, effects: &[] },
        command! { id: "remote.prune", scope: Scope::MenuAction, doc: "Prune stale tracking branches of a remote", status: CommandStatus::Implemented, run: remote_prune, effects: &[] },
        command! { id: "reset.mixed", scope: Scope::MenuAction, doc: "Reset HEAD and the index to a revision", status: CommandStatus::Implemented, run: reset_mixed, effects: &[] },
        command! { id: "reset.soft", scope: Scope::MenuAction, doc: "Move HEAD to a revision", status: CommandStatus::Implemented, run: reset_soft, effects: &[] },
        command! { id: "reset.hard", scope: Scope::MenuAction, doc: "Reset HEAD, index and worktree to a revision", status: CommandStatus::Implemented, run: reset_hard, destructive: true, effects: &[] },
        command! { id: "reset.keep", scope: Scope::MenuAction, doc: "Reset HEAD and the index to a revision, keeping local changes", status: CommandStatus::Implemented, run: reset_keep, effects: &[] },
        command! { id: "reset.branch", scope: Scope::MenuAction, doc: "Reset a branch to a revision", status: CommandStatus::Implemented, run: branch_reset, effects: &[] },
        command! { id: "branch.spinoff", scope: Scope::MenuAction, doc: "Create a branch from the current branch's unpushed commits and check it out", status: CommandStatus::Implemented, run: branch_spinoff, effects: &[] },
        command! { id: "branch.spinout", scope: Scope::MenuAction, doc: "Create a branch from the current branch's unpushed commits without switching", status: CommandStatus::Implemented, run: branch_spinout, effects: &[] },
        command! {
            id: "status.unstage",
            scope: Scope::Status,
            doc: "Unstage the change at point",
            status: CommandStatus::Implemented,
            run: unstage,
            effects: &[],
        },
        command! {
            id: "status.stage-all",
            scope: Scope::Status,
            doc: "Stage all unstaged changes",
            status: CommandStatus::Implemented,
            run: stage_all,
            effects: &[],
        },
        command! {
            id: "status.unstage-all",
            scope: Scope::Status,
            doc: "Unstage all staged changes",
            status: CommandStatus::Implemented,
            run: unstage_all,
            effects: &[],
        },
    ];

    commands.extend([
        command! {
            id: "common.cancel",
            scope: Scope::Common,
            doc: "Cancel the current operation",
            status: CommandStatus::Implemented,
            run: cancel,
            effects: &[],
            target: CommandTarget::View,
        },
        command! {
            id: "common.confirm",
            scope: Scope::Common,
            doc: "Confirm the current selection",
            status: CommandStatus::Implemented,
            run: confirm,
            effects: &[],
        },
        command! {
            id: "common.quit",
            scope: Scope::Common,
            doc: "Close all views",
            status: CommandStatus::Implemented,
            run: quit,
            effects: &[],
            target: CommandTarget::View,
        },
        command! {
            id: "common.cycle-visibility",
            scope: Scope::Common,
            doc: "Cycle the visibility of every section",
            status: CommandStatus::Implemented,
            run: cycle_visibility,
            effects: &[],
        },
        command! {
            id: "common.next-sibling",
            scope: Scope::Common,
            doc: "Move to the next sibling",
            status: CommandStatus::Implemented,
            run: next_sibling,
            effects: &[],
        },
        command! {
            id: "common.prev-sibling",
            scope: Scope::Common,
            doc: "Move to the previous sibling",
            status: CommandStatus::Implemented,
            run: prev_sibling,
            effects: &[],
        },
        command! {
            id: "common.parent-section",
            scope: Scope::Common,
            doc: "Move to the parent section",
            status: CommandStatus::Implemented,
            run: parent_section,
            effects: &[],
        },
        command! {
            id: "common.toggle-text-mode",
            scope: Scope::Common,
            doc: "Toggle text mode",
            status: CommandStatus::Implemented,
            run: toggle_text_mode,
            effects: &[],
        },
        command! {
            id: "common.copy-value",
            scope: Scope::Common,
            doc: "Copy the value at point",
            status: CommandStatus::Implemented,
            run: copy_value,
            effects: &[EffectKind::Clipboard],
        },
        command! {
            id: "common.visit",
            scope: Scope::Common,
            doc: "Visit the thing at point",
            status: CommandStatus::Implemented,
            run: visit,
            effects: &[EffectKind::VisitFile],
        },
        command! {
            id: "status.discard",
            scope: Scope::Status,
            doc: "Discard the change at point",
            status: CommandStatus::Implemented,
            run: discard,
            destructive: true,
            effects: &[],
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
        command! {
            id: "open.dispatch",
            scope: Scope::Common,
            doc: "Show the dispatch menu",
            status: CommandStatus::Implemented,
            run: open_dispatch,
            effects: &[],
            target: CommandTarget::View,
        },
        command! {
            id: "open.commit",
            scope: Scope::Status,
            doc: "Open the commit menu",
            status: CommandStatus::Implemented,
            run: open_commit,
            effects: &[],
        },
        command! {
            id: "commit.create",
            scope: Scope::MenuAction,
            doc: "Commit",
            status: CommandStatus::Implemented,
            run: commit_create,
            effects: &[EffectKind::EditDocument],
        },
        command! { id: "rebase.continue", scope: Scope::MenuAction, doc: "Continue rebase", status: CommandStatus::Implemented, run: rebase_continue, effects: &[] },
        command! { id: "rebase.skip", scope: Scope::MenuAction, doc: "Skip rebase commit", status: CommandStatus::Implemented, run: rebase_skip, effects: &[] },
        command! { id: "rebase.abort", scope: Scope::MenuAction, doc: "Abort rebase", status: CommandStatus::Implemented, run: rebase_abort, effects: &[] },
        command! { id: "rebase.edit-todo", scope: Scope::MenuAction, doc: "Edit paused rebase todo", status: CommandStatus::Implemented, run: rebase_edit_todo, effects: &[] },
        command! { id: "rebase.interactive", scope: Scope::MenuAction, doc: "Select base and edit initial rebase plan", status: CommandStatus::Implemented, run: rebase_initial, effects: &[] },
        command! { id: "rebase.todo-set-action", scope: Scope::RebaseTodo, doc: "Change todo action", status: CommandStatus::Implemented, run: rebase_todo_set_action, effects: &[] },
        command! { id: "rebase.todo-move-before", scope: Scope::RebaseTodo, doc: "Move todo commit", status: CommandStatus::Implemented, run: rebase_todo_move_before, effects: &[] },
        command! { id: "rebase.todo-submit", scope: Scope::RebaseTodo, doc: "Submit rebase todo", status: CommandStatus::Implemented, run: rebase_todo_submit, effects: &[] },
        command! { id: "rebase.todo-pick", scope: Scope::RebaseTodo, doc: "Mark todo commit pick", status: CommandStatus::Implemented, run: rebase_todo_pick, effects: &[] },
        command! { id: "rebase.todo-reword", scope: Scope::RebaseTodo, doc: "Mark todo commit reword", status: CommandStatus::Implemented, run: rebase_todo_reword, effects: &[] },
        command! { id: "rebase.todo-edit", scope: Scope::RebaseTodo, doc: "Mark todo commit edit", status: CommandStatus::Implemented, run: rebase_todo_edit, effects: &[] },
        command! { id: "rebase.todo-squash", scope: Scope::RebaseTodo, doc: "Mark todo commit squash", status: CommandStatus::Implemented, run: rebase_todo_squash, effects: &[] },
        command! { id: "rebase.todo-fixup", scope: Scope::RebaseTodo, doc: "Mark todo commit fixup", status: CommandStatus::Implemented, run: rebase_todo_fixup, effects: &[] },
        command! { id: "rebase.todo-drop", scope: Scope::RebaseTodo, doc: "Mark todo commit drop", status: CommandStatus::Implemented, run: rebase_todo_drop, effects: &[] },
        command! { id: "rebase.todo-up", scope: Scope::RebaseTodo, doc: "Move todo commit up", status: CommandStatus::Implemented, run: rebase_todo_up, effects: &[] },
        command! { id: "rebase.todo-down", scope: Scope::RebaseTodo, doc: "Move todo commit down", status: CommandStatus::Implemented, run: rebase_todo_down, effects: &[] },
        command! {
            id: "commit.amend",
            scope: Scope::MenuAction,
            doc: "Amend HEAD",
            status: CommandStatus::Implemented,
            run: commit_amend,
            effects: &[EffectKind::EditDocument],
        },
        command! {
            id: "commit.extend",
            scope: Scope::MenuAction,
            doc: "Extend HEAD without editing its message",
            status: CommandStatus::Implemented,
            run: commit_extend,
            destructive: true,
            effects: &[EffectKind::EditDocument],
        },
        command! {
            id: "commit.reword",
            scope: Scope::MenuAction,
            doc: "Reword HEAD",
            status: CommandStatus::Implemented,
            run: commit_reword,
            effects: &[EffectKind::EditDocument],
        },
        command! {
            id: "commit.fixup",
            scope: Scope::MenuAction,
            doc: "Create a fixup commit",
            status: CommandStatus::Implemented,
            run: commit_fixup,
            effects: &[],
        },
        command! {
            id: "commit.instant-fixup",
            scope: Scope::MenuAction,
            doc: "Instantly fix up a selected commit",
            status: CommandStatus::Implemented,
            run: commit_instant_fixup,
            effects: &[],
        },
        command! {
            id: "commit.instant-squash",
            scope: Scope::MenuAction,
            doc: "Instantly squash into a selected commit",
            status: CommandStatus::Implemented,
            run: commit_instant_squash,
            effects: &[],
        },
        menu("open.branch", "Open the branch menu", open_branch),
        menu("open.log", "Open the log menu", open_log),
        command! {
            id: "log.current",
            scope: Scope::MenuAction,
            doc: "Show the current log",
            status: CommandStatus::Implemented,
            run: open_current_log,
            effects: &[],
        },
        command! { id: "log.local-branches", scope: Scope::MenuAction, doc: "Show commits reachable from local branches", status: CommandStatus::Implemented, run: open_local_branch_log, effects: &[] },
        command! { id: "log.all-branches", scope: Scope::MenuAction, doc: "Show commits reachable from branches", status: CommandStatus::Implemented, run: open_all_branch_log, effects: &[] },
        command! { id: "log.all-references", scope: Scope::MenuAction, doc: "Show commits reachable from all refs", status: CommandStatus::Implemented, run: open_all_reference_log, effects: &[] },
        command! {
            id: "log.load-more",
            scope: Scope::Log,
            doc: "Load more commits",
            status: CommandStatus::Implemented,
            run: log_load_more,
            effects: &[],
        },
        command! {
            id: "log.select-and-copy",
            scope: Scope::MenuAction,
            doc: "Select commits and copy their object IDs",
            status: CommandStatus::Implemented,
            run: select_and_copy,
            effects: &[EffectKind::Clipboard],
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
        command! {
            id: "open.process-log",
            scope: Scope::Status,
            doc: "Open the process log",
            status: CommandStatus::Implemented,
            run: open_process_log,
            effects: &[],
        },
        command! {
            id: "process-log.clear",
            scope: Scope::ProcessLog,
            doc: "Clear the process log",
            status: CommandStatus::Implemented,
            run: clear_process_log,
            effects: &[],
        },
        command! {
            id: "select.toggle-mark",
            scope: Scope::Select,
            doc: "Toggle the mark on the commit at point",
            status: CommandStatus::Implemented,
            run: toggle_commit_mark,
            effects: &[],
        },
        command! {
            id: "select.enter-rev",
            scope: Scope::Select,
            doc: "Enter a revision by hand",
            status: CommandStatus::Implemented,
            run: enter_revision,
            effects: &[],
        },
        command! {
            id: "menu.save-args",
            scope: Scope::Menu,
            doc: "Save the menu arguments",
            status: CommandStatus::Implemented,
            run: save_menu_args,
            effects: &[],
            target: CommandTarget::View,
        },
        command! {
            id: "menu.reset-args",
            scope: Scope::Menu,
            doc: "Reset the menu arguments",
            status: CommandStatus::Implemented,
            run: reset_menu_args,
            effects: &[],
            target: CommandTarget::View,
        },
        planned(
            "commit-message.history-prev",
            Scope::CommitMessage,
            "Recall the previous commit message",
            "R-EDIT-HISTORY",
        ),
        planned(
            "commit-message.history-next",
            Scope::CommitMessage,
            "Recall the next commit message",
            "R-EDIT-HISTORY",
        ),
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
                if let Some(registered) = commands.last_mut() {
                    registered.destructive = matches!(
                        *command,
                        "reset.hard" | "branch.delete" | "tag.delete" | "worktree.delete"
                    ) || command.starts_with("push.");
                }
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

fn rebase_continue(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    rebase_action(ctx, |repo| repo.rebase_continue())
}

fn rebase_skip(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    rebase_action(ctx, |repo| repo.rebase_skip())
}

fn rebase_abort(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    rebase_action(ctx, |repo| repo.rebase_abort())
}

fn rebase_edit_todo(_ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    Ok(CommandOutcome {
        open_view: Some((ViewKind::RebaseTodo, Placement::Main)),
        ..Default::default()
    })
}

fn rebase_initial(_ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    Ok(CommandOutcome {
        open_selection: Some(SelectionRequest {
            options: crate::git::GitLogOptions::default(),
            settings: CommitSelection {
                purpose: "initial rebase base".to_string(),
                prompt: "Select the exclusive rebase base".to_string(),
                multi: false,
                allow_free_text: false,
            },
            operation: SelectionOperation::InitialRebase,
        }),
        ..Default::default()
    })
}

fn rebase_todo_set_action(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    let oid = ctx
        .cursor
        .and_then(|cursor| {
            ctx.view
                .command_source_tree()
                .unwrap_or_else(|| ctx.view.tree())
                .navigation_node(cursor.line)
        })
        .and_then(|node| node.commit.clone());
    let action = ctx
        .args
        .and_then(|args| args.get("action"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let action = match action {
        "pick" => crate::rebase_todo::TodoAction::Pick,
        "reword" => crate::rebase_todo::TodoAction::Reword,
        "edit" => crate::rebase_todo::TodoAction::Edit,
        "squash" => crate::rebase_todo::TodoAction::Squash,
        "fixup" => crate::rebase_todo::TodoAction::Fixup,
        "drop" => crate::rebase_todo::TodoAction::Drop,
        _ => {
            ctx.effects.push(Effect::message(
                MessageLevel::Warn,
                "Invalid rebase todo action",
            ));
            return Ok(CommandOutcome::default());
        }
    };
    let Some(oid) = oid else {
        ctx.effects.push(Effect::message(
            MessageLevel::Warn,
            "Select a todo commit first",
        ));
        return Ok(CommandOutcome::default());
    };
    match ctx
        .view
        .rebase_todo_mut()
        .ok_or_else(|| crate::Error::new("rebase todo view unavailable"))?
        .set_action(&oid, action)
    {
        Ok(()) => Ok(CommandOutcome {
            refresh: true,
            ..Default::default()
        }),
        Err(error) => {
            ctx.effects
                .push(Effect::message(MessageLevel::Warn, error.to_string()));
            Ok(CommandOutcome::default())
        }
    }
}

fn rebase_todo_move_before(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    let oid = ctx
        .cursor
        .and_then(|cursor| {
            ctx.view
                .command_source_tree()
                .unwrap_or_else(|| ctx.view.tree())
                .navigation_node(cursor.line)
        })
        .and_then(|node| node.commit.clone());
    let before = ctx
        .args
        .and_then(|args| args.get("before"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let Some(oid) = oid else {
        ctx.effects.push(Effect::message(
            MessageLevel::Warn,
            "Select a todo commit first",
        ));
        return Ok(CommandOutcome::default());
    };
    match ctx
        .view
        .rebase_todo_mut()
        .ok_or_else(|| crate::Error::new("rebase todo view unavailable"))?
        .move_before(&oid, before.as_deref())
    {
        Ok(()) => Ok(CommandOutcome {
            refresh: true,
            ..Default::default()
        }),
        Err(error) => {
            ctx.effects
                .push(Effect::message(MessageLevel::Warn, error.to_string()));
            Ok(CommandOutcome::default())
        }
    }
}

fn rebase_todo_submit(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    match ctx
        .view
        .rebase_todo_mut()
        .ok_or_else(|| crate::Error::new("rebase todo view unavailable"))?
        .submit()
    {
        Ok(crate::rebase_todo_view::SubmitOutcome::Saved) => Ok(CommandOutcome {
            pop: true,
            ..Default::default()
        }),
        Ok(crate::rebase_todo_view::SubmitOutcome::Started(Ok(()))) => Ok(CommandOutcome {
            pop: true,
            refresh: true,
            ..Default::default()
        }),
        Ok(crate::rebase_todo_view::SubmitOutcome::Started(Err(error))) => {
            ctx.effects
                .push(Effect::message(MessageLevel::Warn, error.to_string()));
            Ok(CommandOutcome {
                pop: true,
                refresh: true,
                ..Default::default()
            })
        }
        Err(error) => {
            ctx.effects
                .push(Effect::message(MessageLevel::Warn, error.to_string()));
            Ok(CommandOutcome {
                refresh: true,
                ..Default::default()
            })
        }
    }
}

fn rebase_todo_pick(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    rebase_todo_action_key(ctx, crate::rebase_todo::TodoAction::Pick)
}
fn rebase_todo_reword(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    rebase_todo_action_key(ctx, crate::rebase_todo::TodoAction::Reword)
}
fn rebase_todo_edit(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    rebase_todo_action_key(ctx, crate::rebase_todo::TodoAction::Edit)
}
fn rebase_todo_squash(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    rebase_todo_action_key(ctx, crate::rebase_todo::TodoAction::Squash)
}
fn rebase_todo_fixup(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    rebase_todo_action_key(ctx, crate::rebase_todo::TodoAction::Fixup)
}
fn rebase_todo_drop(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    rebase_todo_action_key(ctx, crate::rebase_todo::TodoAction::Drop)
}

fn rebase_todo_action_key(
    ctx: &mut CommandCtx<'_>,
    action: crate::rebase_todo::TodoAction,
) -> Result<CommandOutcome> {
    let oid = ctx
        .cursor
        .and_then(|cursor| {
            ctx.view
                .command_source_tree()
                .unwrap_or_else(|| ctx.view.tree())
                .navigation_node(cursor.line)
        })
        .and_then(|node| node.commit.clone());
    let Some(oid) = oid else {
        ctx.effects.push(Effect::message(
            MessageLevel::Warn,
            "Select a todo commit first",
        ));
        return Ok(CommandOutcome::default());
    };
    match ctx
        .view
        .rebase_todo_mut()
        .ok_or_else(|| crate::Error::new("rebase todo view unavailable"))?
        .set_action(&oid, action)
    {
        Ok(()) => Ok(CommandOutcome {
            refresh: true,
            ..Default::default()
        }),
        Err(error) => {
            ctx.effects
                .push(Effect::message(MessageLevel::Warn, error.to_string()));
            Ok(CommandOutcome::default())
        }
    }
}

fn rebase_todo_up(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    rebase_todo_move_key(ctx, true)
}
fn rebase_todo_down(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    rebase_todo_move_key(ctx, false)
}
fn rebase_todo_move_key(ctx: &mut CommandCtx<'_>, up: bool) -> Result<CommandOutcome> {
    let tree = ctx
        .view
        .command_source_tree()
        .unwrap_or_else(|| ctx.view.tree());
    let selected = ctx
        .cursor
        .and_then(|cursor| tree.navigation_node(cursor.line))
        .and_then(|node| node.commit.clone());
    let Some(selected) = selected else {
        ctx.effects.push(Effect::message(
            MessageLevel::Warn,
            "Select a todo commit first",
        ));
        return Ok(CommandOutcome::default());
    };
    let view = ctx
        .view
        .rebase_todo_mut()
        .ok_or_else(|| crate::Error::new("rebase todo view unavailable"))?;
    match view.move_relative(&selected, up) {
        Ok(()) => Ok(CommandOutcome {
            refresh: true,
            ..Default::default()
        }),
        Err(error) => {
            ctx.effects
                .push(Effect::message(MessageLevel::Warn, error.to_string()));
            Ok(CommandOutcome::default())
        }
    }
}

fn rebase_action(
    ctx: &mut CommandCtx<'_>,
    action: impl FnOnce(&GitRepo) -> Result<()>,
) -> Result<CommandOutcome> {
    let repo = GitRepo::open(ctx.repo_path)?;
    if let Err(err) = action(&repo) {
        ctx.effects.push(Effect::Message {
            level: MessageLevel::Warn,
            text: err.to_string(),
            hint: None,
            detail: None,
        });
    }
    Ok(CommandOutcome {
        refresh: true,
        ..Default::default()
    })
}

/// Run a branch operation, reporting Git failures as a readable message.
fn branch_action(
    ctx: &mut CommandCtx<'_>,
    action: impl FnOnce(&GitRepo) -> Result<()>,
    success: String,
) -> Result<CommandOutcome> {
    let repo = GitRepo::open(ctx.repo_path)?;
    match action(&repo) {
        Ok(()) => ctx.info(success),
        Err(err) => ctx.effects.push(Effect::Message {
            level: MessageLevel::Warn,
            text: err.to_string(),
            hint: None,
            detail: None,
        }),
    }
    Ok(CommandOutcome {
        refresh: true,
        ..Default::default()
    })
}

/// Which `git reset` mode an answered revision applies with.
enum ResetMode {
    /// Reset HEAD and the index.
    Mixed,
    /// Move HEAD only.
    Soft,
    /// Reset HEAD, index and worktree.
    Hard,
    /// Reset HEAD and the index, keeping non-conflicting local changes.
    Keep,
}

/// Reset to the revision answered at the prompt in `mode`, warning instead of
/// acting when the answer is empty.
///
/// Hard mode asks for a second confirmation while the worktree is dirty, so a
/// reset that discards uncommitted work is never silent.
fn reset_answered(
    ctx: &mut CommandCtx<'_>,
    answer: PromptValue,
    mode: ResetMode,
) -> Result<CommandOutcome> {
    let Some(revision) = reset_revision(answer) else {
        ctx.effects.push(Effect::message(
            MessageLevel::Warn,
            "Enter a revision to reset to",
        ));
        return Ok(CommandOutcome::default());
    };
    if matches!(mode, ResetMode::Hard) {
        let repo = GitRepo::open(ctx.repo_path)?;
        match repo.is_dirty() {
            Ok(true) => {
                let commit = match repo.resolve_commit(&revision) {
                    Ok(commit) => commit,
                    Err(err) => {
                        ctx.effects
                            .push(Effect::message(MessageLevel::Warn, err.to_string()));
                        return Ok(CommandOutcome::default());
                    }
                };
                let fingerprint = RepositoryFingerprint::capture(&repo)?;
                return Ok(CommandOutcome {
                    prompt: Some((
                        hard_reset_confirm_prompt(&revision),
                        PromptAction::ResetHardConfirm {
                            label: revision,
                            commit,
                            fingerprint,
                        },
                    )),
                    ..Default::default()
                });
            }
            Ok(false) => {}
            Err(err) => {
                ctx.effects
                    .push(Effect::message(MessageLevel::Warn, err.to_string()));
                return Ok(CommandOutcome::default());
            }
        }
    }
    let (success, action): (String, fn(&GitRepo, &str) -> Result<()>) = match mode {
        ResetMode::Mixed => (format!("Reset to {revision} (mixed)"), GitRepo::reset_mixed),
        ResetMode::Soft => (format!("Reset to {revision} (soft)"), GitRepo::reset_soft),
        ResetMode::Hard => (format!("Reset to {revision} (hard)"), GitRepo::reset_hard),
        ResetMode::Keep => (format!("Reset to {revision} (keep)"), GitRepo::reset_keep),
    };
    branch_action(ctx, |repo| action(repo, &revision), success)
}

/// The trimmed revision text of a prompt answer, unless it is empty or not text.
fn reset_revision(answer: PromptValue) -> Option<String> {
    let PromptValue::Text(revision) = answer else {
        return None;
    };
    let revision = revision.trim().to_owned();
    (!revision.is_empty()).then_some(revision)
}

/// Build the single-line prompt asking for the revision to reset to.
fn reset_revision_prompt() -> ViewPromptParams {
    branch_name_prompt("Reset to", "", "Revision")
}

/// Build the confirmation prompt shown before a hard reset discards changes.
fn hard_reset_confirm_prompt(revision: &str) -> ViewPromptParams {
    branch_name_prompt(
        &format!(
            "Working tree has changes; hard reset to {revision} discards them. Type yes to proceed"
        ),
        "",
        "yes",
    )
}

/// Build a single-line text prompt for a branch name.
fn branch_name_prompt(title: &str, default: &str, placeholder: &str) -> ViewPromptParams {
    ViewPromptParams {
        prompt_id: String::new(),
        kind: PromptKind::Text,
        title: title.to_string(),
        detail: None,
        yes_label: None,
        no_label: None,
        destructive: None,
        default: (!default.is_empty()).then(|| Value::String(default.to_string())),
        placeholder: Some(placeholder.to_string()),
        secret: None,
        items: None,
        allow_custom: None,
        preselected: None,
        min: None,
        max: None,
    }
}

/// Build a single-line text prompt for a branch description.
fn branch_description_prompt(name: &str) -> ViewPromptParams {
    ViewPromptParams {
        prompt_id: String::new(),
        kind: PromptKind::Text,
        title: format!("Description for branch {name}"),
        detail: None,
        yes_label: None,
        no_label: None,
        destructive: None,
        default: None,
        placeholder: Some("Branch description".to_string()),
        secret: None,
        items: None,
        allow_custom: None,
        preselected: None,
        min: None,
        max: None,
    }
}

/// Prompt for the starting point of a new branch.
fn branch_create(_ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    Ok(CommandOutcome {
        prompt: Some((
            branch_name_prompt("Starting point", "", "Branch or revision"),
            PromptAction::BranchCreateStart,
        )),
        ..Default::default()
    })
}

/// Prompt for the name of a branch spun off from the current branch.
fn branch_spinoff(_ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    Ok(CommandOutcome {
        prompt: Some((
            branch_name_prompt("Spin off", "", "New branch name"),
            PromptAction::BranchSpinoff,
        )),
        ..Default::default()
    })
}

/// Prompt for the name of a branch spun out of the current branch.
fn branch_spinout(_ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    Ok(CommandOutcome {
        prompt: Some((
            branch_name_prompt("Spin out", "", "New branch name"),
            PromptAction::BranchSpinout,
        )),
        ..Default::default()
    })
}

/// Which spin variant an answered branch name applies with.
enum SpinMode {
    /// Check out the new branch before moving the source off its commits.
    Spinoff,
    /// Stay on the source branch and mixed-reset it to the merge base.
    Spinout,
}

/// Create a branch from the answered name and spin the current branch's
/// unpushed commits onto it, warning instead of acting on an empty answer.
fn spin_answered(
    ctx: &mut CommandCtx<'_>,
    answer: PromptValue,
    mode: SpinMode,
) -> Result<CommandOutcome> {
    let PromptValue::Text(name) = answer else {
        return Err(crate::Error::new("Expected a branch name"));
    };
    let name = name.trim().to_owned();
    if name.is_empty() {
        ctx.info("A branch name is required");
        return Ok(CommandOutcome::default());
    }
    let repo = match GitRepo::open(ctx.repo_path) {
        Ok(repo) => repo,
        Err(err) => {
            ctx.effects
                .push(Effect::message(MessageLevel::Warn, err.to_string()));
            return Ok(CommandOutcome::default());
        }
    };
    let Some(source) = repo.current_branch() else {
        ctx.effects.push(Effect::message(
            MessageLevel::Warn,
            "Spin requires a checked-out branch",
        ));
        return Ok(CommandOutcome::default());
    };
    let (checkout, note) = match mode {
        SpinMode::Spinoff => (true, None),
        SpinMode::Spinout => match repo.is_dirty() {
            Ok(false) => (false, None),
            Ok(true) => (
                true,
                Some(
                    "the working tree had changes, so HEAD moved to the new branch like a spin-off",
                ),
            ),
            Err(err) => {
                ctx.effects
                    .push(Effect::message(MessageLevel::Warn, err.to_string()));
                return Ok(CommandOutcome::default());
            }
        },
    };
    match spin_branch(&repo, &name, &source, checkout, note) {
        Ok(message) => ctx.info(message),
        Err(err) => ctx
            .effects
            .push(Effect::message(MessageLevel::Warn, err.to_string())),
    }
    Ok(CommandOutcome {
        refresh: true,
        ..Default::default()
    })
}

/// Create `new` at `source` tracking it, then move `source` off its unpushed
/// commits to the merge base with its upstream.
///
/// With nothing to spin off (no upstream, or no unpushed commits) the new
/// branch is still created and the source is left untouched. `note` is appended
/// to the message, describing a spin-out falling back to a spin-off.
fn spin_branch(
    repo: &GitRepo,
    new: &str,
    source: &str,
    checkout: bool,
    note: Option<&str>,
) -> Result<String> {
    let upstream = repo.upstream_ref(source);
    let merge_base = upstream
        .as_deref()
        .map(|upstream| (upstream, repo.merge_base(source, upstream)));
    repo.create_tracking_branch(new, source)?;
    let mut message = format!("Created branch {new} tracking {source}");
    if let Some(note) = note {
        message.push_str(&format!("; {note}"));
    }
    let merge_base = match merge_base {
        None => {
            message.push_str(&format!(
                "; {source} has no upstream, so it was left untouched"
            ));
            return Ok(message);
        }
        Some((upstream, None)) => {
            message.push_str(&format!(
                "; {source} has no merge base with {upstream}, so it was left untouched"
            ));
            return Ok(message);
        }
        Some((_, Some(merge_base))) => merge_base,
    };
    if merge_base == repo.resolve_commit(source)? {
        message.push_str(&format!(
            "; {source} has no unpushed commits, so it was left untouched"
        ));
        return Ok(message);
    }
    if checkout {
        repo.checkout_branch(new)?;
        repo.reset_branch(source, &merge_base)?;
    } else {
        repo.reset_mixed(&merge_base)?;
    }
    message.push_str(&format!("; moved {source} to {merge_base}"));
    Ok(message)
}

/// Prompt for the branch to reset, then for the revision to reset it to.
fn branch_reset(_ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    Ok(CommandOutcome {
        prompt: Some((
            branch_name_prompt("Reset branch", "", "Branch to reset"),
            PromptAction::BranchReset,
        )),
        ..Default::default()
    })
}

/// Refuse menu arguments `command` cannot honour, naming the selected keys.
///
/// Returns `true` after queuing a warning when any switch or option value was
/// selected, so the caller can return without prompting or mutating.
fn refuse_unsupported_args(ctx: &mut CommandCtx<'_>, command: &str) -> Result<bool> {
    let args: crate::menu::Args = match ctx.args.filter(|value| !value.is_null()) {
        Some(value) => serde_json::from_value(value.clone()).map_err(|error| {
            crate::Error::new(format!(
                "Expected {command} switches and option values: {error}"
            ))
        })?,
        None => crate::menu::Args::default(),
    };
    let mut selected = args.switches;
    selected.extend(args.options.into_keys());
    if selected.is_empty() {
        return Ok(false);
    }
    ctx.effects.push(Effect::message(
        MessageLevel::Warn,
        format!(
            "{command} does not support the selected arguments: {}",
            selected.join(", ")
        ),
    ));
    Ok(true)
}

/// Prompt for the name of a lightweight tag, then its target revision.
fn tag_create(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    if refuse_unsupported_args(ctx, "tag.create")? {
        return Ok(CommandOutcome::default());
    }
    Ok(CommandOutcome {
        prompt: Some((
            branch_name_prompt("Create tag", "", "New tag name"),
            PromptAction::TagCreate,
        )),
        ..Default::default()
    })
}

/// Prompt for the name of a new remote, then for its URL.
fn remote_add(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    if refuse_unsupported_args(ctx, "remote.add")? {
        return Ok(CommandOutcome::default());
    }
    Ok(CommandOutcome {
        prompt: Some((
            branch_name_prompt("Add remote", "", "Remote name"),
            PromptAction::RemoteAdd,
        )),
        ..Default::default()
    })
}

/// Prompt for the remote to remove, then require destructive confirmation.
fn remote_remove(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    if refuse_unsupported_args(ctx, "remote.remove")? {
        return Ok(CommandOutcome::default());
    }
    Ok(CommandOutcome {
        prompt: Some((
            branch_name_prompt("Remove remote", "", "Remote to remove"),
            PromptAction::RemoteRemove,
        )),
        ..Default::default()
    })
}

/// Prompt for a remote name and both URLs before changing configuration.
fn remote_configure(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    if refuse_unsupported_args(ctx, "remote.configure")? {
        return Ok(CommandOutcome::default());
    }
    Ok(CommandOutcome {
        prompt: Some((
            branch_name_prompt("Configure remote", "", "Remote name"),
            PromptAction::RemoteConfigure,
        )),
        ..Default::default()
    })
}

/// Prompt for the remote to rename, then for its new name.
fn remote_rename(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    if refuse_unsupported_args(ctx, "remote.rename")? {
        return Ok(CommandOutcome::default());
    }
    Ok(CommandOutcome {
        prompt: Some((
            branch_name_prompt("Rename remote", "", "Remote to rename"),
            PromptAction::RemoteRename,
        )),
        ..Default::default()
    })
}

/// Prompt for the remote whose stale tracking branches should be pruned.
fn remote_prune(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    if refuse_unsupported_args(ctx, "remote.prune")? {
        return Ok(CommandOutcome::default());
    }
    Ok(CommandOutcome {
        prompt: Some((
            branch_name_prompt("Prune remote", "", "Remote to prune"),
            PromptAction::RemotePrune,
        )),
        ..Default::default()
    })
}

/// Prompt for the revision to reset to, mixed mode.
fn reset_mixed(_ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    Ok(CommandOutcome {
        prompt: Some((reset_revision_prompt(), PromptAction::ResetMixed)),
        ..Default::default()
    })
}

/// Prompt for the revision HEAD should move to, soft mode.
fn reset_soft(_ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    Ok(CommandOutcome {
        prompt: Some((reset_revision_prompt(), PromptAction::ResetSoft)),
        ..Default::default()
    })
}

/// Prompt for the revision reset hard should discard changes back to.
fn reset_hard(_ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    Ok(CommandOutcome {
        prompt: Some((reset_revision_prompt(), PromptAction::ResetHard)),
        ..Default::default()
    })
}

/// Prompt for the revision reset keep should apply.
fn reset_keep(_ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    Ok(CommandOutcome {
        prompt: Some((reset_revision_prompt(), PromptAction::ResetKeep)),
        ..Default::default()
    })
}

/// Prompt for the tag to delete.
fn tag_delete(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    if refuse_unsupported_args(ctx, "tag.delete")? {
        return Ok(CommandOutcome::default());
    }
    Ok(CommandOutcome {
        prompt: Some((
            branch_name_prompt("Delete tag", "", "Tag to delete"),
            PromptAction::TagDelete,
        )),
        ..Default::default()
    })
}

/// Prompt for the branch to rename, then for its new name.
fn branch_rename(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    let current = GitRepo::open(ctx.repo_path)?
        .current_branch()
        .unwrap_or_default();
    Ok(CommandOutcome {
        prompt: Some((
            branch_name_prompt("Rename branch", &current, "Branch to rename"),
            PromptAction::BranchRename,
        )),
        ..Default::default()
    })
}

/// Prompt for the branch to delete.
fn branch_delete(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    let current = GitRepo::open(ctx.repo_path)?
        .current_branch()
        .unwrap_or_default();
    Ok(CommandOutcome {
        prompt: Some((
            branch_name_prompt("Delete branch", &current, "Branch to delete"),
            PromptAction::BranchDelete,
        )),
        ..Default::default()
    })
}

/// Prompt for the branch whose description to set, then for its text.
fn branch_configure_description(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    let current = GitRepo::open(ctx.repo_path)?
        .current_branch()
        .unwrap_or_default();
    Ok(CommandOutcome {
        prompt: Some((
            branch_name_prompt("Set branch description", &current, "Branch to describe"),
            PromptAction::BranchDescription,
        )),
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

fn commit_amend(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    commit_operation(ctx, crate::git::CommitKind::Amend)
}
fn commit_extend(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    commit_operation(ctx, crate::git::CommitKind::Extend)
}
fn commit_reword(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    commit_operation(ctx, crate::git::CommitKind::Reword)
}

fn commit_fixup(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    let repo = GitRepo::open(ctx.repo_path)?;
    if repo.status_listing()?.head.is_none() {
        ctx.info("Cannot create a fixup commit in a repository with no commits");
        return Ok(CommandOutcome::default());
    }
    Ok(CommandOutcome {
        open_selection: Some(SelectionRequest {
            options: crate::session::log_options_from_args(None)?,
            settings: CommitSelection {
                purpose: "fixup commit".into(),
                prompt: "Select commit to fix up".into(),
                multi: false,
                allow_free_text: false,
            },
            operation: SelectionOperation::FixupCommit,
        }),
        ..Default::default()
    })
}

fn commit_instant_fixup(_ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    commit_instant_autosquash(crate::instant_rebase::InstantAutosquashKind::Fixup)
}

fn commit_instant_squash(_ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    commit_instant_autosquash(crate::instant_rebase::InstantAutosquashKind::Squash)
}

fn commit_instant_autosquash(
    kind: crate::instant_rebase::InstantAutosquashKind,
) -> Result<CommandOutcome> {
    let purpose = match kind {
        crate::instant_rebase::InstantAutosquashKind::Fixup => "instant fixup",
        crate::instant_rebase::InstantAutosquashKind::Squash => "instant squash",
    };
    Ok(CommandOutcome {
        open_selection: Some(SelectionRequest {
            options: crate::session::log_options_from_args(None)?,
            settings: CommitSelection {
                purpose: purpose.into(),
                prompt: format!("Select commit for {purpose}"),
                multi: false,
                allow_free_text: false,
            },
            operation: SelectionOperation::InstantAutosquash(kind),
        }),
        ..Default::default()
    })
}

fn commit_operation(
    ctx: &mut CommandCtx<'_>,
    kind: crate::git::CommitKind,
) -> Result<CommandOutcome> {
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
    if kind != crate::git::CommitKind::Create
        && (options.reuse_message.is_some() || options.reedit_message.is_some())
    {
        ctx.info("Message reuse and reedit options conflict with HEAD edit actions");
        return Ok(CommandOutcome::default());
    }
    if kind == crate::git::CommitKind::Reword && options.all {
        ctx.info("Reword uses --only and cannot be combined with --all");
        return Ok(CommandOutcome::default());
    }
    if kind == crate::git::CommitKind::Extend
        && status.staged.is_empty()
        && !(options.all && !status.unstaged.is_empty())
        && !(options.allow_empty
            || options.reset_author
            || options.signoff
            || options.author.is_some()
            || options.date.is_some())
    {
        if status.unstaged.is_empty() {
            ctx.info("Nothing staged (or unstaged) to extend");
            return Ok(CommandOutcome::default());
        }
        return Ok(CommandOutcome {
            prompt: Some((
                head_commit_prompt(
                    "Nothing staged. Commit all tracked changes and extend HEAD?",
                    None,
                ),
                PromptAction::ExtendAll {
                    fingerprint: RepositoryFingerprint::capture(&repo)?,
                    options,
                },
            )),
            ..Default::default()
        });
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
        return Ok(CommandOutcome {
            prompt: Some((
                prompt,
                PromptAction::Commit {
                    fingerprint: RepositoryFingerprint::capture(&repo)?,
                    untracked: !status.untracked.is_empty(),
                    operation,
                },
            )),
            ..Default::default()
        });
    }
    ctx.info("Committing…");
    Ok(CommandOutcome {
        commit: Some(crate::git::RepositoryOperation::Commit(operation)),
        ..Default::default()
    })
}

fn head_commit_prompt(title: &str, detail: Option<&str>) -> ViewPromptParams {
    ViewPromptParams {
        prompt_id: String::new(),
        kind: PromptKind::Confirm,
        title: title.into(),
        detail: detail.map(str::to_string),
        yes_label: None,
        no_label: None,
        destructive: Some(true),
        default: None,
        placeholder: None,
        secret: None,
        items: None,
        allow_custom: None,
        preselected: None,
        min: None,
        max: None,
    }
}

fn prepare_head_commit(
    ctx: &mut CommandCtx<'_>,
    repo: &GitRepo,
    operation: crate::git::CommitOperation,
) -> Result<CommandOutcome> {
    if repo.head_is_published()? {
        let prompt = head_commit_prompt("Rewrite published HEAD?", Some(
            "HEAD is reachable from a remote-tracking branch. Rewriting it may require a force push.",
        ));
        return Ok(CommandOutcome {
            prompt: Some((
                prompt,
                PromptAction::PublishedCommit {
                    fingerprint: RepositoryFingerprint::capture(repo)?,
                    operation,
                },
            )),
            ..Default::default()
        });
    }
    ctx.info("Committing…");
    Ok(CommandOutcome {
        commit: Some(crate::git::RepositoryOperation::Commit(operation)),
        ..Default::default()
    })
}

/// Validate structured commit-menu input once, before staging or launching Git.
fn commit_options(
    args: Option<&Value>,
    repo: &GitRepo,
    kind: crate::git::CommitKind,
) -> Result<crate::git::CommitOptions> {
    let args: crate::menu::Args = match args.filter(|value| !value.is_null()) {
        Some(value) => serde_json::from_value(value.clone()).map_err(|error| {
            crate::Error::new(format!(
                "Expected commit switches and option values: {error}"
            ))
        })?,
        None => crate::menu::Args::default(),
    };
    let mut options = crate::git::CommitOptions::default();
    let mut seen = std::collections::HashSet::new();
    for key in args.switches {
        if !seen.insert(key.clone()) {
            return Err(crate::Error::new(format!("Duplicate commit switch {key}")));
        }
        match key.as_str() {
            "-a" => options.all = true,
            "-e" => options.allow_empty = true,
            "-v" => options.verbose = true,
            "-n" => options.no_verify = true,
            "-R" => options.reset_author = true,
            "-s" => options.signoff = true,
            _ => {
                return Err(crate::Error::new(format!(
                    "Unsupported commit switch {key}"
                )))
            }
        }
    }
    for (key, value) in args.options {
        if value.contains('\0') || (value.trim().is_empty() && key != "=S") {
            return Err(crate::Error::new(format!(
                "Missing or invalid value for {key}"
            )));
        }
        match key.as_str() {
            "=A" => {
                let valid = value.rsplit_once('<').is_some_and(|(name, email)| {
                    !name.trim().is_empty() && email.ends_with('>') && email.len() > 2
                });
                if !valid || value.contains(['\n', '\r']) {
                    return Err(crate::Error::new("Author must be Name <email>"));
                }
                options.author = Some(value);
            }
            "=D" => options.date = Some(value),
            "=S" => options.signing_key = Some(value),
            "=C" => options.reuse_message = Some(value),
            "=c" => options.reedit_message = Some(value),
            _ => {
                return Err(crate::Error::new(format!(
                    "Unsupported commit option {key}"
                )))
            }
        }
    }
    if options.reuse_message.is_some() && options.reedit_message.is_some() {
        return Err(crate::Error::new(
            "Reuse and reedit message options conflict",
        ));
    }
    if options.reset_author && options.author.is_some() {
        return Err(crate::Error::new(
            "Reset author cannot be combined with author",
        ));
    }
    if options.reset_author
        && kind == crate::git::CommitKind::Create
        && options.reuse_message.is_none()
        && options.reedit_message.is_none()
    {
        return Err(crate::Error::new(
            "Reset author requires reuse/reedit when creating a commit",
        ));
    }
    for revision in [&mut options.reuse_message, &mut options.reedit_message]
        .into_iter()
        .flatten()
    {
        *revision = repo.resolve_commit(revision)?;
    }
    if let Some(date) = &options.date {
        repo.validate_author_date(date)?;
    }
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

fn open_local_branch_log(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    open_log_with_refs(ctx, "local")
}

fn open_all_branch_log(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    open_log_with_refs(ctx, "branches")
}

fn open_all_reference_log(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    open_log_with_refs(ctx, "all")
}

fn open_log_with_refs(ctx: &mut CommandCtx<'_>, refs: &str) -> Result<CommandOutcome> {
    let mut args = crate::session::log_options_from_args(ctx.args)?;
    args.ref_selection = Some(refs.into());
    Ok(CommandOutcome {
        open_view: Some((ViewKind::Log, Placement::Main)),
        open_args: Some(serde_json::to_value(args)?),
        ..Default::default()
    })
}

/// Push a commit-selection log used to copy one or more commit IDs.
fn select_and_copy(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    let args = ctx.args.and_then(Value::as_object);
    let string_arg = |name: &str, default: &str| {
        args.and_then(|args| args.get(name))
            .and_then(Value::as_str)
            .unwrap_or(default)
            .to_string()
    };
    let bool_arg = |name: &str, default: bool| {
        args.and_then(|args| args.get(name))
            .and_then(Value::as_bool)
            .unwrap_or(default)
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
        tree.line_nodes
            .get(cursor.line)
            .and_then(|line| tree.node(&line.node))
            .filter(|node| node.kind == NodeKind::Commit)
            .and_then(|node| node.commit.clone())
    });
    if oid
        .as_deref()
        .is_some_and(|oid| ctx.view.toggle_commit_mark(oid))
    {
        return Ok(CommandOutcome {
            refresh: false,
            ..Default::default()
        });
    }
    ctx.info("Place the cursor on a selectable commit to mark it");
    Ok(CommandOutcome::default())
}

/// Prompt for a revision and return its validated full commit object ID.
fn enter_revision(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    if !ctx
        .view
        .commit_selection()
        .is_some_and(|selection| selection.allow_free_text)
    {
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
    Ok(CommandOutcome {
        prompt: Some((prompt, PromptAction::Revision)),
        ..Default::default()
    })
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

/// Define a command that saves or restores the open menu's switches/options.
macro_rules! menu_args {
    ($(#[$doc:meta])* $name:ident, $method:ident, $message:literal) => {
        $(#[$doc])*
        fn $name(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
            match ctx.view.menu_mut() {
                Some(menu) => {
                    menu.$method();
                    ctx.info($message);
                }
                None => ctx.info("No menu is open"),
            }
            Ok(CommandOutcome::default())
        }
    };
}

/// Define a command that applies a visibility command to the section at point.
macro_rules! section_visibility {
    ($name:ident, $command:ident) => {
        fn $name(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
            let recursive = ctx.recursive();
            apply_visibility(ctx, VisibilityCommand::$command { recursive })
        }
    };
}

/// Define a command that sets the view's detail level.
macro_rules! detail_level {
    ($name:ident, $level:literal) => {
        fn $name(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
            apply_visibility(ctx, VisibilityCommand::SetDetail($level))
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

menu_args!(
    /// Remember the current switches/options as this menu's defaults.
    ///
    /// Storage is in memory for now; persisted global/per-repo storage arrives
    /// with the menu configuration work (§3.11, §3.18).
    save_menu_args,
    save_args,
    "Saved menu arguments"
);

menu_args!(
    /// Restore the menu's saved switches/options.
    reset_menu_args,
    reset_args,
    "Reset menu arguments"
);

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
        let cursor_oid = ctx.cursor.and_then(|cursor| {
            tree.line_nodes
                .get(cursor.line)
                .and_then(|line| tree.node(&line.node))
                .filter(|node| node.kind == NodeKind::Commit)
                .and_then(|node| node.commit.as_deref())
        });
        let selected = ctx.view.selected_commits(cursor_oid);
        if selected.is_empty() {
            ctx.info("Place the cursor on a commit to confirm the selection");
            return Ok(CommandOutcome::default());
        }
        return Ok(CommandOutcome {
            selected_commits: Some(selected),
            ..Default::default()
        });
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
    if ctx.view.commit_selection().is_some()
        || *ctx.view.kind() == crate::view::ViewKind::RebaseTodo
    {
        return Ok(CommandOutcome {
            pop: true,
            ..Default::default()
        });
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

section_visibility!(toggle_section, Toggle);
section_visibility!(show_section, Show);
section_visibility!(hide_section, Hide);

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

detail_level!(detail_level_1, 1);
detail_level!(detail_level_2, 2);
detail_level!(detail_level_3, 3);
detail_level!(detail_level_4, 4);

fn next_section(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    section_cursor_command(ctx, true)
}

fn cycle_visibility(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    ctx.view.cycle_visibility();
    Ok(CommandOutcome::default())
}

fn hierarchy_cursor(ctx: &mut CommandCtx<'_>, direction: Option<bool>) -> Result<CommandOutcome> {
    let tree = ctx
        .view
        .command_source_tree()
        .unwrap_or_else(|| ctx.view.tree());
    let target = ctx
        .cursor
        .and_then(|cursor| tree.navigation_node(cursor.line))
        .and_then(|node| match direction {
            Some(forward) => tree.sibling(&node.id, forward).or(Some(node)),
            None => tree.parent(&node.id).or(Some(node)),
        });
    Ok(CommandOutcome {
        cursor_nodes: target.map(|node| node.id.clone()).into_iter().collect(),
        ..Default::default()
    })
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
    section_cursor_command(ctx, false)
}

/// Move to a visible structured header of the requested kind.
fn jump(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    let args = ctx.args.and_then(Value::as_object);
    let kind = args
        .and_then(|args| args.get("kind"))
        .and_then(Value::as_str);
    let dir = args
        .and_then(|args| args.get("dir"))
        .and_then(Value::as_i64);
    let Some(kind) = kind.filter(|kind| matches!(*kind, "section" | "file" | "hunk")) else {
        ctx.info("Jump kind must be section, file or hunk");
        return Ok(CommandOutcome::default());
    };
    let Some(dir) = dir.filter(|dir| *dir == 1 || *dir == -1) else {
        ctx.info("Jump direction must be 1 or -1");
        return Ok(CommandOutcome::default());
    };
    let wanted = match kind {
        "section" => NodeKind::Section,
        "file" => NodeKind::File,
        _ => NodeKind::Hunk,
    };
    let Some(cursor) = ctx.cursor else {
        return Ok(CommandOutcome::default());
    };
    let source = ctx
        .view
        .command_source_tree()
        .unwrap_or_else(|| ctx.view.tree());
    let lines: Vec<usize> = source
        .line_nodes
        .iter()
        .enumerate()
        .filter_map(|(line, owner)| {
            (owner.kind == wanted
                && source
                    .node(&owner.node)
                    .is_some_and(|node| node.start_line == line))
            .then_some(line)
        })
        .collect();
    let target = if dir > 0 {
        lines.into_iter().find(|line| *line > cursor.line)
    } else {
        lines.into_iter().rev().find(|line| *line < cursor.line)
    };
    Ok(CommandOutcome {
        cursor: target.map(|line| Cursor { line, column: 0 }),
        ..Default::default()
    })
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
    let source = ctx
        .view
        .command_source_tree()
        .unwrap_or_else(|| ctx.view.tree());
    // A stage request originating in the displayed staged section is never
    // retargeted to an unstaged hunk after a refresh. Visual selections retain
    // precedence and are resolved by their original section below.
    if command == dispatch::Command::Stage
        && selection.is_none()
        && ctx
            .cursor
            .and_then(|cursor| source.navigation_node(cursor.line))
            .is_some_and(|node| {
                node.kind == NodeKind::Hunk && node.section.as_deref() == Some("staged")
            })
    {
        return Ok(CommandOutcome::default());
    }
    let destinations = if command == dispatch::Command::Stage && ctx.selection.is_none() {
        ctx.cursor
            .and_then(|cursor| source.navigation_node(cursor.line))
            .filter(|node| {
                node.kind == NodeKind::Hunk && node.section.as_deref() == Some("unstaged")
            })
            .map(|node| {
                // Capture the successor and source ancestors before Git mutates the view.
                // Never follow the staged hunk's relocation when no sibling survives.
                let file = source.parent(&node.id);
                source
                    .sibling(&node.id, true)
                    .into_iter()
                    .chain(file)
                    .chain(file.and_then(|file| source.parent(&file.id)))
                    .map(|node| node.id.clone())
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let (effects, prompt) =
        stage_or_unstage_at(ctx.view, ctx.repo_path, command, ctx.cursor, selection)?;
    let succeeded = effects.is_empty();
    let move_cursor = succeeded && prompt.is_none();
    ctx.extend(effects);
    Ok(CommandOutcome {
        prompt,
        refresh: true,
        cursor_nodes: if move_cursor {
            destinations
        } else {
            Vec::new()
        },
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
/// Move the cursor to the next (`forward`) or previous section header.
fn section_cursor_command(ctx: &mut CommandCtx<'_>, forward: bool) -> Result<CommandOutcome> {
    Ok(CommandOutcome {
        cursor: section_cursor(ctx.view, ctx.cursor, forward),
        ..Default::default()
    })
}

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
            return Ok((
                vec![Effect::message(
                    MessageLevel::Info,
                    "Place the cursor on a change to stage or unstage.",
                )],
                None,
            ));
        };
        let Some(node) = source
            .line_nodes
            .get(cursor.line)
            .and_then(|line| source.node(&line.node))
        else {
            return Ok((
                vec![Effect::message(
                    MessageLevel::Info,
                    "No change under the cursor.",
                )],
                None,
            ));
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
                    return Ok((
                        vec![Effect::message(MessageLevel::Warn, errors::HUNK_CHANGED)],
                        None,
                    ))
                }
            }
        };

        // An untracked file has no index entry to remove.
        if command == dispatch::Command::Unstage && node.section.as_deref() == Some("untracked") {
            return Ok((
                vec![Effect::message(
                    MessageLevel::Info,
                    dispatch::NOTHING_TO_UNSTAGE,
                )],
                None,
            ));
        }

        dispatch::resolve(command, dispatch_target(node), None)
    };

    let repo = GitRepo::open(repo_path)?;
    if let Some(path) = match &resolution {
        dispatch::Resolution::StageFile { path } | dispatch::Resolution::UnstageFile { path } => {
            Some(path.as_str())
        }
        _ => None,
    } {
        let section = if matches!(resolution, dispatch::Resolution::UnstageFile { .. }) {
            "staged"
        } else {
            file_worktree_section(source, path)
        };
        if !displayed_file_is_current(&repo, source, path, section)? {
            return Ok((
                vec![Effect::message(MessageLevel::Warn, errors::HUNK_CHANGED)],
                None,
            ));
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
                return Ok((
                    vec![Effect::message(MessageLevel::Warn, errors::HUNK_CHANGED)],
                    None,
                ));
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
        return Ok((
            Vec::new(),
            Some((
                prompt,
                PromptAction::Discard {
                    fingerprint: RepositoryFingerprint::capture(&repo)?,
                    resolution,
                    tree: source.clone(),
                },
            )),
        ));
    }
    let effects = match &resolution {
        dispatch::Resolution::Selection {
            start,
            end,
            reverse,
        } => execute_displayed_selection(&repo, source, *start, *end, *reverse, false),
        dispatch::Resolution::DiscardSelection { start, end } => {
            execute_displayed_selection(&repo, source, *start, *end, false, true)
        }
        _ => dispatch::execute(&repo, &resolution, &[]),
    }?;
    Ok((effects, None))
}

/// Check the complete file state captured by the originating render.
fn displayed_file_is_current(
    repo: &GitRepo,
    tree: &Tree,
    path: &str,
    section: &str,
) -> Result<bool> {
    let Some(file) = tree.nodes.iter().find(|node| {
        node.kind == NodeKind::File
            && node.path.as_deref() == Some(path)
            && node.section.as_deref() == Some(section)
    }) else {
        return Ok(false);
    };
    let Some(crate::tree::MutationContent::File(captured)) = file.mutation.as_ref() else {
        return Ok(false);
    };
    let current = repo.file_mutation_identity(
        path,
        captured.original.as_ref().map(|(path, _)| path.as_str()),
        section != "staged",
    )?;
    fn matches(
        captured: &crate::git::FileMutationIdentity,
        current: &crate::git::FileMutationIdentity,
        staged: bool,
    ) -> bool {
        captured.head == current.head
            && captured.index == current.index
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
    tree.nodes
        .iter()
        .find(|node| {
            node.kind == NodeKind::File
                && node.path.as_deref() == Some(path)
                && matches!(node.section.as_deref(), Some("unstaged" | "untracked"))
        })
        .and_then(|node| node.section.as_deref())
        .unwrap_or("unstaged")
}

/// Run a whole-section operation against `repo_path`.
fn run_resolution(repo_path: &str, resolution: dispatch::Resolution) -> Result<Vec<Effect>> {
    let repo = GitRepo::open(repo_path)?;
    dispatch::execute(&repo, &resolution, &[])
}

/// Resolve displayed coordinates before reading Git; fresh diffs only supply
/// current patch coordinates for the selected content identities.
fn execute_displayed_selection(
    repo: &GitRepo,
    tree: &Tree,
    start: u32,
    end: u32,
    staged: bool,
    discard: bool,
) -> Result<Vec<Effect>> {
    let (start, end) = (start.min(end) as usize, start.max(end) as usize);
    let section = if staged { "staged" } else { "unstaged" };
    // Preserve the selection's originating-section rule.
    if tree
        .line_nodes
        .get(start)
        .or_else(|| tree.line_nodes.get(end))
        .and_then(|line| line.section.as_deref())
        != Some(section)
    {
        return Ok(vec![Effect::message(
            MessageLevel::Info,
            crate::patch::NO_CHANGED_LINES,
        )]);
    }
    let selected = |node: &Node| start <= node.start_line && node.start_line <= end;
    let mut ops = Vec::new();
    let mut layouts = Vec::new();
    for file in tree
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::File && node.section.as_deref() == Some(section))
    {
        if end < file.start_line || start > file.end_line {
            continue;
        }
        let Some(path) = file.path.as_deref() else {
            continue;
        };
        let hunks: Vec<_> = tree
            .nodes
            .iter()
            .filter(|node| {
                node.kind == NodeKind::Hunk
                    && node.section.as_deref() == Some(section)
                    && node.path.as_deref() == Some(path)
            })
            .collect();
        let first_op = ops.len();
        if selected(file) {
            ops.push(SelectionOp::WholeFile { path: path.into() });
        } else {
            for hunk in &hunks {
                let Some(id) = hunk.hunk_id.as_deref() else {
                    continue;
                };
                if selected(hunk) {
                    ops.push(SelectionOp::WholeHunk {
                        path: path.into(),
                        hunk_id: id.into(),
                    });
                } else {
                    let lines: Vec<_> = tree
                        .nodes
                        .iter()
                        .filter(|node| {
                            node.kind == NodeKind::Line
                                && node.section.as_deref() == Some(section)
                                && node.path.as_deref() == Some(path)
                                && node.hunk_id.as_deref() == Some(id)
                                && selected(node)
                                && matches!(node.marker, Some('+') | Some('-'))
                        })
                        .filter_map(|node| node.line_index)
                        .collect();
                    if !lines.is_empty() {
                        ops.push(SelectionOp::Lines {
                            path: path.into(),
                            hunk_id: id.into(),
                            lines,
                        });
                    }
                }
            }
        }
        if ops.len() == first_op {
            continue;
        }
        let whole_file = ops[first_op..]
            .iter()
            .any(|op| matches!(op, SelectionOp::WholeFile { .. }));
        if whole_file && !displayed_file_is_current(repo, tree, path, section)? {
            return Ok(vec![Effect::message(
                MessageLevel::Warn,
                errors::HUNK_CHANGED,
            )]);
        }
        let diff = if staged {
            repo.diff_staged(path)?
        } else {
            repo.diff_unstaged(path)?
        };
        // Layout rows are deliberately unused: operations already contain the
        // request tree's identities and line_index offsets, not fresh rows.
        let layout = if whole_file {
            FileLayout::from_diff(&diff, 0).unwrap_or_else(|| FileLayout {
                path: path.to_string(),
                header: String::new(),
                header_line: 0,
                hunks: Vec::new(),
            })
        } else if let Some(layout) = FileLayout::from_diff(&diff, 0) {
            layout
        } else {
            return Ok(vec![Effect::message(
                MessageLevel::Warn,
                errors::HUNK_CHANGED,
            )]);
        };
        let unchanged = ops[first_op..].iter().all(|op| match op {
            SelectionOp::WholeFile { .. } => whole_file,
            SelectionOp::WholeHunk { hunk_id, .. } | SelectionOp::Lines { hunk_id, .. } => {
                // Selection execution also consumes the fresh file header.
                // Compare its semantics, not just content IDs or fresh rows.
                let captured = hunks
                    .iter()
                    .find(|node| node.hunk_id.as_ref() == Some(hunk_id))
                    .and_then(|node| node.mutation.as_ref());
                match captured {
                    Some(crate::tree::MutationContent::Hunk { hunk, .. }) => layout
                        .hunks
                        .iter()
                        .any(|h| &h.hunk.id == hunk_id && h.hunk.metadata == hunk.metadata),
                    _ => false,
                }
            }
        });
        if !unchanged {
            return Ok(vec![Effect::message(
                MessageLevel::Warn,
                errors::HUNK_CHANGED,
            )]);
        }
        layouts.push(layout);
    }
    // Validate every selected identity before mutating any file.
    let result = if discard {
        repo.discard_selection_ops(&layouts, &ops)
    } else {
        repo.apply_selection_ops(&layouts, &ops, staged)
    };
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
