//! Configuration: TOML loading, includes, layering and hot reload (§3.18).
//!
//! Layers run from lowest to highest precedence: built-in defaults, the shipped
//! profile (its bindings are consulted by the keymap resolver, not copied into
//! the user config), the user config with its includes, the client's
//! `initializationOptions`, and finally a trusted per-repository
//! `.magit-lsp.toml`. Tables merge key by key; arrays and scalars replace.

use crate::keymap::{self, EditorMode, Keymap};
use crate::keys::KeyChord;
use std::collections::BTreeMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use toml_edit::{Array, Document, InlineTable, Item, Table, Value as TomlValue};

/// Environment variable naming the config file when no flag is given.
pub const CONFIG_ENV: &str = "MAGIT_LSP_CONFIG";

/// View scopes the server resolves keymaps for.
pub const VIEW_SCOPES: &[&str] = &[
    "common",
    "status",
    "log",
    "log.select",
    "select",
    "menu",
    "commit-message",
    "process-log",
];

/// A shipped default profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Standard,
    Vim,
}

impl Profile {
    /// The profile name as written in config.
    pub fn as_str(self) -> &'static str {
        match self {
            Profile::Standard => "standard",
            Profile::Vim => "vim",
        }
    }

    /// Parse a profile name.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "standard" => Ok(Profile::Standard),
            "vim" => Ok(Profile::Vim),
            other => Err(format!(
                "unknown profile `{other}`; expected `standard` or `vim`"
            )),
        }
    }

    /// The profile's TOML source, shipped inside the binary.
    pub fn source(self) -> &'static str {
        match self {
            Profile::Standard => include_str!("../profiles/standard.toml"),
            Profile::Vim => include_str!("../profiles/vim.toml"),
        }
    }

    /// The label used in error messages for this profile.
    pub fn file_label(self) -> PathBuf {
        PathBuf::from(format!("profiles/{}.toml", self.as_str()))
    }
}

/// Where a binding was declared, for file:line error messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    pub file: PathBuf,
    pub line: usize,
}

/// A parsed binding declaration: keys plus the editor modes it applies in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingSpec {
    pub keys: Vec<KeyChord>,
    pub modes: Vec<EditorMode>,
}

impl BindingSpec {
    /// A binding active in both editor modes.
    pub fn new(keys: Vec<KeyChord>) -> Self {
        Self {
            keys,
            modes: vec![EditorMode::Normal, EditorMode::Visual],
        }
    }
}

/// Bindings declared for one scope.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopeKeys {
    /// Command name (as written) → declaration.
    pub commands: BTreeMap<String, BindingSpec>,
    /// Key chords removed from the profile's bindings for this scope.
    pub unbind: Vec<KeyChord>,
    /// Where each command name was declared.
    pub origins: BTreeMap<String, Origin>,
}

/// User-level key overrides, keyed by scope.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeysConfig {
    pub scopes: BTreeMap<String, ScopeKeys>,
}

impl KeysConfig {
    fn merge(&mut self, other: KeysConfig) {
        for (scope, keys) in other.scopes {
            let entry = self.scopes.entry(scope).or_default();
            for (name, spec) in keys.commands {
                entry.commands.insert(name.clone(), spec);
                if let Some(origin) = keys.origins.get(&name) {
                    entry.origins.insert(name, origin.clone());
                }
            }
            if !keys.unbind.is_empty() {
                entry.unbind = keys.unbind;
            }
        }
    }
}

/// How much context `git diff` shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffSettings {
    pub context: u32,
    pub whitespace: Option<String>,
}

/// Section ordering and initial visibility.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionSettings {
    pub order: Option<Vec<String>>,
    pub default_visibility: Option<String>,
}

/// UI behaviour settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiSettings {
    pub confirm_discard: bool,
}

/// Log view defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogSettings {
    pub default_args: Vec<String>,
    pub margin: Option<u32>,
}

