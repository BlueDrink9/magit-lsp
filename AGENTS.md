# Agent entry point

`magit-lsp` is a Rust LSP server and Neovim client that presents a Git
repository as a structured, server-owned document. The server owns state,
rendering, structure and actions. Clients are thin editor adapters: they display
server-rendered text verbatim, forward input and acknowledge the rendered
version they applied. Long-term goals: thin clients and a backend boundary that
permits other VCS implementations. The wire contract is LSP plus the project
methods and notifications in `src/protocol.rs`.

## Code map

Flow: `src/server.rs` finds or creates the repository's `Session` and opens a
view. The session resolves commands and captured keys (including menu and key
sequences). Actions refresh or mutate through the Git/backend boundary and
return effects (open a view, prompt, render). A view renders into a structured
`Tree`; the server sends text and structure, using edits only against a version
the client acknowledged. The client applies it and acknowledges; the session
records that ack as the base for the next incremental update.

- `src/tree.rs`: server-owned node hierarchy, text and navigation/display metadata.
- `src/view.rs`: the common view contract. `src/status_view.rs`: the status
  view tree, built from Git status and patch hunks.
- `src/session.rs`: open views, rendered trees, key/menu state, versions and
  client acks; refreshes, renders and diffs views.
- `src/commands.rs`: actions, context and effects. `src/keyhandler.rs`: key
  sequences. `src/menu.rs`: transient menus and arguments.
- `src/git.rs`: the Git boundary. `src/patch.rs`: patch/hunk operations. Keep
  VCS work behind these.
- `src/server.rs`: LSP/transport to sessions and notifications.
  `src/protocol.rs`: wire payloads. The Neovim adapter is `clients/neovim/lua/magit-lsp/`,
  a thin display/input client.

The list is a starting point, not exhaustive.

## Read the right source

