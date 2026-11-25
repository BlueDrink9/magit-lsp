//! Render the status view: `Status` → node tree → text and structure.
//!
//! The text format is the one the Neovim client has always shown, so clients
//! only replace buffer lines and never compose the view themselves.

use crate::git::{FileEntry, GitRepo, RebaseState, RenameOrigins, Status};
use crate::patch::Hunk;
use crate::tree::{
    last_nonblank, LineNode, Link, Node, NodeKind, Symbol, Token, Tree, Visibility, VisibilityState,
};
use crate::view::{Ctx, Scope, View, ViewKind, VisibilityCommand};
use crate::Result;
use std::collections::HashMap;

/// The shallowest visibility level: section headers only.
const LEVEL_MIN: u8 = 1;
/// The deepest visibility level: sections, files, hunks and hunk bodies.
const LEVEL_MAX: u8 = 4;
/// The level the synchronous status open starts at, which matches the old full
/// render (and is what Plain rendering's section bound is defined against).
const LEVEL_DEFAULT: u8 = LEVEL_MAX;
/// The level a deferred interactive opening starts at: section headers only.
///
/// Files and their diffs stay closed until the user opens a section; the
/// per-file hunks are then loaded on demand at render time.
const LEVEL_OPENING: u8 = LEVEL_MIN;

/// How long a deferred opening's background hunk fetch may take after the
/// closed render is published before the sections stay closed on demand.
///
/// The deadline is user-directed (TASKS.md `UI-STATUS-DIFFS-LAZY`): a fetch
/// that completes within this window opens the diffs by default, exactly like
/// the synchronous fully-expanded render; a later completion is discarded.
pub const OPEN_PROMISE: std::time::Duration = std::time::Duration::from_millis(500);

/// Whether a fetch completed inside the open promise that started at `published`.
///
/// The comparison is inclusive so the boundary itself still opens the diffs.
pub fn open_within_promise(published: std::time::Instant, completed: std::time::Instant) -> bool {
    completed.saturating_duration_since(published) <= OPEN_PROMISE
}

/// A live status view: the repository root plus the last read status.
pub struct StatusView {
    repo: GitRepo,
    state: StatusState,
    generation: u64,
    pending_refresh: bool,
    /// Hunks loaded for visible files this refresh, keyed by `(path, staged)`.
    hunks: HashMap<(String, bool), Vec<Hunk>>,
    /// Rename origins for the current status read, scanned once per side.
    renames: RenameMaps,
    /// Per-view visibility, keyed by node id so it survives a refresh.
    visibility: Visibility,
    /// The last visibility level set by `more-detail`/`less-detail`.
    level: u8,
    /// Whether a deferred opening still awaits its background hunk fetch.
    ///
    /// Cleared by the first visibility command (or by the fetch installing), so
    /// the fast-path expansion never overrides a user-opened section.
    auto_expand: bool,
    /// Optional maximum rendered line count for a section.
    section_line_limit: Option<usize>,
}

enum StatusState {
    /// No status acquired yet; `branch` lets the early render show `Head:` when known.
    Loading {
        branch: Option<String>,
    },
    Failed(String),
    Ready {
        status: Status,
        rebase: Option<RebaseState>,
    },
}

/// Git state acquired for a particular status-view generation.
pub struct FreshStatus {
    generation: u64,
    status: Status,
    rebase: Option<RebaseState>,
    /// Rename origins for this status read, scanned once per side.
    renames: RenameMaps,
}

/// A background hunk fetch requested by a deferred opening's closed render.
pub struct TimedFetch {
    generation: u64,
    files: Vec<(String, bool)>,
}

/// Hunks fetched in the background for one still-current deferred generation.
pub struct TimedHunks {
    generation: u64,
    hunks: HashMap<(String, bool), Vec<Hunk>>,
    renames: RenameMaps,
}

/// Snapshot-scoped rename origin maps, one whole-repository scan per side.
///
/// They are replaced with the hunks on every installed status read, so a map
/// never outlives the Git state it was scanned from.
#[derive(Default)]
struct RenameMaps {
    staged: Option<RenameOrigins>,
    unstaged: Option<RenameOrigins>,
}

impl RenameMaps {
    /// The origin map for one side, scanning the repository only on first use.
    fn origins(&mut self, repo: &GitRepo, staged: bool) -> Result<&RenameOrigins> {
        let slot = if staged {
            &mut self.staged
        } else {
            &mut self.unstaged
        };
        if slot.is_none() {
            *slot = Some(repo.rename_origins(staged)?);
        }
        // The scan above fills the slot, so the lookup always succeeds.
        slot.as_ref()
            .ok_or_else(|| crate::Error::new("rename origin map was not scanned"))
    }
}

/// The opening visibility: every file section starts closed.
///
/// The states are recorded explicitly rather than left implicit, because an
/// unknown node is expanded: without them a refresh that adds a section would
/// render it expanded.
fn default_visibility() -> Visibility {
    let mut visibility = Visibility::new();
    for section in ["staged", "unstaged", "untracked"] {
        visibility.set(format!("section:{section}"), VisibilityState::Collapsed);
    }
    // The rebase section has no files to load, so it opens with the view.
    visibility.set("section:rebase", VisibilityState::Expanded);
    visibility
}

impl StatusView {
    /// Read the repository status and prepare a view for it.
    pub fn open(repo_path: &str) -> Result<Self> {
        let repo = GitRepo::open(repo_path)?;
        Self::open_repo(repo)
    }

    /// Read status from an already discovered repository.
    pub fn open_repo(repo: GitRepo) -> Result<Self> {
        let mut view = Self::loading(repo);
        let fresh = view.acquire_fresh_state()?;
        view.install_fresh_state(fresh);
        Ok(view)
    }

