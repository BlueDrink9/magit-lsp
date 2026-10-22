//! The command registry: every key action is a [`Command`].
//!
//! §3.5 makes handlers `async` with a `Ui` handle in R-3.1; until then they are
//! plain synchronous functions. [`CommandCtx`] carries the view, repo path,
//! cursor, selection, arguments and effects collector, and is the single place
//! new capabilities are added so that change stays mechanical.

use crate::dispatch;
use crate::errors;
use crate::git::GitRepo;
use crate::patch::FileLayout;
use crate::protocol::{Cursor, Effect, MessageLevel, Selection};
use crate::session::{re_resolve, OpenView};
use crate::tree::{Node, NodeKind, Tree};
use crate::view::VisibilityCommand;
use crate::Result;
use serde_json::Value;
use std::sync::OnceLock;

/// The registry group a command belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Common,
    Status,
    Select,
    Menu,
    CommitMessage,
}

impl Scope {
    /// The scope name as it appears in a command id.
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Common => "common",
            Scope::Status => "status",
            Scope::Select => "select",
            Scope::Menu => "menu",
            Scope::CommitMessage => "commit-message",
        }
    }

    /// Whether a command in this scope applies to a view with `view_scope`.
    ///
    /// `common` commands apply everywhere; the rest apply only to their own
    /// view-kind scope.
    pub fn matches(self, view_scope: &str) -> bool {
        self == Scope::Common || self.as_str() == view_scope
    }
}

/// Whether a command can run, and what to say when it cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandStatus {
    Implemented,
    /// Listed but not built yet; the roadmap item it will arrive with.
    Planned { roadmap_id: &'static str },
    /// The backend cannot do this; the reason is shown instead.
    Unavailable { reason: &'static str },
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

/// What a handler asks the session to do after it runs.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CommandOutcome {
    /// Re-read the backing state before rendering.
    pub refresh: bool,
    /// A cursor to move to that the old node cannot describe.
    pub cursor: Option<Cursor>,
}

/// Everything a command handler may touch, plus the effects it collected.
pub struct CommandCtx<'a> {
    pub view: &'a mut OpenView,
    pub repo_path: &'a str,
    pub cursor: Option<Cursor>,
    pub selection: Option<Selection>,
    pub args: Option<&'a Value>,
    /// The canonical chord that triggered the command through `view/key`.
    pub key: Option<String>,
    effects: Vec<Effect>,
}

