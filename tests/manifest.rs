//! Static keymap manifest tests: per-format structure, snapshots and the
//! VSCode workbench deny-list.
//!
//! Snapshots live in `tests/snapshots/` and are compared byte for byte.
//! Regenerate them with `UPDATE_SNAPSHOTS=1 cargo test --test manifest`.

use magit_lsp::config::{BindingSpec, Config, Profile};
use magit_lsp::keys::KeyChord;
use magit_lsp::manifest::{self, Format, WORKBENCH_ALLOW, WORKBENCH_DENY_LIST};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

const PROFILES: [Profile; 2] = [Profile::Standard, Profile::Vim];

fn manifest_for(profile: Profile) -> manifest::Manifest {
    manifest::build(&[profile], &Config::default()).expect("manifest builds")
}

/// Compare `actual` against `tests/snapshots/<name>`, rewriting on
/// `UPDATE_SNAPSHOTS=1`.
fn snapshot(name: &str, actual: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots")
        .join(name);
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::create_dir_all(path.parent().expect("snapshot dir")).expect("create snapshot dir");
        std::fs::write(&path, actual).expect("write snapshot");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "read snapshot {}: {error}; run with UPDATE_SNAPSHOTS=1",
            path.display()
        )
    });
    assert_eq!(
        expected,
        actual,
        "snapshot {} changed; run with UPDATE_SNAPSHOTS=1",
        path.display()
    );
}

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_magit-lsp"))
}

/// Keep the host's config out of CLI tests.
fn isolated() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().expect("temp dir");
    dir
}

fn isolated_command(dir: &Path) -> Command {
    let mut command = binary();
    command
        .env("XDG_CONFIG_HOME", dir)
        .env_remove("MAGIT_LSP_CONFIG");
    command
}

#[test]
fn json_matches_snapshots_and_has_the_documented_shape() {
    for profile in PROFILES {
        let text = manifest::json(&manifest_for(profile));
        snapshot(&format!("manifest-{}.json", profile.as_str()), &text);

        let value: Value = serde_json::from_str(&text).expect("manifest is JSON");
        let profiles = value["profiles"].as_array().expect("profiles array");
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0]["profile"], profile.as_str());
        let scopes = profiles[0]["scopes"].as_array().expect("scopes array");
        assert_eq!(scopes.len(), magit_lsp::config::VIEW_SCOPES.len());
        let binding = &scopes[0]["bindings"][0];
        for field in [
            "command", "keys", "modes", "label", "doc", "group", "status", "scope",
        ] {
            assert!(binding.get(field).is_some(), "missing `{field}`: {binding}");
        }
        assert!(text.ends_with('\n'), "JSON manifest needs a trailing newline");
    }
}

#[test]
fn vscode_keys_follow_the_section_3_8_table() {
    for (canonical, expected) in [
        ("S", "shift+s"),
        ("?", "shift+/"),
        ("Enter", "enter"),
        ("alt+ArrowDown", "alt+down"),
        ("ctrl+Enter", "ctrl+enter"),
        ("ctrl+c", "ctrl+c"),
        ("Escape", "escape"),
        ("g r", "g r"),
    ] {
        assert_eq!(
            KeyChord::parse(canonical).expect("key parses").to_vscode(),
            expected,
            "VSCode spelling of `{canonical}`"
        );
    }
}

#[test]
fn neovim_keys_follow_the_section_3_8_table() {
    for (canonical, expected) in [
        ("Enter", "<CR>"),
        ("Escape", "<Esc>"),
        ("Tab", "<Tab>"),
        ("shift+Tab", "<S-Tab>"),
        ("Delete", "<Del>"),
        ("Backspace", "<BS>"),
        ("ArrowUp", "<Up>"),
        ("ctrl+c", "<C-c>"),
        ("alt+ArrowDown", "<A-Down>"),
        ("ctrl+Enter", "<C-CR>"),
        ("g g", "gg"),
        ("z a", "za"),
    ] {
        assert_eq!(
            KeyChord::parse(canonical).expect("key parses").to_neovim(),
            expected,
            "Neovim spelling of `{canonical}`"
        );
    }
}

