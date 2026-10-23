# Agent entry point

`magit-lsp` is a Rust LSP server and Neovim client that presents a Git
repository as a structured, server-owned document. The server owns state,
rendering, structure and actions; clients display its text verbatim and forward
input. Long-term goals are thin clients and a backend boundary that permits
other VCS implementations.

## Read the right source

- [`TASKS.md`](./TASKS.md) is canonical for the roadmap, glossary, design and
  protocol (§§1–4), keymaps (§5), views (§6), phases/backlog (§7, §12), testing
  (§8), open questions (§9), and implementation status/deviations (§10).
  Read only the sections relevant to the task; §10 is historical context, not
  proof of current behaviour. Do not duplicate its detailed design or progress
  history here.
- [`README.md`](./README.md) is user-facing: overview, supported features,
  requirements and test instructions.
- [`ui-test-plan.md`](./ui-test-plan.md) defines real-UI test rules and scenario
  syntax (§A.5).
- For code, start at the implementation named by the relevant TASKS section:
  `src/tree.rs` (document structure), `src/status_view.rs` (status tree),
  `src/session.rs` / `src/server.rs` (views and protocol), `src/git.rs` /
  `src/patch.rs` / `src/dispatch.rs` (Git operations), and
  `lua/magit-lsp/` (Neovim client). These are pointers, not a complete inventory.

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
- User-visible changes are not done until a scripted real-UI scenario drives
  the real Neovim UI and its screen output is checked. Follow inspect → act →
  inspect: `scripts/ui start <fixture>`, inspect with `screen`/`state`, act with
  `scripts/ui keys <key>…`, use `scripts/ui wait` (polls idle; never sleep),
  inspect again, and always `scripts/ui stop`. See `ui-test-plan.md` §A and
  TASKS.md §8.8. Run/read the UI tour at the end of a phase.
