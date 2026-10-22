# magit-lsp

An LSP-based version control interface. Treats a repository's status as a
structured document and drives it through a Language Server, with a Neovim
client that renders a Magit/Neogit-style status buffer. MVP backend is Git;
MVP client is Neovim.

## Architecture (brief)

Two processes talking over LSP (JSON-RPC on stdio):

- **`magit-lsp` server (Rust).** Owns all VCS work by shelling out to the `git`
  CLI (like Magit/Neogit), and exposes it as custom requests plus standard LSP
  features (folding).
- **Neovim client (`lua/magit-lsp`).** Starts the server, opens a status view,
  renders the server's `view/render` updates into a `nofile` `magit://status/<repo>`
  buffer, and forwards keys as `view/key` steps against the server's keymap.

The server is designed to do as much as possible so that clients are thin and
easy to build for any editor. See `AGENTS.md` for the protocol and internals.

## Features

- **Status buffer**: branch header plus `Staged changes`, `Unstaged changes`,
  and `Untracked files` sections (with counts), each file expandable into its
  hunks and diff lines. Never blank — shows `Working tree clean` when clean.
- **Staging**: whole file, whole hunk, or a selected range of lines — and the
  symmetric unstage operations — all through `view/invoke`.
- **Visit**: open the real file at the new-side line of the item under the
  cursor (via LSP `definition`).
- **Colourised** buffer (sections, file statuses, `+`/`-` diff lines).
- **Foldable** sections and files: the server owns visibility, and the client
  mirrors Neovim's fold keys to server commands.
- **LSP-provided navigation targets** for jumping between sections, files, and
  hunks.

## Requirements

- Rust (cargo) — builds the `magit-lsp` LSP server
- Neovim 0.11+ — runs the client and the tests (`vim.lsp.start`)
- `git` on `PATH` — all VCS reads/writes shell out to the `git` CLI. The
  integration tests were last verified with `git` 2.55.0 (including the
  unborn-HEAD unstage path, which uses `git reset -q --` with a
  `git rm --cached -f` fallback).

## Running the tests

All tests (Rust + Neovim integration) run from the project root:

```sh
make test
```

Individual pieces:

```sh
make test-rust                             # cargo unit + integration tests
make test-lua                              # native Neovim tests via mini.test
make test-file FILE=tests/test_magit_lsp.lua
```

`make test-lua` clones `mini.nvim` into `build/deps/mini.nvim` on first run
(via `scripts/minimal_init.lua`) — it never touches your user environment. The
Neovim integration tests start the real `target/debug/magit-lsp` binary against
a throwaway git repository under `build/test-repos/`.

## Manual testing

1. Build the server:

   ```sh
   cargo build
   ```

2. From inside a git repository that has some changes, open Neovim and run:

   ```vim
   :lua require('magit-lsp').setup()
   ```

   `setup()` defaults `root_dir` to the current directory. To point at a
   specific repository, pass it explicitly:

   ```vim
   :lua require('magit-lsp').setup({ root_dir = '/path/to/repo' })
   ```

   If `target/debug/magit-lsp` exists the client spawns it directly; otherwise
   it falls back to `cargo run`.

The status buffer is a `nofile` buffer named `magit://status/<repo>`, shown in a
vertical split. It is a read-only view: native editing keys are refused and
`:w`/`:write` is a no-op. Use `g r` to refresh it after changes in the
repository.

## Client configuration

`setup()` takes these options:

| Option | Default | Meaning |
| ------ | ------- | ------- |
| `root_dir` | `vim.fn.getcwd()` | Repository to show |
| `cmd` | `target/debug/magit-lsp`, else `cargo run` | Server command |
| `profile` | `"vim"` | Shipped keymap profile, forwarded as `initializationOptions.profile` |
| `visit_key` | `"<CR>"` | Client-side visit key (see below) |
| `fold_keys` | `{}` | Extra fold mirrors, e.g. `{ ['<Tab>'] = 'common.toggle-section' }` |

The client declares `keymapDelivery = "dynamic"` and `keyModifiers = "basic"`
in terminals (`"full"` in a GUI), then applies every `view/keymap` the server
sends atomically: it deletes exactly the maps it applied last time, sets the
new buffer-local maps and records them in one synchronous call. Keys are
translated from the canonical notation of TASKS.md §3.8 (`ctrl+c`, `g r`,
`shift+Tab`, …) by `lua/magit-lsp/keys.lua`. Nothing is hard-coded: every
binding comes from the server.

Folding is server-owned. The view window uses `foldmethod=manual` with
`foldenable=false`; the client mirrors the native fold keys (`za`, `zA`, `zo`,
`zO`, `zc`, `zC`, `zR`, `zM`, `zr`, `zm`) to the matching `common.*` commands,
`zj`/`zk` to section navigation, and re-maps any user mapping whose right-hand
side is a fold command (e.g. `nnoremap <Tab> za`). `fold_keys` covers
Lua-function mappings, which cannot be inspected.

`common.visit` is still `Planned` (R-4/R-5), so `visit_key` is a documented
temporary client-side exception: while the server's binding is not
`implemented`, the client maps `visit_key` (normal mode) to its own `visit`.
It disappears as soon as the server implements `common.visit`.

## Logging

The server logs with `tracing` and never writes to stdout, which is the LSP
pipe. Logs go to stderr by default; two environment variables control them:

| Variable | Meaning |
| -------- | ------- |
| `MAGIT_LSP_LOG` | Level filter in `EnvFilter` syntax, e.g. `info`, `magit_lsp=debug`, `trace`. Defaults to `warn`. |
| `MAGIT_LSP_LOG_FILE` | Path of a file to append logs to instead of stderr. |

