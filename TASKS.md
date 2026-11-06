# magit-lsp — Roadmap

Last review: ebf9ed1 (Phase 2 end gate; runtime review of 2e29535..ca67ce3 plus review fixes, full `make test` green at this sha)
Phase 2 end gate: T007 is complete, so this is a named phase end. `git diff --shortstat 2e29535..ca67ce3` covers Plain capability/action gates, the echo watchdog, commit-editor delivery, registry-backed draft rows, draft Confirm/Abort, shared editor models and the maintained native runner: 38 paths, 3075 insertions, 1889 deletions, including wire/protocol and lifecycle changes. Serial gate results at `5da5d86`: `make test` ok (build, rust, lua, vscode, keybindings; no orphan server), `make hygiene` exit 0 with `cargo machete`, `knip` and `luacheck` absent on this host so those linters did not run, `make ui-tour` ok. `make ui-test` reports 44 PASS / 5 FAIL; the same five scenarios (`common-navigation`, `hunk-body-fold`, `log-select`, `log-view`, `process-log`) fail identically in a clean extraction of reviewed baseline `2e29535`, so they are pre-existing UI coverage gaps, not Phase 2 regressions, and are owned by a Bugfix entry. The gate review filed one P1 (Plain draft-sync failures leaked rich `view/effects`) and one P2 (undocumented deadlock-preventing refresh thread); both are fixed and ticked, no P0 was raised, and review fixes do not re-trigger a review. `make test` is green again at `ebf9ed1`. Phase 2 is complete and Phase 3 is not started. Completed-ticket compression is left to the concurrent documentation workstream, which owns TASKS.md history reduction.
Documentation cleanup gate: substantial reference relocation and history reduction warrant a bounded preservation review. Runtime implementation changes belong to a concurrent workstream; this documentation gate uses focused documentation checks, not a redundant runtime/native gate.

Resolved checkpoint detail: immutable `d9ea816:TASKS.md`; not current instructions. `Source-only` labels mark provenance, not implementation commits. Scripted model checks are not real-server conformance; Linux native evidence does not establish Windows/macOS behavior, which remain CI follow-ups. Hygiene remains report-only; findings have owners/reasons as recorded in the hygiene allowlist. T007.1n had a timeout before a passing rerun; retain this transient timeout/rerun risk. T007 is complete through its audited children; the Phase 2 end gate is in progress and Phase 3 has not started.

## Contents

