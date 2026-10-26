//! Configuration loading: includes, layering, validation and hot reload.

use magit_lsp::config::{
    default_config_path, load_config, resolve_config_path_from, ConfigError, ConfigService,
    LoadOptions, Profile,
};
use magit_lsp::keymap::Keymap;
use std::ffi::OsStr;
use std::path::Path;
use tempfile::TempDir;

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

fn load(path: &Path) -> Result<magit_lsp::config::Config, ConfigError> {
    load_config(&LoadOptions {
        config_path: Some(path.to_path_buf()),
        ..Default::default()
    })
}

fn status_keymap(config: &magit_lsp::config::Config, command: &str) -> Vec<String> {
    let map = Keymap::resolve("status", config.profile, config).unwrap();
    map.command(command).map(|b| b.keys.clone()).unwrap_or_default()
}

#[test]
fn includes_load_first_and_the_including_file_overrides_them() {
    let dir = TempDir::new().unwrap();
    write(&dir.path().join("a.toml"), "[keys.status]\nstage = [\"a\"]\n");
    write(&dir.path().join("b.toml"), "[keys.status]\nstage = [\"b\"]\n");
    let main = dir.path().join("main.toml");
    write(
        &main,
        "include = [\"a.toml\", \"b.toml\"]\n[keys.status]\nunstage = [\"c\"]\n",
    );

    let config = load(&main).unwrap();
    assert_eq!(status_keymap(&config, "status.stage"), vec!["b"]);
    assert_eq!(status_keymap(&config, "status.unstage"), vec!["c"]);
}

#[test]
fn later_includes_override_earlier_ones() {
    let dir = TempDir::new().unwrap();
    write(&dir.path().join("a.toml"), "[keys.status]\nstage = [\"a\"]\n");
    write(&dir.path().join("b.toml"), "[keys.status]\nstage = [\"b\"]\n");
    let main = dir.path().join("main.toml");
    write(&main, "include = [\"a.toml\", \"b.toml\"]\n");

    let config = load(&main).unwrap();
    assert_eq!(status_keymap(&config, "status.stage"), vec!["b"]);
}

#[test]
fn tables_merge_and_arrays_replace() {
    let dir = TempDir::new().unwrap();
    write(
        &dir.path().join("a.toml"),
        "[keys.status]\nstage = [\"a\", \"b\"]\n",
    );
    let main = dir.path().join("main.toml");
    write(
        &main,
        "include = [\"a.toml\"]\n[keys.status]\nstage = [\"c\"]\nunstage = [\"u\"]\n",
    );

    let config = load(&main).unwrap();
    assert_eq!(status_keymap(&config, "status.stage"), vec!["c"]);
    assert_eq!(status_keymap(&config, "status.unstage"), vec!["u"]);
}