    /// Create a status view without acquiring repository status or rebase state.
    pub fn loading(repo: GitRepo) -> Self {
        Self {
            repo,
            state: StatusState::Loading { branch: None },
            generation: 0,
            pending_refresh: false,
            hunks: HashMap::new(),
            renames: RenameMaps::default(),
            visibility: Visibility::new(),
            level: LEVEL_DEFAULT,
            auto_expand: false,
            section_line_limit: None,
        }
    }

    /// Create a loading status view whose early render can show `Head: <branch>`.
    ///
    /// This is the deferred interactive opening: when the acquisition installs,
    /// the first full render shows only the closed section headers.
    pub fn loading_with_branch(repo: GitRepo, branch: impl Into<String>) -> Self {
        let mut view = Self::loading(repo);
        view.state = StatusState::Loading {
            branch: Some(branch.into()),
        };
        view.level = LEVEL_OPENING;
        view.visibility = default_visibility();
        view.auto_expand = true;
        view
    }

    /// Acquire current Git state tagged with this view's lifecycle generation.
    pub fn acquire_fresh_state(&self) -> Result<FreshStatus> {
        Self::acquire_for_generation(&self.repo, self.generation)
    }

    /// Acquire Git state without holding a session lock, tagged with its view generation.
    pub fn acquire_for_generation(repo: &GitRepo, generation: u64) -> Result<FreshStatus> {
        let status_started = std::time::Instant::now();
        let status = repo.status_listing()?;
        crate::runner::trace_current(
            "phase_status_listing",
            serde_json::json!({ "duration_ms": status_started.elapsed().as_secs_f64() * 1000.0 }),
        );
        let rebase_started = std::time::Instant::now();
        let rebase = repo.rebase_state_from_status(&status)?;
        crate::runner::trace_current(
            "phase_rebase_state",
            serde_json::json!({ "duration_ms": rebase_started.elapsed().as_secs_f64() * 1000.0 }),
        );
        Ok(FreshStatus {
            generation,
            status,
            rebase,
            renames: RenameMaps::default(),
        })
    }

    /// Install acquired state only if no refresh has superseded its generation.
    pub fn install_fresh_state(&mut self, fresh: FreshStatus) -> bool {
        if fresh.generation != self.generation {
            return false;
        }
        self.state = StatusState::Ready {
            status: fresh.status,
            rebase: fresh.rebase,
        };
        // Each install starts a new snapshot: drop hunks loaded for the old
        // Git state so the next render reloads them from the current one.
        self.hunks.clear();
        self.renames = fresh.renames;
        true
    }

    /// The files a deferred opening still needs for its background fetch.
    ///
    /// A refresh renews the generation and exposes a new fetch here, whose
    /// promise starts at the refresh render's publish instant. Returns `None`
    /// once the opening default has been consumed or overridden, so a
    /// user-opened section is never expanded on the fetch's behalf.
    pub fn timed_fetch(&self) -> Option<TimedFetch> {
        if !self.auto_expand || !matches!(self.state, StatusState::Ready { .. }) {
            return None;
        }
        let files = self
            .file_keys()
            .into_iter()
            .filter(|(section, _)| section != "untracked")
            .map(|(section, path)| (path, section == "staged"))
            .collect();
        Some(TimedFetch {
            generation: self.generation,
            files,
        })
    }

    /// Fetch the hunks for a deferred opening off the session lock.
    ///
    /// Failures stay per file (an empty body) exactly like the on-demand fetch,
    /// so one unreadable diff cannot cancel the whole fast path.
    pub fn fetch_timed_hunks(repo: &GitRepo, fetch: TimedFetch) -> TimedHunks {
        let mut renames = RenameMaps::default();
        let mut hunks = HashMap::with_capacity(fetch.files.len());
        for (path, staged) in fetch.files {
            let loaded = match renames
                .origins(repo, staged)
                .and_then(|origins| repo.file_hunks_with_origins(&path, staged, origins))
            {
                Ok(hunks) => hunks,
                Err(error) => {
                    tracing::warn!(path = %path, %error, "failed to load background file body");
                    Vec::new()
                }
            };
            hunks.insert((path, staged), loaded);
        }
        TimedHunks {
            generation: fetch.generation,
            hunks,
            renames,
        }
    }

    /// Install a background fetch only into its still-current deferred opening.
    ///
    /// The fetch beat the promise, so this reproduces the synchronous
    /// fully-expanded render (level `LEVEL_MAX`) and opens the three sections
    /// that still carried the opening Collapsed default. A superseded
    /// generation or a user visibility command discards it silently.
    pub fn install_timed_hunks(&mut self, timed: TimedHunks) -> bool {
        if timed.generation != self.generation
            || !self.auto_expand
            || !matches!(self.state, StatusState::Ready { .. })
        {
            return false;
        }
        self.hunks = timed.hunks;
        self.renames = timed.renames;
        self.level = LEVEL_MAX;
        for section in ["staged", "unstaged", "untracked"] {
            self.visibility
                .set(format!("section:{section}"), VisibilityState::Expanded);
        }
        self.auto_expand = false;
        true
    }

    /// End loading with a readable error if this acquisition is still current.
    pub fn fail_loading(&mut self, message: String, generation: u64) -> bool {
        if self.generation != generation || !matches!(self.state, StatusState::Loading { .. }) {
            return false;
        }
        self.state = StatusState::Failed(message);
        true
    }

    /// Whether this view accepts state-dependent actions.
    ///
    /// An acquisition still in flight has not produced their state yet, so they
    /// wait for it. A failed acquisition is terminal: nothing will arrive to
    /// make the view ready, so actions must proceed and report their own errors
    /// (or refresh) instead of being refused forever.
    pub fn is_ready(&self) -> bool {
        !matches!(self.state, StatusState::Loading { .. })
    }

    /// Coalesce refresh requests that arrive while an acquisition is in flight.
    pub fn queue_refresh_while_loading(&mut self) -> Option<u64> {
        if !matches!(self.state, StatusState::Loading { .. }) {
            return None;
        }
        if !self.pending_refresh {
            self.generation = self.generation.wrapping_add(1);
            self.pending_refresh = true;
        }
        Some(self.generation)
    }

