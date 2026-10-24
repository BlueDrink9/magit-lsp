//! Keymap resolution, profile coverage and the profile rules R1–R6.

use magit_lsp::commands;
use magit_lsp::config::{profile_keys, BindingSpec, Config, Profile};
use magit_lsp::keymap::{command_for_key, EditorMode, Keymap, Match};
use magit_lsp::keys::{Key, KeyChord, KeyStep};
use std::collections::BTreeSet;

fn binding(keys: &[&str]) -> BindingSpec {
    BindingSpec::new(
        keys.iter()
            .map(|key| KeyChord::parse(key).expect("test key"))
            .collect(),
    )
}

fn config_with(profile: Profile, scopes: &[(&str, &[(&str, &[&str])])]) -> Config {
    let mut config = Config {
        profile,
        ..Default::default()
    };
    for (scope, entries) in scopes {
        let entry = config.keys.scopes.entry(scope.to_string()).or_default();
        for (name, keys) in *entries {
            entry.commands.insert(name.to_string(), binding(keys));
        }
    }
    config
}

#[test]
fn every_profile_parses_and_covers_every_command() {
    for profile in [Profile::Standard, Profile::Vim] {
        let keys = profile_keys(profile).expect("profile parses");
        let mut declared = BTreeSet::new();
        for (scope, scope_keys) in &keys.scopes {
            for name in scope_keys.commands.keys() {
                let id = command_for_key(scope, name).unwrap_or_else(|| {
                    panic!(
                        "{}: `{name}` in scope `{scope}` is not a command",
                        profile.as_str()
                    )
                });
                declared.insert(id.to_string());
            }
        }
        for command in commands::registry().iter() {
            // Menu actions are bound by their menu item, not by a profile.
            if command.scope == commands::Scope::MenuAction {
                continue;
            }
            assert!(
                declared.contains(command.id),
                "{} profile does not bind or explicitly unbind `{}`",
                profile.as_str(),
                command.id
            );
        }
    }
}

#[test]
fn every_profile_resolves_without_conflicts() {
    for profile in [Profile::Standard, Profile::Vim] {
        let config = Config {
            profile,
            ..Default::default()
        };
        for scope in ["common", "status", "select", "menu", "commit-message"] {
            Keymap::resolve(scope, profile, &config)
                .unwrap_or_else(|e| panic!("{} scope `{scope}`: {e}", profile.as_str()));
        }
    }
}

fn profile_chords(profile: Profile) -> Vec<KeyChord> {
    let keys = profile_keys(profile).expect("profile parses");
    let mut chords = Vec::new();
    for scope_keys in keys.scopes.values() {
        for spec in scope_keys.commands.values() {
            chords.extend(spec.keys.iter().cloned());
        }
    }
    chords
}

fn vim_rule_violation(chords: &[KeyChord]) -> Option<String> {
    let reserved_motions: BTreeSet<KeyStep> = [
        "v", "V", "ctrl+v", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "h", "j", "k", "l",
        "G", "ctrl+d", "ctrl+u", "ctrl+f", "ctrl+b", "/", "H", "L",
    ]
    .into_iter()
    .map(|key| KeyChord::parse(key).expect("reserved key parses").steps()[0].clone())
    .collect();

    for chord in chords {
        let canonical = chord.to_canonical();
        if canonical == "g g" {
            return Some(format!("vim profile rebinds `{canonical}`"));
        }
        if reserved_motions.contains(&chord.steps()[0]) {
            return Some(format!(
                "vim profile rebinds reserved motion prefix `{:?}` in `{canonical}`",
                chord.steps()[0]
            ));
        }
    }
    None
}

#[test]
fn standard_respects_r1_no_emacs_prefixes_or_alt_letters() {
    for chord in profile_chords(Profile::Standard) {
        let steps = chord.steps();
        if steps.len() >= 2 && steps[0].ctrl && matches!(steps[0].key, Key::Char('c')) {
            panic!("Emacs `ctrl+c` prefix in standard: {chord}");
        }
        if steps[0].ctrl
            && !steps[0].alt
            && !steps[0].shift
            && !steps[0].meta
            && matches!(steps[0].key, Key::Char('x'))
        {
            panic!("Emacs `ctrl+x` prefix in standard: {chord}");
        }
        for step in steps {
            if step.alt && !step.ctrl && !step.meta {
                if let Key::Char(c) = step.key {
                    assert!(
                        !c.is_ascii_alphabetic(),
                        "`alt+<letter>` in standard: {chord}"
                    );
                }
            }
        }
    }
}

#[test]
fn vim_respects_r4_does_not_rebind_vim_motions() {
    let chords = profile_chords(Profile::Vim);
    assert_eq!(vim_rule_violation(&chords), None);
}