#[test]
fn relative_absolute_tilde_and_env_includes_resolve() {
    const CHILD: &str = "MAGIT_LSP_CONFIG_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let env_dir = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        write(
            &env_dir.path().join("env.toml"),
            "[keys.status]\nunstage = [\"b\"]\n",
        );
        write(&home.path().join("inc.toml"), "[keys.status]\nstage-all = [\"t\"]\n");
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "relative_absolute_tilde_and_env_includes_resolve"])
            .env(CHILD, "1")
            .env("HOME", home.path())
            .env("MAGIT_LSP_TEST_INCLUDE", env_dir.path().join("env.toml"))
            .env_remove("XDG_CONFIG_HOME")
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "child output: {}{}",
            String::from_utf8_lossy(&status.stdout),
            String::from_utf8_lossy(&status.stderr)
        );
        return;
    }

    let dir = TempDir::new().unwrap();
    let abs = TempDir::new().unwrap();
    write(&abs.path().join("abs.toml"), "[keys.status]\nstage = [\"a\"]\n");

    let main = dir.path().join("main.toml");
    write(
        &main,
        &format!(
            "include = [\"rel.toml\", {:?}, \"$MAGIT_LSP_TEST_INCLUDE\", \"${{MAGIT_LSP_TEST_INCLUDE}}\", \"~/inc.toml\"]\n[keys.status]\nstage = [\"c\"]\n",
            abs.path().join("abs.toml").to_string_lossy(),
        ),
    );
    write(&dir.path().join("rel.toml"), "[keys.status]\nunstage-all = [\"d\"]\n");
    let env_path = std::env::var("MAGIT_LSP_TEST_INCLUDE").unwrap();
    write(Path::new(&env_path), "[keys.status]\nunstage = [\"b\"]\n");

    // Exercise each expansion independently so one cannot mask the other.
    for include in ["$MAGIT_LSP_TEST_INCLUDE", "${MAGIT_LSP_TEST_INCLUDE}"] {
        let only_env = dir.path().join("only-env.toml");
        write(&only_env, &format!("include = [{include:?}]\n"));
        assert_eq!(status_keymap(&load(&only_env).unwrap(), "status.unstage"), vec!["b"]);
    }

    let config = load(&main).unwrap();
    assert_eq!(status_keymap(&config, "status.stage"), vec!["c"]);
    assert_eq!(status_keymap(&config, "status.unstage"), vec!["b"]);
    assert_eq!(status_keymap(&config, "status.stage-all"), vec!["t"]);
    assert_eq!(status_keymap(&config, "status.unstage-all"), vec!["d"]);
}

#[test]
fn xdg_config_home_sets_the_default_config_path() {
    const CHILD: &str = "MAGIT_LSP_XDG_CONFIG_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let config_home = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "xdg_config_home_sets_the_default_config_path"])
            .env(CHILD, "1")
            .env("XDG_CONFIG_HOME", config_home.path())
            .env("HOME", home.path())
            .env_remove("MAGIT_LSP_CONFIG")
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "child output: {}{}",
            String::from_utf8_lossy(&status.stdout),
            String::from_utf8_lossy(&status.stderr)
        );
        return;
    }
    let expected = Path::new(&std::env::var_os("XDG_CONFIG_HOME").unwrap())
        .join("magit-lsp/config.toml");
    assert_eq!(default_config_path(), Some(expected.clone()));
    write(&expected, "[diff]\ncontext = 9\n");
    assert_eq!(load_config(&LoadOptions::default()).unwrap().diff.context, 9);
}

#[test]
fn nested_relative_includes_use_the_including_files_directory() {
    let dir = TempDir::new().unwrap();
    let main = dir.path().join("main.toml");
    write(&main, "include = [\"nested/first.toml\"]\n");
    write(
        &dir.path().join("nested/first.toml"),
        "include = [\"../shared/second.toml\"]\n",
    );
    write(
        &dir.path().join("shared/second.toml"),
        "[keys.status]\nstage = [\"n\"]\n",
    );
    let config = load(&main).unwrap();
    assert_eq!(status_keymap(&config, "status.stage"), vec!["n"]);
}

#[test]
fn optional_include_may_be_missing() {
    let dir = TempDir::new().unwrap();
    let main = dir.path().join("main.toml");
    write(&main, "include_optional = [\"nope.toml\"]\n");
    load(&main).unwrap();
}

#[test]
fn required_include_missing_names_file_and_line() {
    let dir = TempDir::new().unwrap();
    let main = dir.path().join("main.toml");
    write(&main, "include = [\"nope.toml\"]\n");
    let error = load(&main).unwrap_err().to_string();
    assert!(error.contains("main.toml:1"), "{error}");
    assert!(error.contains("not found"), "{error}");
}

#[test]
fn include_cycles_report_the_chain() {
    let dir = TempDir::new().unwrap();
    write(&dir.path().join("a.toml"), "include = [\"b.toml\"]\n");
    write(&dir.path().join("b.toml"), "include = [\"a.toml\"]\n");
    let main = dir.path().join("main.toml");
    write(&main, "include = [\"a.toml\"]\n");

    let error = load(&main).unwrap_err().to_string();
    assert!(error.contains("cycle"), "{error}");
    assert!(error.contains("a.toml") && error.contains("b.toml"), "{error}");
}

