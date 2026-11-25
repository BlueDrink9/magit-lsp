# Status view start-open configuration (T122, Part 1)

Ticket: `build/tickets/status-view-start-open-config-p1.md` (Part 1 of 2).
Base: `e02f3c5`. Verification is Linux-only; no cross-platform claim is made
without CI evidence.

## What the key does

`[sections] default_visibility` now accepts exactly `auto` (default), `open` or
`collapsed` and seeds the **interactive deferred opening** of a status view:

- `auto` — product default, byte-identical to the previous behaviour: the first
  ready render is the closed section headers, then a background diff fetch
  installs the fully-expanded render only if it beats the open promise.
- `open` — the first ready render is the complete, synchronous fully-expanded
  render (no background fetch is armed).
- `collapsed` — the first ready render is closed and no background fetch is
  armed.

Any other value is a configuration boundary error reported with file and line,
surfaced like other load errors through the `Message` notification. The enum is
named in `docs/ARCHITECTURE.md` §3.18 (`:760-761`).

`--open-promise-ms <ms>` (non-negative integer; absent = the documented
`OPEN_PROMISE` of 500 ms) injects the effective promise used by both gates in
`spawn_timed_hunks`. It exists so tests can deterministically discard
(`0`) or force (`30000`) the batch; it is not a user-facing tuning knob in this
part.

## Sync-path invariance

The key governs only the interactive `view/open` status opening. The synchronous
paths are unchanged under every value:

- `StatusView::open_repo` (`src/status_view.rs:150`) keeps using
  `StatusView::loading`, i.e. unknown nodes expanded, deepest level, no fetch.
- The Plain bootstrap (`Session::open_with_section_line_limit`,
  `src/session.rs:1143`) and `create_view` (`src/session.rs:2878`) do not pass a
  seeded opening and therefore render exactly as before.

## Cost of `open` in Part 1

Seeding the expanded state makes the first ready render complete: `render_tree`
(`src/status_view.rs:408`) inline-fetches every expanded file's hunks
(`src/status_view.rs:461-473`) while the session/repository lock is held,
exactly like the synchronous path. This is accepted for Part 1; the no-cutoff
batch, seeded render gate, seeded-mode install and mode-aware re-arm are Part 2.
`auto`'s post-fast-path inline refetch is likewise untouched.

## No client change

Clients display server-rendered text verbatim and acknowledge the version they
applied; the new key changes only what the server renders. No declared client
capability changed and `clients/neovim/lua/magit-lsp/config.lua` is untouched.

## Evidence

Deterministic checks (all green at the commit recording this note):

- `make test-rust FILTER="--test status_render"` — seeding matrix: `auto`
  reproduces the closed deferred render, `open`'s first render equals the
  synchronous fully-expanded oracle with `timed_fetch() == None`, `collapsed`
  renders closed with no fetch.
- `make test-rust FILTER="--test config"` — `auto` is the default and an
  unknown value is a located `ConfigError`.
- `make test-rust FILTER="--test server_async"` — over the real server,
  `--config` with `default_visibility = "open"` publishes the sync render as the
  first ready render while the default stays closed; `--open-promise-ms 0`
  deterministically discards the batch (the text-mode `@@` assertion can never
  see hunks); `--open-promise-ms 30000` observes the expanded install.
- `MAGIT_LSP_TEST_BINARY=$PWD/scripts/open-promise-server.sh make ui-file
  FILE=tests/ui/scenarios/status-basics.scn` — green. The scenario now waits
  for `+line3 CHANGED` before its fold walk, so it does not depend on the client
  going idle at the right moment.

## Residual risk in suite runs

`make ui-test` (and any other suite run not given `MAGIT_LSP_TEST_BINARY`) keeps
the production 500 ms promise. On a machine slow enough that the first status
fetch exceeds it, the batch is discarded and the scenario's wait for
`+line3 CHANGED` fails even though the server behaved as documented. Use
`MAGIT_LSP_TEST_BINARY=$PWD/scripts/open-promise-server.sh` for the scenario;
wiring that wrapper into the suite run is out of scope for this part.