/// The fully resolved configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub profile: Profile,
    pub trust_repo_config: bool,
    pub keys: KeysConfig,
    pub diff: DiffSettings,
    pub sections: SectionSettings,
    pub ui: UiSettings,
    pub log: LogSettings,
    /// Every file that contributed, for the watcher.
    pub files: Vec<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            profile: Profile::Vim,
            trust_repo_config: false,
            keys: KeysConfig::default(),
            diff: DiffSettings {
                context: 3,
                whitespace: None,
            },
            sections: SectionSettings {
                order: None,
                default_visibility: None,
            },
            ui: UiSettings {
                confirm_discard: true,
            },
            log: LogSettings {
                default_args: Vec::new(),
                margin: None,
            },
            files: Vec::new(),
        }
    }
}

/// A configuration problem, optionally located in a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub message: String,
    pub file: Option<PathBuf>,
    pub line: Option<usize>,
}

impl ConfigError {
    /// A problem with no file location.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            file: None,
            line: None,
        }
    }

    /// Attach a declaration's origin.
    pub fn at(mut self, origin: &Origin) -> Self {
        self.file = Some(origin.file.clone());
        if origin.line > 0 {
            self.line = Some(origin.line);
        }
        self
    }

    /// Attach a file and line.
    pub fn at_file_line(mut self, file: impl Into<PathBuf>, line: usize) -> Self {
        self.file = Some(file.into());
        if line > 0 {
            self.line = Some(line);
        }
        self
    }

    /// Attach only a file.
    pub fn at_file(mut self, file: impl Into<PathBuf>) -> Self {
        self.file = Some(file.into());
        self
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.file, self.line) {
            (Some(file), Some(line)) => {
                write!(f, "{}:{}: {}", file.display(), line, self.message)
            }
            (Some(file), None) => write!(f, "{}: {}", file.display(), self.message),
            _ => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Everything needed to (re)load a configuration.
#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    /// An explicit `--config` path; takes precedence over [`CONFIG_ENV`].
    pub config_path: Option<PathBuf>,
    /// The repository whose `.magit-lsp.toml` may apply.
    pub repo_path: Option<PathBuf>,
    /// The client's `initializationOptions`.
    pub initialization: Option<serde_json::Value>,
}

/// Load and validate a configuration from all layers.
pub fn load_config(options: &LoadOptions) -> Result<Config, ConfigError> {
    let mut config = Config::default();
    let mut files = Vec::new();

    let resolved = resolve_config_path(options.config_path.as_deref());
    if let Some(path) = resolved {
        let explicit = options.config_path.is_some();
        if path.is_file() {
            let mut layered = Layered::default();
            let mut visiting = Vec::new();
            load_file(&path, &mut layered, &mut visiting)?;
            apply_layered(&mut config, layered);
            files.push(path);
        } else if explicit {
            return Err(ConfigError::new("config file not found").at_file(path));
        }
    }

    if let Some(init) = &options.initialization {
        apply_initialization(&mut config, init)?;
    }

    if config.trust_repo_config {
        if let Some(repo) = &options.repo_path {
            let repo_config = repo.join(".magit-lsp.toml");
            if repo_config.is_file() {
                let mut layered = Layered::default();
                let mut visiting = Vec::new();
                load_file(&repo_config, &mut layered, &mut visiting)?;
                // Per-repo config may only tune keys and display settings.
                layered.profile = None;
                layered.trust_repo_config = None;
                apply_layered(&mut config, layered);
                files.push(repo_config);
            }
        }
    }

    for file in files {
        if !config.files.contains(&file) {
            config.files.push(file);
        }
    }
    config.files.sort();
    config.files.dedup();

    // Resolving every scope validates unknown commands and conflicts.
    keymap::validate(&config)?;
    Ok(config)
}

/// Parse a shipped profile's bindings.
pub fn profile_keys(profile: Profile) -> Result<KeysConfig, ConfigError> {
    let source = profile.source();
    let file = profile.file_label();
    let doc = Document::<String>::parse(source.to_string())
        .map_err(|error| toml_error(&file, source, &error))?;
    let mut keys = KeysConfig::default();
    if let Some(item) = doc.get("keys") {
        let table = item.as_table().ok_or_else(|| {
            ConfigError::new("`keys` must be a table").at_file_line(&file, item_line(source, item))
        })?;
        apply_keys(table, &file, source, &mut keys)?;
    }
    Ok(keys)
}