#[test]
fn vscode_matches_snapshots() {
    for profile in PROFILES {
        let text = manifest::vscode(&manifest_for(profile));
        snapshot(&format!("keybindings-{}.json", profile.as_str()), &text);
        let entries: Vec<Value> = serde_json::from_str(&text).expect("keybindings are JSON");
        assert!(entries.iter().all(|entry| entry["command"] == "magit.invoke"
            && entry["args"]["command"].as_str().is_some_and(|id| magit_lsp::commands::registry().get(id).is_some())));
        for entry in &entries {
            assert!(entry["args"]["command"].is_string(), "entry: {entry}");
            assert!(
                entry["when"]
                    .as_str()
                    .expect("when clause")
                    .starts_with("magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && magit.scope == '"),
                "entry: {entry}"
            );
        }
    }

    // `S` (stage-all) keeps its args but becomes `shift+s`.
    let entries = manifest::vscode_entries(&manifest_for(Profile::Standard));
    let stage_all = entries
        .iter()
        .find(|entry| entry.arg_key == "S" && entry.scope == "status")
        .expect("status.stage-all binding");
    assert_eq!(stage_all.key, "shift+s");
}

#[test]
fn contributed_vscode_payloads_are_canonical_server_commands() {
    let package: Value = serde_json::from_str(include_str!("../clients/vscode/package.json"))
        .expect("extension package is JSON");
    let bindings = package["contributes"]["keybindings"].as_array().expect("bindings");
    for binding in bindings {
        assert_eq!(binding["command"], "magit.invoke");
        let id = binding["args"]["command"].as_str().expect("canonical ID");
        assert!(magit_lsp::commands::registry().get(id).is_some(), "unknown server command: {id}");
    }
    for (key, id) in [("f5", "common.refresh"), ("tab", "common.toggle-section")] {
        assert!(bindings.iter().any(|binding| binding["key"] == key && binding["args"]["command"] == id));
    }
}

#[test]
fn neovim_matches_snapshots() {
    for profile in PROFILES {
        let text = manifest::neovim(&manifest_for(profile));
        snapshot(&format!("keymap-{}.neo.json", profile.as_str()), &text);
        let entries: Vec<Value> = serde_json::from_str(&text).expect("bindings are JSON");
        assert!(entries.iter().all(|entry| entry["lhs"].is_string()));
    }

    let entries = manifest::neovim_entries(&manifest_for(Profile::Vim));
    let refresh = entries
        .iter()
        .find(|entry| entry.command == "common.refresh" && entry.scope == "common")
        .expect("vim common.refresh binding");
    assert_eq!(refresh.lhs, "gr");
}

#[test]
fn markdown_matches_snapshots() {
    for profile in PROFILES {
        let text = manifest::markdown(&manifest_for(profile));
        snapshot(&format!("keymap-{}.md", profile.as_str()), &text);
        assert!(text.contains(&format!("# magit-lsp keymap: {}", profile.as_str())));
        assert!(text.contains("## status"));
        assert!(text.contains("| Command | Keys | Status | Description |"));
    }
}

#[test]
fn shipped_profiles_never_shadow_workbench_shortcuts_without_allow() {
    for profile in PROFILES {
        let denied = manifest::denied_entries(&manifest_for(profile));
        assert!(
            denied.is_empty(),
            "{} shadows workbench keys without an allow entry: {denied:?}",
            profile.as_str()
        );
    }
}

#[test]
fn allow_entries_are_scoped_with_reasons() {
    assert!(WORKBENCH_DENY_LIST.contains(&"ctrl+t"));
    assert!(WORKBENCH_DENY_LIST.contains(&"ctrl+c"));
    assert!(WORKBENCH_ALLOW.iter().any(|allow| {
        allow.key == "ctrl+t" && allow.scope.is_none() && !allow.reason.is_empty()
    }));
    assert!(WORKBENCH_ALLOW.iter().any(|allow| {
        allow.key == "ctrl+c" && allow.scope.is_none() && !allow.reason.is_empty()
    }));
    assert!(WORKBENCH_ALLOW.iter().any(|allow| {
        allow.key == "ctrl+s" && allow.scope == Some("menu") && !allow.reason.is_empty()
    }));
    assert!(WORKBENCH_ALLOW.iter().any(|allow| {
        allow.key == "ctrl+r" && allow.scope == Some("menu") && !allow.reason.is_empty()
    }));
}