#[test]
fn vim_r4_validator_rejects_injected_canonical_gg_and_motion_prefixes() {
    for key in ["g g", "ctrl+d x", "h g"] {
        let injected = [KeyChord::parse(key).expect("injected key parses")];
        assert!(
            vim_rule_violation(&injected).is_some(),
            "validator must reject injected `{key}`"
        );
    }

    let allowed: Vec<_> = ["g r", "ctrl+h", "ctrl+shift+d x", "alt+g g"]
        .into_iter()
        .map(|key| KeyChord::parse(key).expect("permitted key parses"))
        .collect();
    assert_eq!(vim_rule_violation(&allowed), None);
}

#[test]
fn profiles_follow_r2_mnemonics_for_status_operations() {
    let expected = [
        ("status.stage", "s"),
        ("status.unstage", "u"),
        ("status.stage-all", "S"),
        ("status.unstage-all", "U"),
        ("open.commit", "c"),
        ("open.branch", "b"),
        ("open.diff", "d"),
        ("open.fetch", "f"),
        ("open.pull", "F"),
        ("open.push", "P"),
        ("open.merge", "m"),
        ("open.rebase", "r"),
        ("open.tag", "t"),
    ];
    // §10 documents Vim's `g l` exception to §5.2: native `l` stays a motion.
    for (profile, log_key) in [(Profile::Standard, "l"), (Profile::Vim, "g l")] {
        let config = Config {
            profile,
            ..Default::default()
        };
        let map = Keymap::resolve("status", profile, &config).expect("status resolves");
        for (command, key) in expected.into_iter().chain([("open.log", log_key)]) {
            assert_eq!(
                map.command(command).unwrap().keys,
                vec![key],
                "{}: {command} mnemonic",
                profile.as_str()
            );
        }
    }
}

#[test]
fn standard_resolved_common_confirm_and_cancel_use_direct_keys() {
    let config = Config {
        profile: Profile::Standard,
        ..Default::default()
    };
    let map = Keymap::resolve("common", Profile::Standard, &config).unwrap();
    assert_eq!(map.command("common.confirm").unwrap().keys, vec!["Enter"]);
    assert!(map
        .command("common.cancel")
        .unwrap()
        .keys
        .contains(&"Escape".to_string()));
}