/// The config path chosen from the flag, the environment, then the default.
pub fn resolve_config_path(cli: Option<&Path>) -> Option<PathBuf> {
    let env = std::env::var_os(CONFIG_ENV);
    resolve_config_path_from(cli, env.as_deref(), default_config_path())
}

/// Pure precedence helper: flag, then environment, then default.
pub fn resolve_config_path_from(
    cli: Option<&Path>,
    env: Option<&std::ffi::OsStr>,
    default: Option<PathBuf>,
) -> Option<PathBuf> {
    if let Some(path) = cli {
        return Some(path.to_path_buf());
    }
    if let Some(value) = env {
        return Some(PathBuf::from(value));
    }
    default
}

/// The OS config path: `$XDG_CONFIG_HOME/magit-lsp/config.toml` (or
/// `~/.config` on Unix), `%APPDATA%\magit-lsp\config.toml` on Windows.
pub fn default_config_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|base| base.join("magit-lsp").join("config.toml"))
    }
    #[cfg(not(windows))]
    {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
        Some(base.join("magit-lsp").join("config.toml"))
    }
}

#[derive(Debug, Default)]
struct Layered {
    profile: Option<Profile>,
    trust_repo_config: Option<bool>,
    keys: KeysConfig,
    diff_context: Option<u32>,
    diff_whitespace: Option<String>,
    sections_order: Option<Vec<String>>,
    sections_default_visibility: Option<String>,
    ui_confirm_discard: Option<bool>,
    log_default_args: Option<Vec<String>>,
    log_margin: Option<u32>,
    files: Vec<PathBuf>,
}

fn apply_layered(config: &mut Config, layered: Layered) {
    if let Some(profile) = layered.profile {
        config.profile = profile;
    }
    if let Some(trust) = layered.trust_repo_config {
        config.trust_repo_config = trust;
    }
    config.keys.merge(layered.keys);
    if let Some(context) = layered.diff_context {
        config.diff.context = context;
    }
    if let Some(whitespace) = layered.diff_whitespace {
        config.diff.whitespace = Some(whitespace);
    }
    if let Some(order) = layered.sections_order {
        config.sections.order = Some(order);
    }
    if let Some(visibility) = layered.sections_default_visibility {
        config.sections.default_visibility = Some(visibility);
    }
    if let Some(confirm) = layered.ui_confirm_discard {
        config.ui.confirm_discard = confirm;
    }
    if let Some(args) = layered.log_default_args {
        config.log.default_args = args;
    }
    if let Some(margin) = layered.log_margin {
        config.log.margin = Some(margin);
    }
    for file in layered.files {
        if !config.files.contains(&file) {
            config.files.push(file);
        }
    }
}

fn load_file(
    path: &Path,
    layered: &mut Layered,
    visiting: &mut Vec<PathBuf>,
) -> Result<(), ConfigError> {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if let Some(start) = visiting.iter().position(|seen| seen == &canonical) {
        let mut chain: Vec<String> = visiting[start..]
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        chain.push(canonical.display().to_string());
        return Err(ConfigError::new(format!(
            "include cycle: {}",
            chain.join(" -> ")
        )));
    }

    let source = std::fs::read_to_string(path)
        .map_err(|error| ConfigError::new(error.to_string()).at_file(path))?;
    let doc = Document::<String>::parse(source.to_string())
        .map_err(|error| toml_error(path, &source, &error))?;

    visiting.push(canonical);
    let result = load_document(&doc, path, &source, layered, visiting);
    visiting.pop();
    result?;

    if !layered.files.contains(&path.to_path_buf()) {
        layered.files.push(path.to_path_buf());
    }
    Ok(())
}

fn load_document(
    doc: &Document<String>,
    file: &Path,
    source: &str,
    layered: &mut Layered,
    visiting: &mut Vec<PathBuf>,
) -> Result<(), ConfigError> {
    load_includes(doc, file, source, layered, visiting)?;
    apply_document(doc, file, source, layered)
}

