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

- `src/lib.rs` — crate root; `Error` (message + optional structured Git failure).
- `src/git.rs` — `git` CLI backend (status, diff, staging). Every index
  unstaging goes through `unstage_paths` (works on unborn HEAD).
- `src/errors.rs` — `UserError` mapping (exit code + stderr → summary/hint)
  and conversion to an error-level `Effect::Message`.
- `src/patch.rs` — unified-diff parsing, selection normalisation, and hunk/line
  patch construction. Counts are left stale; `git apply --recount` infers them.
- `src/dispatch.rs` — maps `(command, node kind, selection)` to an operation and
  executes stage/unstage (discard execution and wiring deferred).
- `src/document.rs` — **legacy**: parses the rendered status document. Slated for
  deletion in R-1.1; do not extend it.
- `src/protocol.rs` — custom JSON-RPC types (`MutationResponse`, `Effect`, …).
- `src/server.rs` — stdio LSP loop, document store, request dispatch.
- `src/vcs.rs` — vestigial gix-based backend stub; unused, to be deleted.
- `lua/magit-lsp/init.lua` — Neovim client (render, highlight, folding, keys,
  read-only buffer).
- `tests/common/mod.rs` — `RepoFixture` and git helpers with a deterministic
  environment (fixed identity/dates, isolated config).
- `tests/git_backend.rs`, `tests/dispatch.rs`, `tests/errors.rs` — Rust
  integration tests on temp repos.
- `tests/test_magit_lsp.lua` — Neovim integration via mini.test (real server).
- `scripts/color_spike.lua` — `:TOhtml` colour-verification spike
  (`make color-spike`).

## Progress (done)

- Git backend via the `git` CLI: status, per-file diffs, stage/unstage of
  whole file, whole hunk, and line ranges; stage/unstage all. Unstaging a path
  works on an unborn HEAD (`unstage_paths`, IMM-1).
- Selection normalisation in `patch.rs` (IMM-2): only `+`/`-` lines in a
  selection are staged; headers select the whole file/hunk; patches are applied
  with `git apply --cached --recount` (IMM-5 audit: no diff computation outside
  `git diff` parsing).
- Readable errors (IMM-3): Git failures become a `UserError` carried in an
  `Effect::Message`; `MutationResponse { status, effects }` on every mutation.
- Neovim status buffer is read-only (IMM-4): `modifiable=false`,
  `readonly=true`, `undolevels=-1`, a single `set_lines` writer, `:w` blocked.
- Node-kind dispatch for stage/unstage in `src/dispatch.rs` (IMM-6).
- Test harness extracted to `tests/common/mod.rs` with `RepoFixture` and
  deterministic git env (IMM-7).
- Colour spike (IMM-8): `scripts/color_spike.lua` writes
  `build/test-output/status-<scheme>-<background>.html` and prints the class →
  link → resolved-fg table; findings in `TASKS.md` §8.4.
- LSP server (`src/server.rs`): request dispatch, document store, capabilities.
- Client (`lua/magit-lsp/init.lua`): `vim.lsp.start`, `vcs://status` nofile
  buffer, rendering, extmark highlighting, LSP folding, navigation.
- LSP-served structure: `textDocument/foldingRange` and `vcs/targets` from a
  parser (`src/document.rs`) over the rendered document (legacy, see R-1.1).
- Stubs retained for commit menus: `vcs/requestTransient` +
  `magit-lsp.executeVCSCommand`.
- Tests: Rust unit (patch, document) + integration (git backend, dispatch,
  errors) + Neovim integration via mini.test (real server + temp repo).
  `make test`.

## Roadmap

The roadmap, glossary, protocol reference and phased plan live in
[TASKS.md](./TASKS.md). Do not add roadmap notes here.

### Decisions already made

| Topic | Decision |
| --- | --- |
| Async runtime | `tokio` |
| Config format | TOML, with `include` of other TOML files (absolute or relative), default location from the OS config dir (XDG on Linux) |
| Neovim default profile | `vim` (evil-style) |
| Non-modal default profile | `standard` (Magit mnemonics, direct keys, no Emacs idioms) |
| Emacs idioms (`C-c C-c`, `M-n`, …) | Not kept. No `emacs` profile is shipped |
| Key notation | W3C `KeyboardEvent.key` names + VSCode-style `ctrl+shift+x` modifiers (TASKS.md §3.8) |
| Section folding | Server-owned, but triggered by the client's own fold keys wherever possible (§3.9) |
| Unimplemented Magit operations | Shown in menus, greyed out, with their default key and description (Neogit style) (§3.12) |
| jj | May be a semi-hard fork. Do not complicate the Git design or drop Git features for jj's sake |
| Diff/patch | Git computes diffs; we parse its output and build patches for `git apply` (this is what Magit does, §3.13) |
| Rendering | Server re-renders fully (as Magit does) and sends line-range edits, so clients stay stable (§3.3) |
| Tests | Mostly Rust, in-process, asserting buffer text and repo state. Neovim tests are kept but few (§8) |

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
- **No backward compatibility.** Rename, reshape and delete protocol methods,
  config keys and Lua APIs freely. Delete the old thing; never leave a shim or
  a deprecated alias behind.
- **Never address hunks by index.** Hunk identity is a content hash
  (`path` + hunk text), so it survives refreshes and concurrent edits. Known
  gap: the current code still uses indices in places — tracked by R-0.5; do not
  add new index-addressed APIs.
- **Never parse rendered text.** Structure must come from the server's tree.
  `src/document.rs` parses the rendered document and is legacy, slated for
  removal in R-1.1; do not extend it.
- **Never print raw JSON or Lua tables to the user.** A failed Git command is
  not a JSON-RPC error: return an `Effect::Message` built from a `UserError`
  (`summary` + optional `hint`), with `detail` available on request.
- Run `make test` before finishing; ensure no orphan `magit-lsp` processes.
