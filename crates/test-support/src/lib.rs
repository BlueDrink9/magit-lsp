//! Shared fixtures and helpers for the integration tests.
//!
//! Every git invocation made here first applies [`apply_determinism_env`], so
//! commit hashes and diff output are stable across machines.

#![allow(dead_code)]

pub mod client_pump;
pub mod editor_model;
pub mod fake_editor;
pub mod recording_prompt_ui;
pub mod recording_ui;

use magit_lsp::git::GitRepo;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, Once, OnceLock};
use tempfile::TempDir;

/// Ten numbered lines, the content most fixtures start from.
pub const TEN_LINES: &str =
    "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\n";

const FIXED_NAME: &str = "Test User";
const FIXED_EMAIL: &str = "test@example.com";
const FIXED_DATE: &str = "2000-01-01T00:00:00+0000";

static DETERMINISM: Once = Once::new();

/// An absolute path git accepts as an argument or environment value.
///
/// Canonicalization keeps paths clear of symlinked temp components (for example
/// macOS `/var`), while the shared helper avoids the verbatim `\\?\` prefix Git
/// for Windows rejects.
pub fn git_compatible_absolute(path: &Path) -> PathBuf {
    magit_lsp::path::canonical_or_raw(path)
}

/// Names Windows reserves for devices, compared against a segment's stem.
const WINDOWS_DEVICE_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "COM0", "COM1", "COM2", "COM3", "COM4",
    "COM5", "COM6", "COM7", "COM8", "COM9", "LPT0", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6",
    "LPT7", "LPT8", "LPT9",
];

/// Why `path` would fail a Windows Git or filesystem operation, if it would.
///
/// Windows reserves the filename characters `< > : " | ? *`, and Git for
/// Windows rejects the verbatim `\\?\` prefix that `canonicalize` returns, so a
/// fixture that only passes on Unix hides the failure. The first segment is a
/// root — empty on Unix (`/tmp/x`) or a drive (`C:`) on Windows, where the
/// colon is not a name character — and is exempt from the name check.
///
/// `check_name_chars` also rejects reserved characters in every path segment,
/// reserved device names (stem-matched, ASCII-case-insensitively, so `nul.txt`
/// and `COM3` are rejected while `console.txt` is not) and a segment ending in
/// a dot or a space.
/// Callers pass `false` for values that are not filenames (git arguments such
/// as commit messages or ignore patterns) so only the verbatim prefix is
/// rejected there.
///
/// See [Naming Files, Paths, and Namespaces].
///
/// [Naming Files, Paths, and Namespaces]:
///     https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file
pub fn windows_path_problem(path: &Path, check_name_chars: bool) -> Option<&'static str> {
    let text = path.to_string_lossy();
    if text.starts_with(r"\\?\") {
        return Some("verbatim `\\\\?\\` paths are rejected by Git for Windows");
    }
    if !check_name_chars {
        return None;
    }
    for (index, segment) in text.split(['/', '\\']).enumerate() {
        if index == 0 {
            let mut chars = segment.chars();
            let drive = matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
                && chars.next() == Some(':')
                && chars.next().is_none();
            if segment.is_empty() || drive {
                continue;
            }
        }
        if segment
            .chars()
            .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
        {
            return Some("windows-reserved filename character in path segment (`< > : \" | ? *`)");
        }
        let stem = segment.split('.').next().unwrap_or(segment);
        if WINDOWS_DEVICE_NAMES
            .iter()
            .any(|device| device.eq_ignore_ascii_case(stem))
        {
            return Some(
                "windows-reserved device name in path segment (`CON`, `NUL`, `COM1`, `LPT1`, ...)",
            );
        }
        if segment.ends_with('.') || segment.ends_with(' ') {
            return Some("windows-reserved trailing dot or space in path segment");
        }
    }
    None
}

