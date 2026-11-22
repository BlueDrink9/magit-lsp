# Client installation and quickstart

Both clients launch an **external** server. The Neovim client never downloads
one; the VSCode client downloads only the pinned managed release described
below. Git must be on the editor's PATH (2.30.8 is the tested recommendation).
Neovim requires 0.11+; VSCode requires 1.85+. No GitHub Release assets,
Marketplace, Open VSX or registry-published VSIX availability is claimed here.
Only a locally packaged VSIX is produced (`make vscode-package`). See
[release gates/platform limits](RELEASING.md) and the
[README installer download-and-review example](../README.md#binary-release-installation-not-yet-published).
Binary users do not need Rust. Until approved assets exist, build from source:

```sh
git clone https://github.com/BlueDrink9/magit-lsp.git
cd magit-lsp
cargo build --release --locked
mkdir -p "$HOME/.local/bin"
install -m 755 target/release/magit-lsp "$HOME/.local/bin/magit-lsp"
"$HOME/.local/bin/magit-lsp" --version
```

The examples below use this absolute server location; change it if installed
elsewhere. Add `~/.local/bin` to PATH **before launching the editor** if using
PATH discovery. GUI editors may not inherit a shell's PATH.

## Mason server package (prepared, not submitted)

[`clients/mason/package.yaml`](../clients/mason/package.yaml) is a prepared
Mason registry submission for the **server**, not the Lua client. It is not in
the hosted registry; `:MasonInstall magit-lsp` is not a published installation
path. Install the client separately using a method below.

For local Linux x86_64 GNU verification, run:

```sh
nix develop -c make mason-install-check
```

This fetches actual mason.nvim `v2.3.1` at
`2a6940af80375532e5e9e7c1f2fc6319a1b7a69d` into disposable storage and uses its
registry, compiler and installer (including vendored dependencies). It derives
the private registry's JSON mechanically from the production YAML, rejecting
unhandled content; no package fields are overridden. Only registry selection
(`github:local/magit-registry@fixture`) and `github.download_url_template` are
redirected to loopback HTTP. No runtime registry clone or YAML compiler tool
is needed.

The archive is produced by `scripts/package-release.sh` from the freshly built
**debug** 0.2.0 server, not a hosted/release-mode binary. A corrupt archive must
fail real tar unpack without a receipt or executable; replacing the served
bytes with the valid archive must install successfully in the same Mason root.
The check asserts receipt/source parity, binlink, `--version`, LSP initialize
and rendered status using that installed executable. A test-only launcher
records process identity before executing the installed binlink; it does not
copy or extract the installed binary. HOME/XDG, manager, registry and install
state are isolated and removed, owned editor/server processes are stopped,
and structured evidence is retained in `build/ui/mason-install/`.

This is local Linux/debug-fixture evidence only: no upstream registry
submission, live release download, clean-machine installation, Windows/macOS
verification or publication is claimed. Git/network access is needed to fetch
the pinned manager; Neovim 0.11+, curl and tar/gzip must already be available.

## Neovim native packages

Install only the client by placing the repository in a native `start` package:

```sh
git clone https://github.com/BlueDrink9/magit-lsp.git \
  "${XDG_DATA_HOME:-$HOME/.local/share}/nvim/site/pack/magit/start/magit-lsp"
```

The client ships `doc/magit.txt`: `:h magit` documents `:Magit [directory]`,
minimal setup, server discovery and protocol-version troubleshooting. Run
`:helptags ALL` if your package manager does not generate help tags.

Add this to `init.lua`. `MagitStatus` is a **user command defined by this
example**, not a built-in plugin command:

```lua
-- native-pack init.lua
vim.api.nvim_create_user_command('MagitStatus', function()
  require('magit-lsp').setup({
    cmd = { vim.fn.expand('~/.local/bin/magit-lsp') },
    root_dir = vim.fn.getcwd(),
    profile = 'vim',
  })
end, {})
vim.keymap.set('n', '<leader>gs', '<cmd>MagitStatus<CR>', { desc = 'Git status' })
```

Start Neovim inside the Git repository you want to inspect, then run
`:MagitStatus` or press `<leader>gs`. It opens status in the current window.
Do **not** call `setup()` unconditionally during startup: it starts the server
and opens status immediately, rather than merely storing configuration.

## Neovim lazy.nvim (on demand)

Add this plugin spec to your existing lazy.nvim configuration. There is no
`opts` or automatic `config = true`: either would cause lazy.nvim's default
configuration to call `setup()` during loading. Only the key callback opens
status:

```lua
-- lazy.nvim plugin spec
return {
  'BlueDrink9/magit-lsp',
  lazy = true,
  keys = {
    { '<leader>gs', function()
      require('magit-lsp').setup({
        cmd = { vim.fn.expand('~/.local/bin/magit-lsp') },
        root_dir = vim.fn.getcwd(),
        profile = 'vim',
      })
    end, desc = 'Git status' },
  },
}
```

Launch inside a Git repository, press `<leader>gs`, and expand a changed file
with Tab. `g r` refreshes status in the `vim` profile. Repeat the key to focus
the existing status view. `root_dir` defaults to Neovim's current directory;
pass an explicit repository directory when needed.

Server discovery, if `cmd` is omitted: explicit `cmd` (an argv list) takes
precedence, then `$MAGIT_LSP_BIN` (one executable path), the plugin checkout's
`target/release/magit-lsp`, `target/debug/magit-lsp`, then `magit-lsp` on PATH.
An invalid `$MAGIT_LSP_BIN` reports an error rather than falling through. There
is no `cargo run` fallback. Profiles are `vim` (Neovim default) and `standard`.

## Neovim plugin-free (Plain) LSP

A general LSP client needs no plugin: this `init.lua` starts the external server
for repository `.magit` documents and commit drafts and keeps the session in
Plain presentation. It declares no `capabilities.experimental.magit` block and
relies on Neovim's built-in `window.showDocument` support, so the server
advertises only standard methods and Plain actions and can reveal prompt
documents and commit drafts with no custom handler; see [client implementation
levels](CLIENT_SPEC.md#client-implementation-levels). The test harness loads this
exact block, so it is the tested reference setup.

<!-- plugin-free-plain-config -->
```lua
-- Plugin-free Plain Neovim configuration (copy into init.lua).
local server = vim.env.MAGIT_LSP_BIN or vim.fn.expand('~/.local/bin/magit-lsp')

local function attach_magit(buf)
  local name = vim.api.nvim_buf_get_name(buf)
  if not (name:match('/%.magit$') or name:match('/COMMIT_EDITMSG$')) then
    return
  end
  local capabilities = vim.lsp.protocol.make_client_capabilities()
  vim.lsp.start({
    name = 'magit-lsp',
    cmd = { server },
    root_dir = vim.fn.getcwd(),
    capabilities = capabilities,
  }, { bufnr = buf })
end

vim.api.nvim_create_autocmd({ 'BufReadPost', 'BufNewFile' }, {
  callback = function(args) attach_magit(args.buf) end,
})
vim.api.nvim_create_autocmd('VimEnter', {
  callback = function() attach_magit(vim.api.nvim_get_current_buf()) end,
})
vim.api.nvim_create_user_command('MagitPlainStatus', function()
  vim.cmd.edit('.magit')
end, { desc = 'Open the magit-lsp status document' })
```

Start Neovim in the repository root (the snippet passes `vim.fn.getcwd()` as
`root_dir`), then run `:MagitPlainStatus` or `:edit .magit`. The status view is
rendered into that buffer; the `.magit` file on disk is presentation only and is
not written by the server. To act, put the cursor on a rendered row and run
`:lua vim.lsp.buf.code_action()`. Commit prompts open the real
`.git/COMMIT_EDITMSG` draft automatically; edit it with ordinary Neovim input
and choose the offered **Confirm commit** or **Abort commit** code action.
The optional `$MAGIT_LSP_BIN` override in the snippet matches the plugin's
server-discovery rule; without it the server is read from `~/.local/bin`.

`make plain-neovim` boots this exact block in an isolated Neovim configuration
with a temporary repository and asserts the rendered status text and Git state.
Test probes and state observations live in `tests/ui/plain_init.lua`, outside
the block, so the snippet above stays user-facing.

## VSCode source quickstart (no published install)

In a source checkout, install locked client dependencies with Node.js 22+:

```sh
npm --prefix clients/vscode ci
extension="$(pwd)/clients/vscode"
code --new-window --extensionDevelopmentPath="$extension" /absolute/path/to/git/repo
```

In that development window's settings, point at the actual absolute executable
file (JSON does not expand `~`):

```json
{
  "magit-lsp.server.path": "/home/YOUR_USER/.local/bin/magit-lsp",
  "magit-lsp.server.source": "system",
  "magit-lsp.profile": "standard"
}
```

`magit-lsp.server.path` must name an executable file; a nonempty value is
started as configured and skips PATH discovery and download, and an invalid
path is reported instead of silently falling back. `magit-lsp.server.source`
defaults to `auto`: an explicit path first, then a version-matching
`magit-lsp` on PATH (the server's `--version` output must match exactly), then
the pinned managed release downloaded into extension storage. `system` uses only
a path or PATH binary and never downloads; `managed` always downloads the pinned
release matching the extension version and ignores the path. Managed downloads
are SHA-256 verified, cached atomically and reused offline. A bare PATH name
belongs to `auto`/`system` discovery, not `magit-lsp.server.path`.

Open the Command Palette and run **Magit: Open Status** (`magit.open`).
`magit-lsp.profile` accepts `standard` (default) or `vim`. Neovim's
`$MAGIT_LSP_BIN` and checkout-target search are **not** VSCode configuration.
The `magit.installServer` and `magit.serverInfo` commands (category Magit LSP)
install and inspect the managed server. This source development host is not a
VSIX installation test.

Package the extension locally with `make vscode-package`; the resulting
`build/vscode/magit-lsp-<version>.vsix` contains the same bytes a publication
would ship. `make vscode-vsix-install` installs that VSIX into an isolated
profile and launches the real extension without `--extensionDevelopmentPath`,
using the seeded managed-cache server. No Marketplace or Open VSX installation
command is provided for this unpublished extension. See
[native evidence capture](SCREENSHOTS.md) for inspection/cleanup.

## Evidence scope and external clients

The automated smoke runs the exact Lua snippets above with a real local server
and temporary native package/HOME/XDG directories, without mini.nvim or network
downloads. It boots the native package, asserts no startup session, then checks
a rendered status after the explicit action. It also loads the documented lazy
spec as valid Lua and asserts a key-driven spec that sets no `opts`, `config` or
`build`. `make lazy-install-check` goes further: it boots the actual lazy.nvim
manager at pinned tag `v11.17.5` (commit `85c7ff3`) from a disposable cache,
loads this exact spec with only its source directory adapted to the checkout,
presses the documented `<leader>gs` mapping through Neovim input, and asserts
the plugin loads from this checkout, the LSP handshake completes and status
renders. That is local Linux evidence, not a clean-machine or hosted install.
Missing Neovim/server prerequisites skip the smoke, so inspect the test output,
not just the exit code:

```sh
PYTHONDONTWRITEBYTECODE=1 MAGIT_LSP_TEST_BINARY=target/release/magit-lsp \
  python3 -m unittest discover -s tests/release -p 'test_client_setup.py' -v
```

`make vscode-vsix-install` covers the packaged VSIX artifact (the same bytes a
publication would ship), not a registry download. State of each manual asset:
the README screenshots under `scripts/readme-shots/` are a human task with no
CI; the Marketplace/Open VSX publication is an open release gate (see
[release gates/platform limits](RELEASING.md)).
Local smoke evidence is not clean-machine, hosted-artifact or published-install
evidence. Native screenshots are separate from headless setup evidence.

Helix, Zed and JetBrains adapters are welcome as COMMUNITY clients through the
[external client specification](CLIENT_SPEC.md): lifecycle, render
acknowledgements, server-owned structure and optional workflow limits. No
first-party implementation or completion date is promised.
