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

Data flow: client opens a view (`view/open`) → server runs `git` / mutates the
index on `view/invoke` → server re-renders the view's `Tree` and sends a
`view/render` notification (full text or line-range edits) → the client replaces
its buffer with the server's lines and acks with `view/rendered`. Reads and
mutations use the same custom request/response pattern (no
`workspace/applyEdit`).

The server owns the document. `src/tree.rs` defines nodes, Node IDs, semantic
tokens, symbols, links and the line → node map; `src/status_view.rs` turns a
`Status` into that tree. Folding ranges, document symbols and definition/link
targets are read from the tree, never from parsing rendered text.

### Custom protocol

| Method | Purpose |
| ------ | ------- |
| `view/open` | Create a view; returns a `ViewDescriptor` plus an initial full `view/render` |
| `view/close` | Drop a view from its session |
| `view/key` | Press one canonical key step; the server resolves chords per view and runs the command |
| `view/invoke` | Run a command (`common.refresh`, `common.*` folds, `status.stage`, `status.unstage`, `status.stage-all`, `status.unstage-all`) |
| `view/resync` | Return the current full text after a lost update |
| `view/setMode` | Switch a view between `interactive` and `text` mode |
| `view/render` (notification) | `{viewId, version, baseVersion?, text?, edits?, cursor?}` |
| `view/keymap` (notification) | `{viewId, scope, version, capture, bindings}` for the view's resolved keymap |
| `view/rendered` (notification) | Client ack `{viewId, version}` |
| `view/resize` (notification) | Client viewport `{viewId, cols, rows}` |
| `textDocument/documentSymbol` | Section → file → hunk outline; the client's jump navigation |
| `textDocument/definition` | Visit the file/new-side line under the cursor |
| `textDocument/documentLink` | File paths as `file://` links |
| `textDocument/foldingRange` | Fold ranges from the tree (sections and files) |

The positional requests above resolve through the view that owns the buffer URI
(`magit://status/<repo>`), against the last tree the client acknowledged.

`view/invoke` returns `{effects}`; the document itself follows as a
`view/render` notification. A failed `git` command is not a JSON-RPC error: it
comes back as an error-level `Effect::Message` with a readable summary (see
`src/errors.rs`). JSON-RPC errors are reserved for protocol faults.

Text mode (§3.14) is a view mode, not a client mode. `common.toggle-text-mode`
and `view/setMode` share the command path: entering text mode narrows the
pushed `view/keymap` to the toggle, refuses every other command with an info
message, queues refreshes (`Session::refresh` keeps a dirty flag) and makes
positional requests return `null`. Leaving it applies the queued refresh and
sends one full-text `view/render`, then the full keymap. Client edits are never
sent to the server.

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
- `src/tree.rs` — view tree: `Node`, `NodeId`, `NodeKind`, tokens, symbols,
  links and the line → node map. Node IDs derive from identity, never position.
  Also converts symbols to `DocumentSymbol`s, resolves `definition`/links and
  answers `common.copy-value` from each node's recorded copy text.
- `src/status_view.rs` — renders `Status` into a `Tree` (text and structure);
  `StatusView` implements the `View` trait.
- `src/menu.rs` — the menu (transient) model, column layout from the viewport,
  text rendering, the argument preview and `MenuView`.
- `src/semantic_tokens.rs` — the tree's tokens as LSP semantic tokens: the
  legend, byte → UTF-16 conversion and delta encoding.
- `src/view.rs` — `View` trait, `ViewDescriptor`, kinds, placement and viewport.
- `src/session.rs` — one `Session` per repository: `ViewStack`, versions, acks,
  cursor succession, text mode (queued refreshes, full-text exit), and the
  tiny R-1.2 command table.
- `src/keyhandler.rs` — per-view pending-chord state and key → command
  resolution (`view/key`).
- `src/keys.rs` — key notation: parse/format canonical key strings, plus the
  VSCode and Neovim translators (§3.8).
- `src/keymap.rs` — resolves bindings per scope and validates the merged keymap.
- `src/manifest.rs` — static manifest (profiles × scopes × bindings), the four
  emitters, `--check` coverage, and the VSCode workbench deny-list.
- `src/cli.rs` — hand-rolled command line: the stdio server by default,
  `keymap --format … [--check]`, `--version`/`--help`; one stdout writer.
- `src/config.rs` — TOML config (includes, layers, origin tracking, hot reload).
- `profiles/standard.toml`, `profiles/vim.toml` — shipped default bindings.
- `src/diff_text.rs` — common-prefix/suffix line-range edits for `view/render`.
- `src/protocol.rs` — custom JSON-RPC types (`Effect`, `ViewRender`, `view/*`
  params and responses).