#[test]
fn parse_errors_name_file_and_line() {
    let dir = TempDir::new().unwrap();
    let main = dir.path().join("main.toml");
    write(&main, "\n[keys.common]\nconfirm = [\"Enter\"\n");
    let error = load(&main).unwrap_err();
    assert_eq!(error.file.as_deref(), Some(main.as_path()));
    assert_eq!(error.line, Some(3), "{error}");
    assert!(error.to_string().contains("main.toml:3:"), "{error}");
}

#[test]
fn unknown_settings_name_file_and_line() {
    let dir = TempDir::new().unwrap();
    let main = dir.path().join("main.toml");
    write(&main, "bogus = 1\n");
    let error = load(&main).unwrap_err().to_string();
    assert!(error.contains("main.toml:1"), "{error}");
    assert!(error.contains("unknown setting"), "{error}");
}

#[test]
fn unknown_command_names_name_file_and_line() {
    let dir = TempDir::new().unwrap();
    let main = dir.path().join("main.toml");
    write(&main, "[keys.status]\nnot-a-command = [\"q\"]\n");
    let error = load(&main).unwrap_err().to_string();
    assert!(error.contains("main.toml:2"), "{error}");
    assert!(error.contains("unknown command"), "{error}");
}

#[test]
fn unbind_removes_a_profile_binding() {
    let dir = TempDir::new().unwrap();
    let main = dir.path().join("main.toml");
    write(&main, "[keys.status]\nunbind = [\"x\"]\n");
    let config = load(&main).unwrap();
    assert!(status_keymap(&config, "status.discard").is_empty());
    assert_eq!(status_keymap(&config, "status.stage"), vec!["s"]);
}

#[test]
fn config_flag_wins_over_environment() {
    let cli = Path::new("/cli/config.toml");
    let env = OsStr::new("/env/config.toml");
    let default = Some("/default/config.toml".into());
    assert_eq!(
        resolve_config_path_from(Some(cli), Some(env), default.clone()),
        Some("/cli/config.toml".into())
    );
    assert_eq!(
        resolve_config_path_from(None, Some(env), default.clone()),
        Some("/env/config.toml".into())
    );
    assert_eq!(resolve_config_path_from(None, None, default), Some("/default/config.toml".into()));
}

