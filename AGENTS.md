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
  `lua/magit-lsp/` and should remain a thin display/input client.

## Read the right source

- [`TASKS.md`](./TASKS.md) is canonical for the roadmap, glossary, design and
  protocol (§§1–4), keymaps (§5), views (§6), phases (§7), review backlog (§12), testing
  (§8), open questions (§9), and implementation status/deviations (§10).
  Read only the sections relevant to the task; §10 is historical context, not
  proof of current behaviour. Do not duplicate its detailed design or progress
  history here.
- [`README.md`](./README.md) is user-facing: overview, supported features,
  requirements and test instructions.
- [`ui-test-plan.md`](./ui-test-plan.md) defines real-UI test rules and scenario
  syntax (§A.5).
- For task-specific code, start with the component above and follow the
  relevant TASKS section. The component list is a starting point, not exhaustive.

## Working conventions

- Keep changes scoped to the requested task. Do not take on unrelated unchecked
  roadmap or review backlog items.
- Every external/public function and type needs a concise docstring (`///` in
  Rust; LuaLS `---` annotation in Lua) describing what it does.
- No backward-compatibility shims: reshape or remove obsolete protocol methods,
  config keys and APIs instead.
- Never address hunks by index. Hunk identity is based on path and content, not
  position, so it survives refreshes and concurrent edits.
- Never parse rendered text to recover structure. Resolve navigation, folding
  and cursor context through the server tree (`src/tree.rs`).
- Clients display server-rendered document text verbatim; do not recompose it.
- Never show raw JSON or Lua tables to users. Git failures are user-facing
  `Effect::Message`s with a readable summary (and optional hint/detail), not
  JSON-RPC errors. See TASKS.md IMM-3, §3.5 and `src/errors.rs`.
- Run `make test` before finishing and ensure no orphan `magit-lsp` processes.

## Verification

- `make test` runs Rust and Neovim integration tests.
- `make ui-unit` tests the real-UI harness; `make ui-test` runs scripted real-UI
  scenarios; `make ui-tour` writes `build/ui/tour.md`.
- Snapshot tests use `expect_file!` with one update command:
  `UPDATE_EXPECT=1 cargo test --test manifest --test parity --test menu --test
  menus` rewrites stale snapshots in `tests/snapshots/` (six files: manifest
  and keybindings JSON per profile, the commit menu at 60/140 columns) and the
  README parity block in place, preserving surrounding prose.
- User-visible changes are not done until a scripted real-UI scenario drives
  the real Neovim UI and its screen output is checked. Follow inspect → act →
  inspect: `scripts/ui start <fixture>`, inspect with `screen`/`state`, act with
  `scripts/ui keys <key>…`, use `scripts/ui wait` (polls idle; never sleep),
  inspect again, and always `scripts/ui stop`. See `ui-test-plan.md` §A and
  TASKS.md §8.8. Run/read the UI tour at the end of a phase.