    /// Take the single queued follow-up when the completed acquisition is stale.
    pub fn take_pending_refresh(&mut self, completed_generation: u64) -> Option<u64> {
        if self.pending_refresh && self.generation != completed_generation {
            self.pending_refresh = false;
            Some(self.generation)
        } else {
            None
        }
    }

    /// Render the visible tree, loading hunks only for expanded files.
    fn render_tree(&mut self) -> Tree {
        if let StatusState::Failed(message) = &self.state {
            let mut tree = Tree {
                version: 0,
                lines: Vec::new(),
                nodes: Vec::new(),
                line_nodes: Vec::new(),
                tokens: Vec::new(),
                symbols: Vec::new(),
                links: Vec::new(),
            };
            tree.lines.push(message.clone());
            tree.line_nodes
                .push(LineNode::new("status:error", NodeKind::Message));
            tree.nodes
                .push(Node::new("status:error", NodeKind::Message, message, 0));
            return tree;
        }
        let StatusState::Ready { status, rebase } = &self.state else {
            let branch = match &self.state {
                StatusState::Loading { branch } => branch.clone(),
                _ => None,
            };
            let mut tree = Tree {
                version: 0,
                lines: Vec::new(),
                nodes: Vec::new(),
                line_nodes: Vec::new(),
                tokens: Vec::new(),
                symbols: Vec::new(),
                links: Vec::new(),
            };
            if let Some(branch) = branch {
                push_head_header(&mut tree, &branch);
            }
            tree.lines.push("Loading repository status…".to_string());
            tree.line_nodes
                .push(LineNode::new("status:loading", NodeKind::Message));
            tree.nodes.push(Node::new(
                "status:loading",
                NodeKind::Message,
                "Loading repository status…",
                0,
            ));
            return tree;
        };
        for (section, path) in self.file_keys() {
            if section == "untracked"
                || !self.visibility.is_expanded(&format!("section:{section}"))
                || !self.visibility.is_expanded(&format!("{section}/{path}"))
            {
                continue;
            }
            let key = (path.clone(), section == "staged");
            if !self.hunks.contains_key(&key) {
                let hunk_started = std::time::Instant::now();
                let loaded =
                    match self.renames.origins(&self.repo, key.1).and_then(|origins| {
                        self.repo.file_hunks_with_origins(&path, key.1, origins)
                    }) {
                        Ok(hunks) => hunks,
                        Err(error) => {
                            tracing::warn!(path = %path, %error, "failed to load file body");
                            Vec::new()
                        }
                    };
                crate::runner::trace_current(
                    "phase_hunk_acquisition",
                    serde_json::json!({ "duration_ms": hunk_started.elapsed().as_secs_f64() * 1000.0, "path": path, "staged": key.1 }),
                );
                self.hunks.insert(key.clone(), loaded);
            }
            if self.level < 4 {
                for hunk in self.hunks.get(&key).into_iter().flatten() {
                    let id = format!("{section}/{path}/hunk:{}", hunk.id);
                    // The level only supplies a default for hunks whose body
                    // the user has not explicitly shown or hidden.
                    if self.visibility.explicit_state(&id).is_none() {
                        self.visibility.set(id, VisibilityState::Collapsed);
                    }
                }
            }
        }
        let visibility = &self.visibility;
        let hunks = &mut self.hunks;
        let mut tree = build_with_limit(
            status,
            visibility,
            self.section_line_limit,
            |section, file| {
                if section == "untracked" {
                    return Vec::new();
                }
                let staged = section == "staged";
                let key = (file.path.clone(), staged);
                hunks.get(&key).cloned().unwrap_or_default()
            },
        );
        add_rebase(&mut tree, *rebase);
        tree
    }

    /// All `(section, path)` pairs in the listing, cloned so callers may mutate.
    fn file_keys(&self) -> Vec<(String, String)> {
        ["staged", "unstaged", "untracked"]
            .iter()
            .flat_map(|section| {
                self.entries(section)
                    .iter()
                    .map(move |file| (section.to_string(), file.path.clone()))
            })
            .collect()
    }

    fn entries(&self, section: &str) -> &[FileEntry] {
        let StatusState::Ready { status, .. } = &self.state else {
            return &[];
        };
        match section {
            "staged" => &status.staged,
            "unstaged" => &status.unstaged,
            _ => &status.untracked,
        }
    }

