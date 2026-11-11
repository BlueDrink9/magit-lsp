#[test]
fn bundled_lua_version_matches_cargo_package_version() {
    let cargo = include_str!("../Cargo.toml");
    let lua = include_str!("../lua/magit-lsp/version.lua");
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
    assert_eq!(lua_version, cargo_version);
}
