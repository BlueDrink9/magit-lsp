# Agent entry point

`magit-lsp` is a Rust LSP server and Neovim client that presents a Git
repository as a structured, server-owned document. The server owns state,
rendering, structure and actions; clients display its text verbatim and forward
input. Long-term goals are thin clients and a backend boundary that permits
other VCS implementations.

## Architecture and code map

The server owns repository state, view structure, rendering and actions. A
client is a thin editor adapter: it displays server-rendered text verbatim,
forwards input and reports which rendered version it has applied. The wire
contract is LSP plus the project methods and notifications in `src/protocol.rs`.

The normal flow is:

1. Opening a document/view reaches `src/server.rs`, which finds or creates the
repository's `Session` and asks it to open a view.
2. Commands and captured keys (including menu/key sequences) are resolved by
the session using `src/commands.rs`, `src/keyhandler.rs` and `src/menu.rs`.
3. An action refreshes or mutates through the Git/backend boundary; resulting
effects may open views, prompt the user, or request a render.
4. A view renders into a structured `Tree`; the server sends text and
structure, using edits only when the client acknowledged the base version.
5. The client applies the notification and acknowledges its view/version. The
session records that ack as the base for a later incremental update.

Start in these components:

- `src/tree.rs` defines the server-owned node hierarchy, text and associated
  navigation/display metadata. Never recover structure by parsing text.
- `src/status_view.rs` builds the status view tree from Git status and patch
  hunks; `src/view.rs` defines the common server-owned view contract.
- `src/session.rs` owns open views, rendered trees, key/menu state, versions
  and client acknowledgements; it refreshes, renders and diffs views.
- `src/commands.rs` describes actions and their context/effects;
  `src/keyhandler.rs` resolves key sequences and `src/menu.rs` handles
  transient menus and arguments.
- `src/git.rs` is the Git repository boundary; `src/patch.rs` models and
  applies patch/hunk operations. Keep VCS work behind these boundaries.
- `src/server.rs` connects LSP/transport requests to sessions and notifications;
  `src/protocol.rs` defines wire payloads. The Neovim adapter lives in
  `clients/neovim/lua/magit-lsp/` and should remain a thin display/input client.

## Read the right source

- **Queue file:** `TASKS.md` is the sole numbered implementation and
  review queue (agent instructions call it "the queue file"). It holds
  `## Immediate`, ad hoc, review, bugfix and feature sections. The checkbox plus
  `Done: <commit>` is the single authoritative completion record; other
  documents link to it and never restate status.
- [`TASKS.md`](./TASKS.md) is canonical for the roadmap and queue. Decisions,
  glossary, correctness contracts and architecture design (§§0–3) live in
  [`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md). Protocol reference (§4)
  lives in [`docs/PROTOCOL.md`](./docs/PROTOCOL.md),
  with detailed client behavior in [`docs/CLIENT_SPEC.md`](./docs/CLIENT_SPEC.md).
  Keymaps ([§5](./docs/KEYMAPS.md#5-default-keymaps-profiles)), views
  ([§6](./docs/VIEWS.md#6-view-catalog)), testing
  ([§8](./docs/TESTING.md#8-testing-strategy)) and open questions
  ([§9](./docs/OPEN_QUESTIONS.md#9-open-questions)) live in maintained docs.
  Read only relevant sections; immutable historical sources are
  provenance, not proof of current behaviour. Do not duplicate detailed design
  or progress history here.
- [`README.md`](./README.md) is user-facing: overview, supported features,
  requirements and test instructions.
- [`ui-test-plan.md`](./ui-test-plan.md) defines real-UI test rules and scenario
  syntax (§A.5).
- For task-specific code, start with the component above and follow the
  relevant TASKS section. The component list is a starting point, not exhaustive.

## Working conventions

- **No-vision policy (mandatory for every agent, including delegated workers and
  subagents):** never use vision, inspect or render images, or invoke
  image/screenshot tools. Do not capture screenshots or add image-snapshot
  tests/capture paths. Native UI verification stays real and interactive, but
  evidence and acceptance are text/structured state only (`.screen`,
  accessibility/DOM text, JSON, wire/API and Git state). Text snapshots are
  allowed. Never impose image-based acceptance.
- **Test only through the Makefile.** Run tests, builds, snapshot updates and UI
  scenarios only via the targets in "Verification". Do not compose your own
  `cargo test`/`cargo build`/`cargo nextest`, `nvim`, `npm test` or
  `python3 scripts/ui run` command lines. If no target fits, stop and say so
  (propose a target) instead of improvising. Never run formatters (pre-commit
  hooks format) and never use `--no-verify`.
- Keep changes scoped to the requested task. Do not take on unrelated unchecked
  roadmap or review backlog items.
- Feature work gets Rust state tests and conformance scenarios. Native-client
  evidence is required only for new visible behavior (Neovim) or changed declared
  capabilities / `clients/vscode/` files (VSCode). A test is permanent only if it
  guards a regression with a stable oracle through existing infrastructure;
  one-off confirmation is a recorded check, not a new test file.
- Executable code longer than ~10 lines lives in a maintained script under
  `scripts/` that an existing make target exercises. Markdown shows an
  invocation, not a program to extract. Historical records are exempt but must be
  labeled with version/date.
- Every external/public function and type needs a concise docstring (`///` in
  Rust; LuaLS `---` annotation in Lua) describing what it does.