fn load_includes(
    doc: &Document<String>,
    file: &Path,
    source: &str,
    layered: &mut Layered,
    visiting: &mut Vec<PathBuf>,
) -> Result<(), ConfigError> {
    for (key, required) in [("include", true), ("include_optional", false)] {
        let Some(item) = doc.get(key) else {
            continue;
        };
        let array = item.as_array().ok_or_else(|| {
            ConfigError::new(format!("`{key}` must be an array of paths"))
                .at_file_line(file, item_line(source, item))
        })?;
        for value in array.iter() {
            let Some(text) = value.as_str() else {
                return Err(ConfigError::new(format!("`{key}` entries must be strings"))
                    .at_file_line(file, span_line(source, value.span())));
            };
            let resolved = resolve_include(file, text);
            if resolved.is_file() {
                load_file(&resolved, layered, visiting)?;
            } else if required {
                return Err(ConfigError::new(format!(
                    "included file not found: {}",
                    resolved.display()
                ))
                .at_file_line(file, span_line(source, value.span())));
            }
        }
    }
    Ok(())
}

fn apply_document(
    doc: &Document<String>,
    file: &Path,
    source: &str,
    layered: &mut Layered,
) -> Result<(), ConfigError> {
    for (key, item) in doc.iter() {
        let line = item_line(source, item);
        match key {
            "include" | "include_optional" => {}
            "profile" => {
                let value = item.as_str().ok_or_else(|| {
                    ConfigError::new("`profile` must be a string").at_file_line(file, line)
                })?;
                layered.profile = Some(Profile::parse(value).map_err(|message| {
                    ConfigError::new(message).at_file_line(file, line)
                })?);
            }
            "trust_repo_config" => {
                let value = item.as_bool().ok_or_else(|| {
                    ConfigError::new("`trust_repo_config` must be a boolean")
                        .at_file_line(file, line)
                })?;
                layered.trust_repo_config = Some(value);
            }
            "keys" => {
                let table = item.as_table().ok_or_else(|| {
                    ConfigError::new("`keys` must be a table").at_file_line(file, line)
                })?;
                apply_keys(table, file, source, &mut layered.keys)?;
            }
            "diff" => {
                let table = settings_table(item, "diff", file, line)?;
                for (name, value) in table.iter() {
                    let line = item_line(source, value);
                    match name {
                        "context" => {
                            layered.diff_context = Some(integer(value, "diff.context", file, line)?)
                        }
                        "whitespace" => {
                            layered.diff_whitespace =
                                Some(string(value, "diff.whitespace", file, line)?)
                        }
                        other => return Err(unknown_key(file, line, "diff", other)),
                    }
                }
            }
            "sections" => {
                let table = settings_table(item, "sections", file, line)?;
                for (name, value) in table.iter() {
                    let line = item_line(source, value);
                    match name {
                        "order" => {
                            layered.sections_order =
                                Some(string_array(value, "sections.order", file, source)?)
                        }
                        "default_visibility" => {
                            layered.sections_default_visibility =
                                Some(string(value, "sections.default_visibility", file, line)?)
                        }
                        other => return Err(unknown_key(file, line, "sections", other)),
                    }
                }
            }
            "ui" => {
                let table = settings_table(item, "ui", file, line)?;
                for (name, value) in table.iter() {
                    let line = item_line(source, value);
                    match name {
                        "confirm_discard" => {
                            let setting = value.as_bool().ok_or_else(|| {
                                ConfigError::new("`ui.confirm_discard` must be a boolean")
                                    .at_file_line(file, line)
                            })?;
                            layered.ui_confirm_discard = Some(setting);
                        }
                        other => return Err(unknown_key(file, line, "ui", other)),
                    }
                }
            }
            "menus" => {
                // The saved-argument persistence scope is deferred (TASKS §0):
                // every `menus.*` setting is obsolete and rejected by its exact
                // name instead of being parsed and silently ignored.
                let table = settings_table(item, "menus", file, line)?;
                for (name, value) in table.iter() {
                    let line = item_line(source, value);
                    return Err(unknown_key(file, line, "menus", name));
                }
            }
            "log" => {
                let table = settings_table(item, "log", file, line)?;
                for (name, value) in table.iter() {
                    let line = item_line(source, value);
                    match name {
                        "default_args" => {
                            layered.log_default_args =
                                Some(string_array(value, "log.default_args", file, source)?)
                        }
                        "margin" => {
                            layered.log_margin = Some(integer(value, "log.margin", file, line)?)
                        }
                        other => return Err(unknown_key(file, line, "log", other)),
                    }
                }
            }
            other => {
                return Err(ConfigError::new(format!("unknown setting `{other}`"))
                    .at_file_line(file, line))
            }
        }
    }
    Ok(())
}

