#[test]
fn bundled_adapter_versions_match_cargo_package_version() {
    let cargo = include_str!("../Cargo.toml");
    let lua = include_str!("../lua/magit-lsp/version.lua");
    let vscode = include_str!("../clients/vscode/package.json");
    let cargo_version = cargo
        .lines()
        .find_map(|line| line.strip_prefix("version = \""))
        .and_then(|line| line.strip_suffix('\"'))
        .expect("Cargo package version");
    let lua_version = lua
        .lines()
        .find_map(|line| line.strip_prefix("local M = { version = '"))
        .and_then(|line| line.strip_suffix("' }"))
        .expect("bundled Lua adapter version");
    assert_eq!(lua_version, cargo_version, "lua/magit-lsp/version.lua");
    let vscode_package: serde_json::Value =
        serde_json::from_str(vscode).expect("clients/vscode/package.json");
    let vscode_version = vscode_package
        .get("version")
        .and_then(serde_json::Value::as_str)
        .expect("clients/vscode/package.json version");
    assert_eq!(vscode_version, cargo_version, "clients/vscode/package.json");
}