impl<'a> CommandCtx<'a> {
    /// Build a context for one `view/invoke` or `view/key`.
    pub fn new(
        view: &'a mut OpenView,
        repo_path: &'a str,
        cursor: Option<Cursor>,
        selection: Option<Selection>,
        args: Option<&'a Value>,
        key: Option<String>,
    ) -> Self {
        Self {
            view,
            repo_path,
            cursor,
            selection,
            args,
            key,
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
        planned("common.confirm", Scope::Common, "Confirm the current selection", "R-3.1"),
        planned("common.quit", Scope::Common, "Close all views", "R-3.1"),
        planned(
            "common.cycle-visibility",
            Scope::Common,
            "Cycle the visibility of every section",
            "R-2.3",
        ),
        planned("common.next-sibling", Scope::Common, "Move to the next sibling", "R-2.3"),
        planned(
            "common.prev-sibling",
            Scope::Common,
            "Move to the previous sibling",
            "R-2.3",
        ),
        planned(
            "common.parent-section",
            Scope::Common,
            "Move to the parent section",
            "R-2.3",
        ),
        planned(
            "common.toggle-text-mode",
            Scope::Common,
            "Toggle text mode",
            "R-2.6",
        ),
        planned("common.copy-value", Scope::Common, "Copy the value at point", "R-2.6"),
        planned("common.visit", Scope::Common, "Visit the thing at point", "R-2.3"),
        planned("status.discard", Scope::Status, "Discard the change at point", "R-3.6"),
        planned("status.reverse", Scope::Status, "Reverse-apply the change at point", "R-3.6"),
        planned("status.apply", Scope::Status, "Apply the change at point", "R-8.2"),
        planned("open.dispatch", Scope::Common, "Show the dispatch menu", "R-2.8"),
        planned("open.commit", Scope::Status, "Open the commit menu", "R-4.1"),
        planned("open.branch", Scope::Status, "Open the branch menu", "R-6.1"),
        planned("open.log", Scope::Status, "Open the log menu", "R-5.1"),
        planned("open.diff", Scope::Status, "Open the diff menu", "R-5.2"),
        planned("open.fetch", Scope::Status, "Open the fetch menu", "R-6.2"),
        planned("open.pull", Scope::Status, "Open the pull menu", "R-6.2"),
        planned("open.push", Scope::Status, "Open the push menu", "R-6.2"),
        planned("open.merge", Scope::Status, "Open the merge menu", "R-7.1"),
        planned("open.rebase", Scope::Status, "Open the rebase menu", "R-7.3"),
        planned("open.stash", Scope::Status, "Open the stash menu", "R-6.5"),
        planned("open.tag", Scope::Status, "Open the tag menu", "R-6.3"),
        planned("open.reset", Scope::Status, "Open the reset menu", "R-6.3"),
        planned("open.revert", Scope::Status, "Open the revert menu", "R-7.2"),
        planned(
            "open.cherry-pick",
            Scope::Status,
            "Open the cherry-pick menu",
            "R-7.2",
        ),
        planned("open.bisect", Scope::Status, "Open the bisect menu", "R-8.2"),
        planned("open.remote", Scope::Status, "Open the remote menu", "R-6.3"),
        planned("open.submodule", Scope::Status, "Open the submodule menu", "R-8.2"),
        planned("open.worktree", Scope::Status, "Open the worktree menu", "R-8.2"),
        planned("open.ignore", Scope::Status, "Open the ignore menu", "R-8.2"),
        planned("open.refs", Scope::Status, "Open the refs view", "R-5.3"),
        planned("open.cherries", Scope::Status, "Open the cherry view", "R-7.2"),
        planned("open.patch-apply", Scope::Status, "Apply a patch", "R-8.2"),
        planned("open.patch-create", Scope::Status, "Create a patch", "R-8.2"),
        planned("open.run", Scope::Status, "Run a Git command", "R-8.2"),
        planned("open.process-log", Scope::Status, "Open the process log", "R-3.3"),
        planned(
            "select.toggle-mark",
            Scope::Select,
            "Toggle the mark on the thing at point",
            "R-5.1",
        ),
        planned(
            "select.enter-rev",
            Scope::Select,
            "Enter a revision by hand",
            "R-5.1",
        ),
        planned("menu.save-args", Scope::Menu, "Save the menu arguments", "R-2.7"),
        planned("menu.reset-args", Scope::Menu, "Reset the menu arguments", "R-2.7"),
        planned(
            "commit-message.history-prev",
            Scope::CommitMessage,
            "Recall the previous commit message",
            "R-4.2",
        ),
        planned(
            "commit-message.history-next",
            Scope::CommitMessage,
            "Recall the next commit message",
            "R-4.2",
        ),
        planned(
            "commit-message.add-signoff",
            Scope::CommitMessage,
            "Add a Signed-off-by trailer",
            "R-4.2",
        ),
    ]);

    Registry { commands }
}

fn refresh(_ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    Ok(CommandOutcome {
        refresh: true,
        cursor: None,
    })
}

/// Cancel has nothing to cancel yet, so `Escape`/`ctrl+c` fall through when
/// the user has a visual selection; otherwise it is a quiet no-op.
fn cancel(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    if ctx.selection.is_some() {
        let key = ctx.key.clone().unwrap_or_else(|| "Escape".to_string());
        ctx.extend(vec![Effect::PassThrough { key }]);
    }
    Ok(CommandOutcome::default())
}

