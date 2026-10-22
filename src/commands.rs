//! The command registry: every key action is a [`Command`].
//!
//! §3.5 makes handlers `async` with a `Ui` handle in R-3.1; until then they are
//! plain synchronous functions. [`CommandCtx`] carries the view, repo path,
//! cursor, selection, arguments and effects collector, and is the single place
//! new capabilities are added so that change stays mechanical.

use crate::dispatch;
use crate::errors;
use crate::git::GitRepo;
use crate::keymap::Keymap;
use crate::menu::Menu;
use crate::patch::FileLayout;
use crate::protocol::{Cursor, Effect, MessageLevel, Selection};
use crate::session::{re_resolve, OpenView};
use crate::tree::{Node, NodeKind, Tree};
use crate::view::{ViewMode, VisibilityCommand};
use crate::Result;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// The registry group a command belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Common,
    Status,
    Select,
    Menu,
    CommitMessage,
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
            Scope::Menu => "menu",
            Scope::CommitMessage => "commit-message",
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
            Scope::MenuAction => !view_scope.starts_with("menu"),
            other => other.as_str() == view_scope,
        }
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
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CommandOutcome {
    /// Re-read the backing state before rendering.
    pub refresh: bool,
    /// A cursor to move to that the old node cannot describe.
    pub cursor: Option<Cursor>,
    /// A mode change the session must apply (and push a keymap for).
    pub mode: Option<ViewMode>,
    /// Push a new menu view and return to the current one on `popView`.
    pub open_menu: Option<Menu>,
    /// Close the current view and return to the previous one.
    pub pop: bool,
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
        Command {
            id: "common.quit",
            scope: Scope::Common,
            doc: "Close all views",
            status: CommandStatus::Implemented,
            run: quit,
        },
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
        planned("common.visit", Scope::Common, "Visit the thing at point", "R-2.3"),
        planned("status.discard", Scope::Status, "Discard the change at point", "R-3.6"),
        planned("status.reverse", Scope::Status, "Reverse-apply the change at point", "R-3.6"),
        planned("status.apply", Scope::Status, "Apply the change at point", "R-8.2"),
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
        menu("open.branch", "Open the branch menu", open_branch),
        menu("open.log", "Open the log menu", open_log),
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
        menu("open.patch-apply", "Open the apply-patches menu", open_patch_apply),
        menu(
            "open.patch-create",
            "Open the format-patches menu",
            open_patch_create,
        ),
        planned("open.refs", Scope::Status, "Open the refs view", "R-5.3"),
        planned("open.cherries", Scope::Status, "Open the cherry view", "R-7.2"),
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

/// Open the dispatch menu, including from inside another menu.
fn open_dispatch(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    open_menu(ctx, "dispatch")
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

/// Copy the value at point as a clipboard effect, from the acked tree.
fn copy_value(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    let source = ctx
        .view
        .acked_tree()
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

/// Cancel closes a menu; elsewhere it falls through when the user has a visual
/// selection and is a quiet no-op otherwise.
fn cancel(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    if ctx.view.menu().is_some() {
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

/// Quit asks the client to close every view of the session.
fn quit(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    ctx.extend(vec![Effect::Quit]);
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
        cursor: section_cursor(ctx.view, ctx.cursor, true),
        ..Default::default()
    })
}

fn prev_section(ctx: &mut CommandCtx<'_>) -> Result<CommandOutcome> {
    Ok(CommandOutcome {
        cursor: section_cursor(ctx.view, ctx.cursor, false),
        ..Default::default()
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