- **Queue file:** [`TASKS.md`](./TASKS.md) is the sole numbered implementation
  and review queue and the canonical roadmap (agent instructions call it "the
  queue file"). Sections: `## Immediate`, ad hoc, review, bugfix, feature. The
  checkbox plus `Done: <commit>` is the single authoritative completion record;
  other documents link to it and never restate status.
- Decisions, glossary, correctness contracts and architecture (§§0–3):
  [`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md). Protocol reference (§4):
  [`docs/PROTOCOL.md`](./docs/PROTOCOL.md), with client behavior in
  [`docs/CLIENT_SPEC.md`](./docs/CLIENT_SPEC.md). Keymaps
  ([§5](./docs/KEYMAPS.md#5-default-keymaps-profiles)), views
  ([§6](./docs/VIEWS.md#6-view-catalog)), testing
  ([§8](./docs/TESTING.md#8-testing-strategy)) and open questions
  ([§9](./docs/OPEN_QUESTIONS.md#9-open-questions)) are in maintained docs.
  Read only relevant sections. Immutable historical sources are provenance, not
  proof of current behaviour. Do not duplicate design or progress history here.
- [`README.md`](./README.md): user-facing overview, features, requirements, test
  instructions. [`ui-test-plan.md`](./ui-test-plan.md): real-UI test rules and
  scenario syntax (§A.5).
- For task-specific code, start from the code map and follow the relevant
  TASKS section.

## Working conventions

- **No-vision policy (every agent, including subagents):** never use vision,
  inspect or render images, or invoke image/screenshot tools. Native UI verification
  stays real and interactive, but evidence and acceptance are text or structured
  state only (`.screen`, accessibility/DOM text, JSON, wire/API and Git state).
  Text snapshots are allowed. Never impose image-based acceptance.
- **Make only.** Run tests, builds, snapshot updates and UI scenarios only via
  the targets in "Verification". Never compose `cargo test`/`build`/`nextest`,
  `nvim`, `npm test` or `python3 scripts/ui run` command lines. If no target
  fits, stop and propose one.
  Run `python*` only when a ticket explicitly directs it.
- Keep changes scoped to the requested task; take on no unrelated backlog.
- **Tests.** Feature work gets Rust state tests and conformance scenarios.
  Native-client evidence is required only for new visible behavior (Neovim) or
  changed declared capabilities / `clients/vscode/` files (VSCode). A test is
  permanent only if it guards a regression with a stable oracle through existing
  infrastructure; one-off confirmation is a recorded check, not a new file.
  Before adding a test, scenario or helper, search for existing ones and extend
  the nearest with matching fixture and setup (a distinct behavior gets its
  own). A test must exercise the stated requirement directly, not an adjacent path.
  Tests should primarily test code rather than environment or setup, think carefully and judiciously about whether the test is necessary if it tests outside this boundary
- Executable code over ~10 lines lives in a maintained script under `scripts/`
  that an existing make target exercises; Markdown shows an invocation.
- Every external/public function and type needs a concise docstring (`///` in
  Rust; LuaLS `---` in Lua).
- No backward-compatibility shims: reshape or remove obsolete protocol methods,
  config keys and APIs, and delete unused declarations.
- **Server owns structure.** Never address hunks by index (identity is path and
  content). Never parse rendered text to recover structure: resolve navigation,
  folding and cursor context through the server tree (`src/tree.rs`). Clients
  display document text verbatim; do not recompose it.
- **Errors.** Avoid showing JSON or Lua tables to users, except behind dedicated debug or log views and flags. Git failures are
  user-facing `Effect::Message`s with a readable summary (optional hint/detail),
  not JSON-RPC errors. See
  [`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md#35-commands-and-effects) and
  `src/errors.rs`.
- **Boundaries.** Validate protocol input, user configuration and Git output at
  their boundaries, then trust the invariants: do not reparse canonical keys,
  invent fallback command statuses, reconstruct displayed selections from fresh
  diffs, or keep alternate production paths solely for tests. Keep recovery only
  where failure is genuinely external: lost render acks, editor buffer lifetime,
  Git failures, client disconnects.
- **Verified vs inferred.** Label only claims that gate a decision or acceptance
  item. VERIFIED needs file:line or command output; otherwise INFERRED. Research
  external behavior (Magit, other backends, the LSP spec) before building on it.
- **Skills** hold task-type guidance. Load every skill your ticket lists:
  `native-ui-verification`, `golden-and-snapshots`, `protocol-and-capabilities`,
  `retirement-sweep` (workers); `phase-gate`, `failure-triage` (orchestrator).

## Verification

### Generated references

Generate through Make targets, never by hand. Commit generated changes
separately from implementation, authored docs and queue bookkeeping, and never
regenerate just to silence a failing behavior test. Details:
[`docs/TESTING.md`](./docs/TESTING.md#documentation-maintenance).

### Make targets

Targets log to `build/test-logs/` and print a short digest (`ok:` or the failure
tail plus the log path, and may alert you to warnings. ). Use `V=1` sparingly, only when the digest is insufficient and
`LOG_TAIL=n` for more of a failure. If warnings were noted as present, only inspect the tail of the warning log

- `make test`: Rust, Neovim and VSCode suites. Narrower:
  `make test-rust FILTER="--test menu"`, `make test-file FILE=tests/<file>.lua`,
  `make vscode-test`.
- `make ui-unit`: real-UI harness tests. `make ui-file FILE=<scenario>.scn`: one
  scenario. `make ui-test`: all scenarios. `make ui-tour`: writes
  `build/ui/tour.md`.
- `make update-snapshots`: rewrites stale snapshots (see `golden-and-snapshots`).
- `make hygiene` and `make format-check`.
- Other targets (docs-check, plain-neovim, plain-presets, schema-check,
  registry-docs-check, vscode-e2e) are in the Makefile `.PHONY` lines; use only
  those.

Rules:

- Run the narrowest relevant target and rerun only failed cases while
  diagnosing. Run one build/test command at a time; never background them or
  start a build while another command is running.
- Only the orchestrator decides when to run full `make test`, `make hygiene`,
  `make ui-tour` and reviews (see `phase-gate`); workers do not.
- Local verification targets Linux only. Do not block Linux work on Windows or
  macOS hosts: record platform-specific risks, keep explicit Windows/macOS CI
  checks as follow-ups, keep code and tests portable, and claim cross-platform
  verification only after CI evidence exists.
- Ensure no orphan `magit-lsp` processes remain after native runs.