- No backward-compatibility shims: reshape or remove obsolete protocol methods,
  config keys and APIs, and delete unused declarations instead of deprecating them.
- Never address hunks by index. Hunk identity is based on path and content.
- Never parse rendered text to recover structure. Resolve navigation, folding
  and cursor context through the server tree (`src/tree.rs`).
- Clients display server-rendered document text verbatim; do not recompose it.
- Never show raw JSON or Lua tables to users. Git failures are user-facing
  `Effect::Message`s with a readable summary (and optional hint/detail), not
  JSON-RPC errors. See [`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md#35-commands-and-effects) and `src/errors.rs`.
- Validate protocol input, user configuration and Git output at their boundaries.
  Once internal values are validated, trust their invariants: do not reparse
  canonical keys, invent fallback command statuses, reconstruct displayed
  selections from fresh diffs, or maintain alternate production paths solely for
  tests. Keep recovery where failure is genuinely external: lost render
  acknowledgements, editor buffer lifetime, Git failures and client disconnects.
- **Verified vs inferred.** Label only claims that gate a decision or an acceptance item. VERIFIED needs file:line or command output; without it the claim is INFERRED. Research external behavior (Magit, other backends, the LSP spec) before building on it.
- **Reuse before adding.** Before adding a test, scenario or helper, search for existing ones and extend the nearest where fixture and setup match; a distinct behavior gets its own. A test must exercise the stated requirement directly, not an adjacent path.
- **Skills** hold task-type guidance. Load every skill your ticket lists:
  `native-ui-verification`, `golden-and-snapshots`, `protocol-and-capabilities`,
  `retirement-sweep` (workers); `phase-gate`, `failure-triage` (orchestrator).

## Verification

### Generated references

- Keep generated references in standalone files linked from authored documentation.
  Generate them through Make targets; do not hand-edit them.
- Ordinary tests cover behavior, protocol semantics and generator/checker logic
  using temporary fixtures. Committed-artifact freshness and Markdown links belong
  in explicit documentation checks and the dedicated documentation CI job, not
  prose assertions in behavior tests.
- Commit generated-reference changes separately from implementation, authored
  documentation and queue bookkeeping. Explain why regeneration was necessary
  in the commit message, naming the source or contract change and generation
  command. Inspect the generated diff; never refresh output merely to silence a
  failing behavior test.

### Make targets

Targets log to `build/test-logs/` and print a short digest (`ok:` or the failure
tail plus the log path). Use `V=1` only when the digest is insufficient and
`LOG_TAIL=n` for more of a failure.

- `make test`: Rust, Neovim and VSCode suites. Narrower: `make test-rust
  FILTER="--test menu"`, `make test-file FILE=tests/<file>.lua`,
  `make vscode-test`.
- `make ui-unit`: real-UI harness tests. `make ui-file FILE=<scenario>.scn`: one
  scenario. `make ui-test`: all scenarios. `make ui-tour`: writes
  `build/ui/tour.md`.
- `make update-snapshots`: rewrites stale snapshots (see `golden-and-snapshots`).
- `make hygiene` and `make format-check`.
- Other targets (docs-check, plain-neovim, plain-presets, schema-check, registry-docs-check, vscode-e2e) are in the Makefile `.PHONY` lines; use only those.

Rules:

- Run the narrowest relevant target and rerun only failed cases while
  diagnosing. Run one build/test command at a time; never background them and
  never start a build while another command is running.
- The orchestrator decides when to run full `make test`, `make hygiene`,
  `make ui-tour` and reviews (see `phase-gate`); workers do not.
- Local execution and verification target Linux only. Do not block Linux work
  because Windows or macOS hosts are unavailable; record platform-specific risks
  and retain explicit Windows/macOS CI checks as follow-ups. Keep code, tests and
  behavior portable, and claim cross-platform verification only after the
  corresponding CI evidence exists.
- Ensure no orphan `magit-lsp` processes remain after native runs.
