# Formatting

Formatter checks do not modify files. The pre-commit hook checks staged content,
so unstaged edits and partial staging remain untouched. Legacy formatting is
checked only when a maintained source file changes; this does not normalize the
whole repository.

Install the locked Lua and JavaScript/TypeScript formatter tools:

```sh
npm ci --prefix clients/vscode
```

Opt in to the repository hook, or remove it later:

```sh
make hooks-install
make hooks-remove
```

Run staged-source checks:

```sh
make format-check
```

CI uses the same runner on committed changed paths. Rust uses the repository's
current Rust environment and `cargo fmt --all -- --check`.

Fix files explicitly with the matching formatter:

```sh
cargo fmt --all
clients/vscode/node_modules/.bin/stylua <lua-files>
clients/vscode/node_modules/.bin/prettier --write <js-ts-files>
```