fn apply_visibility(ctx: &mut CommandCtx<'_>, command: VisibilityCommand) -> Result<CommandOutcome> {
    // Resolve against the acked tree, like every other action.
    let source = ctx
        .view
        .acked_tree()
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

fn next_section(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    Ok(CommandOutcome {
        refresh: false,
        cursor: section_cursor(ctx.view, ctx.cursor, true),
    })
}

fn prev_section(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    Ok(CommandOutcome {
        refresh: false,
        cursor: section_cursor(ctx.view, ctx.cursor, false),
    })
}

fn stage(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    stage_or_unstage(ctx, dispatch::Command::Stage)
}

fn unstage(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    stage_or_unstage(ctx, dispatch::Command::Unstage)
}

fn stage_or_unstage(ctx: &mut CommandCtx<'_>, command: dispatch::Command) -> Result<CommandOutcome> {
    let selection = ctx.selection.map(|selection| dispatch::LineRange {
        start: selection.start_line as u32,
        end: selection.end_line as u32,
    });
    let effects = stage_or_unstage_at(ctx.view, ctx.repo_path, command, ctx.cursor, selection)?;
    ctx.extend(effects);
    Ok(CommandOutcome {
        refresh: true,
        cursor: None,
    })
}

fn stage_all(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    let effects = run_resolution(ctx.repo_path, dispatch::Resolution::StageAllUnstaged)?;
    ctx.extend(effects);
    Ok(CommandOutcome {
        refresh: true,
        cursor: None,
    })
}

fn unstage_all(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    let effects = run_resolution(ctx.repo_path, dispatch::Resolution::UnstageAllStaged)?;
    ctx.extend(effects);
    Ok(CommandOutcome {
        refresh: true,
        cursor: None,
    })
}

/// The cursor line of the next or previous section header.
fn section_cursor(view: &OpenView, cursor: Option<Cursor>, forward: bool) -> Option<Cursor> {
    let cursor = cursor?;
    let source = view.acked_tree().unwrap_or_else(|| view.tree());
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
) -> Result<Vec<Effect>> {
    // Resolve against the last tree the client acknowledged; actions are
    // addressed by the node there, never by index in whatever is current.
    let source = view.acked_tree().unwrap_or_else(|| view.tree());

    let resolution = if let Some(range) = selection {
        // A visual selection wins over the node under the cursor (IMM-2).
        dispatch::resolve(command, dispatch::NodeKind::Other, Some(range))
    } else {
        let Some(cursor) = cursor else {
            return Ok(vec![Effect::message(
                MessageLevel::Info,
                "Place the cursor on a change to stage or unstage.",
            )]);
        };
        let Some(node) = source
            .line_nodes
            .get(cursor.line)
            .and_then(|line| source.node(&line.node))
        else {
            return Ok(vec![Effect::message(
                MessageLevel::Info,
                "No change under the cursor.",
            )]);
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
                    return Ok(vec![Effect::message(
                        MessageLevel::Warn,
                        errors::HUNK_CHANGED,
                    )])
                }
            }
        };

        // An untracked file has no index entry to remove.
        if command == dispatch::Command::Unstage && node.section.as_deref() == Some("untracked") {
            return Ok(vec![Effect::message(
                MessageLevel::Info,
                dispatch::NOTHING_TO_UNSTAGE,
            )]);
        }

        dispatch::resolve(command, dispatch_target(node), None)
    };

    let repo = GitRepo::open(repo_path)?;
    let layouts = match &resolution {
        dispatch::Resolution::Selection { start, end, .. } => layouts_for_selection(
            &repo,
            source,
            *start,
            *end,
            command == dispatch::Command::Unstage,
        )?,
        _ => Vec::new(),
    };
    dispatch::execute(&repo, &resolution, &layouts)
}

/// Run a whole-section operation against `repo_path`.
fn run_resolution(repo_path: &str, resolution: dispatch::Resolution) -> Result<Vec<Effect>> {
    let repo = GitRepo::open(repo_path)?;
    dispatch::execute(&repo, &resolution, &[])
}

/// Parse the file layouts a document-line selection spans.
///
/// Only files in the section the command acts on are diffed, so staging a
/// selection in the unstaged section never reads the index side.
fn layouts_for_selection(
    repo: &GitRepo,
    tree: &Tree,
    start: u32,
    end: u32,
    staged: bool,
) -> Result<Vec<FileLayout>> {
    let (start, end) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    let section = tree
        .line_nodes
        .get(start as usize)
        .or_else(|| tree.line_nodes.get(end as usize))
        .and_then(|line| line.section.as_deref());
    let Some(section) = section else {
        return Ok(Vec::new());
    };
    if section != if staged { "staged" } else { "unstaged" } {
        return Ok(Vec::new());
    }

    let mut layouts = Vec::new();
    for node in tree
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::File && node.section.as_deref() == Some(section))
    {
        let first = node.start_line as u32;
        let last = node.end_line as u32;
        if end < first || start > last {
            continue;
        }
        let Some(path) = node.path.as_deref() else {
            continue;
        };
        let diff = if staged {
            repo.diff_staged(path)?
        } else {
            repo.diff_unstaged(path)?
        };
        if let Some(layout) = FileLayout::from_diff(&diff, node.start_line as u32) {
            layouts.push(layout);
        }
    }
    Ok(layouts)
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
