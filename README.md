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
  buffer, and forwards keys as `view/invoke` commands.

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
- **Foldable** sections and files via LSP folding.
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
`:w`/`:write` is a no-op. Use `gR` to refresh it after changes in the
repository.

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

## Keys (defaults, subject to customization)

| Key      | Action                                                   |
| -------- | -------------------------------------------------------- |
| `s`      | Stage file (on a file headline) or hunk (on a hunk line) |
| `s` (v)  | Stage the visually selected lines                        |
| `u`      | Unstage file or hunk at cursor                           |
| `u` (v)  | Unstage the visually selected lines                      |
| `S`      | Stage all                                                |
| `U`      | Unstage all                                              |
| `<CR>`   | Visit the file at the new-side line under the cursor    |
| `gR`     | Refresh status                                           |
| `]s`/`[s`| Next/previous section                                    |
| `]f`/`[f`| Next/previous file                                       |
| `]h`/`[h`| Next/previous hunk                                       |

## Notes

- The client is deliberately thin: it opens the view, applies `view/render`
  text/edits, acks versions, resyncs on a lost update, and forwards keys as
  `view/invoke` commands. All status structure, staging decisions and cursor
  succession live in the server.
- Like Magit and Neogit, mutations shell out to `git` (e.g. `git apply
  --cached`) rather than using a native git library, so behavior stays on
  parity with the CLI.