#[test]
fn initialization_options_override_user_config() {
    let dir = TempDir::new().unwrap();
    let main = dir.path().join("config.toml");
    write(&main, "profile = \"vim\"\ntrust_repo_config = true\n[keys.status]\nstage = [\"u\"]\n");
    let config = load_config(&LoadOptions {
        config_path: Some(main),
        initialization: Some(serde_json::json!({
            "profile": "standard",
            "trust_repo_config": false,
            "keys": {"status": {"stage": ["i"]}}
        })),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(config.profile, Profile::Standard);
    assert!(!config.trust_repo_config);
    assert_eq!(status_keymap(&config, "status.stage"), vec!["i"]);
}

#[test]
fn per_repo_config_is_ignored_without_trust() {
    let dir = TempDir::new().unwrap();
    let repo = TempDir::new().unwrap();
    write(
        &repo.path().join(".magit-lsp.toml"),
        "[keys.status]\nstage = [\"z\"]\n",
    );

    let untrusted = dir.path().join("untrusted.toml");
    write(&untrusted, "profile = \"vim\"\n");
    let config = load_config(&LoadOptions {
        config_path: Some(untrusted.clone()),
        repo_path: Some(repo.path().to_path_buf()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(status_keymap(&config, "status.stage"), vec!["s"]);

    let trusted = dir.path().join("trusted.toml");
    write(&trusted, "profile = \"vim\"\ntrust_repo_config = true\n");
    let config = load_config(&LoadOptions {
        config_path: Some(trusted),
        repo_path: Some(repo.path().to_path_buf()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(status_keymap(&config, "status.stage"), vec!["z"]);
}

#[test]
fn trusted_repo_config_applies_keys_and_display_but_not_profile_or_trust() {
    let dir = TempDir::new().unwrap();
    let repo = TempDir::new().unwrap();
    let global = dir.path().join("global.toml");
    write(
        &global,
        "profile = \"vim\"\ntrust_repo_config = false\n[keys.status]\nstage = [\"u\"]\n[diff]\ncontext = 7\n[sections]\ndefault_visibility = \"hunks\"\n",
    );
    write(
        &repo.path().join(".magit-lsp.toml"),
        "profile = \"vim\"\ntrust_repo_config = false\n[keys.status]\nstage = [\"z\"]\n[sections]\ndefault_visibility = \"files\"\n[diff]\ncontext = 11\n",
    );
    let config = load_config(&LoadOptions {
        config_path: Some(global),
        repo_path: Some(repo.path().to_path_buf()),
        initialization: Some(serde_json::json!({
            "profile": "standard",
            "trust_repo_config": true,
            "keys": {"status": {"stage": ["i"]}}
        })),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(config.profile, Profile::Standard);
    assert!(config.trust_repo_config);
    assert_eq!(status_keymap(&config, "status.stage"), vec!["z"]);
    assert_eq!(config.sections.default_visibility.as_deref(), Some("files"));
    assert_eq!(config.diff.context, 11);
}

#[test]
fn profile_defaults_to_vim_and_typed_settings_parse() {
    let dir = TempDir::new().unwrap();
    let main = dir.path().join("main.toml");
    write(
        &main,
        "[diff]\ncontext = 7\n[ui]\nconfirm_discard = false\n[log]\ndefault_args = [\"--graph\"]\nmargin = 5\n",
    );
    let config = load(&main).unwrap();
    assert_eq!(config.profile, Profile::Vim);
    assert_eq!(config.diff.context, 7);
    assert!(!config.ui.confirm_discard);
    assert_eq!(config.log.default_args, vec!["--graph"]);
    assert_eq!(config.log.margin, Some(5));
}

#[test]
fn obsolete_menus_save_scope_is_rejected_with_its_exact_name() {
    let dir = TempDir::new().unwrap();
    let main = dir.path().join("main.toml");
    write(&main, "[diff]\ncontext = 7\n[menus]\nsave_scope = \"repo\"\n");
    let error = load(&main).unwrap_err();
    assert!(
        error.to_string().contains("main.toml:4: unknown setting `menus.save_scope`"),
        "{error}"
    );
}

#[test]
fn hot_reload_picks_up_changes_and_keeps_the_previous_config_on_error() {
    let dir = TempDir::new().unwrap();
    let main = dir.path().join("config.toml");
    write(&main, "[keys.status]\nstage = [\"q\"]\n");

    let service = ConfigService::load(LoadOptions {
        config_path: Some(main.clone()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(service.keymap("status").unwrap().command("status.stage").unwrap().keys, vec!["q"]);

    write(&main, "[keys.status]\nstage = [\"w\"]\n");
    let mut reloaded = false;
    for _ in 0..40 {
        if service.reload(false).changed {
            reloaded = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(reloaded, "reload never observed the change");
    assert_eq!(service.keymap("status").unwrap().command("status.stage").unwrap().keys, vec!["w"]);

    write(&main, "[keys.common\nconfirm = [\"Enter\"]\n");
    let outcome = service.reload(false);
    assert_eq!(outcome.errors.len(), 1, "exactly one error");
    assert!(
        outcome.errors[0].to_string().contains("config.toml:"),
        "{}",
        outcome.errors[0]
    );
    assert_eq!(service.keymap("status").unwrap().command("status.stage").unwrap().keys, vec!["w"]);
}