fn apply_keys(
    table: &Table,
    file: &Path,
    source: &str,
    keys: &mut KeysConfig,
) -> Result<(), ConfigError> {
    for (scope, item) in table.iter() {
        let scope_table = item.as_table().ok_or_else(|| {
            ConfigError::new(format!("`keys.{scope}` must be a table"))
                .at_file_line(file, item_line(source, item))
        })?;
        let entry = keys.scopes.entry(scope.to_string()).or_default();
        for (name, value) in scope_table.iter() {
            let line = scope_table
                .key(name)
                .and_then(|key| key.span())
                .map(|span| span_line(source, Some(span)))
                .unwrap_or_else(|| item_line(source, value));
            if name == "unbind" {
                let array = value.as_array().ok_or_else(|| {
                    ConfigError::new(format!("`keys.{scope}.unbind` must be an array"))
                        .at_file_line(file, line)
                })?;
                for element in array.iter() {
                    let text = element.as_str().ok_or_else(|| {
                        ConfigError::new("`unbind` entries must be strings").at_file_line(
                            file,
                            span_line(source, element.span()),
                        )
                    })?;
                    entry.unbind.push(parse_chord(
                        text,
                        file,
                        span_line(source, element.span()),
                    )?);
                }
                continue;
            }
            let spec = parse_binding(value, file, source)?;
            entry.commands.insert(name.to_string(), spec);
            entry.origins.insert(
                name.to_string(),
                Origin {
                    file: file.to_path_buf(),
                    line,
                },
            );
        }
    }
    Ok(())
}

fn parse_binding(item: &Item, file: &Path, source: &str) -> Result<BindingSpec, ConfigError> {
    if let Some(array) = item.as_array() {
        return Ok(BindingSpec::new(parse_chord_array(array, file, source)?));
    }
    if let Some(inline) = item.as_value().and_then(TomlValue::as_inline_table) {
        return parse_binding_table(inline, file, source);
    }
    if let Some(table) = item.as_table() {
        return parse_binding_table_from_table(table, file, source);
    }
    Err(
        ConfigError::new("a binding must be an array of keys or a table with `keys`")
            .at_file_line(file, item_line(source, item)),
    )
}

fn parse_binding_table(
    inline: &InlineTable,
    file: &Path,
    source: &str,
) -> Result<BindingSpec, ConfigError> {
    let keys = inline.get("keys").ok_or_else(|| {
        ConfigError::new("binding table needs a `keys` array")
            .at_file_line(file, span_line(source, inline.span()))
    })?;
    let array = keys.as_array().ok_or_else(|| {
        ConfigError::new("binding `keys` must be an array")
            .at_file_line(file, span_line(source, keys.span()))
    })?;
    let mut spec = BindingSpec::new(parse_chord_array(array, file, source)?);
    if let Some(modes) = inline.get("modes") {
        spec.modes = parse_modes(modes, file, source)?;
    }
    Ok(spec)
}

fn parse_binding_table_from_table(
    table: &Table,
    file: &Path,
    source: &str,
) -> Result<BindingSpec, ConfigError> {
    let keys = table.get("keys").ok_or_else(|| {
        ConfigError::new("binding table needs a `keys` array")
            .at_file_line(file, span_line(source, table.span()))
    })?;
    let array = keys.as_array().ok_or_else(|| {
        ConfigError::new("binding `keys` must be an array")
            .at_file_line(file, item_line(source, keys))
    })?;
    let mut spec = BindingSpec::new(parse_chord_array(array, file, source)?);
    if let Some(modes) = table.get("modes").and_then(Item::as_value) {
        spec.modes = parse_modes(modes, file, source)?;
    }
    Ok(spec)
}