0. [Decisions already made](docs/ARCHITECTURE.md#0-decisions-already-made)
1. [Glossary](docs/ARCHITECTURE.md#1-glossary)
2. [Completed correctness contracts](docs/ARCHITECTURE.md#2-completed-correctness-contracts)
3. [Design decisions](docs/ARCHITECTURE.md#3-design-decisions-expanded)
4. [Protocol reference](docs/PROTOCOL.md#4-protocol-reference)
5. [Default keymaps (profiles)](docs/KEYMAPS.md#5-default-keymaps-profiles)
6. [View catalog](docs/VIEWS.md#6-view-catalog)
7. Numbered implementation queue (this file; [scope archive](./TASKS_PART_SEVEN.md))
8. [Testing strategy](docs/TESTING.md#8-testing-strategy)
9. [Open questions](docs/OPEN_QUESTIONS.md#9-open-questions)

---

## Ad hoc

- [x] **T008.1 — Normalize prompt negotiation for Plain and Rich.** Done: `6957647`. Plain never receives native `view/prompt`; §3.6 records the prompt presentation principle; wire `prompts` semantics clarified. T008 clause 1.
  - Verified: `make test-rust FILTER="--test plain_core --test prompts"` @ `6957647` → ok: rust; `make docs-check` @ `6957647` → ok: docs-check. Plain denies every native prompt kind including `secret`, Unnegotiated stays permissive and declared Rich keeps its existing kind/secret matrix; `custom_ui` maps Plain/Rich/pre-initialize to the three negotiated states.
- [x] **T008.2 — Add tracked Field nodes and render prompt documents.** Done: `5cde72b`. `NodeKind::Field` is a one-line value node; `PromptDocument` validates prompt shape and renders stable rows/fields for all four kinds with a documented 4096-byte single-line field limit.
  - Verified: `make test-rust FILTER="--test plain_prompt --test prompts"` @ `5cde72b` → ok: rust. `tests/plain_prompt.rs` covers exact lines/nodes for confirm, text, choice and multiChoice, secret refusal, empty-item rows, single-line field/value equality and the size limit; `field_kind_serializes_as_field` proves `NodeKind::Field` serializes as `"field"`.
- [x] **T008.3 — Ingest document-prompt field edits.** Done: `fbb1aa3`. `PromptDocument::reconcile` adopts only field-line values located from the authoritative rendered tree; non-field edits, inserted/deleted lines and malformed field nodes set `repaired` and adopt nothing. T008 clauses 2, 5.
  - Verified: `make test-rust FILTER="--test plain_prompt"` @ `fbb1aa3` → ok: rust. Covers field-only adoption, title/row repair, inserted/deleted lines, empty value, CRLF with final terminator, over-limit and stray-CR refusal with the old value kept, and renders after adoption. `reconcile` reads field positions from `NodeKind::Field` nodes only; no public API parses column positions from text.
- [x] **T008.4a — Add the Prompt view kind and wrapper.** `ViewKind::Prompt` wire name and `PromptView` over `PromptDocument`; generic view creation rejects it. T008 clause 5.
- [x] **T008.4b — Open and finish document prompts in the session.** Done: `ec5d90c`. Push above the origin, resume the staged continuation, restore focus and stack depth. T008 clause 5.
  - Verified: `make test-rust FILTER="--test prompts"` @ `85821b5` → ok: rust. `tests/prompts.rs` covers one `PromptView` push above the origin with `Placement::Main` and a full initial render, the double-open error, finish popping `PromptView` then focusing the origin while restoring depth and applying the `--author` option plus the updated menu preview, cancel leaving the option unset, and mismatched/unknown ids leaving stack and pending prompt unchanged.
- [x] **T008.4c — Hold refreshes and clean up prompt views.** Done: `5e4dfb5`. Defer origin refreshes while a prompt is active; closing the prompt or its origin clears the continuation. T008 clause 5.
  - Verified: `make test-rust FILTER="--test prompts"` @ `5e4dfb5` → ok: rust. `tests/prompts.rs` covers a refresh during a document prompt being held for the addressed view, the held dirty origin rendering once when the prompt finishes, closing the prompt view clearing the continuation, and closing the origin closing its prompt.
- [x] **T008.5a1 — Decide prompt delivery and gate session prompts.** Done: `0cab12f`. Shared `PromptDelivery` decision; session rejects only non-native secret prompts with a readable message. T008 clauses 1, 5.
  - Verified: `make test-rust FILTER="--test plain_prompt --test prompts"` @ `0cab12f` → ok: rust. `tests/plain_prompt.rs` covers `delivery` routing confirm/text/choice/multiChoice by native declaration and refusing secret without it; `tests/prompts.rs` covers a client with no prompt declarations staging the author prompt instead of refusing.
- [x] **T008.5a2 — Route prompts through the document view in the server.** Done: `1396871`. Plain and undeclared Rich kinds open a prompt document without blocking; native prompts unchanged; secret refusal still readable. Converted native `server_async` tests keep their assertions. T008 clauses 1, 5.
  - Verified: `make test-rust FILTER="--test server_async --test plain_core"` @ `1396871` → ok: rust. `rich_missing_confirm_opens_a_document_prompt_without_mutation` proves a Rich client declaring `editDocument` but not `confirm` gets a `pushView` of kind `prompt`, no `view/prompt` notification, no commit/index mutation, and the pending-prompt guard refusing the next command.
- [x] **T008.5a3 — Prove a Plain prompt opens and renders a document.** Done: `1ffc72f`. Initialize Plain, stage a menu-option prompt, and assert the prompt view push/render with no `view/prompt` and a pending-prompt guard. T008 clauses 1, 5.
  - Verified: `make test-rust FILTER="--test server_async"` @ `1ffc72f` → ok: rust. `plain_menu_option_prompt_opens_a_document_without_a_native_request` covers a Plain client staging the commit menu-option author prompt: a `pushView` of kind `prompt` with `magit://prompt/<id>`, its `view/render` at version 1, no `view/prompt` notification, and the pending-prompt guard refusing the next command.
- [x] **T008.5b1 — Answer confirm and text prompts in the session.** Done: `26ebd80`. Reconcile field text, map rows to answers, re-render on repair. T008 clauses 1–2, 5.
  - Verified: `make test-rust FILTER="--test prompts --test plain_prompt"` @ `26ebd80` → ok: rust.
- [x] **T008.5b2a — Accept prompt field edits in the Plain document.** Done: `3e95ee6`. Reconcile the prompt view on `didChange`, adopt field text without a repair edit, and retain prompt action snapshots. T008 clauses 1–2, 5.
  - Verified: `make test-rust FILTER="--test prompts --test plain_prompt --test plain_sync"` @ `3e95ee6` → ok: rust.
- [x] **T008.5b2b — Open Plain menu option rows as prompts.** Done: `ebfa0fc`. Offer/execute `menu.run-option`; an implemented Option row prepares the prompt and pushes the prompt document. T008 clauses 5, 7.
  - Verified: `make test-rust FILTER="--test plain_actions --test prompts"` @ `ebfa0fc` → ok: rust. `tests/plain_actions.rs` covers offering and executing the commit menu's `--author` option from the Plain document: the prompt renders into `.magit` with no `view/prompt`, and repeating the same args while the prompt is open returns a readable stale refusal with no second render.
- [ ] **T008.5b2c — Activate confirm and text prompt rows from Plain documents.** Offer and execute `prompt.answer` row actions with stale checks. T008 clauses 1–2, 5.
- [ ] **T008.5c — Activate confirm and text prompt rows from Rich keys.** Enter on a prompt row, reconciled field edits through `didChange`. T008 clauses 1–2, 5.
- [ ] **T008.6 — Render choice and multiChoice document prompts.** Rows, allowCustom field with default, preselection, Done; optional field completion. T008 clause 2.
- [ ] **T008.7 — Declare the typed pick capability.** Purpose/multi/free text/preselection/item fields/fallbackView; no undeclared shape; client declarations and vectors. T008 clause 3.
- [ ] **T008.8 — Implement pick rungs and preference limits.** Native → document → code actions; per-client opt-in; Neovim static `vim.ui.select` and long-list cut. T008 clauses 3, 6, 9.
- [ ] **T008.9 — Implement revision pick semantics.** HEAD-reachable log select, free text with rev-parse, cursor preselection, multi, item opens commit, IMM-6, execution-time recheck. T008 clauses 4, 9.
- [ ] **T008.10 — Route Plain discard and menu rows through document prompts.** Remove `ConfirmationUnavailable`; menu option rows open prompts. T008 clause 7.
- [ ] **T008.11 — Align client prompt behavior with shared vectors.** Lua custom-choice prefill and empty-secret decision; VSCode parity. T008 clause 8.
- [ ] **T008.12 — Generate docs/plain-support.md from the command registry.** Recording-Ui matrix, `Unavailable` reasons, maintained target and staleness check. T008 clause 10.
- [ ] **T008.13 — Add the mixed-rung conformance test.** Cancel at each step; no Git/index change; caller returns; view-stack depth unchanged. T008 clause 11.

- [x] **T007.4c2 — Abort manually attached Plain commit drafts.** Done: `2fd268371f92fbdec3b8ef1db60668d7a66ebd83`. Owner/run/URI/version-bound Abort rejects stale input, preserves Git state and retires rows/actions. Verified: `make test-rust FILTER="--test plain_harness --test server_async"` → ok: rust on the implementation diff; same scenario retains Confirm and declared opening paths.
- [x] **T007.4c — Confirm or abort manually attached commit drafts.** Done: `2fd268371f92fbdec3b8ef1db60668d7a66ebd83`. Attached issued URI through didOpen and exposed Confirm/Abort actions. Exact Confirm body and stale-version rejection landed in `ccf6cbbe055ccb157b59aed525537cdef48f6171`; the existing single scenario enforces both outcomes, terminal retirement and readable guidance.
- [x] **T007.4n1 — Extend the existing Plain native runner for manual commits.** Add no-showDocument Confirm/Abort flows and structured evidence fields to the maintained runner, without adding test/scenario files. Verify runner helpers; native execution belongs to T007.4n2.
  - Done: `41ff608` (runner foundations: `7e1d814`).
  - Verified: `make ui-unit` @ `41ff608` → ok: ui-unit. Runner check: Confirm and Abort each use the numbered picker, ordinary `:edit`, issued URI/row, attached buffer, selected action and Git-state assertions; no new scenario files or mocked production path.
- [x] **T007.4n1a — Disable native showDocument support on the wire.** Done: `c361960`. Explicit support=false survives Neovim's default capability merge; the runner checks both configured and server-effective false. Verified: `make ui-unit` → ok: ui-unit. Prior native failure: `build/test-logs/plain-neovim.UWQ5TW`.
- [x] **T007.4n1b — Stage native manual-commit input after visibility setup.** Done: `eb56e16`. The runner stages through the native picker after visibility setup, requires a nonempty cached diff, and compares Confirm's tree to that captured tree. Verified: `make ui-unit` → ok: ui-unit; native verification remains T007.4n2.
- [x] **T007.4n1c — Acknowledge observed native message prompts before state reads.** Keep native LSP handlers unchanged. Bound state-read retries, capture text evidence, and acknowledge only a concrete hit-enter or more prompt; unrelated RPC failures remain failures.
  - Done: `49cc25f`; recovery follow-up: `82d59c8`.
  - Verified: `make ui-unit` on `82d59c8` plus the added regression tests → ok: ui-unit. Tests cover unchanged non-timeout RPC errors and final timeout preservation without final acknowledgement; prior observed-prompt and text-evidence checks remain enforced.
- [x] **T007.4n1d — Normalize native Git scalar output without changing blob evidence.** Done: `78f98eb`. Strip command line endings only from revision, tree and count scalars; preserve blob, body and diff output.
  - Verified: `make ui-unit` @ `f6a7832` → ok: ui-unit. Subprocess-wrapper tests cover scalar normalization and blob output with either newline form. Earlier native failure: `build/test-logs/plain-neovim.7zVWEw`; fixed before final native gate.
- [x] **T007.4n1e — Preserve exact native commit-body evidence.** Done: `431af86`. Read raw commit object and preserve body bytes; mismatch reports actual and expected representations.
  - Verified: `make ui-unit` @ `78f98eb` → ok: ui-unit. Earlier native failure `build/test-logs/plain-neovim.OhJwJd` reached Confirm but stalled at fresh Abort attachment; n1f addresses buffer retirement.
- [x] **T007.4n1f — Retire the confirmed native draft buffer before reopening.** Done: `9187cae`. Force-wipe only terminal confirmed buffer after exact Git checks and row retirement; assert deletion and fresh Abort attachment.
  - Verified: `make ui-unit` @ `9187cae` → ok: ui-unit. Source inspection verifies lifecycle order and distinct buffer assertion.
- [x] **T007.4n1g — Budget synchronization for the added manual native transitions.** Done: `20c7f9c`. Keep 30 applyEdit cap before manual setup; allow 16 for manual flows; retain global request/no-op limits and primary failures.
  - Verified: `make ui-unit` @ `20c7f9c` → ok: ui-unit. Tests reject both phase overages, request/no-op excess and preserve workflow failures; report records both phase counts.
- [x] **T007.4n2 — Verify native Plain manual commit editing.** Done: `c407a1d`. Both newline variants passed explicit no-showDocument, URI instructions/rows, ordinary attachment, native Confirm/Abort, exact body/tree/worktree and abort Git invariants, row retirement, no custom notifications and cleanup.
  - Verified: `make plain-neovim` → ok: plain-neovim. Evidence: `build/ui/plain-render/without-final-newline/native.json`, `build/ui/plain-render/with-final-newline/native.json`. Earlier native failures are recorded above and in prior evidence; final run passed.
- [x] **T007.4 — Attach manual commit editing without showDocument.** Done: `2fd2683`. The single real-stdio scenario `plain_commit_editor_manual_instructions_cancel_pending_git_run` was expanded; no additional scenario added.
  - Verified: `make test-rust FILTER="--test plain_harness --test server_async"` @ `2fd2683` → ok: rust. Scenario tests absent and declared/rejected showDocument, exact URI row, Confirm body, Abort invariance and terminal retirement.
- [x] **T007.5 — Audit Plain capability acceptance.** Done: containing commit. All six original T007 clauses map to current source and enforcing tests below; no new test or scenario added.
  - 1. Code-action-only clients: `src/presentation/mod.rs:244-261`; `plain_actions::code_actions_work_without_execute_command_declaration` and `code_actions_are_unsupported_without_declaration`; `70eca61`.
  - 2. Neovim `documentChanges` quirk: `src/quirks.rs:78-92`; `quirks::real_server_reports_effective_plain_resolution_and_overrides` and `plain_render::actual_plain_did_open_applies_authoritative_status_without_writing_presentation_file`; quirk resolution and outgoing-format enforcement: `6646086`, `11d1c8b`.
  - 3. Capability declaration comments: `src/presentation/mod.rs:271-289`; all `from_lsp` reads label feature declarations versus sub-features; `70eca61`.
  - 4. Missing-echo recovery: `src/server.rs:849-856`, `src/presentation/plain.rs:294`; `plain_render::silent_stale_success_without_echo_recovers_after_watcher_render` and `accepted_unconfirmed_edit_expires_and_repairs_only_on_new_evidence`; `33230ec`, `53483b9`, `5209f6e`.
  - 5. Standard `workspace/applyEdit`: `tests/test_plain_apply_edit.lua:4-140`; in-process stock handler covers all declaration/version combinations plus a stale second entry; `338b9de`. This is scripted editor evidence, not native evidence.
  - 6. Manual/declared commit editing: current `src/server.rs`; the single real-stdio `plain_commit_editor_manual_instructions_cancel_pending_git_run` scenario retains no-showDocument and declared paths; `0e9223a`, `c3f5a42`, `2264616`, `ccf6cbb`, `2fd2683`. Native evidence is the two `native.json` files recorded under T007.4n2.
  - Verified: `make ui-unit` @ `20c7f9c` → ok: ui-unit (reused; source unchanged since). Native verification: `make plain-neovim` @ `c407a1d` → ok: plain-neovim. Both JSON reports show declared/effective `showDocument=false`, exact URI and commit body, successful Confirm/Abort oracles, zero custom notifications, and complete cleanup.

Canonical roadmap and queue; architecture and glossary: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). See [AGENTS.md](./AGENTS.md). Behaviour claims require current source/tests. Project status: early stage; backward compatibility is not a goal.


## Deferred hygiene follow-ups

Keep this debt open. Run tools through `make hygiene` and focused suites through `make vscode-test`, `make test-rust FILTER="..."`, or `make test-file FILE=...`; a report-only exit is not a clean bill of health. Any additional executable check needs its own Make target before execution.

- [ ] **Resolve the vscode-jsonrpc/node direct dependency** — `clients/vscode/test/e2e/real-file-native.js:391`; declare it in the owning package and verify with pinned Knip plus VSCode E2E dependency install.
- [ ] **Audit unused VSCode exports: commands and context** — `clients/vscode/lib/commands.js` (`openStatus`, `viewport`, `pressCommand`, `invokeCommand`, `mirrorFoldShortcuts`, `keybindingsPath`); `context.js` (`deriveContext`, `inactiveContext`, `staticScope`). Verify focused VSCode tests and Knip.
- [ ] **Audit unused VSCode exports: draft and edit lifecycle** — `clients/vscode/lib/drafts.js` (`cancel`, `close`, `confirm`, `disconnect`, `handleInvoke`, `onChanged`, `onClosed`, `save`, `setContext`, `setup`); `edit.js` (`KIND`, `SCHEME`, `draftAction`, `isEditEffect`, `languageForKind`); `edits.js` (`onDocumentClosed`, `onActiveEditorChanged`, `confirmEditCommand`, `cancelEditCommand`, `currentEdit`). Verify edit lifecycle tests and Knip.
- [ ] **Audit unused VSCode exports: effects, fold keys and menus** — `clients/vscode/lib/effects.js` (`showMessage`, `replay`); `foldkeys.js` (`parseJsonc`, `mapFoldCommand`, `mapFoldKeys`); `menu.js` (`quickPickItems`, `quickPickItem`, `describeItem`, `routeTyped`, `charToKey`); `menus.js` (`showMenu`, `menuItems`, `hideMenu`, `queueSteps`). Verify menu/fold/effect tests and Knip.
- [ ] **Audit unused VSCode exports: prompt, render and requests** — `clients/vscode/lib/prompt.js` (`CUSTOM_ID`, `promptItems`, `answerFor`, `customChoice`); `render.js` (`applyRender`, `applyEdit`); `requests.js` (`press`, `invoke`, `applyRender`, `handlePrompt`, `resync`). Verify prompt/render/request tests and Knip.
- [ ] **Audit unused VSCode exports: semantic, session and views** — `clients/vscode/lib/semantic.js` (`register`); `session.js` (`activate`, `deactivate`, `createClient`); `views.js` (`stateForUri`, `activeState`, `isMenuKind`, `ensureView`, `showView`, `showDocument`, `applyCursor`, `activateView`, `closeView`, `closeRoot`, `focusView`). Verify VSCode integration tests and Knip.
- [ ] **Resolve Lua warning findings** — `lua/magit-lsp/debug.lua:91`, `edit.lua:98`, `highlights.lua:19`, `hooks.lua:42,49,56,63,70,78,103`, `init.lua:122,167,373`, `keymap.lua:83`, `keys.lua:256`, `requests.lua:179`, `view.lua:626,728`, `scripts/color_spike.lua:57`, `tests/spikes/real_file_nvim.lua:14,27,40,47`, `tests/test_magit_lsp.lua:45,514,894,1143,1526,1550,1553,1555,1556,1557,1743`. Verify pinned luacheck; guarded `table.unpack` fallback is tested at line 1743, but LuaJIT runtime reports `table.unpack == nil`.
- [ ] **Reduce evidence archive size** — `docs/evidence/harness-reassessment-inputs.md`; preserve immutable evidence while reducing tracked file below 100000 bytes. Verify `make hygiene` size report.
- [ ] **Split server implementation below hygiene limit** — `src/server.rs`; preserve behavior and reduce below 100000 bytes. Verify focused Rust tests and `make hygiene`.
- [ ] **Split async server tests below hygiene limit** — `tests/server_async.rs`; preserve coverage and reduce below 100000 bytes. Verify `make test-rust FILTER="--test server_async"` and `make hygiene`.
- [ ] **Split Neovim integration tests below hygiene limit** — `tests/test_magit_lsp.lua`; preserve coverage and reduce below 100000 bytes. Verify focused Neovim tests and `make hygiene`.
- [ ] **Split canonical task roadmap below hygiene limit** — `TASKS.md`; retain canonical design while reducing below 100000 bytes. Verify documentation links and `make hygiene`.
- [ ] **Split editor bridge tests below hygiene limit** — `tests/editor_bridge.rs`; preserve coverage and reduce below 100000 bytes. Verify `make test-rust FILTER="--test editor_bridge"` and `make hygiene`.

These decisions are canonical unless superseded by an explicit later decision
here. General agent working conventions and verification guidance live in
`AGENTS.md`.

[Decisions, glossary and completed correctness contracts](docs/ARCHITECTURE.md) are maintained in the architecture reference.

---

## 3. Design decisions (expanded)

Sections 3.1–3.19: see [Architecture and design](docs/ARCHITECTURE.md#3-design-decisions-expanded).


---

## 4. Protocol reference

The current protocol overview lives in [`docs/PROTOCOL.md`](docs/PROTOCOL.md).
Its detailed client behavior contract lives in [`docs/CLIENT_SPEC.md`](docs/CLIENT_SPEC.md).

---

## Keymaps and views

- [Keymap profiles](docs/KEYMAPS.md#5-default-keymaps-profiles)
- [View catalog](docs/VIEWS.md#6-view-catalog)

---

## 7. Numbered implementation queue

This checkbox index in TASKS.md is the sole live numbered status queue. [`TASKS_PART_SEVEN.md`](./TASKS_PART_SEVEN.md) is an archived scope and provenance document; its detailed ticket specifications remain linked and do not track status. Original scope and Verify text remain recoverable from immutable `d04dfa2:TASKS.md`; provenance is not completion evidence. See [migration ledger](docs/evidence/task-queue-migration.md).

### Feature — Phase 2 (acceptance complete; final gate pending)

T005 is complete through its authoritative [completed children](#completed-work); full scope and original acceptance: [T005](./TASKS_PART_SEVEN.md#t005-quirks-table-m).
- [x] **T006 — fake-editor-model.** [Specification](./TASKS_PART_SEVEN.md#t006-fake-editor-model-l).
  - Done: containing commit.
- [x] **T007 — plain-capability-fixes.** [Specification](./TASKS_PART_SEVEN.md#t007-plain-capability-fixes-m). Done: containing commit; all six original clauses and native evidence are audited under T007.5. Final Phase 2 gate remains orchestrator-owned.

### Feature — Phase 3: Plain interaction

- [ ] **T008 — prompts-and-picks.** [Scope](./TASKS_PART_SEVEN.md#t008-prompts-and-picks-l).
- [ ] **T009 — stable-id-selection.** [Scope](./TASKS_PART_SEVEN.md#t009-stable-id-selection-m).
- [ ] **T010 — plain-commit-editing.** [Scope](./TASKS_PART_SEVEN.md#t010-plain-commit-editing-s).

### Feature — Phase 4: Specification

- [ ] **T011 — schema-generation.** [Scope](./TASKS_PART_SEVEN.md#t011-schema-generation-m).
- [ ] **T012 — example-files.** [Scope](./TASKS_PART_SEVEN.md#t012-example-files-s).
- [ ] **T013 — client-spec.** [Scope](./TASKS_PART_SEVEN.md#t013-client-spec-l).

### Feature — Phase 5: Commit selection and rebase

- [ ] **T014 — selected-commit-continuation.** [Scope](./TASKS_PART_SEVEN.md#t014-selected-commit-continuation-m).
- [ ] **T015 — fixup-action.** [Scope](./TASKS_PART_SEVEN.md#t015-fixup-action-m).
- [ ] **T016 — editor-helper-kinds.** [Scope](./TASKS_PART_SEVEN.md#t016-editor-helper-kinds-s).
- [ ] **T017 — rebase-state.** [Scope](./TASKS_PART_SEVEN.md#t017-rebase-state-m).
- [ ] **T018 — rebase-todo.** [Scope](./TASKS_PART_SEVEN.md#t018-rebase-todo-m).
- [ ] **T019 — interactive-rebase.** [Scope](./TASKS_PART_SEVEN.md#t019-interactive-rebase-l).

### Feature — Phase 6: Plain acceptance

- [ ] **T020 — plain-e2e.** [Scope](./TASKS_PART_SEVEN.md#t020-plain-e2e-l).
- [ ] **T021 — helix-smoke.** [Scope](./TASKS_PART_SEVEN.md#t021-helix-smoke-s).
- [ ] **T022 — plain-gate.** [Scope](./TASKS_PART_SEVEN.md#t022-plain-gate-s).

### Feature — Phase 6A: Rich protocol v1 and client prototypes

- [ ] **T112 — rich-protocol-audit.** [Scope](./TASKS_PART_SEVEN.md#t112-rich-protocol-audit-s).
- [ ] **T113 — rich-protocol-decisions.** [Scope](./TASKS_PART_SEVEN.md#t113-rich-protocol-decisions-s).
- [ ] **T114 — rich-protocol-cleanup.** [Scope](./TASKS_PART_SEVEN.md#t114-rich-protocol-cleanup-m).
- [ ] **T105 — vscode-f5-cause.** [Scope](./TASKS_PART_SEVEN.md#t105-vscode-f5-cause-s); rescheduled here, not duplicated under Deferred.
- [ ] **T119 — min-git-version.** [Scope](./TASKS_PART_SEVEN.md#t119-min-git-version-s).
- [ ] **T115 — visual-studio-client.** [Scope](./TASKS_PART_SEVEN.md#t115-visual-studio-client-l).
- [ ] **T116 — zed-client.** [Scope](./TASKS_PART_SEVEN.md#t116-zed-client-l).
- [ ] **T117 — jetbrains-client.** [Scope](./TASKS_PART_SEVEN.md#t117-jetbrains-client-l).
- [ ] **T118 — rich-protocol-freeze.** [Scope](./TASKS_PART_SEVEN.md#t118-rich-protocol-freeze-s).

### Feature — Phase 7: Test stabilization (does not gate 0.2)

- [ ] **T023 — capability-lattice.** [Scope](./TASKS_PART_SEVEN.md#t023-capability-lattice-m).
- [ ] **T024 — host-spike.** [Scope](./TASKS_PART_SEVEN.md#t024-host-spike-s).
- [ ] **T025 — e2e-runner.** [Scope](./TASKS_PART_SEVEN.md#t025-e2e-runner-m).
- [ ] **T026 — host-cases.** [Scope](./TASKS_PART_SEVEN.md#t026-host-cases-l).
- [ ] **T027 — keybinding-compaction.** [Scope](./TASKS_PART_SEVEN.md#t027-keybinding-compaction-m).
- [ ] **T028 — colour-roles.** [Scope](./TASKS_PART_SEVEN.md#t028-colour-roles-s).
- [ ] **T029 — scenario-slimming.** [Scope](./TASKS_PART_SEVEN.md#t029-scenario-slimming-m).

### Feature — Phase 8: Release

Starts after phases 1–6. Do not insert unrelated features between release gates and release. Evidence remains required.

- [ ] **T030 — release-artifacts.** [Scope](./TASKS_PART_SEVEN.md#t030-release-artifacts-m).
- [ ] **T031 — client-install-docs.** [Scope](./TASKS_PART_SEVEN.md#t031-client-install-docs-s).
- [ ] **T032 — release-readme.** [Scope](./TASKS_PART_SEVEN.md#t032-release-readme-m).
- [ ] **T033 — release-go-no-go.** [Scope](./TASKS_PART_SEVEN.md#t033-release-go-no-go-m); explicit human approval remains required.

### Feature — Phase 9: Plain keys (optional; starts after T022)

- [ ] **T034 — key-edit-shapes.** [Scope](./TASKS_PART_SEVEN.md#t034-key-edit-shapes-s-gate).
- [ ] **T035 — key-classifier.** [Scope](./TASKS_PART_SEVEN.md#t035-key-classifier-m).
- [ ] **T036 — key-policy-settings.** [Scope](./TASKS_PART_SEVEN.md#t036-key-policy-settings-m).
- [ ] **T037 — key-execution.** [Scope](./TASKS_PART_SEVEN.md#t037-key-execution-m).
- [ ] **T038 — key-safety-e2e.** [Scope](./TASKS_PART_SEVEN.md#t038-key-safety-e2e-m).
- [ ] **T039 — plain-keys-docs.** [Scope](./TASKS_PART_SEVEN.md#t039-plain-keys-docs-s).

### Feature — Phase 10: Small features

Each ticket below retains its row's full scope and dependencies in the [Phase 10 specification table](./TASKS_PART_SEVEN.md#phase-10-small-features).

- [ ] **T040 — commit-signing.**
- [ ] **T041 — log-variants.**
- [ ] **T042 — remote-config-tags.**
- [ ] **T043 — branch-basics.**
- [ ] **T044 — notes-ignore.**
- [ ] **T045 — theming-docs.**
- [ ] **T046 — forking-docs.**
- [ ] **T047 — config-location-edit.**

### Feature — Phase 11: Read views

Each ticket below retains its row's full scope and dependencies in the [Phase 11 specification table](./TASKS_PART_SEVEN.md#phase-11-read-views).

- [ ] **T048 — view-data.**
- [ ] **T049 — revision-view.**
- [ ] **T050 — diff-view.**
- [ ] **T051 — picker-inspect.**
- [ ] **T052 — refs-view.**
- [ ] **T053 — file-at-revision.**
- [ ] **T054 — reflog-view.**
- [ ] **T055 — line-history-view.**
- [ ] **T056 — log-links-hover.**
- [ ] **T057 — log-semantic-styling.**

### Feature — Phase 12: Shared Git workflows

Each ticket below retains its row's full scope and dependencies in the [Phase 12 specification table](./TASKS_PART_SEVEN.md#phase-12-shared-git-workflows).

- [ ] **T058 — diff-arguments.**
- [ ] **T059 — push-baseline.**
- [ ] **T060 — status-history-sections.**
- [ ] **T061 — unpulled-and-headers.**
- [ ] **T062 — commit-staged-diff.**
- [ ] **T063 — commit-trailers-lint.**
- [ ] **T064 — commit-edit-close.**
- [ ] **T065 — targeted-fixup-squash.**
- [ ] **T066 — instant-rewrite.**
- [ ] **T067 — branch-checkout-spin.**
- [ ] **T068 — network-seam.**
- [ ] **T069 — fetch-push-pull.**
- [ ] **T070 — branch-reset.**
- [ ] **T071 — annotated-tags.**
- [ ] **T072 — stash-views.**
- [ ] **T073 — stash-operations.**

### Feature — Phase 13: Advanced workflows

Each ticket below retains its row's full scope and dependencies in the [Phase 13 specification table](./TASKS_PART_SEVEN.md#phase-13-advanced-workflows).

- [ ] **T074 — merge-conflicts.**
- [ ] **T075 — mergetool-effect.**
- [ ] **T076 — sequencer-state.**
- [ ] **T077 — cherry-view.**
- [ ] **T078 — rebase-options.**
- [ ] **T079 — rebase-merges.**
- [ ] **T080 — scripted-sequence-edit.**
- [ ] **T081 — picker-integrations.**
- [ ] **T082 — blame-view.**
- [ ] **T083 — blame-inline.**
- [ ] **T084 — run-command.**
- [ ] **T085 — patch-workflows.**
- [ ] **T086 — am-state.**
- [ ] **T087 — bisect-state.**
- [ ] **T088 — multi-repo-identity.**
- [ ] **T089 — worktree-operations.**
- [ ] **T090 — submodule-workflows.**
- [ ] **T091 — buffer-repo-context.**
- [ ] **T092 — any-buffer-dispatch.**
- [ ] **T093 — section-layout-policy.**
- [ ] **T094 — full-line-tint.**
- [ ] **T095 — profiling-budgets.**
- [ ] **T096 — incremental-render.**
- [ ] **T097 — windows-paths.**
- [ ] **T098 — jj-fork.**

### Deferred — Phase 14: Not scheduled

DO NOT START these tickets until the owner asks. Each retains its row's full scope and constraints in the [deferred specification table](./TASKS_PART_SEVEN.md#phase-14-deferred-not-scheduled). T105 is rescheduled to Phase 6A, not duplicated here.

- [ ] **T099 — edit-history.**
- [ ] **T100 — windows-editor-bridge.**
- [ ] **T101 — macos-editor-bridge.**
- [ ] **T102 — cross-hunk-selection-audit.**
- [ ] **T103 — same-session-read-redesign.**
- [ ] **T104 — editor-lifecycle-residuals.**
- [ ] **T106 — pick-delivery-modes.**
- [ ] **T107 — skip-at-point-option.**
- [ ] **T108 — server-fuzzy-picker.**
- [ ] **T109 — multi-client-repository.**
- [ ] **T110 — rich-protocol-form.**
- [ ] **T111 — open-design-questions.**

T001–T004 and T120 are complete and excluded from this unchecked index. A checkbox is not permission to bypass dependencies or deferred owner approval. Each linked specification remains mandatory acceptance, not a fresh completion claim.

### Retained gaps

- **A12:** real-dispatch Git-failure handling and removal of `server::mutation`; completed; see the enforcing record in Completed work. T103 remains the separate deferred same-session reader-lock redesign.
- **R-EDIT-REAL:** parent acceptance is not green. T104 is only a subset; last-window exit, dirty shutdown, cross-build, reload, workspace and platform limits remain. Broad fallback review is not comprehensive acceptance.
- Binary files remain whole-file-only; T093 layout work remains open.
- The macOS config-directory choice is resolved; only the root config-edit command and runtime config-location verification remain open. Escape caveat remains.

### Task ticket template and UI rule

Every ticket states **Target Files** (exclusive write boundary), **Reference Files**, **Architectural Constraints**, **Acceptance Criteria**, and exact **Verification Command**; also state dependencies and size. Each ticket inherits its applicable phase gate from its enclosing named phase and the phase map. The phase gate applies when the last unchecked item in a named phase group completes, not when an individual ticket, slice or review-fix batch completes. Follow current review and orchestration instructions in [AGENTS.md](./AGENTS.md#working-conventions). User-visible changes require observable text/structured outcomes and focused native evidence; never images.

---

## 8. Testing strategy

See [Testing strategy](docs/TESTING.md#8-testing-strategy).

## 9. Open questions

See [Open questions](docs/OPEN_QUESTIONS.md#9-open-questions).

---

## Bugfix

- [x] **Phase 2 gate — Declare a rich client in editor-bridge commit tests.** `make test` at `eb8bb1e` fails 18/27 `editor_bridge` tests (`log build/test-logs/rust.zuIHJG`) because they initialize with `json!({})` and still expect the `view/effects` `editDocument` frame. Plain clients now intentionally get `window/showMessage` with the exact `COMMIT_EDITMSG` URI plus a pending draft row (T007.4), and Plain delivery is already enforced by `plain_commit_editor_manual_instructions_cancel_pending_git_run`; the editor-bridge suite owns the rich in-place editor contract instead. Give those 18 tests a rich `experimental.magit` declaration (keys, modifiers, layout, `editDocument` effect, prompts) and keep every sync/cancel/checkpoint assertion unchanged. Done: 5da5d86. Verified: make test-rust FILTER="--test editor_bridge" (27 passed).

- [ ] **Repair the five failing UI scenarios (`vim` profile and `process-log`).** `make ui-test` at `5da5d86` reports 44 PASS / 5 FAIL: `common-navigation` and `hunk-body-fold` (vim profile assertions about staged state and hunk bodies), `log-select` and `log-view` (wait for `Log` times out), `process-log` (expects `Staged changes (1)`, screen shows none). A clean extraction of reviewed baseline `2e29535` under `make ui-test` fails the same five scenarios with the same expectations, so this predates the Phase 2 work. Establish whether the `vim` profile setup or the shared fixtures are wrong (a fixture that silently builds an empty or unstageable repository would explain all five), then fix the cause rather than the expectations, or delete a scenario that no longer matches the server-owned contract.


## Review

- [x] **[P1] Reconcile relocated architecture with the current wire contract.** Done: `e666f27`.
- [x] **[P1] Make bare docs-check validate every maintained reference.** Done: `0474efc`; evidence: `9df2075`.
- [x] **[P2] Stop calling the resolved config-directory choice open.** Done: containing commit.

- [x] **[P1] Route Plain draft-sync failures through standard notifications, not `view/effects`** (`src/server.rs:1828-1840`, `editor::registry().notification_failure` call site) Done: `5fb7977`. Verified: make test-rust FILTER="--test plain_harness --test server_async".
  - **Problem:** The `textDocument/didOpen|didChange|didClose|didSave` failure handler emits every error (and the failed-close `Effect::EditDocument` reopen) through `custom_ui(&shared.client).effects(...)`, which for a `Presentation::Plain` client serializes a `view/effects` notification — the custom rich protocol a thin Plain client does not understand. The start_listener emitter (`src/server.rs:1514-1549`) already branches on Plain and delivers `Effect::Message` via `window/showMessage` and `Effect::EditDocument` via `send_plain_edit_document`; this second delivery seam does not. Result: a Plain client that closes an unsaved commit buffer (failed native close) or sends a stale `didChange` gets a `view/effects` frame it ignores, loses the readable error and the re-presented editor, and leaves the Git helper blocked in its reply loop until an explicit Abort or shutdown. The `plain_commit_editor_manual_instructions_cancel_pending_git_run` scenario only exercises the happy Confirm/Abort path, so this is un-covered.
  - **Why it matters:** Correctness/cohesion — it violates the T007.4 contract that Plain delivery is standard LSP only (the native runner asserts zero custom notifications) and breaks the "draft registry as sole authority" recovery path.
  - **Change:** Extract the Plain-branch body of the start_listener emitter (`src/server.rs:1514-1549`) into one helper, e.g. `fn deliver_plain_effects(shared: &Shared, effects: Vec<Effect>)`, that maps `Effect::Message` through `effect_notifications(&shared.client, …)` and `Effect::EditDocument { run_id, uri, title, .. }` through `send_plain_edit_document` (re-reading `capabilities.show_document` from `shared.client`) followed by the pending-row refresh already done inline at `src/server.rs:1525-1539`. Call that helper from both the listener emitter and the `notification_failure` emit closure at `src/server.rs:1828-1840` (the closure becomes `|effects| deliver_plain_effects(&shared, effects)` for Plain, falling back to `custom_ui(...).effects(...)` for Rich). Do not change `editor::notification_failure`'s signature; keep the "queue while holding the lifecycle lock" comment intact.
  - **Done when:** A Plain (no `experimental.magit`) real-stdio test sends a stale `didChange` or a failed `didClose` on the issued `COMMIT_EDITMSG` URI and asserts the client receives `window/showMessage` (and, on failed close, a `window/showDocument`/manual instruction for the same run) with no `view/effects` notification on the wire; existing Plain and Rich draft tests still pass.
  - **Constraints:** Do not touch `src/editor.rs` beyond its existing `notification_failure` contract. Do not add a new scenario file; extend the existing Plain commit-editor scenario or a `plain_harness` test.

- [x] **[P2] Document the registry-lock-during-emit invariant behind the detached refresh thread** (`src/editor.rs:840-876`, `src/server.rs:1531`) Done: `e70f748`. Verified: make test-rust FILTER="--test plain_harness".
  - **Problem:** `handle_edit` invokes `emitter(...)` while holding `editor::registry().lock()` (the `entries` guard stays alive through the call). The Plain emitter must therefore run `refresh_pending_views` on a separate thread, because that refresh re-enters the registry lock via `render_with_delivery` → `registry().pending_for` and would self-deadlock on the emitter's own thread. The `std::thread::spawn(move || …)` at `src/server.rs:1531` exists precisely to break that cycle, but nothing records it, and the thread is detached (never joined, holds a full `Shared` clone until it finishes).
  - **Why it matters:** Readability/simplicity — a hidden lock-ordering invariant that a future "simplification" (running the refresh inline) would turn into a deadlock.
  - **Change:** Add a comment at `src/server.rs:1531` stating why the refresh must run on a separate thread (the emitter is called under `editor::registry().lock()` in `handle_edit`, and an inline refresh would re-acquire that lock through `EditRegistry::pending_for`). Optionally add a matching note at `src/editor.rs:865` that `emitter` runs while the registry lock is held.
  - **Done when:** Both sites carry a comment that names the lock and the deadlock it prevents; no behavior change and existing Rust tests pass.
  - **Constraints:** Comment-only change; do not restructure lock ownership in this task.

## Completed work

- [x] **Documentation cleanup — Compact capability and recorder completion records.** Done: containing commit.
- [x] **Documentation cleanup — Compact legacy completion records.** Done: containing commit.
- [x] **Documentation cleanup — Condense completed-task records.** Done: containing commit.
- [x] **Documentation cleanup — Condense obsolete history and preserve live work.** Done: containing commit.
- [x] **Documentation cleanup — Restore moved completion records.** Done: `16f4d11`.
- [x] **Documentation cleanup — Finish queue navigation and preservation audit.** Done: `2e051e3`.
  - Bounded audit: commit `16f4d11` compared source titles and references (122/122 checked titles; 102/102 refs; 9/9 original unchecked titles); bounded reviewer confirmed all original pre-cleanup unfinished tasks and requirements remain. T007 worktree-only changes are excluded from staged source comparison.
  - Scope: queue navigation and owned cleanup/review metadata only; no runtime/native acceptance claim. Final line count: 546.
  - Verified: `make docs-check` → ok: docs-check.

- [x] **Documentation cleanup — Add focused documentation verification.** Done: `93bb9ab` (Makefile ownership correction `4c83d0d`).
- [x] **Documentation cleanup — Move decisions and glossary reference.** Done: `3186a69`.
- [x] **Documentation cleanup — Move core design reference.** Done: `9f8f28f`.
- [x] **Documentation cleanup — Move workflow design reference.** Done: `338b9de`.
- [x] **Documentation cleanup — Move expanded design reference.** Done: `338b9de`.
- [x] **Documentation cleanup — Move architecture and glossary reference.** Done: `338b9de`.
- [x] **Documentation cleanup — Move protocol reference.** Done: `0d663c7`.
- [x] **Documentation cleanup — Preserve inline-code heading anchors in link checks.** Done: `7d4bee4`.
- [x] **Documentation cleanup — Move keymaps and view reference.** Done: `e4f259d`.
- [x] **Documentation cleanup — Move testing and open questions reference.** Done: `4537ef1`.
- [x] **Documentation cleanup — Finish maintained reference navigation.** Done: `d9ea816`.
- [x] **Documentation cleanup — Move durable reference material into docs.** Done: `d9ea816`.
- [x] **Documentation cleanup — Repair recent review completion pointers.** Done: `f13dbc1`.
- [x] **Documentation cleanup — Repair older review completion pointers.** Done: `e464429`.
- [x] **Documentation cleanup — Remove resolved review and checkpoint narratives.** Done: `3ec4751`; provenance correction: `f13dbc1`, `e464429`.
- [x] **Documentation cleanup — Compact documentation completion records.** Done: `a45fcde`.
- [x] **Documentation cleanup — Compact T006 and T007 completion records.** Done: `bceb82d`.
- [x] **Documentation cleanup — Compact formatter completion records.** Done: `eb87a14`.
- [x] **Documentation cleanup — Compact quirk completion records.** Done: `5f5f308`.
- [x] **Documentation cleanup — Compact typed-capability completion records.** Done: `3787c66`.
- [x] **Documentation cleanup — Compact recorder and migration completion records.** Done: `408148f`.
- [x] **T006.1a — Extract the pump and allocate fake-editor requests dynamically.** Done `1570ec0`; `make test-rust FILTER="--test plain_harness --test plain_actions"` passed; no real-server didOpen claim.
- [x] **T006.1b — Prove Plain didOpen applyEdit through the pump.** Done `2e60210`; `make test-rust FILTER="--test plain_harness"` passed; real-server pump regression.
- [x] **T006.1 — Extract the configurable client pump and dynamic request IDs.** Done `2e60210` (completes `1570ec0`); original clauses 1 and 7.
- [x] **T006.2a1 — Commit valid minimum model bases and pump initialization.** Done `2e1f392`; focused Rust suite passed.
- [x] **T006.2a2 — Prove module composition and unify registry behavior selection.** Done `219f8e8`; verified at `2e1f392`; exact RichMin declaration and observable behavior covered.
- [x] **T006.2a — Compose minimum bases and anchored behavior modules.** Done `219f8e8` (completes `2e1f392`).
- [x] **T006.2b1 — Guard strict requests and notifications through the pump.** Done `27c8a70790926b0ebdd6c807db9977b477cc9c90`; verified at `219f8e8`.
- [x] **T006.2b2 — Complete response-effect guards and strict feature matrix.** Done `96504c60fde13587bb69f38e2d66f8dd0758366a`; verified at `27c8a70`.
- [x] **T006.2b — Enforce strict server feature declarations in the pump.** Done `96504c60fde13587bb69f38e2d66f8dd0758366a`.
- [x] **T006.2 — Compose declared editor models and strict feature checks.** Done `96504c60fde13587bb69f38e2d66f8dd0758366a`; original clauses 2–4.
- [x] **T006.3a — Apply rich render state from shared vectors.** Done `53483b9`; focused `plain_harness` test passed.
- [x] **T006.3b — Route rich render acknowledgements and resync through the pump.** Done `a4578fe956998619de708c2aee8ee93da20a17dc` (preceded by `7e4c26cf15b95c3e9715b45b3876e4fa92c05f58`); focused suite passed, including real-server ack/resync.
- [x] **T006.3c — Model shared prompt-vector responses.** Done: source-only provenance `e50d516:TASKS.md`; verified at `a4578fe`; dynamic IDs, cancellation, empty/default/custom/secret cases.
- [x] **T006.3 — Model rich renders, acknowledgements and prompt vectors.** Done: source-only provenance `e50d516:TASKS.md`; original clause 5.
- [x] **T006.4a — Bind recorded client presets and named profiles.** Done `cf74f8911d474b5f63c5def41b10864248c8f67e` (preceded by `158692e8c5e09ea3f24c0daf225fcf977152f1ef`); declarations are not native behavior proof.
- [x] **T006.4b — Audit original fake-editor acceptance.** Done: source-only provenance `e50d516:TASKS.md`; verified at `cf74f89`. Tests cover all seven clauses, including real-server didOpen; 12 baseline tests remain and historical trace smoke test was retired (`864f58c`, fixture removed `37fb07d`).
- [x] **T006.4 — Bind recorded client presets and audit T006 acceptance.** Done: source-only provenance `e50d516:TASKS.md`; original clause 6 and final audit.

- T006/T007 prerequisite audit: detailed historical verification is immutable `e50d516:TASKS.md` provenance, not an implementation Done hash. “Containing commit” entries have no exact recorded implementation hash; exact Done and Verified refs remain above. Original T006 clauses: 1/7 T006.1, 2–4 T006.2, 5 T006.3, 6/final audit T006.4. Scripted model checks are not real-server conformance; recorded declarations are not native evidence. T007.1n evidence paths are `build/ui/plain-render/{without-final-newline,with-final-newline}/native.json`; timeout then passing rerun remains a risk. T007.5 requires all six original clauses and native evidence before T007 closes.

- [x] **T007.1 — Gate Plain actions on code-action support.** Done: source-only provenance `e50d516:TASKS.md`; verified on `70eca6154ce29951c05e1f5ae632cfd146e8e6b8`; original clauses 1–3, no executeCommand declaration requirement.
- [x] **T007.1n — Remove the native Plain executeCommand workaround.** Done: source-only provenance `e50d516:TASKS.md`; `make plain-neovim` on `1d0422e9fa600c50b5015de4798b3269b856779d` passed; both variants prove declaration absence, action/render/Git state and cleanup. Evidence: `build/ui/plain-render/{without-final-newline,with-final-newline}/native.json`; timeout after unstage, diagnostic rerun passed without behavior fix.
- [x] **T007.2a — Expire unconfirmed Plain edits safely.** Done: source-only provenance `e50d516:TASKS.md`; `make test-rust FILTER="--test plain_render"` on `53483b9` passed.
- [x] **T007.2b1 — Schedule the correlated echo watchdog.** Done: source-only provenance `e50d516:TASKS.md`; focused Rust suite passed; real-server silent-stale success without echo triggers fresh edit; one-second grace and lifecycle guards covered.
- [x] **T007.2b2 — Prove post-watchdog convergence and readiness.** Done `5209f6e76623db5965983b202f0708fa8e09eb6e`; implementation-current diff based on `33230ec` passed; real-server convergence, second watcher update and safety covered.
- [x] **T007.2b — Schedule and prove missing-echo recovery.** Done `5209f6e76623db5965983b202f0708fa8e09eb6e`; tests in T007.2b1–b2.
- [x] **T007.2 — Recover Plain Applying state after a missing echo.** Done `5209f6e76623db5965983b202f0708fa8e09eb6e`; original clause 4; tests in T007.2a–b.
- [x] **T007.3 — Exercise Neovim applyEdit in process.** Done `338b9de`; `make test-file FILE=tests/test_plain_apply_edit.lua` @ `338b9de` passed; five stock-handler cases cover declaration/version combinations and stale second entry with exact text/tick/results. Original clause 5.
- [x] **T007.4a1 — Present manual Plain commit instructions.** Finish the weak-lifetime Plain delivery branch and start the single real-server scenario proving manual instructions without custom notifications.
  - Done: `0e9223a`.
  - Verified: `make test-rust FILTER="--test plain_harness --test server_async"` @ `0e9223a` → ok: rust. One real Plain scenario stages and opens commit through standard code actions, receives the exact COMMIT_EDITMSG path by window/showMessage, cancels the active progress token, confirms HEAD unchanged and shuts down cleanly.
- [x] **T007.4a2 — Prove declared Plain editor opening.** Extend that same scenario to prove a correlated standard showDocument request and readable fallback on rejection.
  - Done: `c3f5a422c6c1eac2066c44ee815f66ab392c3105`.
  - Verified: `make test-rust FILTER="--test plain_harness --test server_async"` passed on the committed implementation diff. The single scenario covers absent support, correlated successful opening, rejected opening with manual fallback, cancellation, unchanged HEAD and bounded shutdown.
- [x] **T007.4a — Deliver pending Plain commit editors through LSP.** Route managed edit effects to declared showDocument or readable manual-open instructions. Start the single real-server T007.4 scenario; later children extend that same scenario. Done: `c3f5a422c6c1eac2066c44ee815f66ab392c3105`; enforcing scenario recorded under T007.4a1–T007.4a2.
- [x] **T007.4b1 — Expose owner-bound pending draft snapshots.** Add a read-only registry snapshot keyed by owner and repository, with stable run/URI identities and deterministic ordering. Extend existing coverage, not the number of test cases.
  - Done: `8dd8bcf`.
  - Verified: `make test-rust FILTER="--test editor_bridge early_commit_cancellation_does_not_kill_repository_discovery"` → ok: rust. `EditRegistry::pending_for` filters owner, canonical repository and nonterminal registry entries, then sorts by run ID; `early_commit_cancellation_does_not_kill_repository_discovery` asserts exact run/issued URI resolution, owner/repository isolation and removal after cancellation.
- [x] **T007.4b2 — Wire pending draft identities into Plain rendering.** Propagate only owner identity into server rendering; display registry-derived structured URI rows in the active Plain presentation and refresh their lifecycle. Extend the existing T007.4 scenario only.
  - Done: `2264616`.
  - Verified: `make test-rust FILTER="--test plain_harness plain_commit_editor_manual_instructions_cancel_pending_git_run"` and `make test-rust FILTER="--test plain_harness --test server_async"` → ok: rust. `EditRegistry::pending_for` supplies owner/repository-filtered rows; `Tree::append_pending_drafts` sets stable `pending-draft:<run>` identity and exact URI path metadata; `OpenView::render_with_delivery`, `refresh_pending_views` and editor delivery/commit completion refresh the owner’s Plain views without shadow state.
- [x] **T007.4c1 — Confirm manually attached Plain commit drafts.** Offer synchronized version-bound Confirm on the issued draft URI through standard code actions. Extend the existing scenario to prove exact committed text and row removal.
  - Verified: `make test-rust FILTER="--test plain_harness --test server_async"` @ `cd2d152` → ok: rust. Real LSP didOpen/didChange, stale-version rejection, exact commit body, pending-row removal and terminal action retirement; declared opening/cancellation cases retained.
- [x] **T007.4b — Render pending manual commit paths as structured rows.** Derive pending draft rows from trusted registry state and refresh their lifecycle without parsing rendered text. Extend the existing T007.4 scenario only. Done: `2264616`; enforcing snapshot and lifecycle checks recorded under T007.4b1–T007.4b2.
- [x] **Roadmap reconciliation — Restore one authoritative queue.**
  Done: `6d08d85`.
- [x] **Roadmap reconciliation — Index individual unfinished numbered tickets.**
  Done: `6d08d85`.
- [x] **Roadmap reconciliation — Refresh protocol and evidence guidance.**
  Done: `6d08d85`.
- [x] **Roadmap reconciliation — Route active verification through Make.**
  Done: `6d08d85`.

- [x] **Investigate and rationalize trace/capture test infrastructure.**
  Done: `5025937`. Current workflow: [fixture guide](tests/fixtures/lsp_traces/README.md), [UI test plan](ui-test-plan.md).
- [x] **Autofix staged formatting through shared hook and Make commands.**
  Done: `9702973`; final index-publication fix `fa5c3de`. Current guide: [formatting guide](docs/FORMATTING.md).

- [x] **Remove permanent formatter tests and retain one-time verification.**
  Source-only: `bceb82d:TASKS.md`.

Historical references are provenance, not current instructions. Current workflow: [fixture guide](tests/fixtures/lsp_traces/README.md), [UI test plan](ui-test-plan.md), [formatting guide](docs/FORMATTING.md).
This file remains the sole queue; Part Seven is scope/provenance, not a competing queue. T011 owns schema-drift integration; T025 owns native-host integration.

- [x] **T120.1 — Configure pinned Lua and JavaScript/TypeScript formatters.**
- [x] **T120.2 — Implement staged-safe formatter checks.**
- [x] **T120.3a — Add opt-in formatter hook installation.**
- [x] **T120.3a1 — Isolate missing-tool tests and preserve existing hook locations.**
- [x] **T120.3b — Integrate formatter CI checks and contributor commands.**
- [x] **T120.3 — Wire formatter hooks, installation and CI.**

T120 completion details and one-time verification: immutable `bceb82d:TASKS.md`.

- [x] **Record formatter pre-commit hooks and resolve reported Rust formatting errors.**
  Historical formatting checkpoint; superseded by completed T120 and the staged-only hook follow-up above. Use [current formatting guidance](docs/FORMATTING.md), not this record as a workspace-wide formatting claim.

- [x] **T005.1 — Define the evidence-backed quirk registry and resolver.** Source-only: `eb87a14:TASKS.md`.
- [x] **T005.2a — Add validated layered quirk settings.** Source-only: `eb87a14:TASKS.md`.
- [x] **T005.2b1 — Finish and commit initialize quirk resolution.** Source-only: `eb87a14:TASKS.md`.
- [x] **T005.2b2a — Commit real initialize quirk acceptance tests.** Source-only: `eb87a14:TASKS.md`.
- [x] **T005.2b2b — Prove quirk-controlled outgoing edit format.** Source-only: `eb87a14:TASKS.md`. Verified: `d2faf01`.
- [x] **T005.2b2 — Prove effective quirk resolution through the real server.** Source-only: `eb87a14:TASKS.md`.
- [x] **T005.2b — Resolve and report production initialize quirks.** Source-only: `eb87a14:TASKS.md`.
- [x] **T005.2 — Integrate quirk configuration and effective initialize reporting.** Source-only: `eb87a14:TASKS.md`.
- [x] **T005.3a — Drive fake-editor behavior from quirk registry models.** Source-only: `eb87a14:TASKS.md`. Verified: `f224f94`.
- [x] **T005.3b — Prove named server workarounds and universal safety.** Done: `65ee6d9`. Verified: `65ee6d9`.
- [x] **T005.3c — Generate the known-client appendix and audit T005.** Done: `6c95b50`. Verified: `65ee6d9`.
- [x] **T005.3 — Enforce quirk behaviors and generate the client appendix.** Helix showDocument remains explicit-override-only pending positive evidence; scripted models and declaration captures are not new native evidence. Universal echo/hash and one-document safety remain contracts. Other details: immutable `eb87a14:TASKS.md`.
- [x] **Record the approved judgment-based review policy and clarify current phase boundaries.** Current review and orchestration policy: [AGENTS.md](./AGENTS.md#working-conventions); named roadmap phase groups are phase boundaries.
- [x] **Limit local verification to Linux and retain cross-platform CI follow-ups.**
  Local execution is Linux-only for now. Record Windows/macOS risks and required CI checks explicitly; do not claim those platforms were executed locally or block Linux progress solely for unavailable non-Linux hosts.
- [x] **Update execution limits and future reviewer selection.** Current execution limits are in [AGENTS.md](./AGENTS.md); obsolete model-selection and skip-review directions removed.
- [x] **T004.final — Audit and close typed capability declaration acceptance.**
  Source-only: `5f5f308:TASKS.md`. T004 remains closed; Phase 2 remains open through T007.
- [x] **T004.7 — Check presentation emissions against negotiated declarations.**
  Source-only: `5f5f308:TASKS.md`. Preserve production warning-message and main-placement fallbacks; probes do not fail.
- [x] **T004.7a — Complete strict-emitter rejection and placement-fallback tests.**
  Source-only: `5f5f308:TASKS.md`.
- [x] **T004.7b — Cover both secret dependencies and declared optional effects.**
  Done: `b89bf60`; review correction: `78b6f49`.
- [x] **T004.6 — Derive capability requirements from command metadata.**
  Source-only: `5f5f308:TASKS.md`.
- [x] **T004.6a — Derive the optional-effect requirement table from the registry.**
  Source-only: `5f5f308:TASKS.md`.
- [x] **T004.6b — Cover actual optional-effect producers against command requirements.**
  Source-only: `5f5f308:TASKS.md`. Exercised producers, not exhaustive; deferred effects map to initiators.
- [x] **T004.1 — Use camelCase content-action targets on the wire.**
  Source-only: `5f5f308:TASKS.md`.
- [x] **T004.2 — Remove unconsumed rich rendering and key-delivery modes.**
  Source-only: `5f5f308:TASKS.md`.
- [x] **T004.2a — Finish server mode removal and validate supported-mode coverage.**
  Source-only: `5f5f308:TASKS.md`.
- [x] **T004.3 — Deserialize strict typed rich capability declarations.**
  Source-only: `5f5f308:TASKS.md`; verification: `7678242`. Immutable captures remain provenance.
- [x] **T004.3a — Pin Lua verification to the server built by Make.**
  Source-only: `5f5f308:TASKS.md`.
- [x] **T004.3b — Pin native UI verification to the freshly built server.**
  Source-only: `5f5f308:TASKS.md`.
- [x] **T004.4 — Verify real client declarations and repair VSCode initialization wiring.**
  Source-only: `5f5f308:TASKS.md`. Source equality/causality manually checked, not live recapture.
- [x] **T004.4a — Register and natively verify the real VSCode declaration feature.**
  Source-only: `5f5f308:TASKS.md`. Native evidence provenance retained; declaration source equality/causality manually checked.
- [x] **T004.4b — Validate both current native client declarations against the typed schema.**
  Source-only: `5f5f308:TASKS.md`. Immutable captures remain provenance.
- [x] **T004.4b1 — Retain current native Neovim declaration evidence.**
  Source-only: `5f5f308:TASKS.md`. Native evidence provenance retained.
- [x] **T004.4b2 — Test the actual native client blocks against strict deserialization.**
  Source-only: `5f5f308:TASKS.md`. Source equality/causality manually checked, not live recapture.
- [x] **T004.5a — Remove unused process-log identifiers from readable errors.**
  Source-only: `5f5f308:TASKS.md`.
- [x] **T004.5b — Remove the unproduced OpenView effect alias.**
  Done: `3627f46` (implementation: `b16685d`, `f6a859d`).
- [x] **T004.5b1 — Remove the unproduced Rust OpenView effect variant.**
  Done: `b16685d`.
- [x] **T004.5b2 — Remove client OpenView aliases and reconcile the public contract.**
  Done: `3627f46`.
- [x] **T004.5b2a — Delete client effect aliases and verify native stack behavior.**
  Done: `f6a859d`.
- [x] **T004.5b2b — Reconcile remaining public view-effect documentation.**
  Done: source-only `5f5f308:TASKS.md`; native evidence provenance retained.
- [x] **T004.5c — Consolidate the duplicate recording UI test helper.**
  Done: `b5eb865`. `multiChoice` has consumers, but no producing-command proof; native evidence remains provenance.
- [x] **T002 wire-recorder — Complete.** Done: `77bc34d`; portable logger `e34ce5a`/`f7d4bea`; Unicode redaction `6dd4f7e`. Original acceptance: immutable `61370ad:TASKS_PART_SEVEN.md`; capture/native provenance: `83f79f5:TASKS.md`, `8476a8e`. Runtime regressions: `tests/wirelog.rs` (`make test-rust FILTER="--test wirelog"`); three quirk initialize inputs remain, no old JSONL parser gate. Linux evidence is not Windows/macOS or full native-client proof; see [fixture guide](tests/fixtures/lsp_traces/README.md).
- [x] **T003.1 — Configure Lua and VSCode hygiene entry points.**
  Source-only: `3787c66:TASKS.md`.
- [x] **T003.2 — Add the report-only hygiene runner and focused tests.**
  Source-only: `3787c66:TASKS.md`.
- [x] **T003.3 — Audit hygiene findings and validate unused-dependency detection.**
  Source-only: `3787c66:TASKS.md`.
- [x] **T003.2a — Enforce generated-file filtering and hermetic reporter tests.**
  Source-only: `3787c66:TASKS.md`.
- [x] **T003.4 — Document phase hygiene and complete the tooling contract.**
  Source-only: `3787c66:TASKS.md`; report-only hygiene; Linux evidence only, non-Linux CI follow-ups retained.
  Historical T002 recorder integration, append/restart fixes and native capture records: immutable `83f79f5:TASKS.md` (`15a8f63`, `1563bbb`). Fixture consumers retain three quirk initialize inputs; client declarations are tested separately. No old JSONL/sidecar parser gate remains.
- [x] **T001.5b2 — Replace superseded task and history sections.** Source-only: `3787c66:TASKS.md`.
- [x] **T001.5c — Remove duplicate top matter and retired Plain plans.** Source-only: `3787c66:TASKS.md`; original Plain scope T004–T010/T013/T020–T022, keys T034–T039, deferred T099–T111; immutable sources `2ff1ee7:magit-lsp-plain-client-tasks.md` and `2ff1ee7:magit-lsp-plain-keys-tasks.md`; mapping: [migration ledger](docs/evidence/task-queue-migration.md).
- [x] **T001.4b — Qualify historical visual evidence.** Source-only: `3787c66:TASKS.md`.
- [x] **T001.4c — Isolate human-only README image guidance.** Source-only: `3787c66:TASKS.md`.
- [x] **T001 docs-diet — Complete.**
  Done: `d12d5a4`. Enforced by `git diff --check` and bounded migration assertions. Residuals: A12 ownerless ([ledger](docs/evidence/task-queue-migration.md)); R-EDIT-REAL not green, T104 subset only; binary-platform limits and immutable provenance retained (`cbec0bf:TASKS.md`).
- [x] **Retained gap — A12 real-dispatch Git failure handling.** Original scope and acceptance: `da4d422^:REVIEW_REPORT.md` A12. Migrated to `Session`/stdio seams; `server::mutation` and its callers are absent. `tests/server_async.rs::a12_selection_stage_reports_index_lock_as_successful_error_effect` verifies readable real-dispatch failure effects, unchanged Git state, malformed-input JSON-RPC errors and stderr; `tests/errors.rs` and `tests/dispatch.rs` cover error conversion and dispatch. T103 remains the distinct deferred same-session reader-lock redesign. Immutable provenance: `d04dfa2:TASKS.md`; implementation: `4395f73`, `5bb9903`.
  - Verified: `make test-rust FILTER="--test errors --test dispatch --test views --test server_async"` @ `6688ed1` → ok: rust
- [x] **Retained decision — R-NAV.** Four tree-owned commands verified; immutable evidence: `da4d422:TASKS.md`.
- [x] **T001.3b — Compress completed correctness and editor contracts.** Immutable evidence and enforcing checks retained in `da4d422:TASKS.md`.
- [x] **T001.4a — Migrate versioned VSCode host facts and retire NOTES.** Evidence: `docs/evidence/vscode-host-behaviours.md`.
- [x] **T001.5a — Align design and test documents with the replacement queue.** Source-only: `3787c66:TASKS.md`.
- [x] **T001.5b1 — Preserve immutable per-ticket scope and verification provenance.** Source-only: `3787c66:TASKS.md`; [migration ledger](docs/evidence/task-queue-migration.md).


## Resolved review and bugfix index

Immutable detail and original verification: `d9ea816:TASKS.md`. These records are closed, not execution instructions.

- [x] T006 gate — Wire shared editor models into all fake-editor consumers. — Source-only: `d9ea816:TASKS.md`; Verified refs: `2e29535`.
- [x] Checkpoint — Revalidate VSCode regressions without overlapping server rebuilds. — Source-only: `d9ea816:TASKS.md`.
- [x] Checkpoint — Restore full regressions after strict wire migration. — Done: `d996e19`.
- [x] Checkpoint — Establish the staged-successor failure cause. — Source-only: `d9ea816:TASKS.md`.
- [x] Checkpoint — Align staging-cursor regression with the source-successor contract. — Source-only: `d9ea816:TASKS.md`.
- [x] Checkpoint — Restore log-menu key routing and native passthrough. — Source-only: `d9ea816:TASKS.md`.
- [x] Checkpoint — Restore full Git-backend hunk comparisons. — Source-only: `d9ea816:TASKS.md`.
- [x] Checkpoint — Resolve Rust formatting-gate drift. — Source-only: `d9ea816:TASKS.md`.
- [x] [P1] Correlate scripted prompt responses with real server replies — Source-only: `d9ea816:TASKS.md`; Verified refs: `2e29535`.
- [x] [P1] Enforce static keymap delivery in the strict checker — Source-only: `d9ea816:TASKS.md`; Verified refs: `02b7d19`.
- [x] [P2] Put metadata-only rich rendering in the shared contract vectors — Source-only: `d9ea816:TASKS.md`; Verified refs: `43480bc`.
- [x] [P2] Refresh the T006 accumulated-work gate totals — Source-only: `d9ea816:TASKS.md`; Verified refs: `2e29535`.
- [x] [P1] Pin Neovim stale-apply entry ordering — Source-only: `d9ea816:TASKS.md`; Verified refs: `6c95b50`.
- [x] [P1] Prove the real Plain apply pipeline becomes ready — Source-only: `d9ea816:TASKS.md`; Verified refs: `e46632b`.
- [x] [P2] Repair T005 completion provenance and open-phase wording — Source-only: `d9ea816:TASKS.md`; Verified refs: `ae5b78d`.
- [x] [P2] Make capability-match appendix generation exhaustive — Source-only: `d9ea816:TASKS.md`; Verified refs: `5e61d9f`.
- [x] [P1] Make the secret-only declaration test exercise prompt capabilities — Done: `78b6f49`.
- [x] [P1] Remove retired capability modes from current client contracts — Done: `db288e4`.
- [x] [P1] Report checker failures without calling them findings — Source-only: `d9ea816:TASKS.md`.
- [x] [P1] Model the real VSCode entry graph in Knip — Source-only: `d9ea816:TASKS.md`.
- [x] [P1] Check Lua against Neovim's actual LuaJIT standard — Source-only: `d9ea816:TASKS.md`.
- [x] [P1] Complete marker ownership and baseline reconciliation. — Source-only: `d9ea816:TASKS.md`.
- [x] [P1] Give every retained hygiene finding an owner or reason — Source-only: `d9ea816:TASKS.md`.
- [x] [P2] Make hygiene tests portable and prove checker working directories — Done: `caf5588`, `0550f36`, `33d0554`.
- [x] [P1] Finish native Helix smoke shutdown and recipe verification. — Source-only: `d9ea816:TASKS.md`.
- [x] [P1] Make the Helix capture recipe preserve verifiable evidence — Source-only: `d9ea816:TASKS.md`.
- [x] [P2] Enforce trace and initialize-preset consistency — Source-only: `d9ea816:TASKS.md`.
- [x] [P2] State that five T034 action classes remain missing — Original ref: `e34ce5a`.
- [x] [P1] Serialize shared wirelogs on every tested platform — Source-only: `d9ea816:TASKS.md`.
- [x] [P1] Open shared logs with Windows lock-compatible access — Source-only: `d9ea816:TASKS.md`.
- [x] [P1] Make identity replacement byte-index safe — Source-only: `d9ea816:TASKS.md`.
- [x] [P1] Retain the claimed bundled-Neovim native edit — Source-only: `d9ea816:TASKS.md`.
- [x] Checkpoint review — Opt-in wire recorder and completed documentation cutover. — Source-only: `d9ea816:TASKS.md`.
- [x] [P2] Correlated secret-prompt errors are recorded without redaction. — Source-only: `d9ea816:TASKS.md`.
- [x] Checkpoint review — Git mutation identities and structured successors. — Reviewed ref: `dd92f90cdb11d827606a689a88a280f480d7508d`.
- [x] Checkpoint review — Plain protocol versions and process ownership. — Source-only: `d9ea816:TASKS.md`.
- [x] Checkpoint review — Roadmap migration preserves open requirements. — Source-only: `d9ea816:TASKS.md`.
- [x] [P2] A12 is marked complete in the queue index while its sole retained issue remains open. — Source-only: `d9ea816:TASKS.md`.
- [x] [P1] Enforce exact action-fixture metadata and source-row mappings — Source-only: `d9ea816:TASKS.md`.
- [x] [P1] Add a self-contained Neovim selection-and-paste recipe — Source-only: `d9ea816:TASKS.md`.
- [x] [P2] Harden the Helix action recipe cleanup. — Source-only: `d9ea816:TASKS.md`.
- [x] [P2] Harden the Neovim selection recipe cleanup. — Source-only: `d9ea816:TASKS.md`.
- [x] [P2] Repair rapid-recipe syntax without claiming cleanup acceptance. — Source-only: `d9ea816:TASKS.md`.
- [x] [P2] Harden the Neovim rapid-repair recipe cleanup. — Source-only: `d9ea816:TASKS.md`.
- [x] [P2] Make capture-recipe cleanup survive failures — Source-only: `d9ea816:TASKS.md`.
- [x] [P1] Make the phase-gate schema match the actual queue — Source-only: `d9ea816:TASKS.md`.
- [x] [P2] Restore hygiene-test environment isolation — Source-only: `d9ea816:TASKS.md`.
- [x] [P2] Remove duplicate Linux verification policy — Source-only: `d9ea816:TASKS.md`.
- [x] [P2] Remove stale planned-navigation marker claims — Source-only: `d9ea816:TASKS.md`.
- [x] [P1] Finish terminating the owned Helix recipe server — Source-only: `d9ea816:TASKS.md`.
- [x] [P2] Isolate Helix recipe repository setup from host Git config — Source-only: `d9ea816:TASKS.md`.
- [x] [P1] Finish migrating shared test wire fixtures to the strict schema — Source-only: `d9ea816:TASKS.md`.
- [x] [P2] Test duplicate entries in every typed capability set — Source-only: `d9ea816:TASKS.md`.
- [x] [P2] Anchor the fake StaticFeature APIs to real VSCode evidence — Source-only: `d9ea816:TASKS.md`.
- [x] [P1] Remove retired fields from the public rich-protocol contract — Source-only: `d9ea816:TASKS.md`.
- [x] [P2] Pin the bundled Neovim declaration at its producer boundary — Source-only: `d9ea816:TASKS.md`.
