# AGENTS.md — internal notes

Agent-facing progress/roadmap notes. User-facing docs live in `README.md`.

## Project

`magit-lsp` (formerly "majjit"): an LSP server that presents a Git repo as a
structured document, plus a Neovim client rendering a Magit/Neogit-style status
buffer. Rust server + Lua client, talking LSP over stdio.

### Ultimate goals

- **Decouple the server from any client UI.** The server should do as much as
  possible (state, rendering, structure, actions) so clients stay thin.
- **Trivial to build clients for any editor** — Neovim today; VSCode, other
  editors, or a standalone TUI later — by implementing the same protocol.
- **Easy to fork for other VCSs**, including `jj`. Keep the VCS-specific code
  behind a backend boundary and avoid baking Git assumptions into the protocol
  or client.

## Architecture / internals

Data flow: client request → server runs `git` / mutates the index → server
returns the fresh `Status` → client re-renders. Reads and mutations use the
same custom request/response pattern (no `workspace/applyEdit`).

The server parses the rendered status document (`src/document.rs`) to serve
structural LSP features. Because rendering currently lives in the client, the
client pushes the rendered text to the server via `vcs/didRender` (standard
`didOpen`/`didChange` sync is also handled).

### Custom protocol

| Method | Purpose |
| ------ | ------- |
| `vcs/status` | Structured status (branch, staged/unstaged files + hunks) |
| `vcs/diff` | Unified diff for one file (staged or unstaged) |
| `vcs/stageHunk` / `vcs/unstageHunk` | Stage/unstage a whole hunk |
| `vcs/stageLines` / `vcs/unstageLines` | Stage/unstage a line range within a hunk |
| `vcs/stageFile` / `vcs/unstageFile` | Stage/unstage a whole file |
| `vcs/stageAll` / `vcs/unstageAll` | Stage/unstage everything |
| `vcs/targets` | Navigation targets (section/file/hunk + line) |
| `textDocument/foldingRange` | Fold ranges for sections and files |
| `vcs/requestTransient` | **Stub** — commit-menu schema (reserved) |

Every mutation returns a `MutationResponse { status, effects }`, which the
client renders. A failed `git` command is not a JSON-RPC error: it comes back
as an error-level `Effect::Message` with a readable summary (see
`src/errors.rs`). JSON-RPC errors are reserved for protocol faults.

### Source layout

- `src/git.rs` — `git` CLI backend (status, diff, staging).
- `src/errors.rs` — maps Git failures to readable `UserError`s and effects.
- `src/patch.rs` — unified-diff parsing and hunk/line patch reconstruction.
- `src/document.rs` — parses the rendered status document into structure
  (sections/files/hunks) for folding ranges and targets.
- `src/protocol.rs` — custom JSON-RPC request/response types.
- `src/server.rs` — stdio LSP loop, document store, request dispatch.
- `lua/magit-lsp/init.lua` — Neovim client (render, highlight, folding, keys).
- `tests/` — `git_backend.rs` (Rust) and `test_magit_lsp.lua` (Neovim).

## Progress (done)

- Git backend via the `git` CLI: status, per-file diffs, stage/unstage of
  whole file, whole hunk, and line ranges; stage/unstage all.
- Patch engine (`src/patch.rs`): unified-diff parsing + hunk/line patch
  reconstruction with correct line renumbering; applied with `git apply
  --cached`.
- LSP server (`src/server.rs`): request dispatch, document store, capabilities.
- Client (`lua/magit-lsp/init.lua`): `vim.lsp.start`, `vcs://status` nofile
  buffer, rendering, extmark highlighting, LSP folding, navigation.
- LSP-served structure: `textDocument/foldingRange` and `vcs/targets` from a
  parser (`src/document.rs`) over the rendered document.
- Stubs retained for commit menus: `vcs/requestTransient` +
  `magit-lsp.executeVCSCommand`.
- Tests: Rust unit (patch, document) + integration (git backend on temp repos)
  + Neovim integration via mini.test (real server + temp repo). `make test`.

## Roadmap

See [roadmap.md](./roadmap.md)

## Bugs addressed / challenges

- **Partial hunk/line staging.** `git apply --cached` needs correctly renumbered
  hunks; naively filtering a hunk breaks when other changes sit nearby (context
  gaps). Solved with a linear-simulation renumbering algorithm in `patch.rs`,
  validated empirically against real `git apply` for mod/add/delete selections.
- **Blank status buffer / `attempt to concatenate a userdata value`.** The
  server sent `branch: null`; Neovim decodes JSON null as `vim.NIL` (truthy
  userdata), so `'Head: ' .. vim.NIL` crashed `render`. Fixed by making
  `Status.branch` a `String` and adding a defensive `is_nil` in the client.
- **Neovim deprecations.** Migrated `vim.lsp.start_client` → `vim.lsp.start`
  and `vim.lsp.stop_client` → `client:stop(true)`.
- **Slow tests (58s → ~1s).** Each case's first request waited the full 5s
  timeout because the client wasn't initialized yet. Fixed with a
  `client.initialized` readiness wait (and `cargo build` moved to `pre_once`).
- **Orphan server processes.** Child Neovim exit left `magit-lsp` running.
  Fixed with `client:stop(true)` on `VimLeavePre` plus test teardown.
- **`unstageAll` on unborn HEAD.** `git reset HEAD` fails with no commits;
  now detect HEAD and fall back to `git rm -r --cached --ignore-unmatch .`.
- **Visual-mode mappings did nothing.** `'<`/`'>` are stale while still in
  visual mode; use `line('v')` + `line('.')` instead.
- **gix wouldn't compile.** `gix 0.57` failed on rustc 1.98; dropped gix for
  the `git` CLI, which is also what Magit/Neogit do and what staging requires.

## Known limitations

- Renames/copies show only the new path; no hunk-level staging for them.
- Binary files expose no hunks (whole-file staging only).
- No-trailing-newline handling is implemented but lightly tested.
- Paths with spaces/special characters may need patch-header quoting.
- `stage_all` uses `git add -u` (tracked files only); untracked files must be
  staged individually.

## Conventions

- **Comments** are welcome when they explain *why* a decision was made, or
  clarify an otherwise opaque section of code. Keep them short and specific;
  avoid restating the obvious. Do not number comments.
- **Docstrings**: every external (public) function and type should have a
  concise docstring (`///` in Rust, `---` LuaLS annotations in Lua) describing
  what it does, not how.
- Keep the server/client rendered-document format in sync (`src/document.rs`
  parser vs. the Lua renderer).
- Run `make test` before finishing; ensure no orphan `magit-lsp` processes.