- `src/transport.rs` — `Content-Length` framing over async stdio.
- `src/server.rs` — `tokio` stdio LSP loop, view/session registry, request
  dispatch.
- `lua/magit-lsp/init.lua` — Neovim client (open the view, apply
  `view/render`, apply `view/keymap` and forward keys as `view/key` steps,
  semantic-token links, server-owned folding, read-only buffer).
- `lua/magit-lsp/keys.lua` — canonical key notation (§3.8) ⇄ Neovim `lhs`
  translation, a pure function pair with unit tests.
- `lua/magit-lsp/keymap.lua` — atomic buffer-local keymap apply (tracked
  `applied[bufnr]`), fold-key mirroring and capture-key listing.
- `lua/magit-lsp/menu.lua` — per-menu scratch buffers and bottom float/split
  windows, render/keymap application, key forwarding and view effects.
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
  `Effect::Message`; every staging command reports effects this way.
- Neovim status buffer is read-only (IMM-4): `modifiable=false`,
  `readonly=true`, `undolevels=-1`, a single `set_lines` writer, `:w` blocked.
- Node-kind dispatch for stage/unstage in `src/dispatch.rs` (IMM-6).
- Test harness extracted to `tests/common/mod.rs` with `RepoFixture` and
  deterministic git env (IMM-7).
- Colour spike (IMM-8): `scripts/color_spike.lua` writes
  `build/test-output/status-<scheme>-<background>.html`, dumps the buffer's
  semantic tokens and writes `build/test-output/semantic-tokens.md` (group →
  link → resolved fg per scheme); findings in `TASKS.md` §8.4.
- LSP server (`src/server.rs`): request dispatch, view/session registry,
  capabilities.
- Client (`lua/magit-lsp/init.lua`): `vim.lsp.start`, `magit://status/<repo>`
  nofile buffer named from the server's `ViewDescriptor`, `view/render`
  application (full text or line edits, then `view/rendered` ack, `view/resync`
  on a lost update), semantic-token links, symbol-based navigation and
  `definition` for visit.
- Status view tree (R-1.1): `src/tree.rs` + `src/status_view.rs` produce text,
  tokens, symbols, links and a line → node map; `foldingRange` and
  `documentSymbol` come from the tree.
- Navigation (R-1.4): `textDocument/documentSymbol` exposes the section → file →
  hunk tree and replaces `vcs/targets`; `textDocument/definition` is the visit
  mechanism (a `+`/`-`/context line resolves to the new-side line), and
  `textDocument/documentLink` turns path ranges into `file://` links. Deleted
  files have no worktree file, so their definition result is empty. No
  `window/showDocument` request is needed for visit.
- Semantic tokens (R-1.3): `src/semantic_tokens.rs` turns the tree's tokens into
  `textDocument/semanticTokens/full` data (sorted, non-overlapping, UTF-16
  columns, delta-encoded) and advertises the legend. The client links
  `@lsp.type.<magit type>.magit-lsp` / `@lsp.mod.<modifier>.magit-lsp` to
  standard groups per TASKS.md §3.15 and re-applies them on `ColorScheme`;
  there is no client-side extmark highlighting any more.
- Sessions and views (R-1.2): `src/session.rs` owns per-repo `Session`s and a
  `ViewStack`; `view/open`, `view/close`, `view/invoke` (`common.refresh`,
  `status.stage`, `status.stage-all`, `status.unstage-all` and the fold
  commands), `view/resync`, `view/rendered` acks and `view/resize` are wired.
  `view/render` sends line-range edits when the client holds the base version,
  full text otherwise, and a successor cursor by node identity. Position-based
  requests use the last acked tree.
- Visibility (R-1.5): `Visibility` in `src/tree.rs` stores `expanded`/
  `collapsed` per node id, so it survives refreshes. `StatusView` reads a
  porcelain listing (`GitRepo::status_listing`) and calls `file_hunks` only for
  visible files, caching hunks per refresh; collapsed bodies are never diffed.
  `view/invoke` handles `common.toggle/show/hide-section` (with `recursive`),
  `show-all`, `hide-all`, `more-detail`/`less-detail` (levels 1–3) and
  `next-section`/`prev-section`. Folding it on a diff line acts on its file.
  `GitRepo::diff_invocation_count` backs the laziness test.
