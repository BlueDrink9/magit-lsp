# magit-lsp for VS Code

A thin VS Code client for `magit-lsp`. The server owns repository state, view
structure, rendering and actions; this extension only displays the server's
text verbatim, forwards canonical keys and commands, and answers prompts.

## Requirements

- VS Code 1.85 or newer (or a compatible build such as VSCodium).
- A Rust toolchain to build the `magit-lsp` server.
- Node.js 22 or newer for the client test suites.

## Build the server

From the repository root:

```sh
make build                 # or: cargo build
./target/debug/magit-lsp keymap --check   # smoke-test the binary
```

## Run it from source

1. Build the server (above).
2. Open this repository in VS Code.
3. Press <kbd>F5</kbd>. `.vscode/launch.json` starts an Extension Development
   Host with `clients/vscode` loaded and this repository as the workspace.
4. In the new window run **Magit: Open Status** from the command palette.

The extension starts the server named by `magit-lsp.serverPath` (default
`magit-lsp`, resolved on `PATH`). Point it at a built binary in your workspace
or user settings:

```json
{
  "magit-lsp.serverPath": "/home/you/src/magit-lsp/target/debug/magit-lsp",
  "magit-lsp.profile": "standard"
}
```

`magit-lsp.profile` selects the server keymap profile (`standard` or `vim`).
The client requests static key delivery: the server sends rendered text and
view metadata; every contributed keybinding forwards a canonical server command
or key.

`magit-lsp.keybindingsPath` optionally names your `keybindings.json` for
**Magit: Mirror my fold shortcuts**, which shows `magit.invoke` entries for the
editor fold keys you already use.

## Package the extension

From `clients/vscode`:

```sh
npx --yes @vscode/vsce package
```

This writes `magit-lsp-0.1.0.vsix`. Install it with `code --install-extension
magit-lsp-0.1.0.vsix`. If `vsce` is installed globally, `vsce package` is
equivalent.

## Tests

The default suite needs no editor and runs the real server over stdio:

```sh
make vscode-test
```

That target builds the server, runs the Node unit tests and the node
integration test, then checks the generated keybinding manifest for drift.
The same commands individually, from the repository root:

```sh
npm --prefix clients/vscode ci                  # once, installs locked deps
npm --prefix clients/vscode run test:unit       # pure helper tests
npm --prefix clients/vscode run test:integration
node clients/vscode/scripts/gen-keybindings.mjs --check
```

The integration test activates `extension.js` against a fake `vscode` module
and the real `target/debug/magit-lsp` binary. It opens the status view, checks
the displayed text is the server's render, presses `g` and waits for the
server's refreshed render, selects a row in the commit menu QuickPick, and
answers the resulting confirm prompt. It never parses rendered text to recover
structure.

An optional real-editor suite under `test/e2e/` launches a unique Extension
Development Host (never your own window) and drives the actual UI:

```sh
npm --prefix clients/vscode run test:e2e
```

It is skipped with a message when no `code`/`codium` binary or display is
available. Set `MAGIT_VSCODE_BIN` to choose the editor binary.

## Keybinding manifest workflow

`clients/vscode/package.json`'s `contributes.keybindings` is generated from the
server's manifest; do not hand-edit it. After adding or changing a server
command or default key:

```sh
cargo run -- keymap --format vscode --profile standard   # inspect the manifest
node clients/vscode/scripts/gen-keybindings.mjs          # rewrite package.json
node clients/vscode/scripts/gen-keybindings.mjs --check  # verify no drift
```

`--check` exits non-zero when `package.json` is stale; `make vscode-test` and
CI run it. Set `MAGIT_LSP_BIN` to use an existing binary instead of `cargo run`.

## Layout

- `extension.js` — activation entry point.
- `lib/session.js` — language client and extension wiring.
- `lib/views.js` — view lifecycle, virtual documents and cursor placement.
- `lib/menus.js` — menu QuickPicks and ordered key forwarding.
- `lib/requests.js` — `view/open`, `view/key`, `view/invoke`, renders, prompts.
- `lib/effects.js` — server effects (messages, clipboard, view stack, edits).
- `lib/edits.js` — Git-owned edit documents.
- `lib/commands.js` — contributed commands, status opening and fold mirroring.
- `lib/store.js` — state shared by the modules above.
- `lib/context.js`, `lib/render.js`, `lib/menu.js`, `lib/prompt.js`,
  `lib/foldkeys.js`, `lib/edit.js`, `lib/semantic.js` — pure helpers.
- `test/` — Node unit tests, fake editor modules and the node integration test.
- `test/e2e/` — optional real-editor suite.