fn parse_modes(item: &TomlValue, file: &Path, source: &str) -> Result<Vec<EditorMode>, ConfigError> {
    let array = item.as_array().ok_or_else(|| {
        ConfigError::new("`modes` must be an array")
            .at_file_line(file, span_line(source, item.span()))
    })?;
    let mut modes = Vec::new();
    for element in array.iter() {
        let text = element.as_str().ok_or_else(|| {
            ConfigError::new("`modes` entries must be strings")
                .at_file_line(file, span_line(source, element.span()))
        })?;
        modes.push(EditorMode::parse(text).map_err(|message| {
            ConfigError::new(message).at_file_line(file, span_line(source, element.span()))
        })?);
    }
    Ok(modes)
}

fn parse_chord_array(
    array: &Array,
    file: &Path,
    source: &str,
) -> Result<Vec<KeyChord>, ConfigError> {
    let mut keys = Vec::new();
    for element in array.iter() {
        let text = element.as_str().ok_or_else(|| {
            ConfigError::new("key entries must be strings")
                .at_file_line(file, span_line(source, element.span()))
        })?;
        keys.push(parse_chord(
            text,
            file,
            span_line(source, element.span()),
        )?);
    }
    Ok(keys)
}

fn parse_chord(text: &str, file: &Path, line: usize) -> Result<KeyChord, ConfigError> {
    KeyChord::parse(text).map_err(|error| {
        ConfigError::new(format!("invalid key `{text}`: {error}")).at_file_line(file, line)
    })
}

fn settings_table<'a>(
    item: &'a Item,
    name: &str,
    file: &Path,
    line: usize,
) -> Result<&'a Table, ConfigError> {
    item.as_table().ok_or_else(|| {
        ConfigError::new(format!("`{name}` must be a table")).at_file_line(file, line)
    })
}

fn string(item: &Item, what: &str, file: &Path, line: usize) -> Result<String, ConfigError> {
    item.as_str()
        .map(str::to_string)
        .ok_or_else(|| ConfigError::new(format!("`{what}` must be a string")).at_file_line(file, line))
}

fn integer(item: &Item, what: &str, file: &Path, line: usize) -> Result<u32, ConfigError> {
    let value = item.as_integer().ok_or_else(|| {
        ConfigError::new(format!("`{what}` must be an integer")).at_file_line(file, line)
    })?;
    u32::try_from(value)
        .map_err(|_| ConfigError::new(format!("`{what}` must be a non-negative integer")).at_file_line(file, line))
}

fn string_array(
    item: &Item,
    what: &str,
    file: &Path,
    source: &str,
) -> Result<Vec<String>, ConfigError> {
    let array = item.as_array().ok_or_else(|| {
        ConfigError::new(format!("`{what}` must be an array"))
            .at_file_line(file, item_line(source, item))
    })?;
    let mut out = Vec::new();
    for element in array.iter() {
        let text = element.as_str().ok_or_else(|| {
            ConfigError::new(format!("`{what}` entries must be strings"))
                .at_file_line(file, span_line(source, element.span()))
        })?;
        out.push(text.to_string());
    }
    Ok(out)
}

fn unknown_key(file: &Path, line: usize, table: &str, key: &str) -> ConfigError {
    ConfigError::new(format!("unknown setting `{table}.{key}`")).at_file_line(file, line)
}

fn apply_initialization(
    config: &mut Config,
    value: &serde_json::Value,
) -> Result<(), ConfigError> {
    let label = PathBuf::from("<initializationOptions>");
    if let Some(profile) = value.get("profile").and_then(serde_json::Value::as_str) {
        config.profile =
            Profile::parse(profile).map_err(|message| ConfigError::new(message).at_file(&label))?;
    }
    if let Some(trust) = value
        .get("trust_repo_config")
        .and_then(serde_json::Value::as_bool)
    {
        config.trust_repo_config = trust;
    }
    if let Some(keys) = value.get("keys") {
        config.keys.merge(keys_from_json(keys, &label)?);
    }
    Ok(())
}