- R-1.6: the legacy `vcs/*` methods, the `vcs/requestTransient` stub, the
  `workspace/executeCommand` stub and the vestigial `src/vcs.rs` are deleted.
  Every former operation is a `view/invoke` command (`status.stage`,
  `status.unstage`, `status.stage-all`, `status.unstage-all`, plus section
  headers for stage-all/untracked/unstage-all and selections per IMM-2). The
  Lua client is thin: it opens the view, applies `view/render`, acks, resyncs on
  a lost update, and forwards keys; it holds no per-hunk/file/line logic.
- R-2.1: `src/commands.rs` is the one command registry (`Command{id, scope, doc, status, run}` with `Implemented`/`Planned{roadmap_id}`/`Unavailable{reason}`, `Registry::get/iter/by_scope`, built once). Every id named in TASKS.md §5.1–§5.5 is registered: the 14 previously-implemented commands plus all others as `Planned`. `Session::invoke` no longer matches on command names: it looks the command up, answers `Planned`/`Unavailable`/scope mismatches with an info `Effect::Message`, and hands execution to a sync `CommandCtx` handler (async `Ui` in R-3.1). `tests/commands.rs` enumerates the registry and checks Planned commands change nothing.
- R-2.2: `src/keys.rs` parses/formats the `[modifier+]*key` notation with round-trip tests; `profiles/standard.toml` and `profiles/vim.toml` (shipped via `include_str!`) transcribe §5. `src/config.rs` loads TOML with `include`/`include_optional` (relative/absolute/`~`/`$VAR`), detects cycles, merges tables key-by-key with array replacement, tracks `file:line` origins and layers defaults → profile → user config → `initializationOptions` → trusted `.magit-lsp.toml`. `src/keymap.rs` resolves per-scope bindings in the §3.9 order, detects conflicts and answers prefix lookups. `src/main.rs` takes `--config`; `src/server.rs` loads at `initialize`, handles `workspace/didChangeConfiguration` and polls loaded files (150 ms) with one `window/showMessage` per failure, keeping the previous config. Deviations from §5: `common.visit` is declared in the `status` scope (not `common`) so its `Enter` does not clash with `common.confirm`; vim binds `open.log` to `g l` rather than `l` so rule R4 (`h j k l` never rebound) holds.
- R-2.3: `view/key` and `view/keymap` plus `Effect::PassThrough` and capture mode. `src/keyhandler.rs` holds each view's pending chord and resolves one canonical key step via `Keymap::lookup` (`Prefix` keeps collecting, `Exact` runs the command, `None` clears and either passes the whole chord back or swallows it with an "Unbound key" hint when the view captures). `Session::key` shares `run_command` with `Session::invoke`, so the two entry points cannot diverge; cursor and selection come from the request and the node is resolved against the acked tree by identity, never by line index. A pending chord is discarded when the cursor moves and by `common.cancel` (now `Implemented`, which replies `PassThrough` only when a selection is present). `view/keymap` is pushed after `view/open` and re-pushed for every open view on a config hot reload, gated by the client's `capabilities.keymapDelivery` (`dynamic`/`static`/`forwardAll`); `keyModifiers` is stored for later profile warnings. `src/protocol.rs` adds the request/notification types and `KeymapDelivery`/`KeyModifiers`. Tests: `tests/keys_protocol.rs` (in process) and two real-binary stdio tests in `tests/server_async.rs`.
- R-2.4: the Neovim client is fully server-driven. `lua/magit-lsp/keys.lua` translates every §3.8 row both ways (`s`, `S`, `?`, `Enter`↔`<CR>`, `Escape`↔`<Esc>`, `Tab`/`shift+Tab`, `Delete`/`Backspace`, arrows, `ctrl+c`, `alt+ArrowDown`, `ctrl+Enter`, chords, `Space`, `Plus`, F-keys, `ctrl+shift+c`); `lua/magit-lsp/keymap.lua` keeps `applied[bufnr]` and applies each `view/keymap` in one synchronous function (delete exactly the old set, set the new one). Every mapped key sends `view/key` with the canonical step, the mode at press time, the 0-based cursor and the visual selection; a chord maps its full `lhs` and sends its steps in order. `passThrough` is replayed with `nvim_feedkeys(..., 'n', ...)` so the client's own map is not re-triggered. Capture mode adds every printable ASCII key. The hard-coded `keymaps()` set is gone. Folding is `foldmethod=manual`/`nofoldenable`; native fold keys and user mappings whose `rhs` mirrors a fold command are re-mapped to `common.*` (server bindings win), with `fold_keys` for Lua-function mappings. `setup{ profile }` is forwarded as `initializationOptions.profile` and the client declares `keymapDelivery = "dynamic"` + `keyModifiers`. `common.visit` is still `Planned`, so `visit_key` (default `<CR>`) is a documented temporary client-side exception that disappears once the server binding reports `implemented`. Tests: `tests/test_keymap.lua` (translator unit tests) and the keymap/keys/capture/fold cases in `tests/test_magit_lsp.lua`.
- R-2.5: `magit-lsp keymap --format json|vscode|neovim|markdown [--profile standard|vim] [--config <path>] [--check]` emits the static manifest from `src/manifest.rs`; without `--profile` it emits both shipped profiles. `--check` validates conflicts, unknown commands, unparsable keys and R5 coverage, then gates `--format` output. `src/keys.rs` gained `KeyStep`/`KeyChord` translators for VSCode (`S`→`shift+s`, `?`→`shift+/`, `alt+ArrowDown`→`alt+down`, chords space-separated) and Neovim (`Enter`→`<CR>`, `ctrl+Enter`→`<C-CR>`, `g r`→`gr`). `src/cli.rs` is the single stdout writer; the default no-arg path still runs the stdio server. `tests/manifest.rs` snapshots all four formats for both profiles under `tests/snapshots/` (`UPDATE_SNAPSHOTS=1` rewrites), checks the VSCode workbench deny-list for every profile and proves an injected `ctrl+p` is caught. Allow entries with reasons: `ctrl+t` (text-mode toggle), `ctrl+c` (cancel passthrough/copy), `ctrl+s` and `ctrl+r` (menu-scoped). `.github/workflows/ci.yml` runs `make test` plus the manifest snapshot test.
- R-2.6: text mode and `common.copy-value`. `OpenView` carries `mode` and a `dirty` flag; `common.toggle-text-mode` and `view/setMode` both run through `Session::run_command`. Entering text mode narrows `view/keymap` to the toggle (via `Keymap::only`), clears the pending chord and refuses every other command with "Text mode: press <key> to return" before any handler runs; `common.refresh` also sets `dirty`. `Session::refresh` returns `None` (queued) in text mode. Leaving text mode applies the queued refresh and sends one full-text `view/render` (no edits) with the identity successor cursor, then the full keymap. `ViewRender` and `ViewKeymapParams` carry `mode`; `ViewDescriptor.mode` follows the current mode. Positional requests return `null` in text mode (`Shared::is_text_uri`), while an unknown URI keeps its old empty result. `common.copy-value` returns `Effect::Clipboard{text}` built from the acked tree: file → path, hunk → patch hunk text (stored on the node when rendered), diff line → text without marker, selection → marked diff lines joined by `\n`, section → title; commit hashes are a documented R-5.1 hook. The Neovim client makes the buffer modifiable and restores the user's `undolevels` in text mode, drops undo history before replacing text on exit, skips visit/fold mirrors in text-mode keymaps, and handles `clipboard` with `setreg('+')`/`setreg('"')`. Tests: `tests/text_mode.rs` (in process + real stdio for positional null) and the ctrl+t / `y y` cases in `tests/test_magit_lsp.lua`; manifest snapshots were regenerated for the two `implemented` statuses.
- R-2.7: the menu framework (§3.11). `src/menu.rs` holds `Menu { id, title, groups, savedArgs }` with `Switch`/`Option`/`Action` items, readers, statuses, an `Args` snapshot, the argument preview and `MenuView`. Layout uses `viewport.cols`: `Arguments` is vertical, action groups are column blocks wrapped greedily (re-flowed on `view/resize`), and every item gets a stable `menu:<id>/item:<key>` node and `magit.menu.*` tokens (added to the semantic-token legend). `ViewRender.menu` carries the structured model alongside text. `ViewKind::Menu("menu.<name>")`, `Effect::{OpenView, PushView, PopView, FocusView}`, and `Session` push/pop/focus through the `ViewStack`; pushed views get unique `magit://menu.*` URIs and `{parent}:menu.<name>` ids. Menus are `capture = true`, scope `menu.<name>` (resolved through the shared `menu` keymap), placement `bottom`. `Session::key` routes menu views through `keyhandler::resolve_menu`: `-`/`=` then a letter, a bare letter runs an action, `Escape`/`ctrl+c`/`q` pop, `ctrl+s`/`ctrl+r` save/reset arguments, and anything else is swallowed with an "Unbound key" hint. `menu.save-args`/`menu.reset-args` are `Implemented` (in-memory per session; persistence is later config work); `common.cancel`/`common.quit` pop a menu. `open.commit` is `Implemented` and pushes a representative commit menu (switches `-a`/`-e`/`-n`, option `=A`, the full Magit action list as `Planned` R-4.1, and `?` pushing the dispatch menu via `open.dispatch`); `open.dispatch` now pushes a dispatch menu. The Neovim client (`lua/magit-lsp/menu.lua`) creates a scratch buffer per descriptor, shows it as a bottom float (`relative="editor"`, full width, height capped at 40%) or `botright {height}split` with `setup{ menu = { style = "split" } }`, applies renders/keymaps per buffer, forwards keys and handles the view effects; closing a menu window sends `view/close` and refocuses. Tests: `tests/menu.rs` (layout snapshots at 60/100/140, switches/options/actions, the preview, the model, the stack and token decoding) plus the float/split cases in `tests/test_magit_lsp.lua`; manifest snapshots regenerated for the new `implemented` statuses.
- Tests: Rust unit (patch) + integration (git backend, dispatch, errors, status
  rendering, views) + Neovim integration via mini.test (real server + temp
  repo). `make test`.

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
- **Added/removed lines had no colour.** The client linked `MagitDiffAdded` /
  `MagitDiffRemoved` to `diffAdded` / `diffRemoved`, which built-in schemes do
  not define. The semantic-token links now use `@diff.plus` / `@diff.minus`
  (→ `Added` / `Removed`).