    /// The hunk node ids loaded for one file, if its body was ever visible.
    fn hunk_ids(&self, section: &str, path: &str) -> Vec<String> {
        let key = (path.to_string(), section == "staged");
        self.hunks
            .get(&key)
            .map(|hunks| {
                hunks
                    .iter()
                    .map(|hunk| format!("{section}/{path}/hunk:{}", hunk.id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Apply a `more-detail`/`less-detail` level to every known node.
    fn apply_level(&mut self, level: u8) {
        self.level = level.clamp(LEVEL_MIN, LEVEL_MAX);
        let level = self.level;
        let section_state = if level >= 2 {
            VisibilityState::Expanded
        } else {
            VisibilityState::Collapsed
        };
        let hunk_state = if level >= 3 {
            VisibilityState::Expanded
        } else {
            VisibilityState::Collapsed
        };
        let body_state = if level >= 4 {
            VisibilityState::Expanded
        } else {
            VisibilityState::Collapsed
        };
        for section in ["staged", "unstaged", "untracked"] {
            self.visibility
                .set(format!("section:{section}"), section_state);
        }
        for (section, path) in self.file_keys() {
            self.visibility.set(format!("{section}/{path}"), hunk_state);
            for hunk_id in self.hunk_ids(&section, &path) {
                self.visibility.set(hunk_id, body_state);
            }
        }
    }

    /// Toggle/show/hide the node at `line`, optionally with its descendants.
    fn apply_at(
        &mut self,
        tree: &Tree,
        line: Option<usize>,
        state: Option<VisibilityState>,
        recursive: bool,
    ) -> bool {
        let Some(node) = line.and_then(|line| {
            tree.line_nodes
                .get(line)
                .and_then(|entry| tree.node(&entry.node))
        }) else {
            return false;
        };
        let Some(target) = tree.collapsible_ancestor(&node.id) else {
            return false;
        };
        let state = state.unwrap_or_else(|| {
            let displayed_collapsed = target.children.iter().any(|child_id| {
                tree.node(child_id)
                    .is_some_and(|child| child.kind == NodeKind::Message)
            });
            let expanded = self
                .visibility
                .explicit_state(&target.id)
                .map(|state| state == VisibilityState::Expanded)
                .unwrap_or(!displayed_collapsed);
            if expanded {
                VisibilityState::Collapsed
            } else {
                VisibilityState::Expanded
            }
        });
        self.visibility.set(target.id.clone(), state);
        if recursive {
            for id in self.descendants(target) {
                self.visibility.set(id, state);
            }
        }
        true
    }

    /// The collapsible descendants of `node`: files below a section, hunks
    /// below a file. Lines follow their hunk, so they carry no separate state.
    fn descendants(&self, node: &Node) -> Vec<String> {
        match node.kind {
            NodeKind::Section => {
                let section = node.section.as_deref().unwrap_or_default();
                self.file_keys()
                    .into_iter()
                    .filter(|(key, _)| key == section)
                    .flat_map(|(key, path)| {
                        let file_id = format!("{key}/{path}");
                        let mut ids = vec![file_id];
                        ids.extend(self.hunk_ids(&key, &path));
                        ids
                    })
                    .collect()
            }
            NodeKind::File => {
                let section = node.section.as_deref().unwrap_or_default();
                let path = node.path.as_deref().unwrap_or_default();
                self.hunk_ids(section, path)
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod reuse_tests {
    use super::*;
    use crate::view::{Ctx, View, Viewport, VisibilityCommand};
    use std::path::Path;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        assert!(Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap()
            .success());
    }

    fn committed_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        git(dir.path(), &["config", "user.email", "test@example.com"]);
        git(dir.path(), &["config", "user.name", "Test"]);
        std::fs::write(dir.path().join("tracked.txt"), "base\n").unwrap();
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-qm", "base"]);
        dir
    }

    #[test]
    fn loading_render_is_structured_and_fresh_state_install_is_generation_bound() {
        let dir = committed_repo();
        std::fs::write(dir.path().join("dirty.txt"), "not clean\n").unwrap();
        let repo = GitRepo::open(dir.path()).unwrap();
        let mut view = StatusView::loading_with_branch(repo, "main");
        assert!(!view.is_ready());
        let loading = view.render(&Ctx {
            viewport: Viewport::default(),
            pending: &[],
        });
        assert_eq!(loading.lines, ["Head: main", "Loading repository status…"]);
        assert_eq!(loading.line_nodes[0].node, "header");
        assert_eq!(loading.line_nodes[1].node, "status:loading");
        assert!(loading.links.is_empty());
        assert!(loading
            .nodes
            .iter()
            .all(|node| node.kind != NodeKind::Section));

        // A stale completion must not replace the newer loading generation: the
        // early Head render survives instead of the acquired file sections.
        let stale = view.acquire_fresh_state().unwrap();
        assert!(view.queue_refresh_while_loading().is_some());
        assert!(!view.install_fresh_state(stale));
        assert!(!view.is_ready());
        let preserved = view.render(&Ctx {
            viewport: Viewport::default(),
            pending: &[],
        });
        assert_eq!(preserved.lines, loading.lines);

        view.on_refresh().unwrap();
        assert!(view.is_ready());
        let live = view.render(&Ctx {
            viewport: Viewport::default(),
            pending: &[],
        });
        // The deferred install publishes the closed render: headers and counts
        // only, with no file row and no hunk.
        assert!(live.lines[0].starts_with("Head: "), "{:?}", live.lines);
        let body: Vec<&str> = live.lines[1..].iter().map(String::as_str).collect();
        assert_eq!(
            body,
            [
                "",
                "Staged changes (0)",
                "",
                "Unstaged changes (0)",
                "",
                "Untracked files (1)",
            ]
        );
        // Opening the section renders its files.
        let section = live.node("section:untracked").unwrap().start_line;
        assert!(view.set_visibility(
            &live,
            Some(section),
            VisibilityCommand::Show { recursive: false }
        ));
        let opened = view.render(&Ctx {
            viewport: Viewport::default(),
            pending: &[],
        });
        assert!(opened.lines.iter().any(|line| line.contains("dirty.txt")));
        // A full open still matches the synchronous fully-expanded render.
        assert!(view.set_visibility(&opened, None, VisibilityCommand::ShowAll));
        let full = view.render(&Ctx {
            viewport: Viewport::default(),
            pending: &[],
        });
        let mut authoritative = StatusView::open(view.repo.root().to_str().unwrap()).unwrap();
        let expected = authoritative.render(&Ctx {
            viewport: Viewport::default(),
            pending: &[],
        });
        assert_eq!(full.lines, expected.lines);
        std::fs::write(dir.path().join("later.txt"), "new change\n").unwrap();
        view.on_refresh().unwrap();
        let refreshed = view.render(&Ctx {
            viewport: Viewport::default(),
            pending: &[],
        });
        assert!(refreshed
            .lines
            .iter()
            .any(|line| line.contains("later.txt")));
    }

    #[test]
    fn recursive_show_reveals_lazy_hunk_body_at_opening_level() {
        let dir = committed_repo();
        std::fs::write(dir.path().join("tracked.txt"), "changed\n").unwrap();
        let repo = GitRepo::open(dir.path()).unwrap();
        let mut view = StatusView::loading_with_branch(repo, "main");
        let fresh = view.acquire_fresh_state().unwrap();
        assert!(view.install_fresh_state(fresh));

        let ctx = Ctx {
            viewport: Viewport::default(),
            pending: &[],
        };
        let closed = view.render(&ctx);
        let section_line = closed.node("section:unstaged").unwrap().start_line;
        assert!(view.set_visibility(
            &closed,
            Some(section_line),
            VisibilityCommand::Show { recursive: false }
        ));
        // Rendering once loads the file's hunks; at the opening level their
        // bodies are still closed.
        let files = view.render(&ctx);
        assert!(files.lines.iter().any(|line| line.contains("tracked.txt")));
        assert!(!files.nodes.iter().any(|node| node.kind == NodeKind::Line));

        // The hunks are known now, so a recursive Show reaches them.
        let file_line = files.node("unstaged/tracked.txt").unwrap().start_line;
        assert!(view.set_visibility(
            &files,
            Some(file_line),
            VisibilityCommand::Show { recursive: true }
        ));
        let revealed = view.render(&ctx);
        assert!(
            revealed
                .nodes
                .iter()
                .any(|node| node.kind == NodeKind::Line),
            "hunk body not revealed: {:?}",
            revealed.lines
        );
        let again = view.render(&ctx);
        assert!(
            again.nodes.iter().any(|node| node.kind == NodeKind::Line),
            "hunk body did not persist: {:?}",
            again.lines
        );
    }

    #[test]
    fn open_promise_boundary_opens_within_and_rejects_late_completions() {
        let published = std::time::Instant::now();
        assert!(open_within_promise(published, published));
        assert!(open_within_promise(published, published + OPEN_PROMISE));
        assert!(!open_within_promise(
            published,
            published + OPEN_PROMISE + std::time::Duration::from_millis(1)
        ));
    }

    #[test]
    fn default_visibility_expands_rebase_section() {
        let visibility = default_visibility();
        assert_eq!(
            visibility.explicit_state("section:rebase"),
            Some(VisibilityState::Expanded)
        );
    }

    #[test]
    fn current_generation_fresh_state_installs_and_closed_view_is_not_reinstallable() {
        let dir = committed_repo();
        let view = StatusView::loading(GitRepo::open(dir.path()).unwrap());
        let result = view.acquire_fresh_state().unwrap();
        let mut live = Some(view);
        assert!(live.as_mut().unwrap().install_fresh_state(result));
        assert!(live.as_ref().unwrap().is_ready());
        // Closing drops the only install destination; a delayed completion has no path back in.
        let delayed = live.as_ref().unwrap().acquire_fresh_state().unwrap();
        drop(live.take());
        assert!(live.is_none());
        drop(delayed);
    }

    #[test]
    fn open_repo_uses_discovered_root_and_refreshes_live_status_without_rediscovery() {
        let dir = committed_repo();
        let nested = dir.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        let discovered = GitRepo::open(&nested).unwrap();
        // Both the recorded cwd and the repository identity are our canonical
        // form (`crate::path::canonical`), so compare like with like: a std
        // canonicalize here would be verbatim on Windows and never match.
        let root = discovered.root().to_path_buf();
        let before = crate::runner::global_log()
            .iter()
            .last()
            .map(|record| record.id)
            .unwrap_or(0);
        let mut view = StatusView::open_repo(discovered).unwrap();
        let new_records = crate::runner::global_log()
            .iter()
            .into_iter()
            .filter(|record| record.id > before && record.cwd == root.display().to_string())
            .collect::<Vec<_>>();
        assert!(!new_records
            .iter()
            .any(|record| record.args.first().is_some_and(|arg| arg == "rev-parse")));
        assert_eq!(view.repo.root(), root.as_path());
        let first = view.render(&Ctx {
            viewport: Viewport::default(),
            pending: &[],
        });
        assert!(!first.lines.iter().any(|line| line.contains("tracked.txt")));
        std::fs::write(dir.path().join("tracked.txt"), "changed\n").unwrap();
        view.on_refresh().unwrap();
        let refreshed = view.render(&Ctx {
            viewport: Viewport::default(),
            pending: &[],
        });
        assert!(refreshed
            .lines
            .iter()
            .any(|line| line.contains("tracked.txt")));
    }

    #[test]
    fn opened_linked_worktree_keeps_its_worktree_root() {
        let main = committed_repo();
        let linked = tempfile::tempdir().unwrap();
        git(
            main.path(),
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "linked-test",
                linked.path().to_str().unwrap(),
            ],
        );
        let repo = GitRepo::open(linked.path()).unwrap();
        assert!(
            crate::path::same_object(repo.root(), linked.path()).unwrap(),
            "discovered root {:?} is not the linked worktree {:?}",
            repo.root(),
            linked.path()
        );
        let view = StatusView::open_repo(repo).unwrap();
        assert!(
            crate::path::same_object(view.repo.root(), linked.path()).unwrap(),
            "view root {:?} is not the linked worktree {:?}",
            view.repo.root(),
            linked.path()
        );
    }
}

impl View for StatusView {
    fn kind(&self) -> ViewKind {
        ViewKind::Status
    }

    fn is_ready(&self) -> bool {
        StatusView::is_ready(self)
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn title(&self) -> String {
        "Status".to_string()
    }

    fn scope(&self) -> Scope {
        Scope::new("status")
    }

    fn render(&mut self, _ctx: &Ctx<'_>) -> Tree {
        self.render_tree()
    }

    fn set_section_line_limit(&mut self, limit: Option<usize>) -> bool {
        self.section_line_limit = limit.filter(|limit| *limit > 0);
        true
    }

    fn on_refresh(&mut self) -> Result<()> {
        self.generation = self.generation.wrapping_add(1);
        let fresh = self.acquire_fresh_state()?;
        self.install_fresh_state(fresh);
        Ok(())
    }

    fn set_visibility(
        &mut self,
        tree: &Tree,
        line: Option<usize>,
        command: VisibilityCommand,
    ) -> bool {
        // Any explicit visibility action takes ownership of the fold state, so
        // the deferred opening's background fetch must not expand it later.
        self.auto_expand = false;
        match command {
            VisibilityCommand::ShowAll => {
                self.visibility.show_all();
                // A Plain size summary is renderer-imposed, not a stored
                // collapsed override; show-all must explicitly reveal it.
                for section in tree
                    .nodes
                    .iter()
                    .filter(|node| node.kind == NodeKind::Section)
                {
                    self.visibility
                        .set(section.id.clone(), VisibilityState::Expanded);
                }
                self.level = LEVEL_MAX;
                true
            }
            VisibilityCommand::HideAll => {
                self.apply_level(LEVEL_MIN);
                true
            }
            VisibilityCommand::MoreDetail => {
                self.apply_level(self.level.saturating_add(1));
                true
            }
            VisibilityCommand::LessDetail => {
                self.apply_level(self.level.saturating_sub(1));
                true
            }
            VisibilityCommand::SetDetail(level) => {
                self.apply_level(level);
                true
            }
            VisibilityCommand::Show { recursive } => {
                self.apply_at(tree, line, Some(VisibilityState::Expanded), recursive)
            }
            VisibilityCommand::Hide { recursive } => {
                self.apply_at(tree, line, Some(VisibilityState::Collapsed), recursive)
            }
            VisibilityCommand::Toggle { recursive } => self.apply_at(tree, line, None, recursive),
        }
    }
}

/// Build the status view tree from a Git status.
pub fn build(status: &Status) -> Tree {
    // The tree-only builder keeps the old fully-expanded rendering.
    build_with(status, &Visibility::new(), |section, file| {
        if section == "untracked" {
            Vec::new()
        } else {
            file.hunks.clone()
        }
    })
}

fn add_rebase(tree: &mut Tree, state: Option<RebaseState>) {
    let Some(state) = state else {
        return;
    };
    let id = "section:rebase";
    let line = tree.lines.len();
    let label = if state.conflicted {
        "Rebase (conflicted)"
    } else {
        "Rebase (active)"
    };
    tree.lines.push(label.to_string());
    tree.line_nodes.push(LineNode::new(id, NodeKind::Section));
    let mut section = Node::new(id, NodeKind::Section, "Rebase", line);
    section.section = Some("rebase".to_string());
    let mut children = Vec::new();
    for (action, available) in [
        ("continue", !state.conflicted),
        ("skip", true),
        ("abort", true),
    ] {
        let child_id = format!("rebase:{action}");
        let child_line = tree.lines.len();
        let text = format!(
            "  {action}: {}",
            if available {
                "available"
            } else {
                "unavailable (resolve conflicts)"
            }
        );
        tree.lines.push(text.clone());
        tree.line_nodes
            .push(LineNode::new(&child_id, NodeKind::Message));
        let mut child = Node::new(&child_id, NodeKind::Message, text, child_line);
        child.section = Some(if available { action } else { "unavailable" }.to_string());
        child.status = Some("rebase_operation".to_string());
        tree.nodes.push(child);
        children.push(child_id);
    }
    section.children = children;
    section.end_line = tree.lines.len() - 1;
    tree.nodes.push(section);
}

/// Build the status view tree, using `visibility` and loading hunks only for
/// files whose body is visible.
pub(crate) fn build_with<F>(status: &Status, visibility: &Visibility, mut load: F) -> Tree
where
    F: FnMut(&str, &FileEntry) -> Vec<Hunk>,
{
    build_with_limit(status, visibility, None, &mut load)
}

/// Build with an optional rendered-line bound per section.
pub fn build_with_limit<F>(
    status: &Status,
    visibility: &Visibility,
    limit: Option<usize>,
    mut load: F,
) -> Tree
where
    F: FnMut(&str, &FileEntry) -> Vec<Hunk>,
{
    let full = Builder::new(visibility, &mut load, &HashMap::new()).build(status);
    let Some(limit) = limit else {
        return full;
    };
    let summaries: HashMap<String, usize> = full
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Section)
        .filter_map(|section| {
            if visibility.explicit_state(&section.id) == Some(VisibilityState::Expanded) {
                return None;
            }
            let body_lines = section.end_line.saturating_sub(section.start_line);
            (body_lines > limit).then(|| (section.section.clone().unwrap_or_default(), body_lines))
        })
        .collect();
    if summaries.is_empty() {
        return full;
    }
    let mut builder = Builder::new(visibility, &mut load, &summaries);
    builder.populate(status);
    builder.finish()
}

/// Append the `Head:` line and its stable `header` node to a status tree.
///
/// The early loading render and the full status render share this so the node
/// identity and text are byte-identical across the version they replace.
fn push_head_header(tree: &mut Tree, branch: &str) {
    let line = tree.lines.len();
    let branch = if branch.is_empty() { "HEAD" } else { branch };
    tree.lines.push(format!("Head: {branch}"));
    tree.line_nodes
        .push(LineNode::new("header", NodeKind::Header));
    tree.nodes
        .push(Node::new("header", NodeKind::Header, "Head", line));
}

struct Builder<'a, F> {
    tree: Tree,
    visibility: &'a Visibility,
    load: &'a mut F,
    summaries: &'a HashMap<String, usize>,
}

impl<'a, F> Builder<'a, F>
where
    F: FnMut(&str, &FileEntry) -> Vec<Hunk>,
{
    fn new(
        visibility: &'a Visibility,
        load: &'a mut F,
        summaries: &'a HashMap<String, usize>,
    ) -> Self {
        Self {
            tree: Tree {
                version: 0,
                lines: Vec::new(),
                nodes: Vec::new(),
                line_nodes: Vec::new(),
                tokens: Vec::new(),
                symbols: Vec::new(),
                links: Vec::new(),
            },
            visibility,
            load,
            summaries,
        }
    }

    fn build(mut self, status: &Status) -> Tree {
        self.populate(status);
        self.finish()
    }

    fn populate(&mut self, status: &Status) {
        self.header(status);
        self.section("staged", "Staged changes", &status.staged);
        self.section("unstaged", "Unstaged changes", &status.unstaged);
        self.section("untracked", "Untracked files", &status.untracked);
        if status.staged.is_empty() && status.unstaged.is_empty() && status.untracked.is_empty() {
            self.message();
        }
    }

    fn header(&mut self, status: &Status) {
        push_head_header(&mut self.tree, &status.branch);
    }

    fn section(&mut self, key: &str, title: &str, files: &[FileEntry]) {
        let section_id = format!("section:{key}");

        self.tree.lines.push(String::new());
        self.tree
            .line_nodes
            .push(LineNode::new(&section_id, NodeKind::Blank));

        let line = self.tree.lines.len();
        let text = format!("{title} ({})", files.len());
        self.tree.lines.push(text.clone());

        let mut entry = LineNode::new(&section_id, NodeKind::Section);
        entry.section = Some(key.to_string());
        self.tree.line_nodes.push(entry);

        let index = self.tree.nodes.len();
        let mut section = Node::new(&section_id, NodeKind::Section, title, line);
        section.section = Some(key.to_string());
        self.tree.nodes.push(section);

        self.tree.tokens.push(Token {
            line,
            start: 0,
            length: title.len(),
            token_type: "heading".to_string(),
            modifiers: Vec::new(),
        });
        self.tree.tokens.push(Token {
            line,
            start: title.len() + 1,
            length: text.len() - title.len() - 1,
            token_type: "muted".to_string(),
            modifiers: Vec::new(),
        });

        if let Some(hidden) = self.summaries.get(key).copied() {
            let id = format!("{section_id}:overflow");
            let text = format!("{hidden} more lines: expand");
            let row = self.tree.lines.len();
            self.tree.lines.push(text.clone());
            self.tree
                .line_nodes
                .push(LineNode::new(&id, NodeKind::Message));
            self.tree
                .nodes
                .push(Node::new(&id, NodeKind::Message, text, row));
            self.tree.nodes[index].children = vec![id];
        } else if self.visibility.is_expanded(&section_id) {
            let children = files
                .iter()
                .map(|file| self.file(key, file))
                .collect::<Vec<_>>();
            self.tree.nodes[index].children = children;
        }
    }

    fn file(&mut self, section: &str, file: &FileEntry) -> String {
        let file_id = format!("{section}/{}", file.path);
        let line = self.tree.lines.len();
        self.tree
            .lines
            .push(format!("  {} {}", file.status, file.path));

        let mut entry = LineNode::new(&file_id, NodeKind::File);
        entry.section = Some(section.to_string());
        entry.path = Some(file.path.clone());
        entry.status = Some(file.status.clone());
        self.tree.line_nodes.push(entry);

        let index = self.tree.nodes.len();
        let mut file_node = Node::new(&file_id, NodeKind::File, file.path.clone(), line);
        file_node.section = Some(section.to_string());
        file_node.path = Some(file.path.clone());
        file_node.status = Some(file.status.clone());
        file_node.mutation = file
            .mutation_identity
            .clone()
            .map(crate::tree::MutationContent::File);
        self.tree.nodes.push(file_node);

        let staged = section == "staged";
        let modifiers = if staged {
            vec!["staged".to_string()]
        } else {
            Vec::new()
        };
        let path_start = 2 + file.status.len() + 1;
        self.tree.tokens.push(Token {
            line,
            start: 2,
            length: file.status.len(),
            token_type: status_token(&file.status).to_string(),
            modifiers: modifiers.clone(),
        });
        self.tree.tokens.push(Token {
            line,
            start: path_start,
            length: file.path.len(),
            token_type: "path".to_string(),
            modifiers,
        });
        self.tree.links.push(Link {
            line,
            start: path_start,
            length: file.path.len(),
            path: file.path.clone(),
        });

        if self.visibility.is_expanded(&file_id) {
            let hunks = (self.load)(section, file);
            let children = hunks
                .iter()
                .map(|hunk| {
                    let identical = hunks
                        .iter()
                        .filter(|other| other.same_content(hunk))
                        .count();
                    self.hunk(section, file, &file_id, hunk, identical)
                })
                .collect::<Vec<_>>();
            self.tree.nodes[index].children = children;
        }
        file_id
    }

    fn hunk(
        &mut self,
        section: &str,
        file: &FileEntry,
        file_id: &str,
        hunk: &Hunk,
        identical: usize,
    ) -> String {
        let hunk_id = format!("{file_id}/hunk:{}", hunk.id);
        let line = self.tree.lines.len();
        let header = format!(
            "    @@ -{},{} +{},{} @@",
            hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
        );
        self.tree.lines.push(header.clone());

        let mut entry = LineNode::new(&hunk_id, NodeKind::Hunk);
        entry.section = Some(section.to_string());
        entry.path = Some(file.path.clone());
        entry.hunk_id = Some(hunk.id.clone());
        entry.new_line = Some(hunk.new_start);
        self.tree.line_nodes.push(entry);

        let index = self.tree.nodes.len();
        let mut hunk_node = Node::new(&hunk_id, NodeKind::Hunk, "@@", line);
        hunk_node.section = Some(section.to_string());
        hunk_node.path = Some(file.path.clone());
        hunk_node.hunk_id = Some(hunk.id.clone());
        hunk_node.new_line = Some(hunk.new_start);
        hunk_node.copy = Some(hunk_source_text(hunk));
        hunk_node.mutation = Some(crate::tree::MutationContent::Hunk {
            hunk: hunk.clone(),
            identical,
        });
        self.tree.nodes.push(hunk_node);

        let staged = section == "staged";
        let modifiers = if staged {
            vec!["staged".to_string()]
        } else {
            Vec::new()
        };
        self.tree.tokens.push(Token {
            line,
            start: 0,
            length: header.len(),
            token_type: "muted".to_string(),
            modifiers: modifiers.clone(),
        });

        // A removed line has no new-side line; it visits the new line before it.
        let mut last_new = hunk.new_start;
        let lines: &[_] = if self.visibility.is_expanded(&hunk_id) {
            &hunk.lines
        } else {
            &[]
        };
        let children = lines
            .iter()
            .enumerate()
            .map(|(offset, diff_line)| {
                let line = self.tree.lines.len();
                let text = format!("    {}{}", diff_line.marker, diff_line.text);
                self.tree.lines.push(text.clone());
                let id = format!("{hunk_id}/line:{offset}");

                let new_line = match diff_line.marker {
                    '+' | ' ' => diff_line.new_no.unwrap_or(last_new),
                    _ => last_new,
                };
                if matches!(diff_line.marker, '+' | ' ') {
                    last_new = new_line;
                }

                let mut entry = LineNode::new(&id, NodeKind::Line);
                entry.section = Some(section.to_string());
                entry.path = Some(file.path.clone());
                entry.hunk_id = Some(hunk.id.clone());
                entry.marker = Some(diff_line.marker);
                entry.line_index = Some(offset);
                entry.new_line = Some(new_line);
                entry.text = Some(diff_line.text.clone());
                self.tree.line_nodes.push(entry);

                let mut line_node = Node::new(&id, NodeKind::Line, diff_line.text.clone(), line);
                if matches!(diff_line.marker, '+' | '-' | ' ') {
                    line_node.copy = Some(diff_line.text.clone());
                }
                line_node.section = Some(section.to_string());
                line_node.path = Some(file.path.clone());
                line_node.hunk_id = Some(hunk.id.clone());
                line_node.marker = Some(diff_line.marker);
                line_node.line_index = Some(offset);
                line_node.new_line = Some(new_line);
                self.tree.nodes.push(line_node);

                let token_type = match diff_line.marker {
                    '+' => Some("added"),
                    '-' => Some("removed"),
                    _ => None,
                };
                if let Some(token_type) = token_type {
                    self.tree.tokens.push(Token {
                        line,
                        start: 0,
                        length: text.len(),
                        token_type: token_type.to_string(),
                        modifiers: modifiers.clone(),
                    });
                }
                id
            })
            .collect::<Vec<_>>();

        let end_line = self.tree.lines.len().saturating_sub(1);
        self.tree.nodes[index].end_line = end_line;
        self.tree.nodes[index].children = children;
        hunk_id
    }

    fn message(&mut self) {
        self.tree.lines.push(String::new());
        self.tree
            .line_nodes
            .push(LineNode::new("message:clean", NodeKind::Blank));

        let line = self.tree.lines.len();
        let text = "Working tree clean";
        self.tree.lines.push(text.to_string());
        self.tree
            .line_nodes
            .push(LineNode::new("message:clean", NodeKind::Message));
        self.tree
            .nodes
            .push(Node::new("message:clean", NodeKind::Message, text, line));
    }

    fn finish(mut self) -> Tree {
        self.close_ranges();
        self.tree.symbols = self.symbols();
        self.tree
    }

    /// Close section and file ranges at their last non-blank line.
    fn close_ranges(&mut self) {
        let section_starts: Vec<usize> = self
            .tree
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Section)
            .map(|node| node.start_line)
            .collect();
        let file_starts: Vec<usize> = self
            .tree
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::File)
            .map(|node| node.start_line)
            .collect();

        for index in 0..self.tree.nodes.len() {
            let kind = self.tree.nodes[index].kind;
            let start = self.tree.nodes[index].start_line;
            let next = match kind {
                NodeKind::Section => section_starts
                    .iter()
                    .find(|&&line| line > start)
                    .copied()
                    .unwrap_or(self.tree.lines.len()),
                NodeKind::File => {
                    let next_file = file_starts.iter().find(|&&line| line > start).copied();
                    let next_section = section_starts.iter().find(|&&line| line > start).copied();
                    match (next_file, next_section) {
                        (Some(a), Some(b)) => a.min(b),
                        (Some(a), None) => a,
                        (None, Some(b)) => b,
                        (None, None) => self.tree.lines.len(),
                    }
                }
                _ => continue,
            };
            if let Some(end) = last_nonblank(&self.tree.lines, start, next) {
                self.tree.nodes[index].end_line = end;
            }
        }
    }

    fn symbols(&self) -> Vec<Symbol> {
        let tree = &self.tree;
        let hunk_symbol = |hunk: &Node| Symbol {
            name: hunk.label.clone(),
            detail: None,
            kind: "hunk".to_string(),
            node: hunk.id.clone(),
            start_line: hunk.start_line,
            end_line: hunk.end_line,
            children: Vec::new(),
        };
        let file_symbol = |file: &Node| Symbol {
            name: file.label.clone(),
            detail: file.status.clone(),
            kind: "file".to_string(),
            node: file.id.clone(),
            start_line: file.start_line,
            end_line: file.end_line,
            children: file
                .children
                .iter()
                .filter_map(|id| tree.node(id))
                .filter(|node| node.kind == NodeKind::Hunk)
                .map(hunk_symbol)
                .collect(),
        };
        tree.nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Section)
            .map(|section| Symbol {
                name: section.label.clone(),
                detail: Some(section.children.len().to_string()),
                kind: "section".to_string(),
                node: section.id.clone(),
                start_line: section.start_line,
                end_line: section.end_line,
                children: section
                    .children
                    .iter()
                    .filter_map(|id| tree.node(id))
                    .filter(|node| node.kind == NodeKind::File)
                    .map(file_symbol)
                    .collect(),
            })
            .collect()
    }
}

/// Source content from each valid hunk line, without markers or metadata.
fn hunk_source_text(hunk: &Hunk) -> String {
    let mut text = String::new();
    let mut first = true;
    for line in &hunk.lines {
        if !matches!(line.marker, '+' | '-' | ' ') {
            continue;
        }
        if !first {
            text.push('\n');
        }
        text.push_str(&line.text);
        first = false;
    }
    text
}

fn status_token(status: &str) -> &'static str {
    match status {
        "A" => "added",
        "D" => "removed",
        "R" => "changed",
        "?" => "added",
        _ => "changed",
    }
}