#[test]
fn detail_level_keys_are_absolute_in_standard_and_stepwise_in_vim() {
    let standard = Keymap::resolve(
        "common",
        Profile::Standard,
        &Config {
            profile: Profile::Standard,
            ..Default::default()
        },
    )
    .unwrap();
    for level in 1..=4 {
        assert_eq!(
            standard
                .command(&format!("common.detail-level-{level}"))
                .unwrap()
                .keys,
            vec![format!("alt+{level}")]
        );
    }
    let vim = Keymap::resolve(
        "common",
        Profile::Vim,
        &Config {
            profile: Profile::Vim,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(vim.command("common.more-detail").unwrap().keys, vec!["z r"]);
    assert_eq!(vim.command("common.less-detail").unwrap().keys, vec!["z m"]);
    assert_eq!(vim.command("common.show-all").unwrap().keys, vec!["z R"]);
    assert_eq!(vim.command("common.hide-all").unwrap().keys, vec!["z M"]);
}

#[test]
fn ctrl_c_is_cancel_in_every_non_editable_scope_for_both_profiles() {
    // §3.10/§5.5 makes commit-message the sole editable scope; ctrl+c there is
    // intentionally not cancel because standard editors use it to copy.
    for profile in [Profile::Standard, Profile::Vim] {
        for scope in ["common", "status", "select", "menu", "process-log"] {
            let config = Config {
                profile,
                ..Default::default()
            };
            let map = Keymap::resolve(scope, profile, &config)
                .unwrap_or_else(|error| panic!("{} {scope}: {error}", profile.as_str()));
            let cancel = map.command("common.cancel").unwrap_or_else(|| {
                panic!("{} {scope}: common.cancel must resolve", profile.as_str())
            });
            assert!(
                cancel.keys.iter().any(|key| key == "ctrl+c"),
                "{} {scope}: ctrl+c must resolve to common.cancel; got {:?}",
                profile.as_str(),
                cancel.keys
            );
            let steps = KeyChord::parse("ctrl+c").unwrap().steps().to_vec();
            for mode in [EditorMode::Normal, EditorMode::Visual] {
                assert_eq!(
                    map.lookup(&steps, mode),
                    Match::Exact("common.cancel".to_string()),
                    "{} {scope}: ctrl+c cancels in {mode:?}",
                    profile.as_str()
                );
            }
        }
    }
}

#[test]
fn resolution_order_is_scope_user_scope_profile_common_user_common_profile() {
    // Scope user config beats the scope profile.
    let config = config_with(Profile::Vim, &[("status", &[("stage", &["q"])])]);
    let map = Keymap::resolve("status", Profile::Vim, &config).unwrap();
    assert_eq!(map.command("status.stage").unwrap().keys, vec!["q"]);

    // Scope profile beats common user config.
    let config = config_with(Profile::Vim, &[("common", &[("confirm", &["x"])])]);
    let map = Keymap::resolve("select", Profile::Vim, &config).unwrap();
    assert_eq!(map.command("common.confirm").unwrap().keys, vec!["Enter"]);

    // Common user config beats the common profile.
    let config = config_with(Profile::Vim, &[("common", &[("refresh", &["F6"])])]);
    let map = Keymap::resolve("status", Profile::Vim, &config).unwrap();
    assert_eq!(map.command("common.refresh").unwrap().keys, vec!["F6"]);
}

#[test]
fn dispatch_is_reachable_from_status_under_both_profiles() {
    for (profile, expected) in [
        (Profile::Standard, vec!["?", "h"]),
        (Profile::Vim, vec!["?", "g ?"]),
    ] {
        let config = Config {
            profile,
            ..Default::default()
        };
        let map = Keymap::resolve("status", profile, &config).unwrap();
        let binding = map
            .command("open.dispatch")
            .unwrap_or_else(|| panic!("{}: open.dispatch is bound in status", profile.as_str()));
        assert_eq!(
            binding.keys,
            expected,
            "{}: status dispatch keys",
            profile.as_str()
        );

        for key in expected {
            let steps = KeyChord::parse(key).unwrap().steps().to_vec();
            assert_eq!(
                map.lookup(&steps, EditorMode::Normal),
                Match::Exact("open.dispatch".to_string()),
                "{}: `{key}` opens the dispatch",
                profile.as_str()
            );
        }
    }

    // The vim profile keeps `h` a motion (R4).
    let config = Config {
        profile: Profile::Vim,
        ..Default::default()
    };
    let map = Keymap::resolve("status", Profile::Vim, &config).unwrap();
    let h = vec![KeyStep::plain(Key::Char('h'))];
    assert_eq!(map.lookup(&h, EditorMode::Normal), Match::None);
}

#[test]
fn chord_prefix_matching_prefers_longer_chords() {
    let config = Config {
        profile: Profile::Vim,
        ..Default::default()
    };
    let map = Keymap::resolve("status", Profile::Vim, &config).unwrap();

    let g = vec![KeyStep::plain(Key::Char('g'))];
    assert_eq!(map.lookup(&g, EditorMode::Normal), Match::Prefix);

    let gr = KeyChord::parse("g r").unwrap().steps().to_vec();
    assert_eq!(
        map.lookup(&gr, EditorMode::Normal),
        Match::Exact("common.refresh".to_string())
    );

    let nothing = vec![KeyStep::plain(Key::Char('Q'))];
    assert_eq!(map.lookup(&nothing, EditorMode::Normal), Match::None);
}

#[test]
fn conflicts_at_the_same_level_are_reported() {
    let config = config_with(
        Profile::Standard,
        &[("status", &[("stage", &["q"]), ("unstage", &["q"])])],
    );
    let error = Keymap::resolve("status", Profile::Standard, &config).unwrap_err();
    assert!(
        error.to_string().contains("bound to both"),
        "unexpected error: {error}"
    );
}

#[test]
fn unbind_removes_a_profile_binding() {
    let mut config = Config {
        profile: Profile::Vim,
        ..Default::default()
    };
    config
        .keys
        .scopes
        .entry("status".to_string())
        .or_default()
        .unbind
        .push(KeyChord::parse("x").unwrap());

    let map = Keymap::resolve("status", Profile::Vim, &config).unwrap();
    assert!(map.command("status.discard").unwrap().keys.is_empty());
    assert_eq!(map.command("status.stage").unwrap().keys, vec!["s"]);
}

#[test]
fn modes_can_restrict_a_binding() {
    let mut spec = binding(&["[ m"]);
    spec.modes = vec![EditorMode::Normal];
    let mut config = Config {
        profile: Profile::Vim,
        ..Default::default()
    };
    config
        .keys
        .scopes
        .entry("commit-message".to_string())
        .or_default()
        .commands
        .insert("history-prev".to_string(), spec);

    let map = Keymap::resolve("commit-message", Profile::Vim, &config).unwrap();
    let chord = KeyChord::parse("[ m").unwrap().steps().to_vec();
    assert_eq!(
        map.lookup(&chord, EditorMode::Normal),
        Match::Exact("commit-message.history-prev".to_string())
    );
    assert_eq!(map.lookup(&chord, EditorMode::Visual), Match::None);
}