- **Dotted semantic token types work as group names.** Neovim builds
  `@lsp.type.<token type>.<filetype>` and accepts dots in the type, so
  `magit.diff.added` becomes `@lsp.type.magit.diff.added.magit-lsp` with no
  name mangling (verified in Neovim 0.12.5). The status buffer's filetype is
  `magit-lsp`, which keeps those names predictable.
- **Modifier groups stack above type groups.** Semantic tokens apply
  `@lsp.mod.*` at a higher priority than `@lsp.type.*`; Neovim merges the
  attributes, so modifier links must only add attributes (e.g. `staged` →
  `Bold`) instead of replacing a type's foreground.

## Known limitations

- Binary files, mode-only changes and symlink type changes expose no hunks;
  stage/unstage them as a whole file. A hunk/line request for a binary file
  fails with Git's own explanation instead of a patch we built ourselves.
- Copies are not detected (`git diff` runs with `-M`, not `-C`), so a copy
  appears as a new file. Renames keep hunk/line staging.
- Paths with spaces/special characters may need patch-header quoting when a
  synthetic header is unavoidable.
- `stage_all` uses `git add -u` (tracked files only); untracked files must be
  staged individually.
- A chord is mapped as its full `lhs` (`g r` → `gr`), which is simpler than
  capturing the next key after a prefix, but Neovim waits `timeoutlen` after a
  prefix (`g`, `z`) before deciding. The server still owns the pending chord.