Every `git` invocation is logged at `debug` (arguments, working directory, exit
code, duration); failures are logged at `warn`. Patch contents passed to
`git apply` are only logged at `trace`.

## Command line

With no subcommand `magit-lsp` runs the LSP server on stdio, so editors can
spawn it directly. `--config <path>` selects a config file for that server, and
`--version` / `--help` print and exit.

The `keymap` subcommand prints the static keymap manifest — every profile ×
scope × binding — and never speaks LSP, so its output goes to stdout:

```sh
magit-lsp keymap --format json|vscode|neovim|markdown [--profile standard|vim] [--config <path>] [--check]
```

- `json` is the full manifest (`{profiles: [{profile, scopes: [{scope, bindings}]}]}`).
- `vscode` is an array of `contributes.keybindings` entries targeting
  `magit.key`, with `args.key` as the canonical key and a `when` clause such as
  `magit.active && magit.scope == 'status' && !magit.textMode`. It includes a
  second entry for `common.toggle-text-mode` guarded by `magit.textMode`, so
  text mode can be left. Capture mode is dynamic (`type` registration) and has
  no static entries. Generate VSCode keybindings with `--profile`; without it
  both profiles are emitted.
- `neovim` is a list of `{lhs, command, modes, scope}` in Neovim notation.
- `markdown` is one table per profile and scope.

Without `--profile` every shipped profile is emitted. `--check` validates the
merged keymap (conflicts, unknown commands, unparsable keys and profile
coverage) and prints `OK`, or exits non-zero with `file:line` messages. Combine
it with `--format`: the check runs first and output follows only when valid.

Generated VSCode keys are checked against a deny-list of workbench shortcuts
(`ctrl+p`, `ctrl+w`, `ctrl+tab`, `F1`, …); the deliberate overrides live in
`src/manifest.rs` with reasons: `ctrl+t` (`common.toggle-text-mode`),
`ctrl+c` (`common.cancel` passes through to copy when nothing is cancelable),
and `ctrl+s`/`ctrl+r`, which are scoped to menu views.

Snapshots of all four formats for both profiles are committed under
`tests/snapshots/`. Regenerate them with:

```sh
UPDATE_SNAPSHOTS=1 cargo test --test manifest
```

## Configuration

The server reads TOML. Layers, lowest to highest precedence:

1. built-in defaults
2. the shipped profile (`profile = "vim"` — the default — or `"standard"`)
3. the user config, plus its `include`d files
4. the client's `initializationOptions`
5. a per-repository `.magit-lsp.toml` (only keys and display settings, and
   only when the user config sets `trust_repo_config = true`)

The config path is, in order: `--config <path>`, the `MAGIT_LSP_CONFIG`
environment variable, then `<config>/magit-lsp/config.toml` where `<config>` is
`$XDG_CONFIG_HOME` or `~/.config` on Unix (`%APPDATA%` on Windows).

```toml
profile = "vim"                 # "standard" | "vim"
trust_repo_config = false       # allow a repo's .magit-lsp.toml

include = ["keys-work.toml"]    # loaded first; this file overrides them
include_optional = ["local.toml"]  # a missing file is not an error

[keys.common]
confirm = ["Enter"]              # replaces the profile's keys for common.confirm
cancel  = ["Escape", "ctrl+c"]

[keys.status]
stage   = ["s"]
unstage = ["u"]
unbind  = ["x"]                  # remove the profile's binding for x

[keys."commit-message"]
history-prev = { keys = ["[ m"], modes = ["normal"] }

[diff]
context = 3
```

Keys use the notation of §3.8: `ctrl+e`, `alt+ArrowDown`, `Enter`,
`shift+Tab`, `Space`, `Plus`, and chords such as `g r`. Includes may be
relative to the including file, absolute, `~`-prefixed, or use `$VAR` /
`${VAR}`. Tables merge key by key; arrays replace. Cycles and missing required
includes are reported with the offending file and line. Configuration is hot
reloaded when a loaded file changes or the client sends
`workspace/didChangeConfiguration`; a broken edit keeps the previous config and
shows one message naming the file and line.

## Keys (defaults, subject to customization)

The client hard-codes no keys: these are the `vim` profile defaults from
TASKS.md §5. Select `standard` with `setup({ profile = 'standard' })`, or
rebind in `config.toml`.

| Key        | Action                                                   |
| ---------- | -------------------------------------------------------- |
| `s`        | Stage file (on a file headline) or hunk (on a hunk line) |
| `s` (v)    | Stage the visually selected lines                        |
| `u`        | Unstage file or hunk at cursor                           |
| `u` (v)    | Unstage the visually selected lines                      |
| `S`        | Stage all                                                |
| `U`        | Unstage all                                              |
| `Enter`    | Visit the file at the new-side line under the cursor     |
| `g r`      | Refresh status                                           |
| `Tab`, `z a` | Fold/unfold the section or file at point               |
| `z o`/`z c` | Unfold/fold at point                                    |
| `z R`/`z M` | Unfold/fold everything                                  |
| `z r`/`z m` | More/less detail                                        |
| `z j`/`z k` | Next/previous section                                   |
| `] ]`/`[ [` | Next/previous section                                   |
| `g j`/`g k` | Next/previous sibling                                   |
| `g u`      | Parent section                                           |
| `Escape`, `ctrl+c` | Cancel                                           |
| `q`        | Quit                                                     |

## Notes

- The client is deliberately thin: it opens the view, applies `view/render`
  text/edits, acks versions, resyncs on a lost update, and forwards keys as
  `view/key` steps. All status structure, staging decisions, keymaps and cursor
  succession live in the server.
- Like Magit and Neogit, mutations shell out to `git` (e.g. `git apply
  --cached`) rather than using a native git library, so behavior stays on
  parity with the CLI.