/// Make spawned `git` deterministic: fixed identity and dates, no system
/// config, an isolated `HOME`/`XDG_CONFIG_HOME` and `LC_ALL=C`.
///
/// The environment is process-global, so this runs once. `TMPDIR` is left
/// untouched: the example replay normalizes fixture roots (text and ranges
/// together) so recordings stay valid under any temporary-directory path.
pub fn apply_determinism_env() {
    DETERMINISM.call_once(|| {
        let temp_home = TempDir::new().expect("tempdir for HOME").keep();
        let home = git_compatible_absolute(&temp_home);
        std::fs::create_dir_all(home.join("config")).expect("create XDG config dir");
        std::env::set_var("HOME", &home);
        std::env::set_var("XDG_CONFIG_HOME", home.join("config"));
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
        std::env::set_var("LC_ALL", "C");
        std::env::set_var("GIT_AUTHOR_NAME", FIXED_NAME);
        std::env::set_var("GIT_AUTHOR_EMAIL", FIXED_EMAIL);
        std::env::set_var("GIT_COMMITTER_NAME", FIXED_NAME);
        std::env::set_var("GIT_COMMITTER_EMAIL", FIXED_EMAIL);
        std::env::set_var("GIT_AUTHOR_DATE", FIXED_DATE);
        std::env::set_var("GIT_COMMITTER_DATE", FIXED_DATE);
    });
}

