/// Extract the bundled Lua adapter version from its version declaration line.
///
/// Recognizes the exact declaration `local M = { version = <quoted> }` with
/// matching single or double quotes around the version. Returns `None` when
/// the declaration is missing or any delimiter is mismatched.
fn lua_adapter_version(lua: &str) -> Option<&str> {
    lua.lines().find_map(|line| {
        let rest = line.strip_prefix("local M = { version = ")?;
        let rest = rest.strip_suffix(" }")?;
        [('\'', '\''), ('"', '"')]
            .into_iter()
            .find_map(|(open, close)| {
                rest.strip_prefix(open)
                    .and_then(|value| value.strip_suffix(close))
            })
    })
}

/// Assert that the bundled Lua adapter declares `expected` as its version.
fn assert_lua_adapter_version(lua: &str, expected: &str) {
    let version = lua_adapter_version(lua).expect("bundled Lua adapter version");
    assert_eq!(version, expected, "lua/magit-lsp/version.lua");
}

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
    assert_lua_adapter_version(lua, cargo_version);
    let vscode_package: serde_json::Value =
        serde_json::from_str(vscode).expect("clients/vscode/package.json");
    let vscode_version = vscode_package
        .get("version")
        .and_then(serde_json::Value::as_str)
        .expect("clients/vscode/package.json version");
    assert_eq!(vscode_version, cargo_version, "clients/vscode/package.json");
}

#[test]
fn lua_adapter_version_accepts_matching_quotes() {
    assert_eq!(
        lua_adapter_version("local M = { version = '0.2.0' }"),
        Some("0.2.0")
    );
    assert_eq!(
        lua_adapter_version("local M = { version = \"0.2.0\" }"),
        Some("0.2.0")
    );
}

#[test]
fn lua_adapter_version_rejects_missing_or_mismatched_delimiters() {
    for declaration in [
        "local M = { version = 0.2.0 }",
        "local M = { version = '0.2.0\" }",
        "local M = { version = \"0.2.0' }",
        "local M = { version = \"0.2.0\"",
        "return M",
    ] {
        assert_eq!(lua_adapter_version(declaration), None, "{declaration}");
    }
}

#[test]
fn lua_adapter_version_mismatch_still_fails() {
    let result = std::panic::catch_unwind(|| {
        assert_lua_adapter_version("local M = { version = '9.9.9' }", "0.2.0");
    });
    assert!(
        result.is_err(),
        "mismatched Lua version must fail the oracle"
    );
}
