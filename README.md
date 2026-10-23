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

### VSCode client tests

The VSCode extension requires VSCode 1.85 or newer. Running its Node tests also
requires Node.js and npm; exercising the extension against Git requires `git`
and a built Rust server (`cargo`). Install the extension's locked dependencies
and run its automated tests from the repository root:

```sh
npm --prefix clients/vscode ci
npm --prefix clients/vscode test
```

These are Node unit tests for the extension's context, rendering, menu,
fold-shortcut and prompt helpers. They do not launch VSCode or the language
server. `make test` runs only Rust and Neovim tests; it does **not** run the
VSCode tests.

To test the extension in an Extension Development Host, build the server and
install the extension dependencies first. Then create a disposable repository
with a commit and a few changes, configure the development workspace to use the
built server, and launch VSCode with the extension development path:

```sh
cargo build
npm --prefix clients/vscode ci

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
  "magit-lsp.serverPath": "$server",
  "magit-lsp.profile": "standard"
}
EOF
code --new-window --extensionDevelopmentPath="$extension" "$repo"
```

In the Extension Development Host window, open the Command Palette and run
**Magit: Open Status**. The extension starts the configured `magit-lsp` binary
with no arguments; `magit-lsp.serverPath` defaults to `magit-lsp` on `PATH`,
and `magit-lsp.profile` defaults to `standard` (`standard` or `vim`). No
`.vscode/launch.json` or F5 launch setup is provided; the `code` command above
starts the host directly. Delete the temporary repository when finished.

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

   If `target/debug/magit-lsp` exists the client spawns it directly; otherwise
   it falls back to `cargo run`.

The status buffer is a `nofile` buffer named `magit://status/<repo>`, shown in a
vertical split. It is a read-only view: native editing keys are refused and
`:w`/`:write` is a no-op. Use `g r` to refresh it after changes in the
repository.

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
| `cmd` | `target/debug/magit-lsp`, else `cargo run` | Server command |
| `profile` | `"vim"` | Shipped keymap profile, forwarded as `initializationOptions.profile` |
| `visit_key` | `"<CR>"` | Client-side visit key (see below) |
| `fold_keys` | `{}` | Extra fold mirrors, e.g. `{ ['<Tab>'] = 'common.toggle-section' }` |
| `menu` | `{ style = "float" }` | Menu placement: a bottom float (default) or `style = "split"` for `botright {height}split` |

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

Menus are separate `magit://menu.*` buffers created from the server's
`pushView`/`openView` effects. The default placement is a bottom floating
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

The `parity` subcommand prints the Magit command parity table (every switch,
option and action of every dispatch menu, with its Magit key, our command id,
status and roadmap item):

```sh
magit-lsp parity [--format markdown|json]
cargo xtask parity            # the same command through the cargo alias
```

The markdown form is embedded in this README between the `parity:start` and
`parity:end` markers; `UPDATE_SNAPSHOTS=1 cargo test --test parity` rewrites
both the block and `tests/snapshots/parity.md`.

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

## Command parity

Every Magit menu item we have not built yet is still listed in its menu with
Magit's key and description, greyed out and reporting its roadmap item when
pressed. This table is generated by `magit-lsp parity` (also available as
`cargo xtask parity`); `tests/parity.rs` fails when it is stale. Action keys
include the menu's Magit prefix (`c c`); switch and option keys use magit-lsp's
canonical menu notation (`-a` toggles a switch, `=A` sets an option).