/// Run `git` in `dir`, panicking with its stderr when it fails.
pub fn git(dir: &Path, args: &[&str]) -> String {
    apply_determinism_env();
    if let Some(reason) = windows_path_problem(dir, true) {
        panic!("git directory {dir:?} is not Windows-portable: {reason}");
    }
    for arg in args {
        if let Some(reason) = windows_path_problem(Path::new(arg), false) {
            panic!("git argument {arg:?} is not Windows-portable: {reason}");
        }
    }
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run git");
    if !out.status.success() {
        panic!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Write `content` to `path`, creating any missing parent directories.
pub fn write(path: &Path, content: &str) {
    if let Some(reason) = windows_path_problem(path, true) {
        panic!("fixture path {path:?} is not Windows-portable: {reason}");
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent directories");
    }
    std::fs::write(path, content).expect("write file");
}

/// Read `path` into a string.
pub fn read(path: &Path) -> String {
    std::fs::read_to_string(path).expect("read file")
}

/// `git diff --cached -U3` for `f.txt`.
pub fn cached_diff(dir: &Path) -> String {
    git(dir, &["diff", "--cached", "-U3", "--", "f.txt"])
}

/// Number of hunks in a unified diff.
pub fn hunk_count(diff: &str) -> usize {
    diff.lines().filter(|line| line.starts_with("@@")).count()
}

/// `n` numbered lines (`line1` … `line{n}`) under construction.
///
/// Override lines with [`Lines::with`], then render the file with
/// [`std::string::ToString`].
pub fn lines(n: usize) -> Lines {
    Lines((1..=n).map(|i| format!("line{i}")).collect())
}

/// A file body being built from numbered lines; see [`lines`].
pub struct Lines(Vec<String>);

impl Lines {
    /// Replace 1-based line `n` with `text`.
    pub fn with(mut self, n: usize, text: impl Into<String>) -> Self {
        assert!(n >= 1 && n <= self.0.len(), "line {n} is out of range");
        self.0[n - 1] = text.into();
        self
    }

    /// Insert `text` as a new line directly after 1-based line `n`.
    pub fn insert_after(mut self, n: usize, text: impl Into<String>) -> Self {
        assert!(n >= 1 && n <= self.0.len(), "line {n} is out of range");
        self.0.insert(n, text.into());
        self
    }

    /// Remove the `n`th 1-based line.
    pub fn remove(mut self, n: usize) -> Self {
        assert!(n >= 1 && n <= self.0.len(), "line {n} is out of range");
        self.0.remove(n - 1);
        self
    }
}

impl std::fmt::Display for Lines {
    /// Render every line with a trailing newline.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.join("\n"))?;
        f.write_str("\n")
    }
}

/// The worktree content produced by [`RepoFixture::modify_standard`].
pub fn standard_changes() -> String {
    lines(10)
        .with(3, "line3 CHANGED")
        .remove(8)
        .insert_after(5, "NEW line")
        .to_string()
}

/// The worktree content produced by [`RepoFixture::modify_two_hunks`].
pub fn two_hunk_changes() -> String {
    lines(40)
        .with(5, "line5 CHANGED")
        .with(35, "line35 CHANGED")
        .to_string()
}

/// Builder for deterministic temporary repositories.
///
/// Files are written and committed in the order they are added; uncommitted
/// changes are applied last so they stay in the worktree.
pub struct RepoFixture {
    dir: TempDir,
    root: PathBuf,
    commits: Vec<(String, Vec<(String, String)>)>,
    renames: Vec<(String, String)>,
    staged: Vec<(String, String)>,
    changes: Vec<(String, String)>,
    unborn: bool,
    detached: bool,
    /// Whether this fixture was started by [`Self::new`] and so participates in
    /// the automatic plan cache; see [`Self::is_cacheable`].
    cacheable: bool,
}

impl RepoFixture {
    /// Start a fixture whose repository is created by [`Self::build`].
    ///
    /// Fixtures started here are cacheable: [`Self::build`] publishes the first
    /// repository built from a given plan and copies it for later identical
    /// plans.
    pub fn new() -> Self {
        apply_determinism_env();
        let mut fixture = Self::with_tempdir(TempDir::new().expect("tempdir"));
        fixture.cacheable = true;
        fixture
    }

    /// Start a fixture inside `parent`, whose repository is created by
    /// [`Self::build`].
    ///
    /// `parent` may be reached through a symbolic link: [`Self::path`] always
    /// reports the canonical root. The fixture owns the directory it creates
    /// and removes it on drop. Fixtures started here are exempt from the
    /// automatic template cache: [`Self::build`] always runs for real and never
    /// registers or copies a template, so [`fork`] builders and fork-vs-fresh
    /// comparisons keep a genuine build.
    pub fn new_in(parent: &Path) -> Self {
        apply_determinism_env();
        let dir = tempfile::Builder::new()
            .prefix("magit-lsp-fixture")
            .tempdir_in(parent)
            .expect("tempdir in parent");
        Self::with_tempdir(dir)
    }

    /// Wrap `dir`, recording its canonical root so it matches the repository
    /// identity that [`GitRepo::open`] derives.
    ///
    /// The root is normalized through [`git_compatible_absolute`]: canonical on
    /// Unix, plain on Windows (where the canonical verbatim form is unusable as
    /// a git argument).
    fn with_tempdir(dir: TempDir) -> Self {
        let root = git_compatible_absolute(dir.path());
        Self {
            dir,
            root,
            commits: Vec::new(),
            renames: Vec::new(),
            staged: Vec::new(),
            changes: Vec::new(),
            unborn: false,
            detached: false,
            cacheable: false,
        }
    }

    /// Leave the repository without commits; [`Self::staged_file`] then populates
    /// the index.
    pub fn unborn(mut self) -> Self {
        self.unborn = true;
        self
    }

    /// Detach `HEAD` after the commits are made.
    pub fn detached(mut self) -> Self {
        self.detached = true;
        self
    }

    /// `git mv` `from` to `to` after the commits, staging the rename.
    ///
    /// Both names are fixture paths, so they are checked against the Windows
    /// portability rules when recorded. `git mv` is otherwise only reached with
    /// `check_name_chars=false`, which would let a reserved name through.
    pub fn rename(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        assert!(!self.unborn, "an unborn fixture cannot have renames");
        let from = from.into();
        let to = to.into();
        for name in [&from, &to] {
            if let Some(reason) = windows_path_problem(Path::new(name), true) {
                panic!("rename path {name:?} is not Windows-portable: {reason}");
            }
        }
        self.renames.push((from, to));
        self
    }

    /// Commit `files` (path, content) with `message`.
    pub fn commit(
        mut self,
        message: &str,
        files: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        assert!(!self.unborn, "an unborn fixture cannot have commits");
        self.commits.push((
            message.to_string(),
            files
                .into_iter()
                .map(|(path, content)| (path.into(), content.into()))
                .collect(),
        ));
        self
    }

    /// Write `content` to `path` and stage it without committing.
    pub fn staged_file(mut self, path: impl Into<String>, content: impl Into<String>) -> Self {
        self.staged.push((path.into(), content.into()));
        self
    }

    /// Write `content` to `path` in the worktree without staging it.
    pub fn modify(mut self, path: impl Into<String>, content: impl Into<String>) -> Self {
        self.changes.push((path.into(), content.into()));
        self
    }

    /// Apply [`standard_changes`] to `f.txt` without staging it.
    pub fn modify_standard(self) -> Self {
        self.modify("f.txt", standard_changes())
    }

    /// Apply [`two_hunk_changes`] to a 40-line `f.txt` without staging it.
    pub fn modify_two_hunks(self) -> Self {
        self.modify("f.txt", two_hunk_changes())
    }

    /// Create the repository on disk and open it.
    ///
    /// A cacheable fixture ([`Self::new`]) publishes the repository it builds
    /// as a process-global template keyed on its construction plan, so a later
    /// build of the same plan copies that template into a fresh temporary
    /// directory instead of replaying the git commands; every caller still
    /// receives a private repository. The plan is exactly the recorded data
    /// (`commits`, `renames`, `staged`, `changes`, `unborn`, `detached`), which
    /// [`apply_determinism_env`] pins to fixed identities and dates, so
    /// identical plans always produce identical repositories. Caching is safe
    /// for the fake-git server children: `git` here is spawned from the test
    /// process itself, and a child's environment (the fake-git gate) never
    /// reaches it, so a cached template cannot be poisoned. Fixtures started by
    /// [`Self::new_in`] (and the copies [`fork`]/[`fork_in`] return) are exempt:
    /// they always build for real and never register.
    pub fn build(self) -> (RepoFixture, GitRepo) {
        apply_determinism_env();
        let cache_plan = if self.is_cacheable() && !in_template_build() {
            Some(self.auto_plan())
        } else {
            None
        };
        if let Some(plan) = cache_plan.as_deref() {
            if let Some(template) = cached_auto_template(&plan_template_name(plan), plan) {
                let (mut fixture, repo) = copy_template(&template, None);
                refresh_copied_index(fixture.path());
                // The fixture still records the plan it asked for, so callers
                // can inspect it (and `auto_template_name` stays stable), and it
                // keeps its `new` origin even though the repository is a copy.
                fixture.commits = self.commits;
                fixture.renames = self.renames;
                fixture.staged = self.staged;
                fixture.changes = self.changes;
                fixture.unborn = self.unborn;
                fixture.detached = self.detached;
                fixture.cacheable = true;
                return (fixture, repo);
            }
        }
        let path = self.root.clone();
        git(&path, &["init", "-q"]);
        git(&path, &["symbolic-ref", "HEAD", "refs/heads/main"]);
        git(&path, &["config", "user.email", FIXED_EMAIL]);
        git(&path, &["config", "user.name", FIXED_NAME]);
        for (message, files) in &self.commits {
            for (name, content) in files {
                write(&path.join(name), content);
            }
            git(&path, &["add", "."]);
            git(&path, &["commit", "-q", "-m", message]);
        }
        for (from, to) in &self.renames {
            if let Some(parent) = path.join(to).parent() {
                std::fs::create_dir_all(parent).expect("create rename parent");
            }
            git(&path, &["mv", from.as_str(), to.as_str()]);
        }
        if self.detached {
            git(&path, &["checkout", "-q", "--detach"]);
        }
        for (name, content) in &self.staged {
            write(&path.join(name), content);
            git(&path, &["add", name.as_str()]);
        }
        for (name, content) in &self.changes {
            write(&path.join(name), content);
        }
        let repo = GitRepo::open(&path).expect("open repo");
        if let Some(plan) = cache_plan {
            self.register_auto_template(&plan_template_name(&plan), &plan);
        }
        (self, repo)
    }

    /// Whether this fixture was started by [`Self::new`], including the copies
    /// [`Self::build`] serves from the automatic template cache.
    ///
    /// False for [`Self::new_in`] fixtures and for the copies [`fork`] and
    /// [`fork_in`] return: those are exempt from the automatic cache.
    pub fn is_cacheable(&self) -> bool {
        self.cacheable
    }

    /// Registry name [`Self::build`] uses for this fixture's construction plan.
    ///
    /// Derived only from the recorded plan, so every fixture that records the
    /// same plan yields the same name, including exempt [`Self::new_in`]
    /// fixtures (which never register it). The `auto-` prefix keeps these names
    /// from colliding with the human names given to [`fork`].
    pub fn auto_template_name(&self) -> String {
        plan_template_name(&self.auto_plan())
    }

    /// Serialization of the recorded construction plan.
    ///
    /// This is the identity behind [`Self::auto_template_name`]'s hash and is
    /// stored with each automatic template so a lookup can reject a hash
    /// collision instead of serving a repository built from another plan.
    fn auto_plan(&self) -> String {
        format!(
            "{:?}{:?}{:?}{:?}{:?}{:?}",
            self.commits, self.renames, self.staged, self.changes, self.unborn, self.detached
        )
    }

    /// Publish this fixture's finished repository as the automatic template
    /// `name`, unless another build already registered it.
    ///
    /// `plan` is stored beside the directory as the identity that [`Self::build`]
    /// checks before copying.
    fn register_auto_template(&self, name: &str, plan: &str) {
        let mut cache = templates().lock().expect("template cache lock");
        if cache.contains_key(name) {
            return;
        }
        let dir = template_root().path().join(name);
        std::fs::create_dir_all(&dir).expect("create automatic template directory");
        copy_tree(&self.root, &dir);
        cache.insert(
            name.to_owned(),
            Template {
                path: dir,
                plan: Some(plan.to_owned()),
            },
        );
    }

    /// Canonical root directory of the temporary repository.
    pub fn path(&self) -> &Path {
        &self.root
    }

    /// Run `git` inside the repository.
    pub fn git(&self, args: &[&str]) -> String {
        git(self.path(), args)
    }
}

/// Process-global root that owns every cached repository template.
///
/// Lives outside every test's temporary directory so templates outlive the
/// fixture that built them. The root survives for the lifetime of the process,
/// but initialization registers an exit-time sweep that removes the whole tree
/// when the test binary exits normally, after every test thread has joined (so
/// no template copy is in flight). A process that dies without running exit
/// handlers (for example on `SIGKILL`) leaves its root behind; the stale root
/// has a random [`TempDir`] name and does not affect later runs.
static TEMPLATE_ROOT: OnceLock<TempDir> = OnceLock::new();

/// Filesystem path of [`TEMPLATE_ROOT`], read by the exit-time sweep.
static TEMPLATE_ROOT_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Remove [`TEMPLATE_ROOT`] when the test binary exits.
///
/// Registered with `atexit` exactly once from [`template_root`]. Exit handlers
/// run after the test harness has joined its threads, so no copy can still be
/// reading the tree this removes.
extern "C" fn sweep_template_root() {
    if let Some(path) = TEMPLATE_ROOT_PATH.get() {
        let _ = std::fs::remove_dir_all(path);
    }
}

/// The process-global template root, created and armed for exit-time removal on
/// first use.
fn template_root() -> &'static TempDir {
    TEMPLATE_ROOT.get_or_init(|| {
        let root = TempDir::new().expect("template root");
        let _ = TEMPLATE_ROOT_PATH.set(root.path().to_path_buf());
        // SAFETY: the handler only reads the immutable path stored in
        // `TEMPLATE_ROOT_PATH` and removes that tree; it runs at exit, after
        // the harness has joined every test thread.
        if unsafe { libc::atexit(sweep_template_root) } != 0 {
            panic!("register template root sweep");
        }
        root
    })
}

