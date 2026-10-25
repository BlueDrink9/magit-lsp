//! The parity table generator (R-2.8): coverage, statuses and the README
//! block. The README block is the only parity snapshot; the CLI output is
//! compared against the generators directly.

use magit_lsp::commands::{registry, CommandStatus};
use magit_lsp::menu::MenuStatus;
use magit_lsp::menus::{self, ItemDef};
use magit_lsp::parity;
use expect_test::expect_file;
use std::path::PathBuf;
use std::process::Command;

fn readme_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("README.md")
}

#[test]
fn every_menu_item_has_a_parity_row() {
    let rows = parity::rows();
    for def in menus::all() {
        let menu = menus::build(&def);
        for group in &menu.groups {
            for item in &group.items {
                let suffix = format!(" {}", item.key());
                assert!(
                    rows.iter().any(|row| row.menu == def.id
                        && (row.magit_key == item.key() || row.magit_key.ends_with(&suffix))),
                    "{} has no parity row for {}",
                    def.id,
                    item.key()
                );
            }
        }
    }
}

#[test]
fn every_action_row_matches_the_registry_and_the_menu_status() {
    let mut actions = 0;
    for def in menus::all() {
        let menu = menus::build(&def);
        for group in &def.groups {
            for item in &group.items {
                let ItemDef::Action {
                    key,
                    command,
                    magit,
                    roadmap,
                    ..
                } = item
                else {
                    continue;
                };
                actions += 1;
                assert!(!magit.is_empty(), "{}/{} has no Magit command", def.id, key);
                let registered = registry()
                    .get(command)
                    .unwrap_or_else(|| panic!("{command} is registered"));
                let menu_status = menu.item(key).expect("menu item").status();
                if roadmap.is_empty() {
                    assert_eq!(
                        registered.status,
                        CommandStatus::Implemented,
                        "{command} is implemented"
                    );
                    assert_eq!(menu_status, &MenuStatus::Implemented, "{command}");
                } else {
                    assert_eq!(
                        registered.status,
                        CommandStatus::Planned { roadmap_id: roadmap },
                        "{command}"
                    );
                    assert_eq!(
                        menu_status,
                        &MenuStatus::Planned {
                            roadmap_id: roadmap.to_string()
                        },
                        "{command}"
                    );
                }
            }
        }
    }
    assert!(actions > 150, "only {actions} actions");
}

#[test]
fn json_rows_carry_the_documented_fields() {
    let text = parity::json();
    let rows: Vec<serde_json::Value> = serde_json::from_str(&text).expect("parity JSON");
    assert_eq!(rows.len(), parity::rows().len());
    for row in &rows {
        for field in ["magit", "magitKey", "command", "status", "menu"] {
            assert!(row.get(field).is_some(), "missing {field}: {row}");
        }
    }
    assert!(text.ends_with('\n'));
}

#[test]
fn the_cli_prints_the_generated_table() {
    let output = Command::new(env!("CARGO_BIN_EXE_magit-lsp"))
        .args(["parity"])
        .output()
        .expect("run parity");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), parity::markdown());

    let output = Command::new(env!("CARGO_BIN_EXE_magit-lsp"))
        .args(["parity", "--format", "json"])
        .output()
        .expect("run parity --format json");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), parity::json());
}

#[test]
fn the_readme_block_is_generated_and_up_to_date() {
    let path = readme_path();
    let readme = std::fs::read_to_string(&path).expect("read README.md");
    let start = readme
        .find(parity::README_START)
        .expect("README has the parity start marker");
    let end = readme
        .find(parity::README_END)
        .expect("README has the parity end marker");
    // The expected file is the hand-written prose around a freshly generated
    // block, so an `UPDATE_EXPECT=1` run rewrites the block in place and
    // preserves the prose.
    let expected = format!(
        "{}{}{}",
        &readme[..start],
        parity::readme_block(),
        &readme[end + parity::README_END.len() + 1..]
    );
    expect_file![path].assert_eq(&expected);
}
