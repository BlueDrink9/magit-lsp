# Status view start-open configuration (T122)

Tickets: `build/tickets/status-view-start-open-config-p1.md` (configuration and
seeding) and `build/tickets/status-view-start-open-config-p2.md` (the streaming
batch). Bases: `e02f3c5` (Part 1) and `e1d8250` (Part 2 start). Verification is
Linux-only; no cross-platform claim is made without CI evidence.

## What the key does

`[sections] default_visibility` accepts exactly `auto` (default), `open` or
`collapsed` and seeds the **interactive deferred opening** of a status view:

- `auto` — product default, byte-identical to the previous behaviour: the first
  ready render is the closed section headers, then a background diff fetch
  installs the fully-expanded render only if it beats the open promise.
- `open` — the first ready render shows the expanded sections and their file
  rows; the hunks stream in from a background batch with no cutoff (below).
- `collapsed` — the first ready render is closed and no background fetch is
  armed.

Any other value is a configuration boundary error reported with file and line,
surfaced like other load errors through the `Message` notification. The enum is
named in `docs/ARCHITECTURE.md` §3.18 (`:760-761`).

`--open-promise-ms <ms>` (non-negative integer; absent = the documented
`OPEN_PROMISE` of 500 ms) injects the effective promise used by both gates in
`spawn_timed_hunks`. It exists so tests can deterministically discard (`0`) or
force (`30000`) an `auto` batch; it is not a user-facing tuning knob.

## Streaming contract for `open`

- `StatusView::timed_fetch` arms for every seeded-open Ready view regardless of
  `auto_expand` (the seed deliberately sets `auto_expand=false` so the install
  cannot rewrite the seeded state). Its `TimedFetch` is `unbounded`.
- `spawn_timed_hunks` bypasses **both** promise gates for an unbounded fetch:
  there is no deadline on the wait and no completion re-check. The batch
  completes and installs with one re-render, diffed against the acknowledged
  version like any other render (incremental edits when the base matches).
- While `awaiting_batch` is set, `render_tree` never synchronously fetches an
  uncached expanded file. This single gate covers the first ready render, the
  slow path and every refresh render, so no seeded-open render holds the session
  mutex and repository lock across the whole diff batch.
- `install_timed_hunks` in open mode installs on generation match alone and
  never rewrites visibility or level. A user collapse during streaming therefore
  wins: the post-install render respects it.
- Text mode keeps the install unpublished and unversioned (the existing bail)
  and renders it as a full replace when text mode ends.

## Refresh policy

- A command invocation of `common.refresh` (not a key binding) re-arms a
  no-cutoff batch at any distance from the opening: for a seeded-open view,
  streaming is the contract, not a short window. The generation bump discards
  the superseded batch's late install, and the refreshed render itself stays
  rows-only.
- Watcher, mutation and key-bound refreshes do **not** re-arm (cfa0e22's
  command-only policy). Their render is rows-only as well; `settle_open_refresh`
  then returns the view to on-demand rendering, so a user re-expansion of a
  section inline-fetches that one file through the normal lazy path.
- `auto` keeps its existing behaviour byte-identical in the fast and slow paths
  (closed first render, promise-bound batch, and the untouched post-fast-path
  inline refetch).

## Sync-path invariance

The key and the streaming gate govern only the interactive `view/open` status
opening. The synchronous paths are unchanged under every value:

- `StatusView::open_repo` (`src/status_view.rs:207`) keeps using
  `StatusView::loading` and installs fresh state directly, i.e. unknown nodes
  expanded, deepest level, hunks inline.
- The Plain bootstrap (`Session::open_with_section_line_limit`,
  `src/session.rs:1147`) and `create_view` (`src/session.rs:2892`) do not pass a
  seeded opening and therefore render exactly as before.

## No client change

Clients display server-rendered text verbatim and acknowledge the version they
applied; the new key changes only what the server renders. No declared client
capability changed and `clients/neovim/lua/magit-lsp/config.lua` is untouched.

## Evidence

Deterministic checks (all green at the commit recording this note):

- `make test-rust FILTER="--test status_render"` — the seeding matrix: `auto`
  reproduces the closed deferred render with a promise-bound batch (`open`'s
  seed publishes expanded file rows without inline diffs and its unbounded batch
  installs the synchronous fully-expanded oracle, `collapsed` renders closed
  with no fetch), and an install from a superseded generation is discarded while
  the refreshed opening re-arms its own batch.
- `make test-rust FILTER="--test config"` (Part 1) — `auto` is the default and
  an unknown value is a located `ConfigError`.
- `make test-rust FILTER="--test server_async"` — over the real server:
  - `status_open_slow_batch_streams_hunks_by_incremental_edit` holds the first
    diff in Git: the ready render shows expanded file rows with no `@@` before
    the gate is entered, and after release one follow-up render carries
    `baseVersion` + `edits` that reproduce the synchronous fully-expanded oracle.
  - `status_open_refresh_during_batch_rearms_rows_only_then_hunks` refreshes
    while the batch is held: the command answers without waiting on the diff,
    the refreshed render stays rows-only, and the renewed no-cutoff batch
    restores the hunks.
  - `status_open_user_collapse_during_batch_wins_over_the_install` collapses the
    staged section while the batch is held: the post-install render still hides
    that section and shows the unstaged hunks.
  - `status_open_config_open_streams_to_the_complete_buffer` serves the config
    with the default promise: the rows-first seed streams into the complete
    buffer; the same fixture under `auto` still publishes the closed render
    first. The existing `auto` refresh and slow-path tests are unchanged.
- `MAGIT_LSP_BIN=$PWD/scripts/open-promise-server.sh make ui-file
  FILE=tests/ui/scenarios/status-basics.scn` (Part 1) — green. The scenario
  waits for `+line3 CHANGED` before its fold walk, so it does not depend on the
  client going idle at the right moment.

## Residual risk in suite runs

`make ui-test` (and any other suite run not given `MAGIT_LSP_BIN`) keeps
the production 500 ms promise for `auto`. On a machine slow enough that the
first status fetch exceeds it, the batch is discarded and the scenario's wait
for `+line3 CHANGED` fails even though the server behaved as documented. Use
`MAGIT_LSP_BIN=$PWD/scripts/open-promise-server.sh` for the scenario;
wiring that wrapper into the suite run is out of scope.