<!-- parity:start -->
| Magit command | Magit key | magit-lsp command | Status | Menu |
| --- | --- | --- | --- | --- |
| `-` | `-a` | `-` | Implemented | commit |
| `-` | `-e` | `-` | Implemented | commit |
| `-` | `-v` | `-` | Implemented | commit |
| `-` | `-n` | `-` | Implemented | commit |
| `-` | `-R` | `-` | Implemented | commit |
| `-` | `-s` | `-` | Implemented | commit |
| `-` | `=A` | `-` | Implemented | commit |
| `-` | `=D` | `-` | Implemented | commit |
| `-` | `=S` | `-` | Implemented | commit |
| `-` | `=C` | `-` | Implemented | commit |
| `-` | `=c` | `-` | Implemented | commit |
| `magit-commit-create` | `c c` | `commit.create` | Implemented | commit |
| `magit-commit-extend` | `c e` | `commit.extend` | Planned R-4.1 | commit |
| `magit-commit-amend` | `c a` | `commit.amend` | Planned R-4.1 | commit |
| `magit-commit-reword` | `c w` | `commit.reword` | Planned R-4.1 | commit |
| `magit-commit-reshelve` | `c d` | `commit.reshelve` | Planned R-4.1 | commit |
| `magit-commit-fixup` | `c f` | `commit.fixup` | Planned R-4.1 | commit |
| `magit-commit-squash` | `c s` | `commit.squash` | Planned R-4.1 | commit |
| `magit-commit-alter` | `c A` | `commit.alter` | Planned R-4.1 | commit |
| `magit-commit-augment` | `c n` | `commit.augment` | Planned R-4.1 | commit |
| `magit-commit-revise` | `c W` | `commit.revise` | Planned R-4.1 | commit |
| `magit-commit-instant-fixup` | `c F` | `commit.instant-fixup` | Planned R-4.1 | commit |
| `magit-commit-instant-squash` | `c S` | `commit.instant-squash` | Planned R-4.1 | commit |
| `magit-rebase-reword-commit` | `c R` | `commit.reword-past` | Planned R-4.1 | commit |
| `magit-commit-autofixup` | `c x` | `commit.autofixup` | Planned R-4.1 | commit |
| `magit-commit-absorb-modules` | `c X` | `commit.absorb-modules` | Planned R-4.1 | commit |
| `-` | `?` | `open.dispatch` | Implemented | commit |
| `-` | `-m` | `-` | Implemented | branch |
| `-` | `-r` | `-` | Implemented | branch |
| `magit-branch.<branch>.description` | `b d` | `branch.configure-description` | Planned R-6.1 | branch |
| `-` | `=u` | `-` | Implemented | branch |
| `-` | `=r` | `-` | Implemented | branch |
| `-` | `=p` | `-` | Implemented | branch |
| `-` | `=R` | `-` | Implemented | branch |
| `-` | `=P` | `-` | Implemented | branch |
| `magit-update-default-branch` | `b B` | `branch.update-default` | Planned R-6.1 | branch |
| `magit-checkout` | `b b` | `branch.checkout` | Planned R-6.1 | branch |
| `magit-branch-checkout` | `b l` | `branch.checkout-local` | Planned R-6.1 | branch |
| `magit-branch-orphan` | `b o` | `branch.orphan` | Planned R-6.1 | branch |
| `magit-checkout-remote-ref` | `b r` | `branch.checkout-remote` | Planned R-6.1 | branch |
| `magit-branch-and-checkout` | `b c` | `branch.create-and-checkout` | Planned R-6.1 | branch |
| `magit-branch-spinoff` | `b s` | `branch.spinoff` | Planned R-6.1 | branch |
| `magit-worktree-checkout` | `b w` | `branch.worktree-checkout` | Planned R-6.1 | branch |
| `magit-branch-create` | `b n` | `branch.create` | Planned R-6.1 | branch |
| `magit-branch-spinout` | `b S` | `branch.spinout` | Planned R-6.1 | branch |
| `magit-worktree-branch` | `b W` | `branch.worktree-create` | Planned R-6.1 | branch |
| `magit-branch-configure` | `b C` | `branch.configure` | Planned R-6.1 | branch |
| `magit-branch-rename` | `b m` | `branch.rename` | Planned R-6.1 | branch |
| `magit-branch-reset` | `b x` | `branch.reset` | Planned R-6.1 | branch |
| `magit-branch-delete` | `b k` | `branch.delete` | Planned R-6.1 | branch |
| `magit-branch-shelve` | `b h` | `branch.shelve` | Planned R-6.1 | branch |
| `magit-branch-unshelve` | `b H` | `branch.unshelve` | Planned R-6.1 | branch |
| `-` | `?` | `open.dispatch` | Implemented | branch |
| `-` | `-p` | `-` | Implemented | fetch |
| `-` | `-t` | `-` | Implemented | fetch |
| `-` | `-u` | `-` | Implemented | fetch |
| `-` | `-F` | `-` | Implemented | fetch |
| `magit-fetch-from-pushremote` | `f p` | `fetch.from-pushremote` | Planned R-6.2 | fetch |
| `magit-fetch-from-upstream` | `f u` | `fetch.from-upstream` | Planned R-6.2 | fetch |
| `magit-fetch-other` | `f e` | `fetch.other` | Planned R-6.2 | fetch |
| `magit-fetch-all` | `f a` | `fetch.all` | Planned R-6.2 | fetch |
| `magit-fetch-branch` | `f o` | `fetch.branch` | Planned R-6.2 | fetch |
| `magit-fetch-refspec` | `f r` | `fetch.refspec` | Planned R-6.2 | fetch |
| `magit-fetch-modules` | `f m` | `fetch.modules` | Planned R-6.2 | fetch |
| `magit-branch-configure` | `f C` | `fetch.configure` | Planned R-6.2 | fetch |
| `-` | `?` | `open.dispatch` | Implemented | fetch |
| `-` | `-f` | `-` | Implemented | pull |
| `-` | `=r` | `-` | Implemented | pull |
| `-` | `-A` | `-` | Implemented | pull |
| `-` | `-F` | `-` | Implemented | pull |
| `magit-pull-from-pushremote` | `F p` | `pull.from-pushremote` | Planned R-6.2 | pull |
| `magit-pull-from-upstream` | `F u` | `pull.from-upstream` | Planned R-6.2 | pull |
| `magit-pull-branch` | `F e` | `pull.other` | Planned R-6.2 | pull |
| `magit-pull-into-upstream` | `F U` | `pull.into-upstream` | Planned R-6.2 | pull |
| `magit-fetch-all-no-prune` | `F f` | `pull.fetch-remotes` | Planned R-6.2 | pull |
| `magit-fetch-all-prune` | `F F` | `pull.fetch-remotes-prune` | Planned R-6.2 | pull |
| `-` | `?` | `open.dispatch` | Implemented | pull |
| `-` | `-f` | `-` | Implemented | push |
| `-` | `-F` | `-` | Implemented | push |
| `-` | `-h` | `-` | Implemented | push |
| `-` | `-n` | `-` | Implemented | push |
| `-` | `-u` | `-` | Implemented | push |
| `-` | `-T` | `-` | Implemented | push |
| `-` | `-t` | `-` | Implemented | push |
| `-` | `=o` | `-` | Implemented | push |
| `magit-push-current-to-pushremote` | `P p` | `push.to-pushremote` | Planned R-6.2 | push |
| `magit-push-current-to-upstream` | `P u` | `push.to-upstream` | Planned R-6.2 | push |
| `magit-push-current` | `P e` | `push.current` | Planned R-6.2 | push |
| `magit-push-other` | `P o` | `push.other` | Planned R-6.2 | push |
| `magit-push-refspecs` | `P r` | `push.refspecs` | Planned R-6.2 | push |
| `magit-push-matching` | `P m` | `push.matching` | Planned R-6.2 | push |
| `magit-push-tag` | `P T` | `push.tag` | Planned R-6.2 | push |
| `magit-push-tags` | `P t` | `push.tags` | Planned R-6.2 | push |
| `magit-push-notes-ref` | `P n` | `push.notes-ref` | Planned R-6.2 | push |
| `magit-branch-configure` | `P C` | `push.configure` | Planned R-6.2 | push |
| `-` | `?` | `open.dispatch` | Implemented | push |
| `-` | `-f` | `-` | Implemented | merge |
| `-` | `-n` | `-` | Implemented | merge |
| `-` | `=s` | `-` | Implemented | merge |
| `-` | `=X` | `-` | Implemented | merge |
| `-` | `-b` | `-` | Implemented | merge |
| `-` | `-w` | `-` | Implemented | merge |
| `-` | `=a` | `-` | Implemented | merge |
| `-` | `-S` | `-` | Implemented | merge |
| `-` | `-s` | `-` | Implemented | merge |
| `magit-merge-plain` | `m m` | `merge.plain` | Planned R-7.1 | merge |
| `magit-merge-editmsg` | `m e` | `merge.editmsg` | Planned R-7.1 | merge |
| `magit-merge-nocommit` | `m n` | `merge.nocommit` | Planned R-7.1 | merge |
| `magit-merge-absorb` | `m a` | `merge.absorb` | Planned R-7.1 | merge |
| `magit-merge-preview` | `m p` | `merge.preview` | Planned R-7.1 | merge |
| `magit-merge-squash` | `m s` | `merge.squash` | Planned R-7.1 | merge |
| `magit-merge-dissolve` | `m d` | `merge.dissolve` | Planned R-7.1 | merge |
| `-` | `?` | `open.dispatch` | Implemented | merge |
| `-` | `-k` | `-` | Implemented | rebase |
| `-` | `-p` | `-` | Implemented | rebase |
| `-` | `-r` | `-` | Implemented | rebase |
| `-` | `-u` | `-` | Implemented | rebase |
| `-` | `=s` | `-` | Implemented | rebase |
| `-` | `=X` | `-` | Implemented | rebase |
| `-` | `=a` | `-` | Implemented | rebase |
| `-` | `-f` | `-` | Implemented | rebase |
| `-` | `-d` | `-` | Implemented | rebase |
| `-` | `-t` | `-` | Implemented | rebase |
| `-` | `-A` | `-` | Implemented | rebase |
| `-` | `-S` | `-` | Implemented | rebase |
| `-` | `-i` | `-` | Implemented | rebase |
| `-` | `-h` | `-` | Implemented | rebase |
| `-` | `=x` | `-` | Implemented | rebase |
| `-` | `-g` | `-` | Implemented | rebase |
| `-` | `-s` | `-` | Implemented | rebase |
| `magit-rebase-onto-pushremote` | `r p` | `rebase.onto-pushremote` | Planned R-7.3 | rebase |
| `magit-rebase-onto-upstream` | `r u` | `rebase.onto-upstream` | Planned R-7.3 | rebase |
| `magit-rebase-branch` | `r e` | `rebase.elsewhere` | Planned R-7.3 | rebase |
| `magit-rebase-interactive` | `r i` | `rebase.interactive` | Planned R-7.3 | rebase |
| `magit-rebase-subset` | `r s` | `rebase.subset` | Planned R-7.3 | rebase |
| `magit-rebase-edit-commit` | `r m` | `rebase.edit-commit` | Planned R-7.3 | rebase |
| `magit-rebase-reword-commit` | `r w` | `rebase.reword-commit` | Planned R-7.3 | rebase |
| `magit-rebase-remove-commit` | `r k` | `rebase.remove-commit` | Planned R-7.3 | rebase |
| `magit-rebase-autosquash` | `r f` | `rebase.autosquash` | Planned R-7.3 | rebase |
| `magit-reshelve-since` | `r t` | `rebase.change-dates` | Planned R-7.3 | rebase |
| `magit-rebase-continue` | `r r` | `rebase.continue` | Planned R-7.3 | rebase |
| `magit-rebase-abort` | `r a` | `rebase.abort` | Planned R-7.3 | rebase |
| `-` | `?` | `open.dispatch` | Implemented | rebase |
| `-` | `-u` | `-` | Implemented | stash |
| `-` | `-a` | `-` | Implemented | stash |
| `magit-stash-both` | `z z` | `stash.both` | Planned R-6.5 | stash |
| `magit-stash-index` | `z i` | `stash.index` | Planned R-6.5 | stash |
| `magit-stash-worktree` | `z w` | `stash.worktree` | Planned R-6.5 | stash |
| `magit-stash-keep-index` | `z x` | `stash.keep-index` | Planned R-6.5 | stash |
| `magit-stash-push` | `z P` | `stash.push` | Planned R-6.5 | stash |
| `magit-snapshot-both` | `z Z` | `stash.snapshot-both` | Planned R-6.5 | stash |
| `magit-snapshot-index` | `z I` | `stash.snapshot-index` | Planned R-6.5 | stash |
| `magit-snapshot-worktree` | `z W` | `stash.snapshot-worktree` | Planned R-6.5 | stash |
| `magit-wip-commit` | `z r` | `stash.wip-commit` | Planned R-6.5 | stash |
| `magit-stash-apply` | `z a` | `stash.apply` | Planned R-6.5 | stash |
| `magit-stash-pop` | `z p` | `stash.pop` | Planned R-6.5 | stash |
| `magit-stash-drop` | `z k` | `stash.drop` | Planned R-6.5 | stash |
| `magit-stash-list` | `z l` | `stash.list` | Planned R-6.5 | stash |
| `magit-stash-show` | `z v` | `stash.show` | Planned R-6.5 | stash |
| `magit-stash-branch` | `z b` | `stash.branch` | Planned R-6.5 | stash |
| `magit-stash-branch-here` | `z B` | `stash.branch-here` | Planned R-6.5 | stash |
| `magit-stash-format-patch` | `z f` | `stash.format-patch` | Planned R-6.5 | stash |
| `-` | `?` | `open.dispatch` | Implemented | stash |
| `-` | `-f` | `-` | Implemented | tag |
| `-` | `-e` | `-` | Implemented | tag |
| `-` | `-a` | `-` | Implemented | tag |
| `-` | `-s` | `-` | Implemented | tag |
| `-` | `=u` | `-` | Implemented | tag |
| `magit-tag-create` | `t t` | `tag.create` | Planned R-6.3 | tag |
| `magit-tag-release` | `t r` | `tag.release` | Planned R-6.3 | tag |
| `magit-tag-delete` | `t k` | `tag.delete` | Planned R-6.3 | tag |
| `magit-tag-prune` | `t p` | `tag.prune` | Planned R-6.3 | tag |
| `-` | `?` | `open.dispatch` | Implemented | tag |
| `magit-branch-reset` | `X b` | `reset.branch` | Planned R-6.3 | reset |
| `magit-file-checkout` | `X f` | `reset.file` | Planned R-6.3 | reset |
| `magit-reset-mixed` | `X m` | `reset.mixed` | Planned R-6.3 | reset |
| `magit-reset-soft` | `X s` | `reset.soft` | Planned R-6.3 | reset |
| `magit-reset-hard` | `X h` | `reset.hard` | Planned R-6.3 | reset |
| `magit-reset-keep` | `X k` | `reset.keep` | Planned R-6.3 | reset |
| `magit-reset-index` | `X i` | `reset.index` | Planned R-6.3 | reset |
| `magit-reset-worktree` | `X w` | `reset.worktree` | Planned R-6.3 | reset |
| `-` | `?` | `open.dispatch` | Implemented | reset |
| `-` | `=m` | `-` | Implemented | cherry-pick |
| `-` | `=s` | `-` | Implemented | cherry-pick |
| `-` | `-F` | `-` | Implemented | cherry-pick |
| `-` | `-x` | `-` | Implemented | cherry-pick |
| `-` | `-e` | `-` | Implemented | cherry-pick |
| `-` | `-g` | `-` | Implemented | cherry-pick |
| `-` | `-s` | `-` | Implemented | cherry-pick |
| `magit-cherry-copy` | `A A` | `cherry-pick.pick` | Planned R-7.2 | cherry-pick |
| `magit-cherry-apply` | `A a` | `cherry-pick.apply` | Planned R-7.2 | cherry-pick |
| `magit-cherry-harvest` | `A h` | `cherry-pick.harvest` | Planned R-7.2 | cherry-pick |
| `magit-merge-squash` | `A m` | `cherry-pick.squash` | Planned R-7.2 | cherry-pick |
| `magit-cherry-donate` | `A d` | `cherry-pick.donate` | Planned R-7.2 | cherry-pick |
| `magit-cherry-spinout` | `A n` | `cherry-pick.spinout` | Planned R-7.2 | cherry-pick |
| `magit-cherry-spinoff` | `A s` | `cherry-pick.spinoff` | Planned R-7.2 | cherry-pick |
| `-` | `?` | `open.dispatch` | Implemented | cherry-pick |
| `-` | `=m` | `-` | Implemented | revert |
| `-` | `-e` | `-` | Implemented | revert |
| `-` | `-E` | `-` | Implemented | revert |
| `-` | `=s` | `-` | Implemented | revert |
| `-` | `-g` | `-` | Implemented | revert |
| `-` | `-s` | `-` | Implemented | revert |
| `magit-revert-and-commit` | `V V` | `revert.commit` | Planned R-7.2 | revert |
| `magit-revert-no-commit` | `V v` | `revert.changes` | Planned R-7.2 | revert |
| `-` | `?` | `open.dispatch` | Implemented | revert |
| `-` | `-n` | `-` | Implemented | bisect |
| `-` | `-p` | `-` | Implemented | bisect |
| `-` | `=o` | `-` | Implemented | bisect |
| `-` | `=n` | `-` | Implemented | bisect |
| `magit-bisect-start` | `B B` | `bisect.start` | Planned R-8.2 | bisect |
| `magit-bisect-run` | `B s` | `bisect.run` | Planned R-8.2 | bisect |
| `magit-bisect-good` | `B g` | `bisect.good` | Planned R-8.2 | bisect |
| `magit-bisect-mark` | `B m` | `bisect.mark` | Planned R-8.2 | bisect |
| `magit-bisect-skip` | `B k` | `bisect.skip` | Planned R-8.2 | bisect |
| `magit-bisect-reset` | `B r` | `bisect.reset` | Planned R-8.2 | bisect |
| `-` | `?` | `open.dispatch` | Implemented | bisect |
| `-` | `-f` | `-` | Implemented | remote |
| `-` | `=u` | `-` | Implemented | remote |
| `-` | `=U` | `-` | Implemented | remote |
| `-` | `=s` | `-` | Implemented | remote |
| `-` | `=S` | `-` | Implemented | remote |
| `-` | `=O` | `-` | Implemented | remote |
| `-` | `=h` | `-` | Implemented | remote |
| `magit-remote-add` | `M a` | `remote.add` | Planned R-6.3 | remote |
| `magit-remote-rename` | `M r` | `remote.rename` | Planned R-6.3 | remote |
| `magit-remote-remove` | `M k` | `remote.remove` | Planned R-6.3 | remote |
| `magit-remote-configure` | `M C` | `remote.configure` | Planned R-6.3 | remote |
| `magit-remote-prune` | `M p` | `remote.prune` | Planned R-6.3 | remote |
| `magit-remote-prune-refspecs` | `M P` | `remote.prune-refspecs` | Planned R-6.3 | remote |
| `magit-remote-unshallow` | `M z` | `remote.unshallow` | Planned R-6.3 | remote |
| `magit-update-default-branch` | `M d u` | `remote.update-default` | Planned R-6.3 | remote |
| `-` | `?` | `open.dispatch` | Implemented | remote |
| `-` | `-f` | `-` | Implemented | submodule |
| `-` | `-r` | `-` | Implemented | submodule |
| `-` | `-N` | `-` | Implemented | submodule |
| `-` | `-C` | `-` | Implemented | submodule |
| `-` | `-R` | `-` | Implemented | submodule |
| `-` | `-M` | `-` | Implemented | submodule |
| `-` | `-U` | `-` | Implemented | submodule |
| `magit-submodule-add` | `o a` | `submodule.add` | Planned R-8.2 | submodule |
| `magit-submodule-register` | `o r` | `submodule.register` | Planned R-8.2 | submodule |
| `magit-submodule-populate` | `o p` | `submodule.populate` | Planned R-8.2 | submodule |
| `magit-submodule-update` | `o u` | `submodule.update` | Planned R-8.2 | submodule |
| `magit-submodule-synchronize` | `o s` | `submodule.synchronize` | Planned R-8.2 | submodule |
| `magit-submodule-unpopulate` | `o d` | `submodule.unpopulate` | Planned R-8.2 | submodule |
| `magit-submodule-remove` | `o k` | `submodule.remove` | Planned R-8.2 | submodule |
| `magit-list-submodules` | `o l` | `submodule.list` | Planned R-8.2 | submodule |
| `magit-fetch-modules` | `o f` | `submodule.fetch` | Planned R-8.2 | submodule |
| `-` | `?` | `open.dispatch` | Implemented | submodule |
| `magit-worktree-checkout` | `Z b` | `worktree.checkout` | Planned R-8.2 | worktree |
| `magit-worktree-branch` | `Z c` | `worktree.create` | Planned R-8.2 | worktree |
| `magit-worktree-move` | `Z m` | `worktree.move` | Planned R-8.2 | worktree |
| `magit-worktree-delete` | `Z k` | `worktree.delete` | Planned R-8.2 | worktree |
| `magit-worktree-status` | `Z g` | `worktree.visit` | Planned R-8.2 | worktree |
| `-` | `?` | `open.dispatch` | Implemented | worktree |
| `magit-gitignore-in-topdir` | `i t` | `ignore.topdir` | Planned R-8.2 | ignore |
| `magit-gitignore-in-subdir` | `i s` | `ignore.subdir` | Planned R-8.2 | ignore |
| `magit-gitignore-in-gitdir` | `i p` | `ignore.gitdir` | Planned R-8.2 | ignore |
| `magit-gitignore-on-system` | `i g` | `ignore.system` | Planned R-8.2 | ignore |
| `magit-skip-worktree` | `i w` | `ignore.skip-worktree` | Planned R-8.2 | ignore |
| `magit-no-skip-worktree` | `i W` | `ignore.no-skip-worktree` | Planned R-8.2 | ignore |
| `magit-assume-unchanged` | `i u` | `ignore.assume-unchanged` | Planned R-8.2 | ignore |
| `magit-no-assume-unchanged` | `i U` | `ignore.no-assume-unchanged` | Planned R-8.2 | ignore |
| `-` | `?` | `open.dispatch` | Implemented | ignore |
| `-` | `=n` | `-` | Implemented | log |
| `-` | `=a` | `-` | Implemented | log |
| `-` | `=s` | `-` | Implemented | log |
| `-` | `=u` | `-` | Implemented | log |
| `-` | `=g` | `-` | Implemented | log |
| `-` | `-i` | `-` | Implemented | log |
| `-` | `-I` | `-` | Implemented | log |
| `-` | `=M` | `-` | Implemented | log |
| `-` | `=m` | `-` | Implemented | log |
| `-` | `=p` | `-` | Implemented | log |
| `-` | `-D` | `-` | Implemented | log |
| `-` | `-f` | `-` | Implemented | log |
| `-` | `-r` | `-` | Implemented | log |
| `-` | `-g` | `-` | Implemented | log |
| `-` | `-c` | `-` | Implemented | log |
| `-` | `-d` | `-` | Implemented | log |
| `-` | `-S` | `-` | Implemented | log |
| `-` | `-h` | `-` | Implemented | log |
| `-` | `-p` | `-` | Implemented | log |
| `-` | `-s` | `-` | Implemented | log |
| `magit-log-current` | `l l` | `log.current` | Planned R-5.1 | log |
| `magit-log-other` | `l o` | `log.other` | Planned R-5.1 | log |
| `magit-log-head` | `l h` | `log.head` | Planned R-5.1 | log |
| `magit-log-related` | `l u` | `log.related` | Planned R-5.1 | log |
| `magit-log-branches` | `l L` | `log.local-branches` | Planned R-5.1 | log |
| `magit-log-all-branches` | `l b` | `log.all-branches` | Planned R-5.1 | log |
| `magit-log-all` | `l a` | `log.all-references` | Planned R-5.1 | log |
| `magit-log-reflog` | `l R` | `log.reflog-objects` | Planned R-5.1 | log |
| `magit-log-matching-branches` | `l B` | `log.matching-branches` | Planned R-5.1 | log |
| `magit-log-matching-tags` | `l T` | `log.matching-tags` | Planned R-5.1 | log |
| `magit-log-merged` | `l m` | `log.merged` | Planned R-5.1 | log |
| `magit-reflog-current` | `l r` | `log.reflog-current` | Planned R-5.1 | log |
| `magit-reflog-other` | `l O` | `log.reflog-other` | Planned R-5.1 | log |
| `magit-reflog-head` | `l H` | `log.reflog-head` | Planned R-5.1 | log |
| `magit-wip-log-index` | `l i` | `log.wiplog-index` | Planned R-5.1 | log |
| `magit-wip-log-worktree` | `l w` | `log.wiplog-worktree` | Planned R-5.1 | log |
| `magit-shortlog` | `l s` | `log.shortlog` | Planned R-5.1 | log |
| `-` | `?` | `open.dispatch` | Implemented | log |
| `-` | `-b` | `-` | Implemented | diff |
| `-` | `-w` | `-` | Implemented | diff |
| `-` | `-D` | `-` | Implemented | diff |
| `-` | `-W` | `-` | Implemented | diff |
| `-` | `=a` | `-` | Implemented | diff |
| `-` | `=m` | `-` | Implemented | diff |
| `-` | `-M` | `-` | Implemented | diff |
| `-` | `-C` | `-` | Implemented | diff |
| `-` | `-H` | `-` | Implemented | diff |
| `-` | `-R` | `-` | Implemented | diff |
| `-` | `=c` | `-` | Implemented | diff |
| `-` | `-S` | `-` | Implemented | diff |
| `-` | `-s` | `-` | Implemented | diff |
| `magit-diff-dwim` | `d d` | `diff.dwim` | Planned R-5.2 | diff |
| `magit-diff-range` | `d r` | `diff.range` | Planned R-5.2 | diff |
| `magit-diff-paths` | `d p` | `diff.paths` | Planned R-5.2 | diff |
| `magit-diff-unstaged` | `d u` | `diff.unstaged` | Planned R-5.2 | diff |
| `magit-diff-staged` | `d s` | `diff.staged` | Planned R-5.2 | diff |
| `magit-diff-working-tree` | `d w` | `diff.worktree` | Planned R-5.2 | diff |
| `magit-show-commit` | `d c` | `diff.show-commit` | Planned R-5.2 | diff |
| `magit-stash-show` | `d t` | `diff.show-stash` | Planned R-5.2 | diff |
| `-` | `?` | `open.dispatch` | Implemented | diff |
| `-` | `-3` | `-` | Implemented | patch-apply |
| `-` | `-R` | `-` | Implemented | patch-apply |
| `-` | `=p` | `-` | Implemented | patch-apply |
| `-` | `-c` | `-` | Implemented | patch-apply |
| `-` | `-k` | `-` | Implemented | patch-apply |
| `-` | `-b` | `-` | Implemented | patch-apply |
| `-` | `-d` | `-` | Implemented | patch-apply |
| `-` | `-t` | `-` | Implemented | patch-apply |
| `-` | `-g` | `-` | Implemented | patch-apply |
| `-` | `-s` | `-` | Implemented | patch-apply |
| `magit-am-apply-maildir` | `w m` | `patch.apply-maildir` | Planned R-8.2 | patch-apply |
| `magit-am-apply-patches` | `w w` | `patch.apply-patches` | Planned R-8.2 | patch-apply |
| `magit-patch-apply` | `w a` | `patch.apply-plain` | Planned R-8.2 | patch-apply |
| `-` | `?` | `open.dispatch` | Implemented | patch-apply |
| `magit-patch-create` | `W c` | `patch.create` | Planned R-8.2 | patch-create |
| `magit-am` | `W w` | `open.patch-apply` | Implemented | patch-create |
| `magit-patch-apply` | `W a` | `patch.apply-plain` | Planned R-8.2 | patch-create |
| `magit-patch-save` | `W s` | `patch.save` | Planned R-8.2 | patch-create |
| `magit-request-pull` | `W r` | `patch.request-pull` | Planned R-8.2 | patch-create |
| `-` | `?` | `open.dispatch` | Implemented | patch-create |
| `magit-blame` | `b` | `blame.start` | Planned R-8.1 | blame |
| `magit-blame-removal` | `r` | `blame.removal` | Planned R-8.1 | blame |
| `magit-blame-reverse` | `f` | `blame.reverse` | Planned R-8.1 | blame |
| `magit-blame-echo` | `e` | `blame.echo` | Planned R-8.1 | blame |
| `magit-blame-next-chunk` | `n` | `blame.next-chunk` | Planned R-8.1 | blame |
| `magit-blame-next-chunk-same-commit` | `N` | `blame.next-chunk-same-commit` | Planned R-8.1 | blame |
| `magit-blame-previous-chunk` | `p` | `blame.previous-chunk` | Planned R-8.1 | blame |
| `magit-blame-previous-chunk-same-commit` | `P` | `blame.previous-chunk-same-commit` | Planned R-8.1 | blame |
| `magit-blame-cycle-style` | `c` | `blame.cycle-style` | Planned R-8.1 | blame |
| `-` | `?` | `open.dispatch` | Implemented | blame |
<!-- parity:end -->

## Notes

- The client is deliberately thin: it opens the view, applies `view/render`
  text/edits, acks versions, resyncs on a lost update, and forwards keys as
  `view/key` steps. All status structure, staging decisions, keymaps and cursor
  succession live in the server.
- Like Magit and Neogit, mutations shell out to `git` (e.g. `git apply
  --cached`) rather than using a native git library, so behavior stays on
  parity with the CLI.
