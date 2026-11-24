# magit-lsp

[![GitHub last commit](https://img.shields.io/github/last-commit/BlueDrink9/magit-lsp?label=last%20commit)](https://github.com/BlueDrink9/magit-lsp)
[![Licensing: GPL-3.0-only / Apache-2.0](https://img.shields.io/badge/licensing-GPL--3.0--only%20%2F%20Apache--2.0-blue)](LICENSING.md)

Magit, the powerful and much-loved Git interface, as a language server any editor can use.

**Neovim · VS Code · Helix\*** and anything else with an LSP client

Stage hunks or single lines (select them, or use visual mode), commit, branch,
tag, reset and browse history. It's fully keyboard-driven, with discoverable
menus, and it integrates natively into any LSP-capable editor, even with minimal plugin
support.

The purpose is a Git interface whose state, structure, rendering and keys live
in the server, so a client only displays server-rendered text, forwards input
and acknowledges versions. That gives every client the same behaviour and gives
other editors one specification to implement. Like Magit and Neogit, repository
work shells out to the `git` CLI rather than using a native Git library.

See [client installation and quickstart](docs/CLIENT_INSTALL.md),
[native screenshot capture/manual fallback](docs/SCREENSHOTS.md), and the
[external client specification](docs/CLIENT_SPEC.md). Helix, Zed and JetBrains
adapters are welcome as COMMUNITY clients through that specification; no
first-party implementations are promised.

## Workflows it makes easy

<!-- T032: list only what docs/parity.md shows as Implemented. -->

Rewriting history is mostly about being sure of the target. Each of these lets
you look at the commit before anything runs.

- **Find the commit first.** Filter the log by author, date, message, changed
  string or path, and copy commit IDs.
- **Put a change in the commit it belonged in.** Stage just those hunks or
  lines, then fix them up into an earlier commit picked from the log.
- **Fixups without ceremony.** Fold staged changes into a commit and let the
  rebase happen in one step.
- **Rebases you can see.** Reorder and drop commits in an interactive todo view
  before submitting it.
- **Branch surgery.** Spin commits off onto a new branch, or reset a branch to a
  commit you picked from the log, with a confirmation when it would discard
  work.


## Binary release installation (not yet published)

Rust source builds remain the available installation path until the first
approved assets are published. The reviewed standalone installer is prepared for
Linux x86_64 GNU (glibc >=2.35); hosted/clean-machine release evidence is pending.
After assets exist, download the installer from a reviewed repository revision
without needing a clone, inspect it, then run (Python >=3.9 and curl required):

```sh
revision=REPLACE_WITH_REVIEWED_COMMIT_SHA
curl --fail --location --proto '=https' --proto-redir '=https' \
  "https://raw.githubusercontent.com/BlueDrink9/magit-lsp/$revision/scripts/install.sh" \
  --output magit-lsp-install.sh
less magit-lsp-install.sh                          # review before execution
sh magit-lsp-install.sh                            # latest, ~/.local/bin
sh magit-lsp-install.sh --version v0.1.0 --install-dir "$HOME/.local/bin"
```

These examples are conditional, not a claim that v0.1.0 assets are available.
There is no sudo or source-build fallback. Add the install directory to PATH;
Git and an editor client are still required, but binary users do not need Rust.
Checksums over HTTPS check integrity, not independent publisher authenticity.
See [release contracts, platform limits and approval gates](docs/RELEASING.md).

## Architecture (brief)

Two processes talking over LSP (JSON-RPC on stdio):

- **`magit-lsp` server (Rust).** Owns all VCS work by shelling out to the `git`
  CLI (like Magit/Neogit), and exposes it as custom requests plus standard LSP
  features (folding).
- **Neovim client (`clients/neovim/lua/magit-lsp`).** Starts the server, opens a status view,
  renders the server's `view/render` updates into a `nofile` `magit://status/<repo>`
  buffer, and forwards keys as `view/key` steps against the server's keymap.
- **VSCode client (`clients/vscode`).** Launches an external server and displays
  its views; currently available as source or a locally packaged VSIX
  (`make vscode-package`), not a published Marketplace/Open VSX install.

The server is designed to do as much as possible so that clients are thin and
easy to build for other editors. See [CLIENT_SPEC](docs/CLIENT_SPEC.md) for the
wire contract and `AGENTS.md` for the code map.

A general LSP client can drive the server in Plain mode by following the
[client implementation levels](docs/CLIENT_SPEC.md#client-implementation-levels):
omit `capabilities.experimental.magit` and declare the standard capabilities it
supports. The tested clients are the bundled Neovim client (Lua and UI scenarios,
`make test`), the [plugin-free Neovim Plain
configuration](docs/CLIENT_INSTALL.md#neovim-plugin-free-plain-lsp) (`make
plain-neovim`) and the VS Code extension (`make vscode-test`). The Helix TOML in
the specification is a reference configuration, not a tested client.

## Quickstart

Both clients launch an external server. Nothing is published yet: there is no
GitHub Release asset, Marketplace, Open VSX or registry install, and the VS Code
extension is built from source or a locally packaged VSIX (`make vscode-package`).
Full native-package, lazy.nvim and source-extension steps are in
[client installation and quickstart](docs/CLIENT_INSTALL.md); the documented
lazy spec is exercised against the real manager by `make lazy-install-check`.

Build the server from a checkout first; binary users do not need Rust once an
approved asset exists.

```sh
git clone https://github.com/BlueDrink9/magit-lsp.git
cd magit-lsp
cargo build --release --locked
```

### Neovim (0.11+)

The Neovim client is the `clients/neovim` subdirectory of this checkout; the
repository root is not a Neovim runtime. Add that client directory to
`'runtimepath'`, run Neovim inside a Git repository, and start the client;
server discovery finds `target/release/magit-lsp` in the checkout:

```vim
:set runtimepath+=/path/to/magit-lsp/clients/neovim
:lua require('magit-lsp').setup()
```

`status` opens in place and `g r` refreshes it in the default `vim` profile.
For a native package or lazy.nvim install (a clone under your Neovim data
directory, with `cmd` pointing at the installed binary), copy the tested snippets
in [client installation](docs/CLIENT_INSTALL.md#neovim-native-packages): a
native package adds `clients/neovim` from `init.lua`, and a lazy spec appends
`plugin.dir .. '/clients/neovim'` and sources its `plugin/magit-lsp.lua` entry
point in the `config(plugin)` hook, registering `:Magit` without opening status
eagerly. Once the client is loaded, `:h magit` documents
`:Magit [directory]`, setup/server discovery and protocol-version
troubleshooting; run `:helptags ALL` if your installation does not generate
help tags for the client's `doc` directory.

To try the client from a full checkout without changing your own Neovim
configuration, the `make neovim` launcher builds the debug server and starts an
interactive Neovim that loads only this checkout, pointing `$MAGIT_LSP_BIN` at
`target/debug/magit-lsp`:

```sh
make neovim                      # open the current working directory
make neovim REPO=/path/to/repo   # open a chosen repository
```

It does not load user configuration, plugin managers or the test initializer.
Neovim's config/data/state/cache directories, plus shada, swap, backup, undo
and view files, live in a temporary directory removed when Neovim exits. HOME
is preserved, so Git and external helpers keep the caller's normal global
configuration (identity, signing and includes) and repository-local settings
still take precedence; a temporary include-only global Git config keeps the
original XDG `git/config` and `~/.gitconfig` visible while editor paths are
isolated. PATH and terminal variables are inherited; credential-helper or SSH
integration is not asserted beyond that inherited environment. The automated
targets below (`make test-lua` via `scripts/minimal_init.lua`, and
tmux-driven `make ui-test`/`make ui-file`) are test harnesses, not interactive
launchers.

### VS Code (1.85+)

Install the locked extension dependencies (Node.js 22+ and pnpm) and start an
Extension Development Host on a repository:

```sh
npm --prefix clients/vscode ci
code --new-window --extensionDevelopmentPath="$PWD/clients/vscode" /absolute/path/to/repo
```

Then set `magit-lsp.server.path` to the absolute server executable (or
`magit-lsp.server.source` to `system`/`managed`) and run **Magit: Open Status**
from the Command Palette. See the
[VS Code source quickstart](docs/CLIENT_INSTALL.md#vscode-source-quickstart-no-published-install).

## Features

### Implemented today

- **Status buffer**: branch header plus `Staged changes`, `Unstaged changes`,
  and `Untracked files` sections (with counts), each file expandable into its
  hunks and diff lines. Never blank — shows `Working tree clean` when clean.
- **Staging**: whole file, whole hunk, or a selected range of lines — and the
  symmetric unstage operations — all through `view/invoke`.
- **Discard**: discard a file, hunk, or selected changed lines from the
  worktree after a destructive confirmation. Discarded content is not saved to
  a private ref; use Git commits or stashes when you need recoverable history.
- **Auto-refresh**: a file watcher watches the worktree (ignoring noisy `.git`
  internals) and refreshes open status views when tracked files or the index
  change, so changes made outside Neovim appear without `g r`. Events are
  debounced; while `index.lock` exists the refresh is deferred and retried.
- **Text mode**: `common.toggle-text-mode` (`ctrl+t`) turns the view into plain
  editable text. Keys stop being forwarded, refreshes are queued, and leaving
  text mode restores the server's text in one full render. Edits are never sent
  to or parsed by the server.
- **Copy value**: `common.copy-value` (`ctrl+shift+c` in `standard`, `y y` in
  `vim`) copies the file path, hunk patch text, diff line or section title at
  point through a `clipboard` effect, so the system clipboard works without
  entering text mode.
- **Visit**: open the real file at the new-side line of the item under the
  cursor (via LSP `definition`).
- **Menus (transients)**: `open.commit` (`c` in the status view) pushes a
  server-rendered commit menu in a bottom window. Switches (`-a`), options
  (`=A`) and actions (`c`) are handled by the server; the argument preview
  (`git commit --all`) updates as switches change. Every not-yet-built action is
  listed as `Planned` and reports its roadmap item. `Escape`, `ctrl+c` or `q`
  closes the menu; `ctrl+s`/`ctrl+r` save/reset the current arguments.
- **Commit message editor**: `c c` with a staged change starts `git commit`
  through the [editor bridge](docs/ARCHITECTURE.md#310-editor-bridge-open-gits-editor-inside-the-users-editor). The server now owns versioned managed
  drafts: saving checkpoints only; explicit finish or normal user close submits
  the selected saved checkpoint, while explicit abort cancels. Native Neovim
  and VSCode adapters are still pending migration (R-EDIT-4/5/6); native commit
  editing is not currently claimed working. The server's in-memory history keeps
  up to 50 submitted messages / 1 MiB per repository, including failed commits,
  with previous/next recall and unsent-draft restoration; recall never saves or
  commits. Adapter authors should use the versioned `draft/history` replacement
  contract in [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md#310-editor-bridge-open-gits-editor-inside-the-users-editor), not client-owned history. Credential prompts from
  Git (SSH passphrases, HTTPS tokens) still go through the same bridge.
- **Colourised** buffer (sections, file statuses, `+`/`-` diff lines).
- **Foldable** sections and files: the server owns visibility, and the client
  mirrors Neovim's fold keys to server commands.
- **LSP-provided navigation targets** for jumping between sections, files, and
  hunks.
- **Readable Git errors**: common failures such as rejected pushes, missing
  files and conflicts show a plain-language summary and
  actionable hint; raw output remains available as detail when no specific
  explanation applies.
- **Log and commit selection**: from status, `l l` (`standard`) or `l l`
  (`vim`) opens the current-branch log. The log uses the editor's native
  navigation and search, loads older commits with `+`, and supports log-menu
  filters. In the log menu, `c` opens a select-and-copy picker; use `Enter` to
  confirm and copy the selected commit ID, or `Escape`/`q` to cancel and return
  without copying. Selection supports the configured multi-selection controls.

### Plain keys (experimental, opt-in)

Plain keys are off by default and are an experimental opt-in for clients that
cannot bind code actions to keys. With keys enabled for one document, typing an
eligible key in the status view can stage a file: the server recovers the typed
edit, runs the same command the code action would run and repairs the text, and
the repair echo is not replayed. Destructive, pasted or multi-key, planned,
stale and keys-off input is refused without changing Git; a chord such as `g r`
and commit-menu switches such as `-a` continue against server state. This is a
subset, not Phase 9 completion: hunk, visual, unstage and navigation cases and
the wider property/native matrix (T034–T039) are still unverified. The policy,
per-document `Keys: on/off` toggle and command limits are in
[Plain-key guidance](docs/PLAIN_KEYS.md); per-command availability is in the
generated [Plain command support](docs/plain-support.md). The tested behavior
is in `tests/plain_keys_exec.rs` and
`tests/ui/scenarios/plain-keys-recovered.scn`.

### Planned

The generated [parity reference](docs/parity.md) is the authoritative
implemented-versus-Planned list: every Magit menu item that is not built still
appears in its menu, greyed out, and reports its roadmap item when pressed.
Planned work is not advertised above. In summary:

- read views for revisions, diffs, refs, reflog and line history
  (R-5.1–R-5.4; T049–T057 in [TASKS.md](TASKS.md#7-numbered-implementation-queue));
- most branch, push, pull, fetch, merge, rebase, cherry-pick and stash actions —
  `docs/parity.md` marks each remaining row `Planned` with its roadmap item;
- blame, submodule, worktree, bisect, patch and ignore workflows (R-8.x).

Menus already show these as `Planned`, so a missing action is visible in the UI
rather than implied to work.

## Requirements

- An external `magit-lsp` server. Rust/cargo is needed only for source builds,
  not for users of a future approved binary release.
- Neovim 0.11+ or VSCode 1.85+ for the respective client. Source VSCode setup
  also uses Node.js 22+ and pnpm 10; see the [install guide](docs/CLIENT_INSTALL.md).
- `git` on `PATH` — all VCS reads/writes shell out to the `git` CLI. Git
  **2.30.8 is the tested recommendation**, not a hard minimum; older versions are not blocked, but some
  operations may fail. Failures on an older Git produce a tracing warning.
  The recommended-Git CI job is configured to build exact Git 2.30.8 from a checksum-verified source archive and test
  the backend, status and patch cases on it. The full suite also runs on the
  runner's Git version.

## Running the tests

All tests (Rust + Neovim integration) run from the project root:

```sh
make test
```

Local prerequisites: `git` (2.30.8 recommended), a Rust toolchain (`cargo`,
stable), Neovim (any recent version with Lua) and Node.js 22 or newer with
pnpm for the VSCode client tests. See below for the exact pieces.

Individual pieces:

```sh
make test-rust                             # cargo unit + integration tests
make test-lua                              # native Neovim tests via mini.test
make test-file FILE=tests/test_magit_lsp.lua
make vscode-test                           # VSCode node tests + keybinding drift check
```

`make test-lua` clones `mini.nvim` into `build/deps/mini.nvim` on first run
(via `scripts/minimal_init.lua`) — it never touches your user environment. The
Neovim integration tests start the real `target/debug/magit-lsp` binary against
a throwaway git repository under `build/test-repos/`.

`make test` does not run the real-UI Python suites. `make ui-unit` tests the
scenario/driver harness itself, while `make ui-test` (and `make ui-tour`) drive
real Neovim screens; run them separately when changing the UI harness or
user-visible rendering.

### VSCode client tests

The VSCode extension requires VSCode 1.85 or newer. Running its Node tests also
requires Node.js and pnpm; exercising the extension against Git requires `git`
and a server binary (the native harness currently expects a local `cargo build`). Install the extension's locked dependencies
and run its automated tests from the repository root:

```sh
make vscode-deps
make vscode-test
```

These are Node unit tests for the extension's context, rendering, menu,
fold-shortcut and prompt helpers. They do not launch VSCode or the language
server. `make vscode-test` runs the same node tests plus
`node clients/vscode/scripts/gen-keybindings.mjs --check`, which regenerates
`contributes.keybindings` from the server's static manifest and fails when the
committed one is stale (e.g. after editing a profile in Rust without running
the generator). Make installs locked dependency versions from
`clients/vscode/pnpm-lock.yaml` when needed, using a shared pnpm store. `make test` includes
`vscode-test`; CI uses the same Make bootstrap.

To test the extension in an Extension Development Host, build the server and
install the extension dependencies first. Then create a disposable repository
with a commit and a few changes, configure the development workspace to use the
built server, and launch VSCode with the extension development path:

```sh
cargo build
make vscode-deps

server="$(pwd)/target/debug/magit-lsp"
extension="$(pwd)/clients/vscode"
repo="$(mktemp -d)"
git -C "$repo" init
printf 'original\n' > "$repo/tracked.txt"
git -C "$repo" add tracked.txt
git -C "$repo" -c user.name='Magit Test' -c user.email='magit-test@example.invalid' commit -m initial
printf 'edited\n' >> "$repo/tracked.txt"
printf 'untracked\n' > "$repo/new.txt"
mkdir -p "$repo/.vscode"
cat > "$repo/.vscode/settings.json" <<EOF
{
  "magit-lsp.server.path": "$server",
  "magit-lsp.profile": "standard"
}
EOF
code --new-window --extensionDevelopmentPath="$extension" "$repo"
```

In the Extension Development Host window, open the Command Palette and run
**Magit: Open Status**. The extension starts the configured `magit-lsp` binary
with no arguments. `magit-lsp.server.path` is an absolute executable file;
`magit-lsp.server.source` defaults to `auto` (explicit path, then a
version-matching `magit-lsp` on `PATH`, then the pinned managed download), and
`system` avoids downloading. `magit-lsp.profile` defaults to `standard`
(`standard` or `vim`). No `.vscode/launch.json` or F5 launch setup is provided;
the `code` command above starts the host directly. Delete the temporary
repository when finished.

Manual smoke checklist:

- Confirm the status view shows the modified tracked file and untracked file;
  expand a file/hunk and check that the diff renders.
- Stage and unstage a change, then refresh and confirm the Git status updates.
- Open the commit menu (`c`), toggle an option and close it with Escape.
- Toggle text mode with `ctrl+t` and back. VSCode's virtual status document is
  read-only, so text mode displays server-rendered text but does not allow
  editing; arbitrary pass-through chords are also not replayed by this static
  client.

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

   The client spawns the first server binary it finds: an explicit
   `setup({ cmd = ... })`, `$MAGIT_LSP_BIN`, `target/release/magit-lsp`,
   `target/debug/magit-lsp`, then `magit-lsp` on `$PATH`. There is no
   `cargo run` fallback: when none is usable, `setup()` reports a clear error
   instead of starting a surprise build.

   If Neovim reports `capabilities.experimental.magit render is required and
   must be a string`, its Lua client and server binaries are from different
   protocol versions. Refresh the preferred checkout release binary with
   `make release` (this builds locally; it does not install or publish). Or
   explicitly select the debug binary for this invocation (replace
   `/path/to/repo` with your repository):

   ```sh
   make build
   MAGIT_LSP_BIN="$(pwd)/target/debug/magit-lsp" nvim /path/to/repo
   ```

   Server discovery prefers `target/release/magit-lsp` over
   `target/debug/magit-lsp`; `make release` refreshes that preferred checkout
   binary. An older release binary can therefore shadow the newly rebuilt debug
   binary when `MAGIT_LSP_BIN` is unset. Keep the Lua client and selected server
   from the same checkout/version. Do not restore the retired `render`
   capability.

The status buffer is a `nofile` buffer named `magit://status/<repo>`, shown in
the current window by replacing its buffer. It is a read-only view: native
editing keys are refused and `:w`/`:write` is a no-op. Use `g r` to refresh it
after changes in the repository. `setup({ status = { placement = 'vsplit' } })`
opens it in a vertical split instead (see below).

Press the text-mode key (`ctrl+t` in both shipped profiles) to turn it into
plain editable text. The buffer becomes modifiable, the user's `undolevels` is
restored and every binding but the toggle is removed. While text mode is on,
the server queues refreshes and answers position-based LSP requests with
`null`. Pressing the key again replaces the buffer with the server's current
text (`undolevels` goes back to `-1` first, so undo cannot reach stale edits).

## Client configuration

`setup()` takes these options:

| Option | Default | Meaning |
| ------ | ------- | ------- |
| `root_dir` | `vim.fn.getcwd()` | Repository to show |
| `cmd` | discovered: `$MAGIT_LSP_BIN`, `target/release`, `target/debug`, `$PATH` | Server command |
| `profile` | `"vim"` | Shipped keymap profile, forwarded as `initializationOptions.profile` |
| `status` | `{ placement = "replace" }` | Status window: replace the current window (`"replace"`), or open a `"vsplit"`, `"split"` or new `"tab"` |
| `fold_keys` | `{}` | Extra fold mirrors, e.g. `{ ['<Tab>'] = 'common.toggle-section' }` |
| `menu` | `{ style = "float" }` | Menu placement: a bottom float (default) or `style = "split"` for `botright {height}split` |

The client declares `keymapDelivery = "dynamic"` and `keyModifiers = "basic"`
in terminals (`"full"` in a GUI), then applies every `view/keymap` the server
sends atomically: it deletes exactly the maps it applied last time, sets the
new buffer-local maps and records them in one synchronous call. Keys are
translated from the canonical notation in
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md#38-key-notation) (`ctrl+c`, `g r`,
`shift+Tab`, …) by `clients/neovim/lua/magit-lsp/keys.lua`. Nothing is hard-coded: every
binding comes from the server.

Folding is server-owned. The view window uses `foldmethod=manual` with
`foldenable=false`; the client mirrors the native fold keys (`za`, `zA`, `zo`,
`zO`, `zc`, `zC`, `zR`, `zM`, `zr`, `zm`) to the matching `common.*` commands,
`zj`/`zk` to section navigation, and re-maps any user mapping whose right-hand
side is a fold command (e.g. `nnoremap <Tab> za`). `fold_keys` covers
Lua-function mappings, which cannot be inspected.

Visit resolution is server-owned. The `common.visit` binding the server sends
names the file and new-side line from its own tree, and the client only opens
the location in a `visitFile` effect; it never parses rendered text. With the
default `replace` placement the status view owns the only window, so the file
opens there and the status buffer stays listed (`:b`, `:ls` return to it); with
a split or tab placement the file opens in the other window so the status view
stays visible.

Menus are separate `magit://menu.*` buffers created from the server's
`pushView` effects. The default placement is a bottom floating
window (`relative="editor"`, full width, height = rendered lines capped at 40%
of the screen); `setup({ menu = { style = "split" } })` uses
`botright {height}split` instead. The client only applies `view/render`,
forwards keys and resizes the window; all menu layout and key matching happens
on the server. Closing the menu window by hand sends `view/close` and returns
focus to the previous view.

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

Two helper modes are internal to the editor bridge and never speak LSP:

- `--editor-helper <path>` is Git's `GIT_EDITOR`: it sends the file to the
  running server and blocks until the client confirms or cancels.
- `--askpass <prompt>` is Git's `core.askPass` credential helper: it forwards
  the prompt (SSH passphrases, HTTPS tokens) to the running server and prints
  the answer on stdout. The secret is never written to any log.

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

`tests/snapshots/` holds six committed snapshots: the JSON manifest and the
generated VSCode keybindings for both profiles (the R-2.5 CI snapshot), and
the commit menu rendered at 60 and 140 columns. The other large menus
(rebase, push, diff) are checked against structural invariants instead of
spacing snapshots: every rendered line fits the viewport, action columns
reflow to fewer columns at narrower widths, and every planned action's label
carries a `muted` token with the `planned` modifier over exactly that label.

Snapshot tests use `expect_file!` with a single update mechanism. Run this to
rewrite stale snapshots — and the README parity block below — in place,
preserving the hand-written prose around them:

```sh
UPDATE_EXPECT=1 cargo test --test manifest --test parity --test menu --test menus
```

The `parity` subcommand prints the Magit command parity table (every switch,
option and action of every dispatch menu, with its Magit key, our command id,
status and roadmap item):

```sh
magit-lsp parity [--format markdown|json]
cargo xtask parity            # the same command through the cargo alias
```

See the [generated parity reference](docs/parity.md).

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
[docs/KEYMAPS.md](docs/KEYMAPS.md#5-default-keymaps-profiles). Select `standard` with `setup({ profile = 'standard' })`, or
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

## Compared with Neogit and Edamagit

[Neogit](https://github.com/NeogitOrg/neogit) is a Neovim Git interface inspired
by Magit: a Lua plugin, MIT-licensed, that shells out to the `git` executable and
offers popups for commit, branch, rebase, stash, tag and more. Its menus can use
Telescope, fzf-lua, mini.pick, snacks.nvim or `vim.ui.select` for selection, and
its README notes that many popups use the item under the cursor or the visual
selection. [Edamagit](https://github.com/kahole/edamagit) is a keyboard-driven VS
Code extension, "Magit for VSCode", published on the VS Code Marketplace and Open
VSX, with popups, a dispatch menu, VSCodeVim support and Forge features; its
README lists remaining work such as more diffing and logging, bisect, patches and
subtrees.

Both are established, editor-specific interfaces with much broader implemented
action coverage than magit-lsp's current parity table. Comparing them fairly
means evaluating the three candidate differences rather than asserting an
advantage:

- **Key-handling speed.** magit-lsp's keymap lives in the server: the client
  forwards each key as a `view/key` step and renders the result. Neogit and
  Edamagit resolve keys in the editor process, so magit-lsp adds an LSP round
  trip per key. No benchmark in this repository compares the three and no speed
  claim is made; the design benefit is identical key semantics in every client,
  not lower latency.
- **Scriptable command interface.** `magit-lsp keymap` and `magit-lsp parity`
  print the keymap manifest and the Magit parity table without speaking LSP, and
  the running server exposes actions as `view/invoke`/`view/key` requests.
  Neogit and Edamagit are driven through their editor's API (Lua API and VS Code
  commands). That is a difference in where the interface lives, not a measured
  advantage.
- **Commit at the cursor.** Neogit documents that popups consume the item under
  the cursor or a visual selection. magit-lsp's log currently offers
  select-and-copy of a commit ID, while most commit actions that would consume a
  commit at point are still `Planned` (R-4.1 in `docs/parity.md`). This is not an
  advantage today.

For Neovim specifically, Neogit is the more complete choice today and needs no
external process. magit-lsp's reason to exist is the shared server and the other
clients, not a Neovim-only feature: if you work across Neovim and VS Code, or
want to implement another client against one specification, that is the case it
is built for.

## Command parity

Every Magit menu item we have not built yet is still listed in its menu with
Magit's key and description, greyed out and reporting its roadmap item when
pressed. This table is generated by `magit-lsp parity` (also available as
`cargo xtask parity`); `tests/parity.rs` fails when it is stale. Action keys
include the menu's Magit prefix (`c c`); switch and option keys use magit-lsp's
canonical menu notation (`-a` toggles a switch, `=A` sets an option).

[Generated parity reference](docs/parity.md)

## Notes

- The client is deliberately thin: it opens the view, applies `view/render`
  text/edits, acks versions, resyncs on a lost update, and forwards keys as
  `view/key` steps. All status structure, staging decisions, keymaps and cursor
  succession live in the server.
- Like Magit and Neogit, mutations shell out to `git` (e.g. `git apply
  --cached`) rather than using a native git library, so behavior stays on
  parity with the CLI.

## License

The server and other unexcepted project material are GNU GPL-3.0-only. The
Neovim client, VS Code client, protocol definition, and listed specifications
are Apache-2.0-licensed; see [LICENSING.md](LICENSING.md) and
[LICENCES/Apache-2.0](LICENCES/Apache-2.0). Third-party dependencies retain their own licenses.

Inspired by Magit for Emacs. This project is an independent implementation and is not affiliated with the Magit project.

## How this code was made and tested

I direct this project: I set the requirements, made the design decisions,
reviewed the changes and ran the release checks. The code was written with
substantial AI-agent assistance under that direction, and nothing lands without
my review.

Testing is automated where it can be: Rust unit and integration suites, native
Neovim tests through mini.test, Node tests for the VS Code extension, real
interactive Neovim UI scenarios for user-visible behaviour, and release-contract
checks for the installer and package layout (see [TESTING.md](docs/TESTING.md)).
Native verification is done by a person and is Linux-only today; Windows and
macOS are not verified locally, and checks for them remain follow-up work. The
[release gates](docs/RELEASING.md) list what is still unproven.

These tests catch regressions; they are not a substitute for human review. A
green suite does not make the project correct, and the known limits are listed
rather than hidden.