- The default `bottom` menu is a floating window (`menu.lua`); `setup({ menu = { style = "split" } })` uses a bottom split. `-`/`=` require a second letter, so a chord prefix is never ambiguous. Option prompts do not exist yet (R-3.1): setting an unset option reports the roadmap message, while clearing a set one works.
- Option values are programmatically presettable through `view/open` `args.saved_args` (or a menu's `saved_args`); there is no prompt UI yet.
- `common.visit` is `Planned`, so `visit_key` maps Enter client-side for now; it
  is removed automatically once the server's binding is `implemented`.

## Conventions

- **Comments** are welcome when they explain *why* a decision was made, or
  clarify an otherwise opaque section of code. Keep them short and specific;
  avoid restating the obvious. Do not number comments.
- **Docstrings**: every external (public) function and type should have a
  concise docstring (`///` in Rust, `---` LuaLS annotations in Lua) describing
  what it does, not how.
- The client displays `document.text` verbatim; never recompose view text in a
  client.
- **No backward compatibility.** Rename, reshape and delete protocol methods,
  config keys and Lua APIs freely. Delete the old thing; never leave a shim or
  a deprecated alias behind.
- **Never address hunks by index.** Hunk identity is a content hash
  (`path` + hunk text), so it survives refreshes and concurrent edits.
- **Never parse rendered text.** Structure must come from the server's tree
  (`src/tree.rs`); folding ranges, symbols, definitions and cursor context
  resolve against nodes, never against line shapes.
- **Never print raw JSON or Lua tables to the user.** A failed Git command is
  not a JSON-RPC error: return an `Effect::Message` built from a `UserError`
  (`summary` + optional `hint`), with `detail` available on request.
- Run `make test` before finishing; ensure no orphan `magit-lsp` processes.