fn keys_from_json(
    value: &serde_json::Value,
    file: &Path,
) -> Result<KeysConfig, ConfigError> {
    let mut keys = KeysConfig::default();
    let Some(scopes) = value.as_object() else {
        return Err(ConfigError::new("`keys` must be an object").at_file(file));
    };
    for (scope, entries) in scopes {
        let Some(entries) = entries.as_object() else {
            return Err(ConfigError::new(format!("`keys.{scope}` must be an object")).at_file(file));
        };
        let entry = keys.scopes.entry(scope.clone()).or_default();
        for (name, spec) in entries {
            if name == "unbind" {
                let Some(array) = spec.as_array() else {
                    return Err(
                        ConfigError::new(format!("`keys.{scope}.unbind` must be an array"))
                            .at_file(file),
                    );
                };
                for key in array {
                    let Some(text) = key.as_str() else {
                        return Err(ConfigError::new("`unbind` entries must be strings").at_file(file));
                    };
                    entry.unbind.push(parse_chord(text, file, 0)?);
                }
                continue;
            }
            let parsed = match spec {
                serde_json::Value::Array(items) => {
                    let mut chords = Vec::new();
                    for key in items {
                        let Some(text) = key.as_str() else {
                            return Err(ConfigError::new("key entries must be strings").at_file(file));
                        };
                        chords.push(parse_chord(text, file, 0)?);
                    }
                    BindingSpec::new(chords)
                }
                serde_json::Value::Object(table) => {
                    let Some(items) = table.get("keys").and_then(serde_json::Value::as_array) else {
                        return Err(
                            ConfigError::new("binding object needs a `keys` array").at_file(file),
                        );
                    };
                    let mut chords = Vec::new();
                    for key in items {
                        let Some(text) = key.as_str() else {
                            return Err(ConfigError::new("key entries must be strings").at_file(file));
                        };
                        chords.push(parse_chord(text, file, 0)?);
                    }
                    let mut spec = BindingSpec::new(chords);
                    if let Some(modes) = table.get("modes").and_then(serde_json::Value::as_array) {
                        spec.modes.clear();
                        for mode in modes {
                            let Some(text) = mode.as_str() else {
                                return Err(
                                    ConfigError::new("`modes` entries must be strings").at_file(file)
                                );
                            };
                            spec.modes.push(
                                EditorMode::parse(text).map_err(|message| {
                                    ConfigError::new(message).at_file(file)
                                })?,
                            );
                        }
                    }
                    spec
                }
                _ => {
                    return Err(ConfigError::new(
                        "a binding must be an array of keys or an object with `keys`",
                    )
                    .at_file(file))
                }
            };
            entry.commands.insert(name.clone(), parsed);
            entry.origins.insert(
                name.clone(),
                Origin {
                    file: file.to_path_buf(),
                    line: 0,
                },
            );
        }
    }
    Ok(keys)
}

fn resolve_include(including: &Path, raw: &str) -> PathBuf {
    let expanded = expand_vars(raw);
    let path = if let Some(rest) = expanded.strip_prefix("~/") {
        match std::env::var_os("HOME") {
            Some(home) => PathBuf::from(home).join(rest),
            None => PathBuf::from(rest),
        }
    } else if expanded == "~" {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("~"))
    } else {
        PathBuf::from(expanded)
    };
    if path.is_absolute() {
        path
    } else {
        including
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(path)
    }
}

/// Expand `$VAR` and `${VAR}` in an include path.
fn expand_vars(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        let braced = chars.peek() == Some(&'{');
        if braced {
            chars.next();
        }
        let mut name = String::new();
        while let Some(&next) = chars.peek() {
            if next.is_ascii_alphanumeric() || next == '_' {
                name.push(next);
                chars.next();
            } else {
                break;
            }
        }
        if braced {
            if chars.peek() == Some(&'}') {
                chars.next();
            }
        }
        if name.is_empty() {
            out.push('$');
        } else {
            out.push_str(&std::env::var(&name).unwrap_or_default());
        }
    }
    out
}

fn toml_error(file: &Path, source: &str, error: &toml_edit::TomlError) -> ConfigError {
    let line = error
        .span()
        .map(|span| span_line(source, Some(span)))
        .unwrap_or(1);
    ConfigError::new(error.message().to_string()).at_file_line(file, line)
}

fn item_line(source: &str, item: &Item) -> usize {
    span_line(source, item.span())
}

