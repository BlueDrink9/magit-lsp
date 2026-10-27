# Completed roadmap history (archived 2026-10-28)

Historical evidence removed from `TASKS.md` at baseline commit `49efd16`.
This archive records past status and verification, **not current behavior, active
work, or scheduling authority**. Consult source/tests for current behavior and
`REVIEW_REPORT.md` for review evidence. Root-relative references below are
repository paths.

## Completed phases

### Phase 0 — Groundwork

- [x] R-0.1…R-0.7: immediate fixes (§2), porcelain-v2 parsing (including
  rename/unusual paths), separate Untracked section, patch edge-case fixtures,
  content-based hunk identity with stale-action rejection, tracing off the LSP
  pipe and Tokio stdio. Patch/backend boundaries remain §§3.13/3.19.

### Phase 1 — Server-owned document and sessions

- [x] R-1.1…R-1.6: server tree/rendering, sessions/view stack, full/edit
  delivery with ack/resync, Node-ID cursor continuity, tokens, symbols/visit
  and lazy server-owned visibility (§§3.1–3.4, 3.15; §4). Removed old `vcs/*`
  methods and text parsing. Visit uses `definition`, not `showDocument`.

### Phase 2 — Commands, keymaps, config, text mode, menu framework, second client

- [x] R-2.1…R-2.9: command registry, TOML keymaps/config/reload, dynamic and
  static input delivery, Neovim atomic mappings/visual capture/fold mirroring,
  manifest CLI/checks, text mode/copy, viewport-driven menus and Planned
  dispatch/parity inventory. VSCode is the thin second client (static bindings,
  context keys, QuickPick menus, fold mirroring). Canonical requirements remain
  §§3.5, 3.8–3.14, 3.18 and §5.

### Phase 3 — Effects, prompts, runner, process log, editor bridge

- [x] R-3.1…R-3.4: prompts/`ScriptedUi`, runner queue/progress/cancel/ring log,
  process-log view and editor-bridge core with a fake-client commit test.
  Historical core milestone only; it did not close R-3.4b or Phase 5.

### Phase 3U — Real-UI harness and UI bug fixes

- [x] R-UI-1…R-UI-6: tmux/ANSI/fixture/scenario harness and debug API; queued
  pre-open renders; help/menu-stack fixes; read-only W10 fix; action/flag/chord
  rendering; commit feedback/progress cleanup; tour and CI. Historical final
  result: `make test`, `make ui-unit`, `make ui-test` green, 13 PASS / 0 XFAIL /
  0 FAIL across both profiles/sizes. `make ui-tour` wrote `build/ui/tour.md`.
  Bug 5 did not reproduce (11–16 ms); latency remained a regression guard, not
  a diagnosed slowdown. See `ui-test-plan.md` for harness contracts.
- [x] R-UI-7 Status fills the window by default, with client placement option;
  visit works without another window. README and scenarios covered replacement
  and split placement; historical UI acceptance passed both profiles/sizes.
- [x] R-VS-1 VSCode build/test/documentation review; historical completion
  evidence and resolution details are recorded at baseline `49efd16`,
  `TASKS.md` lines 2278–2283.

### Phase 3N — Neovim client debt

- [x] R-NV-1…R-NV-4: async request queue; unified view manager; isolated debug
  instrumentation; split structure/defaults, binary lookup, server-owned visit
  and token highlight suggestions. Historical UI/test gates passed; see
  `REVIEW_REPORT.md` for review evidence.

### Phase 3.4b — Editor bridge clients and askpass

- [x] Neovim/VSCode `editDocument`, `--askpass`, and `commit-message-editor`.
  Historical evidence: real-file editor flow, typed commit, cancel, client
  disconnect and secret-never-logged tests in `tests/editor_bridge.rs`.

### Phase 3 continued — file watcher, discard, error mapping

- [x] R-3.5 file watcher auto-refresh and `index.lock` handling. UI scenario
  `auto-refresh` historically passed both profiles and sizes.
- [x] R-3.6 discard for files, hunks and selections with destructive
  confirmation; declined confirmation preserves changes. Historical
  `discard-confirm` UI scenario passed both profiles/sizes.
- [x] R-3.7 readable `UserError` mappings; missing-path `git add` regression and
  `git-error-readable` UI scenario historically passed. See §2 IMM-3 for the
  canonical error contract.

