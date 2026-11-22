/// Extract the version from the bundled Lua adapter's module declaration.
///
/// The declaration `local M = { version = <version> }` tolerates either quote
/// style and routine whitespace around the braces and assignments. A missing
/// declaration, or one that is unquoted, empty, unterminated or followed by
/// anything but the closing brace, yields `None` so callers fail closed.
fn lua_adapter_version(source: &str) -> Option<&str> {
    let declaration = source.lines().find_map(|line| {
        let declaration = line.trim().strip_prefix("local")?;
        // `localM` is not a declaration of the module table.
        if declaration.trim_start().len() == declaration.len() {
            return None;
        }
        let declaration = declaration.trim_start().strip_prefix('M')?;
        match declaration.chars().next() {
            // `M.check` and friends are not the declaration itself.
            Some(c) if c.is_alphanumeric() || c == '_' => return None,
            _ => Some(declaration),
        }
    })?;
    let rest = declaration.trim_start().strip_prefix('=')?;
    let rest = rest.trim_start().strip_prefix('{')?;
    let rest = rest.trim_start().strip_prefix("version")?;
    let rest = rest.trim_start().strip_prefix('=')?;
    let rest = rest.trim_start();
    let quote = match rest.chars().next() {
        Some(quote) if quote == '\'' || quote == '"' => quote,
        _ => return None,
    };
    let value = &rest[quote.len_utf8()..];
    let version = &value[..value.find(quote)?];
    if version.is_empty() {
        return None;
    }
    let trailing = value[version.len() + quote.len_utf8()..].trim();
    (trailing == "}").then_some(version)
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
    let lua_version = lua_adapter_version(lua).expect("bundled Lua adapter version");
    assert_eq!(lua_version, cargo_version, "lua/magit-lsp/version.lua");
    let vscode_package: serde_json::Value =
        serde_json::from_str(vscode).expect("clients/vscode/package.json");
    let vscode_version = vscode_package
        .get("version")
        .and_then(serde_json::Value::as_str)
        .expect("clients/vscode/package.json version");
    assert_eq!(vscode_version, cargo_version, "clients/vscode/package.json");
}

/// Supported declaration formatting: either quote style and routine whitespace.
#[test]
fn lua_adapter_version_accepts_supported_formatting() {
    for source in [
        "local M = { version = '0.2.0' }\n",
        "local M = { version = \"0.2.0\" }\n",
        "local M={version='0.2.0'}",
        "\n  local\tM\t=\t{\tversion\t=\t\"0.2.0\"\t}\n",
        "--- Bundled Neovim adapter version.\nlocal M = { version = \"0.2.0\" }\n",
    ] {
        assert_eq!(lua_adapter_version(source), Some("0.2.0"), "{source:?}");
    }
}

/// Missing and malformed declarations yield no version, so the real test that
/// relies on them fails closed instead of comparing an empty value.
#[test]
fn lua_adapter_version_rejects_missing_or_malformed_declarations() {
    for source in [
        // No module table declaration at all.
        "--- Bundled Neovim adapter version.\nreturn M\n",
        // Module table without a version key.
        "local M = {}\n",
        // Unquoted value.
        "local M = { version = 0.2.0 }\n",
        // Unterminated string.
        "local M = { version = \"0.2.0\" \n",
        // Trailing junk after the closing brace.
        "local M = { version = \"0.2.0\" } -- trailing\n",
        // Empty version.
        "local M = { version = \"\" }\n",
        // Mismatched quote delimiters.
        "local M = { version = '0.2.0\" }\n",
        "local M = { version = \"0.2.0' }\n",
        // A different identifier, not the module table.
        "local Mx = { version = \"0.2.0\" }\n",
    ] {
        assert_eq!(lua_adapter_version(source), None, "{source:?}");
    }
}

/// A well-formed but unequal adapter version still fails the equality check the
/// bundled adapter test performs against the Cargo package version.
#[test]
fn lua_adapter_version_mismatch_fails_version_comparison() {
    let parsed = lua_adapter_version("local M = { version = \"0.0.1\" }\n")
        .expect("fixture bundled Lua adapter version");
    assert_ne!(
        parsed,
        env!("CARGO_PKG_VERSION"),
        "lua/magit-lsp/version.lua"
    );
}