/// Templates already built in this process, keyed by template name: [`fork`]'s
/// human `name`, or the plan-derived `auto-` name of an automatic template.
static TEMPLATES: OnceLock<Mutex<HashMap<String, Template>>> = OnceLock::new();

/// One registered template: the directory to copy and, for automatic templates,
/// the serialized construction plan that keyed it.
struct Template {
    /// Directory holding the built repository.
    path: PathBuf,
    /// Plan serialization of an automatic template; `None` for [`fork`] names.
    plan: Option<String>,
}

/// The process-global template registry.
fn templates() -> &'static Mutex<HashMap<String, Template>> {
    TEMPLATES.get_or_init(|| Mutex::new(HashMap::new()))
}

thread_local! {
    /// Set while a [`fork_in`] builder runs with the registry lock held, where
    /// consulting the automatic cache would re-enter that lock.
    static IN_TEMPLATE_BUILD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether this thread is currently inside a [`fork_in`] builder.
fn in_template_build() -> bool {
    IN_TEMPLATE_BUILD.with(|flag| flag.get())
}

/// Run `f` with automatic template caching disabled on this thread.
///
/// Clears the flag on unwind too, so a panicking builder cannot leave later
/// builds on the same thread uncacheable.
fn without_auto_cache<T>(f: impl FnOnce() -> T) -> T {
    struct Clear;

    impl Drop for Clear {
        fn drop(&mut self) {
            IN_TEMPLATE_BUILD.with(|flag| flag.set(false));
        }
    }

    IN_TEMPLATE_BUILD.with(|flag| flag.set(true));
    let _clear = Clear;
    f()
}

/// The registered directory of the automatic template `name`, if built.
fn cached_template(name: &str) -> Option<PathBuf> {
    templates()
        .lock()
        .expect("template cache lock")
        .get(name)
        .map(|template| template.path.clone())
}

/// The directory of the automatic template `name`, if one was built from
/// exactly `plan`.
///
/// The name's hash keys the registry, but a collision must never serve a
/// repository built from another plan, so the stored serialization is compared
/// before the copy.
fn cached_auto_template(name: &str, plan: &str) -> Option<PathBuf> {
    let cache = templates().lock().expect("template cache lock");
    let template = cache.get(name)?;
    (template.plan.as_deref() == Some(plan)).then(|| template.path.clone())
}

/// Registry name of the automatic template for the serialized construction
/// `plan`: `auto-` plus a 64-bit hash of the plan in hex.
fn plan_template_name(plan: &str) -> String {
    let mut hasher = DefaultHasher::new();
    plan.hash(&mut hasher);
    format!("auto-{:016x}", hasher.finish())
}

/// Canonical directory of the cached template called `name`, once built.
///
/// `None` until a [`fork`]/[`fork_in`] call with that name has run its builder,
/// or a cacheable [`RepoFixture::build`] has published its plan. Automatic
/// templates are registered under [`RepoFixture::auto_template_name`]'s
/// plan-derived `auto-` names, which cannot collide with `fork`'s names.
pub fn template_path(name: &str) -> Option<PathBuf> {
    cached_template(name)
}

/// Build the repository template `name` once per process, then return a fresh
/// copy of it through the template root.
///
/// [`RepoFixture::build`] cannot be cached within a fixture because every
/// fixture owns and removes its directory. The first call runs `build` inside
/// the process-global template root, passing it the directory to build in;
/// later calls skip `build` and copy the template into a new temporary
/// directory, so each caller receives an independent repository interchangeable
/// with [`RepoFixture::build`]'s output. Builders must construct their
/// repository with [`RepoFixture::new_in`], which is exempt from the automatic
/// cache that [`RepoFixture::build`] otherwise consults.
pub fn fork(
    name: &str,
    build: impl FnOnce(&Path) -> (RepoFixture, GitRepo),
) -> (RepoFixture, GitRepo) {
    fork_in(name, None, build)
}

/// Like [`fork`], but create the copy inside `parent`, mirroring
/// [`RepoFixture::new_in`].
pub fn fork_in(
    name: &str,
    parent: Option<&Path>,
    build: impl FnOnce(&Path) -> (RepoFixture, GitRepo),
) -> (RepoFixture, GitRepo) {
    let template = {
        let mut cache = templates().lock().expect("template cache lock");
        match cache.get(name) {
            Some(template) => template.path.clone(),
            None => {
                let root = template_root();
                let dir = root.path().join(name);
                std::fs::create_dir_all(&dir).expect("create template directory");
                // This holds the registry lock, so `build` must not consult the
                // automatic cache below; the guard keeps that true even if a
                // builder one day uses `RepoFixture::new`.
                let (fixture, _) = without_auto_cache(|| build(&dir));
                let path = fixture.path().to_path_buf();
                // The registry owns the template now, so keep its directory
                // instead of letting the fixture remove it on drop.
                let _ = fixture.dir.keep();
                cache.insert(
                    name.to_owned(),
                    Template {
                        path: path.clone(),
                        plan: None,
                    },
                );
                path
            }
        }
    };
    copy_template(&template, parent)
}

/// Copy the repository at `template` into a fresh temporary directory (inside
/// `parent` when given) and open the copy.
fn copy_template(template: &Path, parent: Option<&Path>) -> (RepoFixture, GitRepo) {
    let copy = match parent {
        Some(parent) => tempfile::Builder::new()
            .prefix("magit-lsp-fixture")
            .tempdir_in(parent),
        None => TempDir::new(),
    }
    .expect("copy fixture template");
    copy_tree(template, copy.path());
    let fixture = RepoFixture::with_tempdir(copy);
    let repo = GitRepo::open(fixture.path()).expect("open copied repository");
    (fixture, repo)
}

/// Make a copied repository's index as functional as a fresh build's.
///
/// A copy keeps the template's index bytes, whose stat data describes the
/// template's files; refreshing rewrites that stat cache in place so later Git
/// commands see the copy's own files.
fn refresh_copied_index(dir: &Path) {
    if !dir.join(".git/index").is_file() {
        return;
    }
    apply_determinism_env();
    let out = Command::new("git")
        .args(["update-index", "--refresh"])
        .current_dir(dir)
        .output()
        .expect("failed to run git update-index --refresh");
    // Exit 1 means some paths need updating (the fixtures' uncommitted changes),
    // which is legitimate and still rewrites the stat data.
    assert!(
        matches!(out.status.code(), Some(0) | Some(1)),
        "git update-index --refresh failed in {dir:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Recursively copy `source` into `target`, preserving file contents and modes.
///
/// Directory permissions are replayed as the last step of each directory, after
/// its children have been copied: a template directory may deny writes (and a
/// parent may deny writes in its subtree), so applying them up front would make
/// the copy fail. Symlinks are copied as links, not followed.
fn copy_tree(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create copied directory");
    for entry in std::fs::read_dir(source).expect("read template directory") {
        let entry = entry.expect("read template entry");
        let from = entry.path();
        let to = target.join(entry.file_name());
        let file_type = entry.file_type().expect("inspect template entry");
        if file_type.is_dir() {
            copy_tree(&from, &to);
        } else if file_type.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(
                std::fs::read_link(&from).expect("read template symlink"),
                &to,
            )
            .expect("copy template symlink");
            #[cfg(not(unix))]
            panic!("cannot copy template symlink {from:?}");
        } else {
            std::fs::copy(&from, &to).expect("copy template file");
        }
    }
    let permissions = std::fs::metadata(source)
        .expect("inspect template directory mode")
        .permissions();
    std::fs::set_permissions(target, permissions).expect("copy template directory mode");
}

/// A merge dated January 2 over January 1 parents, plus unmerged local,
/// remote-only and tag-only tips, for backend and visible log assertions.
pub fn log_variants_fixture() -> (RepoFixture, GitRepo) {
    let (fixture, repo) = RepoFixture::new()
        .commit("base history", [("base.txt", "base\n")])
        .commit("main history", [("main.txt", "main\n")])
        .build();
    fixture.git(&["checkout", "-b", "side", "HEAD~1"]);
    write(&fixture.path().join("side.txt"), "side\n");
    fixture.git(&["add", "."]);
    fixture.git(&["commit", "-m", "side history"]);
    fixture.git(&["checkout", "main"]);
    let merge = Command::new("git")
        .args(["merge", "--no-ff", "side", "-m", "merge history"])
        .env("GIT_AUTHOR_DATE", "2000-01-02T00:00:00+0000")
        .env("GIT_COMMITTER_DATE", "2000-01-02T00:00:00+0000")
        .current_dir(fixture.path())
        .output()
        .unwrap();
    assert!(
        merge.status.success(),
        "{}",
        String::from_utf8_lossy(&merge.stderr)
    );
    fixture.git(&["branch", "-d", "side"]);
    for (branch, subject) in [
        ("independent", "local only"),
        ("remote-tip", "remote only"),
        ("tag-tip", "tag only"),
    ] {
        fixture.git(&["checkout", "-b", branch, "main~2"]);
        write(&fixture.path().join("extra.txt"), subject);
        fixture.git(&["add", "."]);
        fixture.git(&["commit", "-m", subject]);
        match branch {
            "remote-tip" => {
                fixture.git(&["update-ref", "refs/remotes/origin/remote", "HEAD"]);
            }
            "tag-tip" => {
                fixture.git(&["tag", "tag-only"]);
            }
            _ => {}
        }
        fixture.git(&["checkout", "main"]);
        if branch != "independent" {
            fixture.git(&["branch", "-D", branch]);
        }
    }
    (fixture, repo)
}

/// A repository with a ten-line tracked `f.txt`.
pub fn init_repo() -> (RepoFixture, GitRepo) {
    RepoFixture::new()
        .commit("init", [("f.txt", TEN_LINES)])
        .build()
}

/// A repository whose only content is a staged new file (no commits yet).
pub fn init_unborn_repo() -> (RepoFixture, GitRepo) {
    RepoFixture::new()
        .unborn()
        .staged_file("f.txt", "a\nb\nc\n")
        .build()
}

/// Real-server stdio transport shared by integration suites.
pub mod stdio;

/// Narrow JSON Schema validation shared by the schema and example suites.
pub mod schema;

/// Fake `git` executables (gated and stalling) shared by the real-server suites.
///
/// Unix only: the helper writes `#!/bin/sh` scripts and needs Unix file modes
/// (`set_mode`), so non-Unix compilations leave it out entirely.
#[cfg(unix)]
pub mod fake_git;

/// In-process session harness shared by the session suites.
pub mod session;

#[allow(unused_imports)]
pub use session::{keymaps, message_texts, TestSession, STATUS_VIEWPORT, WIDE_VIEWPORT};