fn span_line(source: &str, span: Option<std::ops::Range<usize>>) -> usize {
    let Some(span) = span else {
        return 1;
    };
    let end = span.start.min(source.len());
    source[..end].bytes().filter(|byte| *byte == b'\n').count() + 1
}

/// A loaded configuration plus its resolved keymaps, with hot reload.
pub struct ConfigService {
    inner: Mutex<ServiceState>,
}

struct ServiceState {
    options: LoadOptions,
    config: Arc<Config>,
    keymaps: Arc<BTreeMap<String, Keymap>>,
    error: Option<ConfigError>,
    observed: u64,
}

/// What a reload attempt did.
#[derive(Debug, Clone, Default)]
pub struct ReloadOutcome {
    /// The watched files changed (even if the new config was invalid).
    pub changed: bool,
    /// Errors to show; the previous configuration is still active.
    pub errors: Vec<ConfigError>,
}

impl ConfigService {
    /// Load a configuration and resolve its keymaps.
    pub fn load(options: LoadOptions) -> Result<Self, ConfigError> {
        let config = load_config(&options)?;
        let keymaps = keymap::validate(&config)?;
        let observed = hash_files(&config.files);
        Ok(Self {
            inner: Mutex::new(ServiceState {
                options,
                config: Arc::new(config),
                keymaps: Arc::new(keymaps),
                error: None,
                observed,
            }),
        })
    }

    /// A service holding the defaults, for when startup loading failed.
    pub fn with_defaults(options: LoadOptions, error: ConfigError) -> Self {
        let config = Config::default();
        let keymaps = keymap::validate(&config).unwrap_or_default();
        Self {
            inner: Mutex::new(ServiceState {
                options,
                config: Arc::new(config),
                keymaps: Arc::new(keymaps),
                error: Some(error),
                observed: 0,
            }),
        }
    }

    /// Apply the client's `initializationOptions` and reload.
    pub fn set_initialization(&self, init: Option<serde_json::Value>) -> ReloadOutcome {
        {
            let mut state = self.lock();
            state.options.initialization = init;
        }
        self.reload(true)
    }

    /// Reload if a watched file changed (or unconditionally when `force`).
    pub fn reload(&self, force: bool) -> ReloadOutcome {
        let mut state = self.lock();
        let options = state.options.clone();
        let observed = hash_files(&state.config.files);
        if !force && observed == state.observed {
            return ReloadOutcome::default();
        }
        match load_config(&options).and_then(|config| {
            let keymaps = keymap::validate(&config)?;
            Ok((config, keymaps))
        }) {
            Ok((config, keymaps)) => {
                state.observed = hash_files(&config.files);
                state.error = None;
                state.config = Arc::new(config);
                state.keymaps = Arc::new(keymaps);
                ReloadOutcome {
                    changed: true,
                    errors: Vec::new(),
                }
            }
            Err(error) => {
                // Keep the previous config so the user's bindings do not vanish.
                state.observed = observed;
                state.error = Some(error.clone());
                ReloadOutcome {
                    changed: true,
                    errors: vec![error],
                }
            }
        }
    }

    /// The current configuration.
    pub fn config(&self) -> Arc<Config> {
        self.lock().config.clone()
    }

    /// The resolved keymap for a scope.
    pub fn keymap(&self, scope: &str) -> Option<Keymap> {
        self.lock().keymaps.get(scope).cloned()
    }

    /// Every resolved keymap.
    pub fn keymaps(&self) -> Arc<BTreeMap<String, Keymap>> {
        self.lock().keymaps.clone()
    }

    /// The files the watcher should poll.
    pub fn files(&self) -> Vec<PathBuf> {
        self.lock().config.files.clone()
    }

    /// The last reload error, if the current config is the previous good one.
    pub fn last_error(&self) -> Option<ConfigError> {
        self.lock().error.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ServiceState> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn hash_files(files: &[PathBuf]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for file in files {
        file.hash(&mut hasher);
        match std::fs::read(file) {
            Ok(bytes) => {
                bytes.hash(&mut hasher);
            }
            Err(_) => 0u8.hash(&mut hasher),
        }
    }
    hasher.finish()
}
