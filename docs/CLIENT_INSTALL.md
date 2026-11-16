# Client installation and quickstart

Both clients launch an **external** server; neither downloads it automatically.
Git must be on the editor's PATH (2.30.8 is the tested recommendation). Neovim
requires 0.11+; VSCode requires 1.85+. No GitHub Release assets, Marketplace,
Open VSX or downloadable VSIX availability is claimed here. See
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

## Neovim native packages

Install only the client by placing the repository in a native `start` package:

```sh
git clone https://github.com/BlueDrink9/magit-lsp.git \
  "${XDG_DATA_HOME:-$HOME/.local/share}/nvim/site/pack/magit/start/magit-lsp"
```

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

## VSCode source quickstart (not a published install)

In a source checkout, install locked client dependencies with Node.js 22+:

```sh
npm --prefix clients/vscode ci
extension="$(pwd)/clients/vscode"
code --new-window --extensionDevelopmentPath="$extension" /absolute/path/to/git/repo
```

In that development window's settings, set the actual absolute executable path
(JSON does not expand `~`):

```json
{
  "magit-lsp.serverPath": "/home/YOUR_USER/.local/bin/magit-lsp",
  "magit-lsp.profile": "standard"
}
```

Open the Command Palette and run **Magit: Open Status** (`magit.open`).
`magit-lsp.serverPath` defaults to `magit-lsp` on the editor's PATH; the server
is launched without arguments. `magit-lsp.profile` accepts `standard` (default)
or `vim`. Neovim's `$MAGIT_LSP_BIN` and checkout-target search are **not**
VSCode configuration. This source development host is not a VSIX installation
test, and no Marketplace installation command is provided for an unpublished
extension. See [native evidence capture](SCREENSHOTS.md) for inspection/cleanup.

## Evidence scope and external clients

The isolated smoke below runs the exact Lua snippets above using a real local
server and temporary native package/HOME/XDG directories, without mini.nvim or
network downloads. It asserts no startup session, then a rendered status after
the explicit action. The lazy spec is evaluated and its callback invoked with
the client on runtimepath; **this does not run the lazy.nvim manager**, its
download or key-loading machinery. Missing Neovim/server prerequisites skip
the smoke, so inspect the test output, not just the exit code:

```sh
PYTHONDONTWRITEBYTECODE=1 MAGIT_LSP_TEST_BINARY=target/release/magit-lsp \
  python3 -m unittest discover -s tests/release -p 'test_client_setup.py' -v
```

Local smoke evidence is not clean-machine, hosted-artifact, lazy-manager or
published-VSIX evidence; those release gates remain open. Native screenshots
are separate from headless setup evidence. Community Helix, Zed and JetBrains
adapters are welcome: start with the [external client specification](CLIENT_SPEC.md)
for lifecycle, render acknowledgements, server-owned structure and optional
workflow limits. No first-party implementation or completion date is promised.