#[test]
fn deny_list_detects_an_injected_key_without_an_allow_entry() {
    let mut config = Config {
        profile: Profile::Standard,
        ..Default::default()
    };
    config
        .keys
        .scopes
        .entry("status".to_string())
        .or_default()
        .commands
        .insert(
            "stage".to_string(),
            BindingSpec::new(vec![KeyChord::parse("ctrl+p").expect("key parses")]),
        );

    let manifest = manifest::build(&[Profile::Standard], &config).expect("manifest builds");
    let denied = manifest::denied_entries(&manifest);
    assert!(
        denied
            .iter()
            .any(|entry| entry.key == "ctrl+p" && entry.scope == "status"),
        "expected ctrl+p in status to be denied: {denied:?}"
    );
}

#[test]
fn version_help_and_unknown_subcommands() {
    let output = binary().arg("--version").output().expect("run --version");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("magit-lsp "));

    let output = binary().arg("--help").output().expect("run --help");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("keymap"));

    let output = binary().arg("nope").output().expect("run unknown subcommand");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Usage:"));
}

#[test]
fn keymap_check_passes_for_shipped_profiles() {
    let tmp = isolated();
    let output = isolated_command(tmp.path())
        .args(["keymap", "--check"])
        .output()
        .expect("run keymap --check");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "OK");
}

/// A `file:line` prefix on the first line of `stderr`.
fn has_file_line(stderr: &str, file: &str) -> bool {
    stderr.lines().any(|line| {
        line.contains(file)
            && line
                .split_once(file)
                .and_then(|(_, rest)| rest.strip_prefix(':'))
                .and_then(|rest| rest.chars().next())
                .is_some_and(|c| c.is_ascii_digit())
    })
}

#[test]
fn keymap_check_reports_conflicts_with_file_and_line() {
    let tmp = isolated();
    let config = tmp.path().join("conflict.toml");
    std::fs::write(&config, "[keys.status]\nstage = [\"q\"]\nunstage = [\"q\"]\n")
        .expect("write config");

    let output = binary()
        .args(["keymap", "--check", "--config"])
        .arg(&config)
        .output()
        .expect("run keymap --check");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("bound to both"), "stderr: {stderr}");
    assert!(has_file_line(&stderr, "conflict.toml"), "stderr: {stderr}");
}

#[test]
fn keymap_check_reports_unknown_commands_with_file_and_line() {
    let tmp = isolated();
    let config = tmp.path().join("unknown.toml");
    std::fs::write(&config, "[keys.status]\nnope = [\"q\"]\n").expect("write config");

    let output = binary()
        .args(["keymap", "--check", "--config"])
        .arg(&config)
        .output()
        .expect("run keymap --check");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown command `nope`"), "stderr: {stderr}");
    assert!(has_file_line(&stderr, "unknown.toml"), "stderr: {stderr}");
}

#[test]
fn keymap_format_json_without_profile_emits_every_profile() {
    let tmp = isolated();
    let output = isolated_command(tmp.path())
        .args(["keymap", "--format", "json"])
        .output()
        .expect("run keymap --format json");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    assert_eq!(value["profiles"].as_array().expect("profiles").len(), 2);
}

#[test]
fn check_with_format_emits_only_when_valid() {
    let tmp = isolated();
    let output = isolated_command(tmp.path())
        .args(["keymap", "--check", "--format", "markdown", "--profile", "vim"])
        .output()
        .expect("run keymap --check --format markdown");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("# magit-lsp keymap: vim"), "stdout: {stdout}");

    let bad = tmp.path().join("bad.toml");
    std::fs::write(&bad, "[keys.status]\nnope = [\"q\"]\n").expect("write config");
    let output = binary()
        .args(["keymap", "--check", "--format", "markdown", "--config"])
        .arg(&bad)
        .output()
        .expect("run invalid keymap --check --format");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty(), "must not emit on a failed check");
}

#[test]
fn formats_parse_from_the_command_line() {
    for (name, format) in [
        ("json", Format::Json),
        ("vscode", Format::Vscode),
        ("neovim", Format::Neovim),
        ("markdown", Format::Markdown),
    ] {
        assert_eq!(Format::parse(name).expect("format parses"), format);
        assert_eq!(format.as_str(), name);
    }
    assert!(Format::parse("yaml").is_err());
}