## Phase 3E — Managed commit drafts and cancellable Git runs

Decision dated 2026-10-27. This archive is historical evidence, not current
behavior. Earlier R-EDIT-1/2 refinements, including failed attempts and
superseded evidence, remain at baseline `49efd16`, `TASKS.md` lines 1640–1662.
R-EDIT-2 has Linux-only acceptance; R-EDIT-6 owns A11/cutover. macOS R-EDIT-9
and Windows R-EDIT-7 remain separate and unclaimed. Status at archive time:
R-EDIT-3 and R-EDIT-8 were still open; both have since completed in the main
roadmap. R-EDIT-4/5/6/7/9 are still open there — the main queue is authoritative
for current status.

- [x] R-EDIT-1 — Specify lifecycle and process feasibility. Design only; no
  implementation/platform execution claimed. Contract is canonical in §3.10;
  native acceptance maps to R-EDIT-4/5/A11 and process adversarial tests to
  R-EDIT-2/A11.
- [x] R-EDIT-2 — Own runs and safely cancel Git children. **Linux acceptance
  only.** Full numbered refinement history, including two failed native
  attempts, supervised correction, final UI evidence and the Linux acceptance
  audit is preserved in the original baseline at `TASKS.md` commit `49efd16`
  lines 1640–1662. Linux gate passed 2026-10-28; macOS and Windows are not
  claimed. Limits include crash/native recovery, setsid escapees, same-user or
  nested hooks, and unreapable-child ownership transfer (§3.10).
- [x] R-EDIT-3 — Versioned draft and validated writeback. **Server-only; native
  adapters/cutover are not integrated.** On 2026-10-28 production integration
  added owner-bound opaque drafts before Git, private one-shot helper
  credentials, validated target/kind, managed URI/template/version, ordered
  full-sync updates and typed save/finish barriers. Exact checkpoint writes
  precede helper success; save does not release Git, didClose detaches, and
  common.confirm cannot bypass validation. Failure retains helper/snapshots;
  terminal cleanup rejects late input. Former realFile commit tests now use
  this production protocol, without opt-ins/fallbacks. Scoped evidence:
  `cargo test --test editor_drafts --test editor_bridge --test server_async`
  (19 draft, 20 bridge, 21 async tests) and `cargo test --lib
  server::commit_tests` (1 test); logs at `/tmp/opencode/r-edit-3-verification.log`.
  Native R-EDIT-4/5, R-EDIT-6 cutover, R-EDIT-8 history (completed since this
  record), macOS R-EDIT-9 and Windows R-EDIT-7 remain open/unclaimed. Askpass/status stdio paths passed the
  scoped gate; A29/text-mode were unchanged and not newly verified.

## Phase 4 — Read views

- [x] R-5.1 Log (graph, decorations, filters, lazy loading) and reusable select
  mode. Historical `make test`, `make ui-unit`, `make ui-test`, `make ui-tour`
  and whitespace verification passed; `log-view` and `log-select` evidence was
  inspected across profiles/sizes. Substantive R-5.1 completion history and
  acceptance details are preserved in the original baseline at `TASKS.md`
  commit `49efd16` lines 1712–1737. Review findings are in `REVIEW_REPORT.md`.

## Historical review evidence

### §11 completed client and harness reviews

Completed Neovim, VSCode and UI-harness review history was present in §§11.1–11.3
at baseline `49efd16`; it is retained verbatim as historical evidence in that
baseline (Neovim lines 2233–2277, VSCode 2278–2283, harness 2284–2297). It is
not an active task list. The VSCode resolution/evidence history formerly at
§12 lines 2522–2543 is likewise retained in the baseline. Current actionable
review ownership/status is summarized in `REVIEW_REPORT.md` and the compact
main-roadmap summary.

### §12 checked review/test tasks

All checked R-5.1 review and test-review task descriptions, constraints,
verification records, negative controls and evidence were removed from the
main roadmap and remain available at baseline `49efd16`, `TASKS.md` lines
2300–2840. This includes the long completed test-suite review history. The
main roadmap keeps open review work in its §7 Review group (the raw-source-yank
ticket as `R-COPY-SOURCE`, plus `R-NAV`) and the explicit BUG-READ-1 deferral;
none of these is completed by this archive. `REVIEW_REPORT.md`
contains the actual A12 checked entry and review evidence.
