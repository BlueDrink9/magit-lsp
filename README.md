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
- **Neovim client (`lua/magit-lsp`).** Starts the server, renders the returned
  structure into a `nofile` `vcs://status` buffer, and maps keys to requests.

The server is designed to do as much as possible so that clients are thin and
easy to build for any editor. See `AGENTS.md` for the protocol and internals.

## Features

- **Status buffer**: branch header plus `Staged changes`, `Unstaged changes`,
  and `Untracked files` sections (with counts), each file expandable into its
  hunks and diff lines. Never blank — shows `Working tree clean` when clean.
- **Diff viewing**: open a file's unified diff in a side split.
- **Staging**: whole file, whole hunk, or a selected range of lines — and the
  symmetric unstage operations.
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

The status buffer is a `nofile` buffer named `vcs://status`, shown in a vertical
split. It is a read-only view: native editing keys are refused and `:w`/`:write`
is a no-op. Use `gR` to refresh it after changes in the repository.

## Keys (defaults, subject to customization)

| Key      | Action                                                   |
| -------- | -------------------------------------------------------- |
| `s`      | Stage file (on a file headline) or hunk (on a hunk line) |
| `s` (v)  | Stage the visually selected lines                        |
| `u`      | Unstage file or hunk at cursor                           |
| `u` (v)  | Unstage the visually selected lines                      |
| `S`      | Stage all                                                |
| `U`      | Unstage all                                              |
| `<CR>`   | Show the diff for the file at cursor                     |
| `gR`     | Refresh status                                           |
| `]s`/`[s`| Next/previous section                                    |
| `]f`/`[f`| Next/previous file                                       |
| `]h`/`[h`| Next/previous hunk                                       |

## Notes

- The `vcs/requestTransient` request and the `magit-lsp.executeVCSCommand`
  `executeCommand` stub are intentionally kept for the upcoming commit-menu
  (transient/SDUI) feature; they are not wired to real behavior yet.
- Like Magit and Neogit, mutations shell out to `git` (e.g. `git apply
  --cached`) rather than using a native git library, so behavior stays on
  parity with the CLI.
