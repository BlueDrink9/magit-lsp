# magit-lsp — Roadmap

Last review: 84fb4bd; Phase 4 end gate over `92c78fb..84fb4bd` (11 commits, 15 paths, +2655/-321) delivering T012 example-files and T013 client-spec: six recorded `docs/examples/*.jsonl` files with loader/schema validation and replay against the real binary (Plain and rich), generated worked-exchanges and capability-levels blocks in `docs/CLIENT_SPEC.md`, the five client implementation levels with checklists keyed only to existing repo identifiers, the classification rule pinned to `negotiate` and recorded initialize traces, the recorded Neovim `documentChanges` fact, and the README Plain-mode statement naming only tested clients. Serial `make test`, `make hygiene` and `make ui-tour` at this HEAD all exited 0 (`make test` digests `build`/`rust`/`lua`/`vscode-typecheck`/`vscode`/`keybindings`; `ui-tour` wrote 32 scenario sections to `build/ui/tour.md`; `pgrep` shows no orphan servers). `make hygiene` exits 0 but reports three optional tools missing on this host — `cargo-machete`, `knip`, `luacheck` — recorded honestly as advisory, not a clean-tool report. Bounded two-scout review (no reviewer tool available) found no P0-P2: all 70 identifiers cited in the levels docs resolve in the tree, schema drift/tamper and replay/divergence assertions are non-vacuous with exact-after-normalization comparison and no sleeps, and the one suspected Helix test claim was disproven (`tests/plain_harness.rs:139` exists; the doc explicitly disclaims native Helix runs). P3 notes: the bundled-Neovim rich classification test asserts only `Ok(Presentation::Rich(_))` without field-level checks; example divergence coverage flips one frame per presentation; divergence tests couple to the mismatch error-string format. Earlier gate scope follows.

Previous gate: 92c78fb; substantial-chunk gate over `f10d8a0..92c78fb` (16 commits, 69 paths, +10893/-150) delivering T011 schema-generation end to end: tagged Plain action targets, directional payload/method/capability schemas with `update-schema`/`schema-check` drift, generated TypeScript and LuaLS wire types, real client `checkJs` with permanent positive/negative fixtures, generated client-spec payload tables, shared effect/prompt lists consumed by both clients, schema drift in CI, and recorded-fixture validation. Serial `make test` and `make ui-test` at that HEAD both exited 0; success logs deleted, so no per-suite counts are claimed. No P0-P2 found; P3 notes: `RichCapabilityDeclaration` mirrors `MagitCapabilities` (schema-only, constant-bounded); Lua per-client layout list hand-maintained (legitimate).

Gate rationale: the accumulated close-routing, basic-modifier contract, terminal-input and native-fixture repairs since `7fae07c` span 17 paths (+574/-60). Run `make test` and `make ui-test` serially before bounded review; this is a substantial-chunk gate, not a roadmap phase end. Concurrent dirty release/tooling work remains excluded.
Prior gates: Phase 2 `make test`, report-only `make hygiene`, and `make ui-tour` passed at `5da5d86`; `make test` passed again at `ebf9ed1`. Phase 3 `make test` and `make ui-tour` passed at `b4dae94`; `make hygiene` was report-only, with `cargo machete`, `knip` and `luacheck` absent. Focused verification at `bfb60d1`: `make test-rust FILTER="--test plain_draft --test editor_bridge" V=1` passed. No new runtime run for this source review. `make ui-test` previously reported 44 PASS / 5 baseline failures (`common-navigation`, `hunk-body-fold`, `log-select`, `log-view`, `process-log`), reproduced at baseline `2e29535`; all five were repaired in the Bugfix slice below.
Documentation cleanup gate: substantial reference relocation and history reduction warrant a bounded preservation review. Runtime implementation changes belong to a concurrent workstream; this documentation gate uses focused documentation checks, not a redundant runtime/native gate.

Resolved checkpoint detail: immutable `d9ea816:TASKS.md`; not current instructions. `Source-only` labels mark provenance, not implementation commits. Scripted model checks are not real-server conformance; Linux native evidence does not establish Windows/macOS behavior, which remain CI follow-ups. Hygiene remains report-only; findings have owners/reasons as recorded in the hygiene allowlist. T007.1n had a timeout before a passing rerun; retain this transient timeout/rerun risk.

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
- [x] **T008.5b2c — Activate confirm and text prompt rows from Plain documents.** Done: `5564a03`. Offer and execute `prompt.answer` row actions with stale checks. T008 clauses 1–2, 5.
  - Verified: `make test-rust FILTER="--test plain_actions --test prompts"` @ `5564a03` → ok: rust.
  - Check: a captured `prompt.answer` replayed after a repaired title edit returns the stale message and leaves exactly one prompt.
- [x] **T008.5c1 — Reconcile Rich prompt document edits.** Done: `514422d`. Route `didChange` for an open prompt view through `reconcile_prompt_document`; adopt field text without a revert edit and re-render repairs. T008 clauses 1–2, 5.
  - Verified: `make test-rust FILTER="--test server_async"` @ `514422d` → ok: rust.
- [x] **T008.5c2 — Activate confirm and text prompt rows from Rich keys.** Done: `8a02a7e`. Enter on a prompt row answers with the reconciled field text and drives the continuation. T008 clauses 1–2, 5.
  - Verified: `make test-rust FILTER="--test server_async --test prompts"` @ `8a02a7e` → ok: rust. `rich_confirm_row_enter_answers_without_mutation` proves Enter on a non-row leaves the prompt active and Enter on `No` pops it with HEAD/index/worktree unchanged and no `view/prompt`; `rich_text_prompt_submit_row_answers_with_the_reconciled_field` proves a `didChange` field of `Alice` plus Enter on `Submit` pops the prompt and re-renders the menu with the author.
- [x] **T008.6a1 — Answer choice and multiChoice document prompts.** Add Cancel, choice Submit when allowCustom, toggle-and-re-render semantics, Done and custom/default answers. T008 clause 2.
  - Done: `3223e7d`. `PromptDocument::answer` maps every row to a typed `PromptAction`; choice rows answer ids, the untouched default or the custom text, multi-choice item rows toggle and keep the prompt open, and unknown rows change nothing.
  - Verified: `make test-rust FILTER="--test prompts --test plain_prompt"` @ `3223e7d` → ok: rust.
- [x] **T008.7a — Define the typed pick protocol and capability guard.** Pick request/response types (purpose, multi, free text, preselection, item fields, fallbackView); optional rich `pick` declaration; reject every undeclared shape in negotiation tests. T008 clause 3.
  - Done: `d8b36fd`. Typed pick request/response payloads and the optional rich `pick` capability declaration; negotiation tests reject every undeclared shape.
  - Verified: `make test-rust FILTER="--test plain_core --test prompts --test quirks"` @ `d8b36fd` → ok: rust.
- [x] **T008.7b — Declare pick support in both clients.** Lua static `vim.ui.select` and VSCode `showQuickPick` capability blocks plus exact-declaration tests; pick vectors move to T008.8 with the first emitter. T008 clause 3.
  - Done: `6887df2`. Both rich capability blocks declare `pick`; exact-declaration tests assert it. Native pick-presentation evidence is deferred to T008.8, the first ticket that emits a pick; this ticket changes declarations only.
  - Verified: `make test-lua && make vscode-test` @ `6887df2` → ok.
- [x] **T008.8a — Add the Lua native pick transport.** `view/pick` → `vim.ui.select`, single-select guard, `view/pickRespond`, `maxItems`, shared pick vectors. T008 clauses 3, 9.
  - Done: `d4cfc43`. `view/pick` drives `vim.ui.select` and answers `view/pickRespond`; undeclared multi/free-text/over-100 shapes warn once and cancel; `pick.maxItems = 100` is declared and shared pick vectors cover selection, cancellation and a two-item single select.
  - Verified: `make test-file FILE=tests/test_pick.lua && make test-lua` @ `d4cfc43` → ok: lua.
- [x] **T008.8b — Add the VSCode native pick transport.** `showQuickPick` mapping (multi, description/detail, preselection), handler, shared pick vectors. T008 clause 3.
  - Done: `6fcf2c5`. `view/pick` drives `showQuickPick` and answers `view/pickRespond` with item ids; undeclared free-text and grouped shapes warn and cancel; shared pick vectors, preselection/description/detail pass-through and both integration responses are covered.
  - Verified: `make vscode-test` @ `6fcf2c5` → ok: build, vscode, keybindings.
- [x] **T008.10 — Enable Plain discard through the document confirm.** Remove `ConfirmationUnavailable` from `PlainCapabilities::content_action`; `status.discard` is offered and executed in Plain and opens the existing confirm prompt document; cancel changes nothing and confirm discards. Menu option rows already open prompts (T008.5b2b). T008 clause 7.
  - Done: `8ffb45d18c29cfc05441de200fa518ef979caa92`. `status.discard` is a normal Plain content action; cancel restores the status document and confirm restores the worktree file, with no Git change before answering and no `view/prompt`.
  - Verified: `make test-rust FILTER="--test plain_actions --test plain_core"` @ `8ffb45d18c29cfc05441de200fa518ef979caa92` → ok: rust. `plain_discard_opens_a_document_confirm_and_cancel_changes_nothing` and `unavailable_actions_are_omitted_without_disabled_support` cover the rewritten paths.
- [x] **T008.9 — Implement revision pick semantics.** Done: `2c3cf0f5f61159cd7a39272616ecf0abceb9ee44` (via T008.9a–e). Head-reachable log select, free text with rev-parse, cursor preselection, multi, item opens commit, IMM-6, execution-time recheck. T008 clauses 4, 9.
- [x] **T008.9a — Use log selection for commit-message options.** Done: `e177b11`. `Reader::Commit` options (`--reuse-message`, `--reedit-message`) open the log in select mode; confirming returns the commit id to the staged option; cancel returns to the menu unchanged. T008 clause 4.
  - Verified: `make test-rust FILTER="--test prompts --test log_selection --test menus"` @ `e177b11` → ok: rust. `tests/prompts.rs` covers the `=C` pick and `=c` cancel; `tests/log_selection.rs` is unchanged.
- [x] **T008.9b — Validate ref answers and revision values.** Done: `309c83f`. `Reader::Ref` options accept free text, resolve it with `git rev-parse --verify` or `git remote get-url` and report an invalid value with a readable message; `Session::validate_option_value` covers commit and ref values. T008 clauses 4, 9.
  - Verified: `make test-rust FILTER="--test prompts --test menus --test commands"` @ `309c83f` → ok: rust.
- [x] **T008.9b2 — Recheck stored revision values before dispatch.** Done: `60edee5`. A set `Reader::Commit` or `Reader::Ref` menu option that no longer resolves is refused with a warn message and no dispatch. T008 clause 9.
  - Verified: `make test-rust FILTER="--test prompts --test menus --test commands"` @ `60edee5` → ok: rust; `a_stale_commit_option_is_refused_before_dispatch` covers an unreachable picked commit after `reset`/reflog expiry/`gc`.
- [x] **T008.9c1 — Stage ref and remote picks with a document fallback.** `Execution::Pick`, `PendingPrompt.pick`, derived choice prompt, `Reader::Remote`, repo ref/remote item enumeration, and document delivery. T008 clauses 3, 4. Done: `200c5532b4c4ce6b59bb712c7bab82fcaada7c04`. Verified: make test-rust FILTER="--test prompts --test plain_actions --test menus" @ `200c5532b4c4ce6b59bb712c7bab82fcaada7c04` → ok.
- [x] **T008.9c2 — Deliver native picks when declared.** `view/pick` notification, `view/pickRespond` handling, native/document rung order, `maxItems` cut marker, and one-side shape filtering. T008 clauses 3, 9.
  - Done: `0b54901`. A declared pick is delivered as `view/pick` and the answer resumes the staged prompt; undeclared clients keep the document fallback. Two `server_async` tests cover the native answer and the fallback.
  - Verified: `make test-rust FILTER="--test server_async --test plain_core --test prompts"` @ `0b54901` → ok: rust.
- [x] **T008.9c3 — Apply pick preference limits per client.** Done: `fb721e576dfb3d7027fece1856a0c020f3a9f32e` (via T008.9c3a–b2). A user preference inside the declared shape wins; beyond it needs the explicit per-client config key by `clientInfo.name`, otherwise fallback plus a readable message. T008 clause 6.
- [x] **T008.9c3a — Add pick preference configuration.** `PickSettings` with a global `max_items` and per-client `clients`, parsed and layered from `[pick]`; invalid values are rejected with file and line. T008 clause 6.
  - Done: `c7dadc3`.
  - Verified: `make test-rust FILTER="--test config"` @ `c7dadc3` → ok.
- [x] **T008.9c3b1 — Honour the global pick item preference.** Truncate the staged pick to `pick.max_items` when it is inside the declared limit; when it exceeds the limit, use the document picker and report the reason. T008 clause 6.
  - Done: `5f3de54`.
  - Verified: `make test-rust FILTER="--test server_async --test config"` @ `5f3de54` → ok.
- [x] **T008.9c3b2 — Allow a per-client pick limit override.** Use `pick.clients[clientInfo.name]` to allow a native pick beyond the declared limit when explicitly configured; otherwise keep the document fallback and message. T008 clause 6.
  - Done: `fb721e576dfb3d7027fece1856a0c020f3a9f32e`.
  - Verified: `make test-rust FILTER="--test server_async --test config"` @ `fb721e576dfb3d7027fece1856a0c020f3a9f32e` → ok.
- [x] **T008.9d — Activate choice and multiChoice rows from Plain documents.** Offer and execute item, Submit, Done and Cancel rows with stale checks; end-to-end via the T008.9c pick. T008 clause 2.
  - Done: `9f0a70e20eb7a60c6049dc271c860fcd51db4f36`.
  - Verified: `make test-rust FILTER="--test plain_actions --test prompts"` @ `9f0a70e20eb7a60c6049dc271c860fcd51db4f36` → ok.
- [x] **T008.9e — Activate choice and multiChoice rows from Rich keys.** Enter on item rows toggles, Done/Cancel closes, Submit returns the custom field; end-to-end via the T008.9c pick. T008 clause 2.
  - Done: `2c3cf0f5f61159cd7a39272616ecf0abceb9ee44`.
  - Verified: `make test-rust FILTER="--test server_async --test prompts"` @ `2c3cf0f5f61159cd7a39272616ecf0abceb9ee44` → ok.
- [x] **T008.11 — Align client prompt behavior with shared vectors.** Lua custom-choice prefill and empty-secret decision; VSCode parity. T008 clause 8.
  - Done: `98a0a4c`. Custom choice clears the item-id default before the text prompt; empty secret answers cancel in both clients.
  - Verified: `make test-lua && make vscode-test && make docs-check` @ `98a0a4c` → ok.
- [x] **T008.12 — Generate docs/plain-support.md from the command registry.** Done: `ebe11b7` (via T008.12a–c). Recording-Ui matrix, `Unavailable` reasons, maintained target and staleness check. T008 clause 10.
- [x] **T008.12a — Type command effect declarations and add plain_availability.** Replace `required_effect` with `effects: &'static [EffectKind]`; one availability function folds the content-action rule and drives both Rich and Plain gating. T008 clause 10.
  - Done: `f717778`.
  - Verified: `make test-rust FILTER="--test plain_core --test plain_actions --test commands --test editor_bridge --test log_selection"` @ `f717778` → ok.
- [x] **T008.12b — Generate and check docs/plain-support.md.** Done: `e87a50c0eaabaa35384eb50628aabde5ab8ecc88`. Registry-derived table across minimal/edits/full capability profiles, a maintained update target, and a staleness test. Verified: `make test-rust FILTER="--test plain_support"` @ `e87a50c` → ok. T008 clause 10.
- [x] **T008.12c — Enforce declarations and test unsupported effects.** Assert emitted optional effects are declared; exercise each effect-declaring command against RecordingUi with and without the capability, expecting `Unavailable{reason}` or a readable message and clean cancel. T008 clause 10. Done: `ebe11b7`.
  - Verified: `make test-rust FILTER="--test commands --test plain_core --test plain_actions --test editor_bridge"` @ `ebe11b7` → ok.
- [x] **T008.13 — Add the mixed-rung conformance test.** Done: `b549480` (T008.13a), `6179b54` (T008.13b). Cancel at each step; no Git/index change; caller returns; view-stack depth unchanged. T008 clause 11.
- [x] **T008.13a — Test native-pick cancellation in the mixed-rung flow.** Declare native `pick` without native `confirm`; cancel the `=u` upstream native `view/pick` via `view/pickRespond`; assert caller returns, option stays unset, no Git/index change and view-stack depth is unchanged. T008 clause 11.
  - Done: `b549480`. `cancelled_native_pick_on_the_mixed_rung_leaves_the_menu_and_repo_unchanged` in `tests/server_async.rs` covers the cancel, the unchanged option, HEAD/index/worktree/lock, and the usable menu view.
  - Verified: `make test-rust FILTER="--test server_async cancelled_native_pick_on_the_mixed_rung" V=1` @ `b01ce98` → 1 passed.
- [x] **T008.13b — Test document-confirm cancellation after a native pick in the mixed-rung flow.** In the same mixed-rung client (`pick` declared, `confirm` undeclared), answer the `=u` native `view/pick`, trigger `commit.create` to open the document `confirm` prompt, and cancel it on the `No` row; assert the prompt pops, caller returns, no Git/index change and view-stack depth is unchanged. T008 clause 11.
  - Done: `6179b54`. `cancelled_document_confirm_after_a_native_pick_on_the_mixed_rung_leaves_the_repo_unchanged` in `tests/server_async.rs` answers the native pick, opens the document confirm, cancels on the `No` row, and checks the pop, no `editDocument`/`view/prompt`, HEAD/index/worktree/lock, and the usable status view.
  - Verified: `make test-rust FILTER="--test server_async mixed_rung" V=1` @ `6179b54` → 2 passed.
- [x] **T009.1 — Render the log select view as a Plain document.** The `log.select` scope renders into the single Plain document URI with its header, prompt and commit rows; `Reader::Commit` menu options open the same selection in Plain mode, and an already-set option is cleared as in Rich mode. T009 clauses 1, 5.
- [x] **T009.2 — Offer confirm, cancel and toggle-mark as code actions.** `common.confirm`, `common.cancel` and `select.toggle-mark` are offered on `log.select` documents; the commit identity comes from `ActionTarget` (`node_id` + `mutation_hash`) over structured commit nodes, never from rendered text. T009 clauses 2, 4.
  - Done: `plain_log_select_offers_commit_code_actions` in `tests/plain_select.rs` offers `common.confirm`, `common.cancel` and `select.toggle-mark` on a `log.select` document; the confirm target carries the structured `log:commit:<oid>` node id and a mutation hash, and confirming copies the commit id to the clipboard.
  - Verified: `make test-rust FILTER="--test plain_select" V=1` @ `8280e85` → 3 passed.
- [x] **T009.3 — Use a field for free-text revisions.** `log.select` renders a one-line `NodeKind::Field` for the revision; document edits to that field are reconciled with the existing field rules (single line, 4096-byte limit, no stray CR), and confirming resolves the revision with `git rev-parse --verify`. T009 clause 3.
  - Done: `plain_log_select_revision_field_drives_confirm` and `plain_log_select_invalid_revision_changes_nothing_and_keeps_the_picker` in `tests/plain_select.rs` edit the `log:field:rev` field via `textDocument/didChange`; a symbolic revision resolves to the full commit id and an invalid revision changes nothing.
  - Verified: `make test-rust FILTER="--test plain_select" V=1` → 5 passed.
- [x] **T009.4 — Return the caller on confirm and cancel.** Confirming a selection returns the chosen commit ids to the staged continuation and restores the caller view; cancelling restores the caller without mutation. T009 clause 5.
  - Done: `plain_log_select_confirm_returns_the_caller_menu_with_the_chosen_commit` and `plain_log_select_cancel_returns_the_caller_without_changes` in `tests/plain_select.rs` stage the `=C` menu option through a Plain `log.select`, confirm a commit row back into `--reuse-message=<oid>` and cancel back to the unchanged, usable menu.
  - Verified: `make test-rust FILTER="--test plain_select" V=1` @ `70d617d` → 7 passed; `--test log_selection` → 12 passed.
- [x] **T009.5 — Reject a stale selection without changes.** A `log.select` action carrying a stale `view_generation`, `node_id` or `mutation_hash` is refused with a readable message and changes nothing. T009 clause 6.
- [x] **T009.6 — Add the Plain select conformance tests.** `tests/plain_select.rs` covers the Plain log select document, code actions, the free-text field, stale rejection, and the fixup scenario: select a fixup target, then in a second run cancel; the second run changes nothing. T009 clause 7.
  - Done: `plain_log_select_fixup_target_survives_a_cancelled_second_run` in `tests/plain_select.rs` confirms a fixup-purpose `log.select` target, then opens a second fixup-purpose selection and cancels it; the cancel emits no clipboard, restores the caller document and leaves HEAD, the index and the worktree untouched while the first chosen commit id stays the same.
  - Covering tests: clause 1 document `plain_commit_menu_option_renders_the_log_select_document`; clause 2 code actions `plain_log_select_offers_commit_code_actions`; clause 3 free-text field `plain_log_select_revision_field_drives_confirm`, `plain_log_select_invalid_revision_changes_nothing_and_keeps_the_picker`; clause 4 `ActionTarget` identity `plain_log_select_offers_commit_code_actions`, `plain_log_select_tampered_target_is_refused_without_changes`; clause 5 caller return `plain_log_select_confirm_returns_the_caller_menu_with_the_chosen_commit`, `plain_log_select_cancel_returns_the_caller_without_changes`, `plain_already_set_commit_option_clears_and_restores_the_menu`; clause 6 stale rejection `plain_log_select_stale_generation_is_refused_without_changes`, `plain_log_select_tampered_target_is_refused_without_changes`; clause 7 fixup `plain_log_select_fixup_target_survives_a_cancelled_second_run`.
  - Verified: `make test-rust FILTER="--test plain_select" V=1` @ `838203a` → 10 passed.
- [x] **T010.1 — Replace an open Plain commit buffer with the new template.** When the client declares `showDocument` and the document store shows the `COMMIT_EDITMSG` URI is already open, send a versioned `applyEdit` that replaces the whole buffer with the new template before sending `window/showDocument` with `takeFocus`; when the URI is not open, send `window/showDocument` only. Identify the document by its canonical path and URI, never by `languageId`. T010 clause 1.
  - Verified: `make test-rust FILTER="--test plain_draft --test editor_bridge" V=1` @ `d4f8e09` → ok: rust (27 editor_bridge + 3 plain_draft passed; manual instructions path asserted in `manual_plain_commit_instructions_have_no_apply_edit`).
- [x] **T010.2 — Cancel a Plain commit run when no document opens.** If no `didOpen` arrives for the issued `COMMIT_EDITMSG` URI in a fixed time, cancel the run and show a readable message; mark the commit commands `Unavailable`. T010 clause 3. Done: `9537e52`. Verified: `make test-rust FILTER="--test plain_draft --test editor_bridge" V=1` @ `9537e52` → ok: rust (27 editor_bridge + 5 plain_draft).
- [x] **T010.3 — Bind Plain commit Confirm and Abort to the exact version.** Confirm is a code action that uses the latest synchronized content at the exact version; a close confirms the saved file; Abort is a code action running `common.cancel {runId}` and showing the status view; a close that arrives after an abort is ignored; unrelated documents are rejected. T010 clause 4.
  - Done: `stale_explicit_confirm_is_refused_and_keeps_the_draft_pending`, `explicit_abort_creates_no_commit_and_restores_the_status_view`, `close_after_abort_is_ignored` and `unrelated_document_notifications_are_rejected_without_touching_the_draft` in `tests/plain_draft.rs`; a Plain client's unrelated document notifications are refused, while its workspace repository's commit message file stays ownable before a run.
  - Verified: `make test-rust FILTER="--test plain_draft --test editor_bridge" V=1` @ `51da1b7` → ok: rust (27 editor_bridge + 9 plain_draft).
- [x] **T010.4a — Cover save-then-close and dirty Discard.** `tests/plain_draft.rs` covers a saved file confirmed by close, and a dirty buffer discarded on close using the prior saved content. T010 clause 5 (save then close; dirty Discard).
  - Done: `saved_file_close_commits_the_disk_text` and `dirty_close_commits_the_prior_saved_disk_text` in `tests/plain_draft.rs`.
  - Verified: `make test-rust FILTER="--test plain_draft --test editor_bridge" V=1` @ `f930ff7` → ok: rust (27 editor_bridge + 11 plain_draft).
- [x] **T010.4b — Cover dirty Save and dirty Cancel.** `tests/plain_draft.rs` covers a dirty buffer saved without a commit, and a dirty buffer cancelled on close while the buffer stays open. T010 clause 5 (dirty Save; dirty Cancel).
  - Done: `dirty_save_creates_no_commit_and_keeps_the_draft_pending` and `dirty_cancel_keeps_the_draft_pending_and_the_editor_open` in `tests/plain_draft.rs`.
  - Verified: `make test-rust FILTER="--test plain_draft --test editor_bridge" V=1` @ `829eceb` → ok: rust (27 editor_bridge + 13 plain_draft).
- [x] **T010.4c — Cover an empty commit message.** `tests/plain_draft.rs` covers an empty saved message where Git aborts the commit and the server reports it. T010 clause 5 (empty text saved).
  - Done: `empty_saved_message_is_refused_by_git_and_reported` in `tests/plain_draft.rs`.
  - Verified: `make test-rust FILTER="--test plain_draft --test editor_bridge" V=1` @ `aeb7ec7` → ok: rust (27 editor_bridge + 14 plain_draft).
- [x] **T010.4d — Cover a second session.** `tests/plain_draft.rs` covers a second Plain session getting no draft actions for another session's draft. T010 clause 5 (a second session gets no actions).
  - Done: `second_session_gets_no_draft_actions_and_cannot_forge_confirm` in `tests/plain_draft.rs`; a second Plain session pointed at the same repository offers no Confirm/Abort for the first session's draft, and a forged Confirm/Abort carrying that run's identity is refused without a commit.
  - Verified: `make test-rust FILTER="--test plain_draft --test editor_bridge" V=1` @ `0345fff` → ok: rust (27 editor_bridge + 15 plain_draft).

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


## Bugfix

- [x] **Repair Phase 5 recorded example drift.** The substantial-chunk gate at `4e25db9` failed `plain_examples_replay_against_the_real_binary` (`build/test-logs/rust.ZfjZ4r`). Reconcile recorded examples with intentional command registry changes without weakening exact replay validation.
  - test: recorded examples replay against the real binary with exact normalized frame equality.
  - Verification: `make test-rust FILTER="--test examples"`.
  - Verified: `make test-rust FILTER="--test examples"` @ `4e25db9` → ok: rust. Updated intentional code-action ordering and `commit.fixup` title (`Create a fixup commit`) in both fixtures; exact normalized frame equality retained. Mismatch diagnostics report first JSON path.

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

### Feature — Phase 2 (acceptance and recorded final gate complete)

T005 is complete through its authoritative [completed children](#completed-work); full scope and original acceptance: [T005](./TASKS_PART_SEVEN.md#t005-quirks-table-m).
- [x] **T006 — fake-editor-model.** [Specification](./TASKS_PART_SEVEN.md#t006-fake-editor-model-l).
  - Done: containing commit.
- [x] **T007 — plain-capability-fixes.** [Specification](./TASKS_PART_SEVEN.md#t007-plain-capability-fixes-m). Done: containing commit; all six original clauses and native evidence are audited under T007.5. Gate evidence is recorded above.

### Feature — Phase 3: Plain interaction (complete; gate recorded)

- [x] **T008 — prompts-and-picks.** [Scope](./TASKS_PART_SEVEN.md#t008-prompts-and-picks-l). Done: `6957647`..`6179b54` (via T008.1–T008.13b). T008 clauses 1–11.
- [x] **T009 — stable-id-selection.** [Scope](./TASKS_PART_SEVEN.md#t009-stable-id-selection-m). Done: `8280e85`..`cb8423c` (via T009.1–T009.6). T009 clauses 1–7.
- [x] **T010 — plain-commit-editing.** [Scope](./TASKS_PART_SEVEN.md#t010-plain-commit-editing-s). Done: `7c62e6c`..`600411d` (via T010.1–T010.4d). T010 clauses 1–5.

### Feature — Phase 4: Specification

- [x] **T011 — schema-generation.** [Scope](./TASKS_PART_SEVEN.md#t011-schema-generation-m).
  - Done: 92c78fb (containing commit of the last slice). Delivered across children: tagged Plain action targets (T011.1 `e6db680`), directional Plain payload schemas + `update-schema`/`schema-check` drift targets (T011.2 `f5cd1dc`), effect/menu/render schemas (T011.3 `1fcfce5`), the 16 view payload roots (T011.4 `ba42672`), the hand-maintained method catalog (T011.5 `5887917`), generated TypeScript declarations (T011.6 `c7771d5`/`66b8f9f`), real `checkJs` client checking (T011.7 `2b968a6` + T011.9 `ff550b0`, combinator-sibling fix T011.8 `d5573c2`), generated LuaLS annotations (T011.10 `f33bf3c`), shared effect/prompt lists generated from the typed capability block and consumed by both clients with key translators/token maps still separate (T011.11 `1abac39`, T011.12 `18c9d4a`), schema drift in CI (T011.13 `395541a`), generated client-spec payload tables (T011.14 `db870f0`) and recorded-capability fixture validation against a generated `rich-capabilities.json` schema (T011.15 `92c78fb`). Acceptance 5's example-file half is delivered by T012, which owns the creation of `docs/examples/` and validates each file against these schemas in `tests/examples.rs`; no example files existed before it.
  - [x] **T011.11 — Generate the shared effect and prompt lists from the typed block.** Expose the optional effect and prompt-kind names from the server's capability validation as public constants, and emit them as drift-checked generated artifacts both clients can consume.
  - [x] **T011.12 — Consume the generated shared lists in both clients.** Emit client-loadable generated files for the shared optional-effect and prompt-kind lists, and replace each client's hardcoded declaration with them.
    - test: generated client files participate in drift detection, and each client's capability declaration is proven equal to the server constants through its own existing suite (Lua unit and VSCode checks).
    - check: `make update-schema`, `make schema-check`, `make test-lua` and `make vscode-test` pass. Key translators and token mappings stay hand-maintained per client; runtime behavior and declared capabilities are unchanged.
      - Verified: make update-schema && make schema-check && make test-lua && make vscode-test; make test-rust FILTER="--test schema" @ 1abac39 → ok
    - Done: containing commit.
    - test: generated artifacts match the typed constants exactly (visitFile/clipboard/editDocument; confirm/text/choice/multiChoice/secret) and missing or modified output fails drift checks.
    - check: `make update-schema`, `make schema-check` and focused schema tests pass. Client consumption, key translators and token mappings stay separate follow-ups.
      - Verified: make update-schema && make schema-check && make test-rust FILTER="--test schema" @ f33bf3c → ok
  - [x] **T011.13 — Run schema drift checks in CI.** Add the generated-schema drift target to the maintained CI workflow so committed artifacts cannot silently diverge.
    - check: the tracked CI workflow runs the drift target in the existing test job; the untracked local release workflow and all unrelated user files stay untouched.
      - Verified: make schema-check @ 18c9d4a → ok
  - [x] **T011.15 — Validate recorded capability fixtures against generated schemas.** Generate a schema for the `capabilities.experimental.magit` declaration and check every recorded fixture carrying one against it.
    - test: each recorded fixture's `magit` declaration validates against the generated capability schema, and a deliberately corrupted fixture copy fails validation in a test.
    - check: `make update-schema`, `make schema-check` and focused schema tests pass. Example-file validation remains owned by T012, which creates those files.
      - Verified: make update-schema && make schema-check && make test-rust FILTER="--test schema" @ db870f0 → ok
  - [x] **T011.14 — Generate client-spec payload tables from the schemas.** Replace the hand-maintained `Core payload quick reference` prose in `docs/CLIENT_SPEC.md` with generated payload tables derived from the committed directional schemas and method catalog.
    - test: the generated tables are proven current by a drift test that regenerates them and compares the committed block, and a tampered or missing block fails; existing doc checks keep passing.
    - check: `make update-schema`, `make schema-check` and focused schema tests pass. Keep hand-written non-payload guidance (lifecycle, presentation, checklists) untouched; only payload field/type/required content is generated.
      - Verified: make update-schema && make schema-check && make test-rust FILTER="--test schema" && make docs-check @ 395541a → ok
    - Done: containing commit.
  - [x] **T011.10 — Generate LuaLS wire annotations from directional schemas.** Emit `lua/magit-lsp/types.lua` through the existing schema generation/drift pipeline, with namespaced definitions that preserve input/output contracts.
    - test: generated annotations cover every payload root and retain literal tags, required/optional fields, refs, union/intersection constraints, arrays and unconstrained JSON; missing or modified output fails drift checks.
    - check: `make update-schema`, `make schema-check` and focused schema tests pass. Do not alter runtime client behavior or merge independent key/token maps.
      - Verified: make update-schema && make schema-check && make test-rust FILTER="--test schema" @ 2b968a6 → ok
  - [x] **T011.9 — Establish checked VSCode wire boundaries.** Commit the production `checkJs` configuration and generated-type annotations as a prerequisite to T011.7's negative fixture and full-suite verification.
    - check: `make vscode-typecheck` passes over all production JavaScript with real Node/VSCode declaration dependencies and generated payload types in handlers; preserve existing runtime semantics, with no new missing-activation fallback or stricter tab-URI rejection.
      - Verified: make vscode-typecheck @ d5573c2 → ok
  - [x] **T011.8 — Preserve schema siblings beside TypeScript combinators.** Fix declaration generation so constraints such as `SerializedItem.activationSteps` beside `oneOf`/`anyOf`/`allOf` are not dropped.
    - test: real generated menu-item declarations require `activationSteps`, and combinator sibling constraints remain present; deterministic drift checks still cover the regenerated declarations.
    - check: `make update-schema`, `make schema-check` and focused schema tests pass. Prerequisite discovered while typing the VSCode consumer; do not fold consumer setup into this fix.
      - Verified: make update-schema && make schema-check && make test-rust FILTER="--test schema" @ 66b8f9f → ok
  - [x] **T011.7 — Type-check VSCode JavaScript against generated wire declarations.** Add `tsc --checkJs` to `make vscode-test`, including production `extension.js` and `lib/*.js`, with generated payload types at wire boundaries.
    - test: the maintained client type check accepts valid payloads and rejects a concrete malformed wire payload in a permanent compile-time fixture; generated declarations are consumed by production handlers, not merely checked in isolation.
    - check: `make vscode-test` passes. Keep runtime behavior and declared capabilities unchanged; use explicit boundary types, not blanket suppressions or hand-maintained duplicate wire interfaces.
      - Verified: make vscode-test @ ff550b0 → ok: build, vscode-typecheck, vscode, keybindings; `clients/vscode/test/types.js` consumes `ViewRender`/`Menu`/`ViewPromptParams` through `lib/render.js`, `lib/menu.js` and `lib/prompt.js`, and `@ts-expect-error` rejects a menu item without `activationSteps` (TS2578 when it becomes accepted) and an invalid `ViewMode`.
    - Done: containing commit.
  - [x] **T011.6 — Generate TypeScript wire declarations from directional schemas.** Emit `clients/vscode/types/protocol.d.ts` through the existing schema generator, preserving per-root directional definitions and including the method catalog's params/result mapping.
    - test: generated declarations preserve required/optional fields, literal tags, unions/intersections, arrays, refs, arbitrary string view kinds and unconstrained JSON; missing or modified declarations fail drift checks.
    - check: `make update-schema`, `make schema-check` and focused schema tests pass. Wiring `tsc --checkJs` into the client remains the next independently verifiable slice.
      - Verified: make update-schema && make schema-check && make test-rust FILTER="--test schema" @ 5887917 → ok
    - Done: containing commit.
  - [x] **T011.5 — Catalog custom protocol methods and their payload schemas.** Add the hand-maintained typed method table and generate its deterministic catalog under `docs/schema/`, referencing the existing directional payload roots.
    - test: cover every custom view request and notification, checking names against protocol trait constants or notification constants, correct directions, payload references and explicit request results. Keep generic `Value` results unconstrained unless the runtime contract proves a narrower shape.
    - check: `make update-schema`, `make schema-check` and focused schema tests pass; the catalog participates in missing/tampered artifact detection. Standard LSP envelopes and `magit.command` variants remain for the fixture-validation slice.
      - Verified: make update-schema && make schema-check && make test-rust FILTER="--test schema" @ ba42672 → ok
    - Done: containing commit.
  - [x] **T011.4 — Schema remaining custom view payloads.** Generate directional schemas for the remaining view request/response/notification payloads, including prompt and pick answers and keymap bindings.
    - test: Prompt unions remain `anyOf`; `Option<Value>` remains unconstrained JSON; custom view kinds remain strings; every custom method payload has an artifact with the correct direction.
    - check: Schema drift and focused protocol/prompt/pick/keymap suites pass without changing wire behavior.
      - Verified: make update-schema && make schema-check && make test-rust FILTER="--test schema --test server_async --test prompts --test plain_prompt --test keymap" @ 1fcfce5 → ok
    - Done: containing commit.
  - [x] **T011.3 — Schema server-rendered effects and menus.** Extend generation to `Effect`, `Menu` and `ViewRender`, accounting for string-serialized `ViewKind` and the flattened menu-item `activationSteps` serialization.
    - test: Schemas describe real serialized menu switch/option/action items and effects; arbitrary view-kind strings stay strings; output activation steps are required without imposing them on deserialization input.
    - check: Generated artifact drift and focused existing menu/view suites pass without wire changes.
      - Verified: make update-schema && make schema-check && make test-rust FILTER="--test schema --test menu --test menus --test views" @ f5cd1dc → ok
    - Done: containing commit.
  - [x] **T011.2 — Generate directional schemas for Plain command payloads.** Add the typed generator and Make update/drift targets, initially covering `PlainActionParams`, `EditConfirmParams`, `EditAbortParams`, `EditConfirmation` and `EditAbortResult`. This is a schema foundation, not the full method catalog or T011 completion.
    - Done: containing commit.
    - test: Deterministic committed JSON Schemas use deserialization contracts for input and serialization contracts for output; required target tags and confirmation names match Rust serde behavior; drift fails without updating artifacts.
    - check: Update/check targets run through Make; pre-existing Makefile edits remain unstaged and unchanged.
      - Verified: `make update-schema && make schema-check && make test-rust FILTER="--test schema --test dispatch"` @ `e6db680` → ok: update-schema, schema-check, rust.
  - [x] **T011.1 — Tag Plain action targets explicitly.** Replace the untagged target union with a required camelCase `kind` discriminator (`content`, `view`, `node`, `menuItem`); migrate every offered target and the maintained client contract without accepting legacy shapes.
    - Done: containing commit. `PlainActionTarget` is internally tagged with required `kind`; every offered action and the client spec carry it, and untagged input is rejected.
    - test: Round-trip every variant, reject missing/unknown/mismatched tags, and exercise offered content/view/node/menu actions through existing Plain integration suites.
    - check: Preserve retained-render identity validation and stale/tampered-target refusal; no client reconstruction or compatibility path.
      - Verified: `make test-rust FILTER="--test dispatch --test plain_actions --test plain_select --test plain_stack --test plain_harness"` @ `86d12d3` → ok: rust. `plain_action_target_roundtrips_each_tagged_variant` asserts the exact content/view/node/menuItem shapes; `plain_action_target_rejects_missing_unknown_and_mismatched_tags` rejects all three failure classes; existing Plain suites exercise offered actions and stale/tampered refusal unchanged.
- [x] **T012 — example-files.** [Scope](./TASKS_PART_SEVEN.md#t012-example-files-s).
  - Done: 1e83443 (containing commit of the last slice). Delivered across children: the JSONL line format, loader and per-line schema validation with the Plain open example (T012.1 `f118882`), the Plain code-action and document-prompt examples (T012.2 `da056e3`), the rich open/render/ack/key recording with progress order (T012.3 `153d57b`), the commit edit confirm and abort examples (T012.4 `a2e06c0`), c2s replay against the real binary for the Plain and commit examples (T012.5 `5f305aa`), and rich replay with deterministic id/URI/version/run-id normalization plus first-seen 40-hex hash mapping, a rich divergence test and no sleeps (T012.6 `1e83443`).
  - [x] **T012.1 — Example format, loader and schema validation with the Plain open example.** Define the JSONL line format (`dir`, `message`, `note?`, `source`), the loader, per-line validation against the generated schemas, and the first example file covering minimal initialize and a Plain open.
    - test: every line in every example file parses, `dir`/`source` are legal, custom-method params/results validate against the matching directional schema from the method catalog, and a corrupted example line fails validation.
    - check: `make test-rust FILTER="--test examples"` passes. Standard LSP framing (initialize, notifications without custom payloads) is envelope-checked only, honestly labeled; the recorded live replay against the real binary is a later slice.
      - Verified: `make test-rust FILTER="--test examples"` @ `92c78fb` → ok: rust. `committed_example_files_parse_and_validate_against_the_method_catalog` validates `docs/examples/plain-open.jsonl` against the method catalog; `corrupted_example_lines_fail_validation` rejects an illegal `dir`, a missing `source`, schema-violating params, a wrong-direction custom method and a result missing a required field. `make test-rust FILTER="--test schema"` also stayed green after extracting the shared validator.
  - [x] **T012.2 — Plain code-action and document-prompt examples.** Record the code-action stage (Plain status document, code actions, executeCommand with tagged targets) and a document prompt exchange, each as a `docs/examples/` file validated by the existing suite.
    - test: every line in each new file passes the T012.1 loader/validator, and the files exercise the tagged `kind` targets and prompt request/response shapes.
    - check: `make test-rust FILTER="--test examples"` passes. Lines are recorded from a real TestServer run, not invented.
      - Verified: `make test-rust FILTER="--test examples"` @ `f118882` → ok: rust. The shared loader validates both new files; `plain_code_action_example_records_tagged_targets_and_execution` pins the offered `content`/`node` targets and the `status.stage` executeCommand, and `plain_document_prompt_example_records_the_prompt_document_exchange` pins the prompt document render, the absence of `view/prompt`, the prompt view effect and the `prompt.answer` node target.
  - [x] **T012.3 — Rich open/render/ack/key loop example with progress order.** Record the rich client lifecycle (initialize with `experimental.magit`, view/open, render, rendered ack, key, progress ordering) as one example file.
    - test: every line passes the loader/validator, and the file pins the documented progress-before/after-response ordering claims.
    - check: `make test-rust FILTER="--test examples"` passes. Lines are recorded from a real TestServer run.
      - Verified: `make test-rust FILTER="--test examples"` @ `da056e3` → ok: rust. `docs/examples/rich-lifecycle.jsonl` is recorded from a real rich TestServer run (`initialize` with the v1 `experimental.magit` declaration, `view/open`, `view/render` v1, `view/rendered` ack, `view/key` `S`/`status.stage-all`, progress handshake, refreshed renders v2/v3, ack v3, shutdown); temporary paths are normalized to `/repo` and outbound frames gain the normalized `jsonrpc`. `rich_lifecycle_example_pins_the_progress_and_render_order` pins create → begin → end → key response → refresh render, the shared `magit-lsp/mutation/1` token, and the `Git`/`view/key` begin payload. Divergence found for T013 review: a fast `g` refresh (`common.refresh`) emits `window/workDoneProgress/create` only — no visible begin/end — because the refresh finishes and `end_progress` runs before the client's create response marks the run visible, so CLIENT_SPEC's "progress end precedes the key response" example does not hold for that key; the recorded file uses a mutation key instead. CLIENT_SPEC prose is left for T013.
  - [x] **T012.4 — Commit edit confirm and abort example.** Record the real-file commit editing exchange (draft open/applyEdit/confirm and the abort variant) as example files.
    - test: every line passes the loader/validator, covering both the confirm completion and the abort cancellation paths.
    - check: `make test-rust FILTER="--test examples"` passes. Lines are recorded from a real TestServer run.
      - Verified: `make test-rust FILTER="--test examples"` @ `153d57b` → ok: rust (8 tests). `docs/examples/commit-edit-confirm.jsonl` and `docs/examples/commit-edit-abort.jsonl` are recorded from real Plain TestServer runs (status open, `status.stage`, `open.commit`, the Commit menu item, the pre-opened draft replaced by a versioned `workspace/applyEdit` template, `window/showDocument`, draft reopen, the message `didChange`, the offered run-bound `common.confirm`/`common.cancel`, then `type: "confirmed"` with the committed subject or `type: "aborted"` with no commit); temporary paths are normalized to `/repo`, the server binary path to `/magit-lsp/target/debug/magit-lsp`, opaque run ids to `run-1`, and outbound frames gain the omitted `jsonrpc`. `commit_edit_confirm_example_records_the_draft_and_its_confirmation` pins the delivered template, the offered synchronized confirm and its `confirmed` completion; `commit_edit_abort_example_records_the_cancellation` pins the offered abort and its `aborted` completion without a commit report.
  - [x] **T012.5 — Replay client-to-server example lines against the real binary.** A harness test sends each example's `c2s` lines (with repository fixture substitution) to the real server and compares normalized `s2c` lines.
    - test: replaying each example reproduces its recorded `s2c` frames after deterministic normalization (ids, URIs, versions), and a deliberately divergent expectation fails.
    - check: `make test-rust FILTER="--test examples"` passes. Normalization rules are documented in the test; flaky ordering is not papered over with sleeps.
      - Verified: `make test-rust FILTER="--test examples"` @ `a2e06c0` → ok: rust, four consecutive runs. `plain_examples_replay_against_the_real_binary` sends the recorded `c2s` frames of `plain-open`, `plain-code-action`, `plain-document-prompt`, `commit-edit-abort` and `commit-edit-confirm` to a real `TestServer` over per-file fixtures and compares every `s2c` frame in order; `a_divergent_example_expectation_fails_the_replay` corrupts a recorded status render and asserts the mismatch. Documented normalization: the temporary repository root becomes `/repo`, the server binary path becomes `/magit-lsp/target/debug/magit-lsp`, outbound frames gain the `jsonrpc` member the server omits, the client's replies are retargeted to the actual `magit-lsp/outbound/N` request id and run id, and opaque commit run ids become `run-1`; versions and all other values stay exact. Recordings that exclude the dynamic `view/keymap` push (the code-action and commit files) skip only those notifications; the prompt recording keeps its keymap in strict order. No sleeps: each phase ends when its recorded frames arrive.
      - `docs/examples/commit-edit-confirm.jsonl` was re-recorded from a fresh run: its old completion tail captured a transient post-confirm status render that only collapsed the diff, which the current server coalesces into one clean-status edit. The replacement lines carry the fresh run's values.
    - Follow-up: rich replay was delivered by T012.6.
  - [x] **T012.6 — Replay the rich-lifecycle example against the real binary.** Extend the replay harness to the rich example file (initialize with `experimental.magit`, view/open, render, ack, key with progress ordering).
    - test: the replay reproduces the recorded rich `s2c` frames after deterministic normalization including commit hashes mapped by position (40-hex → `hash-N`), ids, URIs and versions; a divergent rich expectation fails.
    - check: `make test-rust FILTER="--test examples"` passes. No sleeps; phase-scoped membership where independent notifications can reorder.
      - Verified: `make test-rust FILTER="--test examples"` @ `5f305aa` → ok: rust (12 tests). `plain_examples_replay_against_the_real_binary` now replays `docs/examples/rich-lifecycle.jsonl` over the same fixture as `plain-open`, answering `window/workDoneProgress/create` by the observed request id and sending the recorded `view/rendered` acks, and compares every rich `s2c` frame in order. `canonical_frame` normalizes both sides with independent `HashNormalizer` maps that turn standalone 40-hex commit/object hashes into `hash-1`, `hash-2`, ... by first-seen position while leaving the 64-hex patch content hashes exact; ids, URIs and versions keep their recorded form. `a_divergent_rich_expectation_fails_the_replay` corrupts the initial rich render and asserts the mismatch. Frames stay in strict order with no sleeps: the server emits each render before its keymap, so the rich recording needs no phase grouping.
- [x] **T013 — client-spec.** [Scope](./TASKS_PART_SEVEN.md#t013-client-spec-l).
  - Done: 7169349 (containing commit of the last slice). Delivered across children: the generated worked exchanges (T013.1 `a62d659`), the five client implementation levels (T013.2 `648b5e3`), and the generated capability-to-level table, classification rule, recorded Neovim `documentChanges` fact and README Plain-mode statement (T013.3 `7169349`).
  - [x] **T013.1 — Generate the worked exchanges into the client spec.** Replace the hand-written illustrative exchange in `docs/CLIENT_SPEC.md` (~lines 538-559) with a generator-emitted block of all six `docs/examples/*.jsonl` files.
    - test: a committed `BEGIN/END GENERATED WORKED EXCHANGES` block matches fresh generation from the example files, and tampering with the committed block fails drift.
    - check: the hand-written jsonl exchange is gone, each generated subsection carries the example name and recorded provenance, the stale progress-order prose after it is rewritten against the recorded evidence (mutation key: create → begin → end before the key response; fast `g` refresh emits create only), and `make docs-check` passes.
    - Verification: `make update-schema && make schema-check && make test-rust FILTER="--test schema" && make docs-check`.
      - Verified: `make update-schema && make schema-check && make test-rust FILTER="--test schema" && make docs-check` @ `3bc7a3d` → ok: update-schema, schema-check, rust, docs-check. `committed_worked_exchanges_match_generation` pins the generated block (all six `docs/examples/*.jsonl` files, one fenced `jsonl` block each, deterministic compact `message` lines); `client_spec_worked_exchanges_drift_on_tamper_and_missing_block` fails drift for a modified provenance line and a removed block. `docs/CLIENT_SPEC.md` carries the generated block in place of the hand-written illustration, and the following prose describes the recorded mutation key (`S`/`status.stage-all`: create → begin → end before the key response) and the fast `g` refresh (create only).
  - [x] **T013.2 — Five-level client structure.** Rewrite the stale "Plain mode: standard-LSP clients (target specification; not yet implemented)" section (and the Plain table rows) as Levels 0-4: connect/sync, Plain read-only, Plain actions, Plain extras, rich.
    - test: each level documents sends/receives, the capabilities to declare and what fails without them, links its worked exchange from the generated block, and a conformance checklist keyed only to identifiers that exist in the repo (suites, scenario files, editor-model behaviors, example files).
    - check: stale claims removed (not-yet-implemented header, basic-modifier-filtering pending line, `Plain (target)` table row, target-server voice in the capability table, the seven invented harness profile names); `tests/ui/plain_init.lua` and the Helix TOML referenced; every feature claim names its proving test; no current/target design mixing.
    - Verification: `make test-rust FILTER="--test schema" && make docs-check`.
    - Verified: `make test-rust FILTER="--test schema" && make docs-check` @ `a62d659` → ok: rust and docs-check. Levels 0-4 now document sends/receives, capability degradation, a worked-exchange link and a checklist of existing repository identifiers; the stale target claims and invented profile names are removed; `tests/ui/plain_init.lua`, the Helix reference TOML and the generated `docs/plain-support.md` are referenced.
  - [x] **T013.3 — Capability-to-level classification and README statement.** Add the capability-to-level table, the classification rule (absent `experimental.magit` selects Plain; standard capabilities derive the Plain level; quirks overrides), the Neovim applyEdit/documentChanges facts from the generated quirks appendix and recorded traces, and the README Plain-mode statement.
    - test: the classification rule and capability-to-level table are pinned against `src/server.rs` capability resolution and the recorded initialize traces; README names only tested clients.
    - check: `make docs-check` passes and the README states that a general LSP client works in Plain mode when following the spec.
    - Verification: `make test-rust FILTER="--test schema" && make docs-check`.
    - Verified: `make test-rust FILTER="--test schema" && make docs-check` @ `648b5e3` → ok: rust (46 tests) and docs-check. `capability_levels_pin_the_plain_capability_fields` pins the generated rows against `PlainCapabilities`' field set (via its `Debug` shape, excluding runtime `commit_draft_expired`) and probes each declaration pointer through `negotiate`; `plain_classification_follows_negotiate_and_recorded_traces` pins absent-selector Plain, invalid-rich rejection, the current bundled Neovim declaration as rich, the recorded plain Neovim and Helix traces, and `assume-document-changes` → `Capability::DocumentChanges`/`neovim-accepts-undeclared-documentChanges`; `client_spec_capability_levels_drift_on_tamper_and_missing_block` fails drift for a tampered or removed block. CLIENT_SPEC documents the classification rule and the recorded Neovim `documentChanges` fact; README states the Plain-mode path and names only the Neovim and VS Code tested clients, with Helix as a reference configuration.

### Feature — Phase 5: Commit selection and rebase

- [x] **T014 — selected-commit-continuation.** [Scope](./TASKS_PART_SEVEN.md#t014-selected-commit-continuation-m).
  - Done: c9571c6 (containing commit of the last slice). Delivered across children: the confirm/cancel/stale select-mode continuation contract (T014.1 `d479920`) and the native Neovim commit-select confirm/cancel scenarios (T014.2 `c9571c6`).
  - [x] **T014.1 — Complete and pin the select-mode continuation contract.** Confirm delivers the full selected commit id, the captured operation arguments and the caller to the waiting operation; cancel restores the caller without mutation; a stale selection is refused readably without losing the caller.
    - test: session/command state tests cover confirm, cancel and a stale selection for the select-mode continuation (menu-argument and command-captured paths as they exist); selection identity is asserted as the full structured-node commit id, never rendered text or a row index.
    - check: any missing confirm/cancel/stale handling is implemented; `make test-rust FILTER="--test commands --test log_commands"` passes.
    - Verification: `make test-rust FILTER="--test commands --test log_commands"`.
    - Verified: `make test-rust FILTER="--test commands --test log_commands"` @ `d479920` → ok: rust.
  - [x] **T014.2 — Native commit-select scenarios.** Record the select-mode continuation end to end in real Neovim via new `tests/ui/scenarios/commit-select-*.scn` (confirm and cancel at minimum; stale if a deterministic pre-start fixture can produce it).
    - test: scenarios assert the caller view is restored with the captured argument after confirm and untouched after cancel, using text snapshots.
    - check: `make ui-file FILE=tests/ui/scenarios/commit-select-confirm.scn` and the cancel counterpart exit 0; no orphan `magit-lsp` processes remain.
    - Verification: `make ui-file FILE=tests/ui/scenarios/commit-select-confirm.scn && make ui-file FILE=tests/ui/scenarios/commit-select-cancel.scn`.
    - Verified: `make ui-file FILE=tests/ui/scenarios/commit-select-confirm.scn && make ui-file FILE=tests/ui/scenarios/commit-select-cancel.scn` @ `c9571c6` → ok: ui-file for both. `commit-select-confirm.scn` drives `l` → `c` → search → `Enter`, then asserts the status caller is restored (`Unstaged changes`, `view_stack.count == 1`), the clipboard equals `git rev-parse HEAD~2`, and the tree is clean; `commit-select-cancel.scn` drives `l` → `c` → `Escape` and asserts the status caller restored, the clipboard still `sentinel`, `HEAD` unchanged, and the tree clean. Snapshots `select-picker` and `caller-restored` written for both. `pgrep -af magit-lsp` showed no orphan servers.
- [x] **T015 — fixup-action.** [Scope](./TASKS_PART_SEVEN.md#t015-fixup-action-m). Research (2026-11-11, fold into children): the commit menu already declares the action (`src/menus/commit.rs:71` `action("f", "commit.fixup", "Fixup", "magit-commit-fixup", "R-4.1")`) with NO handler in src/commands.rs — T015 implements the handler. Mechanism is `git commit --fixup=<target>` (standalone fixup commit on HEAD, no rebase/sequencer), using T014's server-owned log-select to pick the target commit; the captured target + caller must survive the continuation (same Select continuation shape T014.1 delivered). `commit.squash` (`s`, sibling "Edit" group action) is the analogous `git commit --squash=<target>` and is ALSO missing a handler — decide whether T015 covers both fixup and squash or fixup only (Magit R-4.1a covers both). The `commit.instant-fixup`/`instant-squash` (`R`/`S`, "Edit and rebase" group) are autosquash-rebase, NOT here — folded into T019. Exact-repository-state test: staged content becomes a commit whose message is `fixup! <target subject>` and whose parent is unchanged; cancel returns to caller with nothing committed; failures readable.
  - Scope decision: T015 implements fixup only, as its linked acceptance specifies. `commit.squash` remains outside this ticket; instant-fixup/instant-squash remain T019.
  - [x] **T015.1 — Fixup selection handler and repository-state tests.** Implement `commit.fixup` through the server-owned log-select continuation, preserving caller and arguments.
    - test: confirm creates `fixup! <selected subject>` from staged content on the prior HEAD, restores the caller, and targets the full structured commit id; cancel leaves repository state unchanged and restores the caller; failures are readable.
    - Verification: `make test-rust FILTER="--test commands --test log_commands"`.
    - [x] **T015.1a — Fixup handler and confirm regression.** Implementation: `ba58ceb`; regression: `37a4e57`. Implement the selected-fixup path and one exact-repository-state confirm test.
      - test: confirm commits staged content with `fixup! <selected subject>`, prior HEAD as parent, full structured target identity and restored caller.
      - Verification: `make test-rust FILTER="--test commands --test log_commands"`.
      - Verified: `make test-rust FILTER="--test commands --test log_commands"` on `ba58ceb` implementation content with `37a4e57` regression → ok: rust.
    - [x] **T015.1b — Fixup cancellation and failure regressions.** Pin cancellation and readable Git failures, repairing behavior if needed.
      - test: cancel preserves HEAD/index and restores the caller; Git failure is a readable message and leaves the caller usable.
      - Verification: `make test-rust FILTER="--test commands --test log_commands"`.
      - Verified: `make test-rust FILTER="--test commands --test log_commands"` @ `ba58ceb` → ok: rust; cancellation preserves HEAD/index and caller, Git failure emits an error and leaves caller view open.
  - [x] **T015.2 — Native fixup selection scenario.** Add the real-Neovim fixup scenario using structured/text evidence only. Done: containing commit.
    - test: select a non-HEAD target, confirm, and assert exact fixup subject, parent, committed staged content and restored caller; cancel preserves HEAD/index and restores the caller.
    - check: the scenario passes and leaves no orphan server.
     - Verification: `make ui-file FILE=tests/ui/scenarios/commit-fixup.scn`.
     - Verified: `make ui-file FILE=tests/ui/scenarios/commit-fixup.scn && make ui-file FILE=tests/ui/scenarios/commit-fixup-cancel.scn` @ `8c25d27` → both `ok: ui-file`; confirm asserts fixup subject, parent, committed index tree/content and restored caller; cancel asserts HEAD/index preservation and caller restoration. `pgrep -af magit-lsp` showed no orphan servers.
- [x] **T016 — editor-helper-kinds.** [Scope](./TASKS_PART_SEVEN.md#t016-editor-helper-kinds-s). Done: 241d292 plus T016.2.
  - [x] **T016.1 — Kind-owned draft targets and completion policy.** Separate commit-message and rebase-todo draft validation and finish policy; resolve the worktree-aware todo target through Git.
    - test: each kind accepts only its expected helper kind/target; todo rejects commit-style close and supports explicit confirmation/cancellation.
     - Verification: `make test-rust FILTER="--test editor_bridge --test menus"`.
     - Verified: `make test-rust FILTER="--test editor_bridge --test menus"` @ `2f3c7e1` → ok: rust. Both draft kinds reject wrong helper kind/target; todo rejects close and confirms or aborts explicitly.
  - [x] **T016.2 — Kind-aware registry integration and history isolation.** Wire kind selection through registry/helper dispatch without adding rebase initiation or todo parsing. Done: containing commit.
    - test: registry accepts the expected todo helper, rejects wrong kind/target, and cannot expose or retain commit-message history for todo runs; commit-message behavior stays unchanged.
     - Verification: `make test-rust FILTER="--test editor_bridge"`.
      - Verified: `make test-rust FILTER="--lib editor_history_tests" && make test-rust FILTER="--test editor_bridge"` @ df88c46 → both ok: rust. `editor_history_tests::todo_registry_runs_never_retain_commit_history` seeds commit history, rejects wrong todo helper kind/target, saves todo and verifies both abort and confirm leave history unchanged; neighboring commit-message retention coverage remains passing. Earlier `--test editor_bridge` did not execute this unit regression.
- [x] **T017 — rebase-state.** [Scope](./TASKS_PART_SEVEN.md#t017-rebase-state-m).
  - [x] **T017.1 — Typed Git rebase state detection.** Detect worktree-aware merge/apply rebase state and conflicts through the Git boundary. Done: containing commit.
    - test: real repositories cover no operation, active rebase and conflict classification without confusing cherry-pick/revert with rebase.
     - Verification: `make test-rust FILTER="--test rebase_state"`.
     - Verified: `make test-rust FILTER="--test rebase_state"` @ `02e5e01` → ok: rust. Real Git operations cover idle, merge/apply rebase conflicts, conflicting cherry-pick/revert exclusions, git am exclusion, and linked-worktree isolation; revert asserts `REVERT_HEAD`.
  - [x] **T017.2 — Rebase state actions and status.** Render server-owned operation state and implement continue, skip and abort with readable failures.
    - test: exact repository state and rendered structure cover action success, conflict, and invalid operation attempts.
    - Verification: `make test-rust FILTER="--test rebase_state --test commands --test menus"`.
     - [x] **T017.2a — Rebase operation commands.** Implement continue, skip and abort behind the Git boundary; register executable menu actions. Done: `1db0220`, `d31087b`.
      - test: exact repository state covers continue/skip/abort and readable conflict/no-operation outcomes.
      - Verification: `make test-rust FILTER="--test rebase_state --test commands --test menus"`.
      - [x] **T017.2a1 — Rebase execution and menu contracts.** Complete Git execution, command registration, executable menu metadata and exact-state regression fixtures. Done: containing commit.
        - test: independent real conflict fixtures cover continue, skip and abort; registry and menu contracts expose implemented operations.
        - Verification: `make test-rust FILTER="--test rebase_state --test commands --test menus"`.
      - [x] **T017.2a2 — Rebase command error effects.** Exercise unresolved-conflict and no-operation failures through command dispatch. Done: containing commit.
         - test: failures emit readable `Effect::Message`, preserve repository state and leave the caller usable.
         - Verification: `make test-rust FILTER="--test rebase_state"`.
          - Verified: `make test-rust FILTER="--test rebase_state"` @ 1db0220 → ok: rust. SessionTest dispatch covers idle and unresolved continue failures, message effects, unchanged HEAD/index/conflicts and caller usability. T017.2, .2b and .3 remain open.
    - [x] **T017.2b — Structured rebase status.** Render active/conflicted rebase state and operation availability using server-owned tree metadata.
      - test: status state and tree update as operations progress and disappear after abort/completion.
      - Verification: `make test-rust FILTER="--test rebase_state --test status_render"`.
      - Verified: `make test-rust FILTER="--test rebase_state --test status_render"` @ 7be7622 → ok: rust. Live session refresh asserts stable section/operation metadata through idle, conflict, staged resolution, completion and abort.
  - [x] **T017.3 — Native rebase state conformance.** Exercise status and operation actions through real Neovim input using text and Git evidence.
    - test: `rebase-state.scn` asserts conflict/status, operation outcomes and usable caller state.
    - Verification: `make ui-file FILE=tests/ui/scenarios/rebase-state.scn`.
      - Verified: `make ui-file FILE=tests/ui/scenarios/rebase-state.scn` @ 12bdfbb → ok: ui-file. Native text and Git assertions pass; `pgrep -af magit-lsp` shows no orphan server.
- [ ] **T018 — rebase-todo.** [Scope](./TASKS_PART_SEVEN.md#t018-rebase-todo-m). Design (advisor 2026-11-11, fold into children): no dry-run validator exists — `git rebase --edit-todo` mutates in place and leaves a broken file, so the server self-validates a strict closed grammar (reject-by-default) and writes atomically (temp+rename) in `.git/rebase-merge/`. `.git/sequencer/todo` is cherry-pick/revert, NOT rebase — out of scope; every interactive rebase uses `.git/rebase-merge/git-rebase-todo`. Node identity = full SHA via `git cat-file --batch-check` (require `type=commit`, unambiguous, no dupes); subject/command are fields, never the identity. Pre-write checks: first line must not be squash/fixup; emit `drop` rather than delete; SHA set must equal the read set; content-hash stale guard. Reject `exec/break/label/reset/merge/update-ref`. Decide at split: (a) scope edits an already-paused rebase (blocking `GIT_SEQUENCE_EDITOR` at `rebase -i` start is a separate larger ticket); (b) `rebase.updateRefs=true` inserts `update-ref` lines even into linear rebases — accept as an opaque pinned node or document the limitation.
- [ ] **T019 — interactive-rebase.** [Scope](./TASKS_PART_SEVEN.md#t019-interactive-rebase-l). Fold in `commit.instant-fixup`/`commit.instant-squash` (commit-menu "Edit and rebase" group, `R`/`S`, R-4.1) at the end: both run `git rebase -i --autosquash` against a selected target and therefore share T017's failed-rebase/conflict state and the T019 continue/abort/conflict handling — they are NOT T015 (`commit.fixup` `f` = `git commit --fixup=<target>`, a standalone fixup commit with no rebase/sequencer). Add a `commit.instant-*` handler + autosquash invocation + exact-repository-state/conflict tests once T019's interactive-rebase machinery exists; do not schedule them separately.

### Feature — Phase 6: Plain acceptance

- [ ] **T020 — plain-e2e.** [Scope](./TASKS_PART_SEVEN.md#t020-plain-e2e-l).
- [ ] **T021 — helix-smoke.** [Scope](./TASKS_PART_SEVEN.md#t021-helix-smoke-s).
- [ ] **T022 — plain-gate.** [Scope](./TASKS_PART_SEVEN.md#t022-plain-gate-s).

### Feature — Phase 6A: Rich protocol v1 and client prototypes

0.2 gate subset: T105, T112, T113, T114 and T119. T115/T116/T117 are non-gating prototypes; T118 stable freeze is non-gating.

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

Starts after phases 1–6 and the Phase 6A 0.2 gate subset (T105/T112/T113/T114/T119), not the non-gating prototypes or T118 stable freeze. Do not insert unrelated features between release gates and release. Evidence remains required.

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

- [x] **Target restored status content explicitly in process-log UI regression.** Slice of five-scenario repair. Native failure follows process-log search and pop: status cursor/viewport is below the staged header, not missing staged Git state (the pre-open staged assertion passes). The client contract restores view identity/focus, not topline. test: retain staging, process recording, stack/pop and commit-menu checks; after pop/refresh explicitly target the staged header in the restored buffer before screen assertions and independently assert staged patch content through Git. Do not add client viewport behavior or weaken existing expectations. Verification: `make ui-file FILE=tests/ui/scenarios/process-log.scn`. Done: containing commit.
  - Verified: `make ui-file FILE=tests/ui/scenarios/process-log.scn` @ adac686 → `ok: ui-file`; PASS for `standard` and `vim` at 100x30 and 80x24. Staging, `git apply` process recording, `view_stack.count == 2` then `1`, commit menu, floating-window close and `no_message` assertions all ran unchanged; after the refresh `goto "Staged changes (1)"` targets the restored header and the new `sh: git diff --cached -- f.txt | grep -F -- '+line3 CHANGED'` oracle passes in every combination. `back-to-status.screen` now starts at `Staged changes (1)` with `+line3 CHANGED` at all four profile/size pairs; no orphan native processes after the run.
  - Check: base `build/ui/process-log/standard-80x24/failure.screen` shows the restored status viewport starting mid-hunk at `-line3`/`+line3 CHANGED` (f.txt body) with the statusline reading `19,1` at `Unstaged changes (1)` while `Staged changes (1)` sits at buffer line 3 above the viewport; the preceding line-15 assertion on that header had already passed, so staging was intact and only the viewport was offscreen. Pop restores view identity/focus, not topline (`docs/CLIENT_SPEC.md` main view push/pop), and the explicit `goto` now scrolls the supported header into view instead of adding client viewport behavior.

- [x] **Preserve terminal backtab for basic modifier clients.** Prerequisite for native shifted-Tab delivery. test: `basic` keymap delivery and capture retain exact `shift+Tab` while still omitting arbitrary Shift/Meta chords, including wire-level keymap coverage. Update the capability contract to name terminal backtab explicitly; never claim full modifiers or spoof terminal identity. Verification: `make test-rust FILTER="--test keymap --test server_async"`. Native verification remains in the shifted-Tab harness slice below. Done: containing commit.
  - Verified: `make test-rust FILTER="--test keymap --test server_async"` @ b02d98f → ok (21 keymap + 47 server_async tests passed); `basic` delivers and captures `shift+Tab`, drops `ctrl+shift+Tab`, `meta+c` and `k meta+c` while `full` keeps them.
  - Check: `docs/CLIENT_SPEC.md` names the lone portable terminal backtab exception and states basic implies no full modifier support or terminal-identity spoofing. Native Neovim delivery acceptance is delegated to the shifted-Tab harness slice; no VSCode runtime evidence claimed.

- [x] **Deliver shifted Tab as terminal backtab in native UI harness.** Slice of the five-scenario repair. test: canonical `shift+Tab` reaches the existing `<S-Tab>` mapping via terminal backtab, and `common-navigation` exercises the unchanged visibility sequence at both vim sizes. Keep CSI-u handling for other modified keys intact. check: record native text evidence and process cleanup. Verification: `make ui-unit`; `make ui-file FILE=tests/ui/scenarios/common-navigation.scn`. Done: containing commit.
  - Verified: `make ui-unit` → `ok: ui-unit` (shifted Tab stays on `BTab`; `C-S-Tab`, `C-Enter`, ctrl-arrow and modified page keys keep their CSI-u bytes). `make ui-file FILE=tests/ui/scenarios/common-navigation.scn` → `PASS common-navigation vim *`, `1 PASS, 0 XFAIL, 0 FAIL` at both vim sizes (100x30 and 80x24).
  - Check: native text evidence is the unchanged hierarchy/sections-only/folded-siblings/hunk-headers/expanded sequence printed by the passing scenario; `pgrep -af "magit-lsp|nvim"` shows no leftover native processes after the run.
  - Cause: the harness terminal declares basic modifiers, so the delivered keymap only retains exact `shift+Tab` (previous slice); the remaining gap was encoding, and the driver now sends shifted Tab as terminal backtab (`BTab`) so the existing `<S-Tab>` mapping receives it. No isolated evidence that CSI-u shifted Tab is undecoded; the portable backtab route is used instead.

- [x] **Build two-hunk UI repository before client startup.** Prerequisite for the hunk-body-fold repair. test: the registered `two-hunk-siblings` fixture creates exactly two unstaged hunks in `f.txt` at lines 5 and 35 plus an independent modified `g.txt`, before any client or watcher starts. Verification: `make ui-unit`. Done: 1fc9fa3.
  - Verified: `make ui-unit` @ b888574 → ok; `test_two_hunk_siblings` asserts f.txt hunk headers `@@ -2,7 +2,7 @@` and `@@ -32,7 +32,7 @@` with `+line5 CHANGED`/`+line35 CHANGED` and g.txt `+line2 CHANGED`.

- [x] **Repair vim hunk-body folding UI regression.** Slice of the five-scenario repair. test: `hunk-body-fold` folds the containing hunk from a body line, preserves its sibling across refresh, and unfolds/refolds from the title at both declared sizes without stale-view messages. check: record the proven cause and native text/structured evidence; do not weaken version validation or add arbitrary sleeps. Verification: `make ui-file FILE=tests/ui/scenarios/hunk-body-fold.scn`. Done: a8bd4e0.
  - Verified: `make ui-file FILE=tests/ui/scenarios/hunk-body-fold.scn` @ 1fc9fa3 → PASS for vim at 100x30 and 80x24; `body-folded` and `refreshed-fold` snapshots hide `+line5 CHANGED` while keeping the sibling `+line35 CHANGED`, `M f.txt` and `M g.txt`; `title-unfolded` restores the body; `title-folded` refolds it; both `no_message /stale/` expectations passed and no stale message appeared.
  - Check: the failing trace (`/tmp/opencode/hunk-post.jsonl`, `.server.jsonl`) shows the client receiving and applying an unsolicited newer render v5 at `t_unix_ms` 1794310251020.610/…021.076 around the second Tab, while the server logged `view/key` id 4 at …020.681 and emitted no `render_sent` for that id; the handler thus refused the key against a newer acknowledged version and the view showed `This view is stale; please retry on the current view.` That refusal is lawful, so version validation was left intact and no sleep was added. The scenario now uses the pre-built `two-hunk-siblings` fixture instead of an `sh` rewrite plus `git commit`, removing the out-of-band repository write that preceded the refresh. The extra unsolicited refresh being caused by that write racing the watcher is inferred from the ordering, not directly observed.

- [x] **Repair log UI scenarios for current vim bindings and selection behavior.** Prerequisite slice of the five-scenario repair below. test: `log-view` and `log-select` drive the real current keymap in both profiles and assert server-owned log/selection behavior. Diagnose stale `g l` and `Select` expectations against the current contract rather than weakening assertions. Verification: `make ui-file FILE=tests/ui/scenarios/log-view.scn` and `make ui-file FILE=tests/ui/scenarios/log-select.scn`. Done: b888574.
  - Verified: `make ui-file FILE=tests/ui/scenarios/log-view.scn` @ 16d725e → PASS across `standard` 100x30/80x24 and `vim` 100x30/80x24; `log-header` snapshots show `Log`, `latest decorated subject`, `middle graph subject`, `main` and `v1`, and `log-search` shows `old searchable subject` with `/old searchable subject [1/1]`.
  - Verified: `make ui-file FILE=tests/ui/scenarios/log-select.scn` @ 16d725e → PASS across both profiles and sizes; `select-log` snapshots show `Log — copy commits` and `Select commit(s) to copy`, and the scenario's clipboard assertions confirm `getreg('"')` equals `git rev-parse HEAD~2` for the old searchable subject and stays `sentinel` after `Escape`.
  - Check: the stale assumption was the `vim` opener `g l`; `profiles/vim.toml:54` and `profiles/standard.toml:52` both bind `open.log` to `l`, and the status log menu's `l` action opens `log.current`. `Select` was not stale in effect (it matched the prompt substring) but is now the exact server-owned prompt `Select commit(s) to copy`. Baseline at `16d725e`: both scenarios failed only in the vim combos at `wait timed out for screen text 'Log'`; standard combos passed. No orphan `magit-lsp` processes after the runs (`pgrep -x magit-lsp` empty).

- [x] **Pin queued Plain renders across echo/response ordering.** Diagnostic prerequisite for the flaky `plain_draft` ticket below. test: deterministically queue a newer authoritative render while an older edit is in flight, complete echo and response in both orders, and assert the newer edit targets the latest observed client version and converges to the newer retained action snapshot. check: record whether either schedule demonstrates a lost render; passing schedules narrow the investigation but do not establish the original flake's cause. Verification: `make test-rust FILTER="--test plain_sync"`. Done: containing commit.
  - Verified: `make test-rust FILTER="--test plain_sync"` @ `555560a` → 11 passed; `queued_render_survives_echo_and_response_in_either_order`.
  - Check: neither schedule lost a render. Echo-before-response releases the queued edit on the confirmed echo; response-before-echo releases it on the late echo. Both emit an edit targeting the latest observed client version (2) and replace the echoed text with the newer authoritative text, converging to the newer retained snapshot (`view-new`, generation 2). This narrows the historical `plain_draft` flake to paths outside this controller ordering (transport, generation metadata or readiness), but does not prove the original cause.

- [x] **Phase 2 gate — Declare a rich client in editor-bridge commit tests.** `make test` at `eb8bb1e` fails 18/27 `editor_bridge` tests (`log build/test-logs/rust.zuIHJG`) because they initialize with `json!({})` and still expect the `view/effects` `editDocument` frame. Plain clients now intentionally get `window/showMessage` with the exact `COMMIT_EDITMSG` URI plus a pending draft row (T007.4), and Plain delivery is already enforced by `plain_commit_editor_manual_instructions_cancel_pending_git_run`; the editor-bridge suite owns the rich in-place editor contract instead. Give those 18 tests a rich `experimental.magit` declaration (keys, modifiers, layout, `editDocument` effect, prompts) and keep every sync/cancel/checkpoint assertion unchanged. Done: 5da5d86. Verified: make test-rust FILTER="--test editor_bridge" (27 passed).

- [x] **Fix the flaky `tests/plain_draft.rs` suite (commit menu never offered).** `make test-rust FILTER="--test plain_draft" V=1` fails intermittently at `bfb60d1` (about 2 of 10 runs; also reproduced at `f53b4b6`, so it predates the `bfb60d1` P1 fix). Two symptoms: (a) `commit menu timed out` — the structural wait finds no `menu.run-action` code action with a string `target.itemKey` on any rendered line, and the client text at timeout is the post-stage status view with no menu (generation 3); (b) a 10 s `recv` timeout at `tests/common/stdio.rs:142` right after `open.commit` is executed (id 93), with the last Git activity being `diff --cached f.txt` and the commit-menu render never reaching the client. The menu render therefore either never leaves the server or arrives with a `textDocument.version` that `FakeEditor::apply_edit` rejects; `PlainDocument::expire` (`src/presentation/plain.rs:344-357`) then marks that `(authoritative.hash, client.hash)` pair `rejected` and never retries it, while `PlainDocument::prepare` (`410-451`) refuses new renders while `in_flight` is `Some`. `plain_code_actions` (`src/server.rs:3584-3618`) only offers actions once the client text and the server's synced document text both equal a retained render hash, so a dropped render makes the menu unreachable. Determine which of the two it is before changing anything, then fix the cause.
  - **Observed trace; root cause unproven (2026-11-08, `/tmp/opencode/diag_fail.log`).** The trace lists one status-document `workspace/applyEdit` targeting version 2, client version 3, and actions carrying `viewGeneration: 3`. Client document versions and view generations are separate counters. This observation does not establish a stale edit, rejection, or the missing-menu cause.
    - Observed evidence, not diagnosis: targeting v2 normally advances the client to v3; this alone proves neither a second stale edit nor a rejection. `/tmp/opencode/diag_fail.log` (run 7) retains `status edits=` and server stderr evidence; symptoms and artifact remain useful, but mechanism is unproven.
    - Instrumentation plan: correlate run/URI/incarnation, generation, render creation, prepare suppression/in-flight state, outbound request ID/version, response, `didChange` echo and expiry. Diagnose before production changes or retry masking; ten consecutive passes are supplementary, not regression proof.
  - Diagnostic artifact: `/tmp/opencode/diag_fail.log` (run 7); the `status edits=` block near line 335 lists observed status-document edits. Correlate it with server/controller events before assigning cause. The server stderr tail is in the same log.
  - test: add a deterministic regression for the diagnosed cause without weakening real Git commit, HEAD, message, status, or offered-action assertions. check: `make test-rust FILTER="--test plain_draft" V=1` passes 10 consecutive runs; repetition supplements, not replaces, the regression.
  - check: `make test-rust FILTER="--test plain_draft --test editor_bridge" V=1` and `make test-rust FILTER="--test plain_actions --test plain_sync" V=1` still pass.
  - Constraints: no arbitrary sleeps and no fixed retry loops that hide a real failure — every wait needs a deadline and a clear panic message. Do not change `src/` production code unless the captured evidence proves a production defect. Identify the commit menu structurally (offered `menu.run-action` with a string `target.itemKey`), never by grepping the text: the status view contains `Committing…` during a run, so a `"Commit"` grep matches the wrong line.
  - Partial harness work is uncommitted in `tests/plain_draft.rs`: `wait_for_commit_menu` rewritten as a structural poll, `wait_for_head_change` added and used before the confirm assertions, and the `f.txt` line lookups changed from `unwrap_or(0)` to `.expect(...)`. That work took the flake from about 50% to about 20% but did not eliminate it. It also still contains temporary diagnostic instrumentation in the `wait_for_commit_menu` timeout branch (dumps `status_edits`, the client version, the last 20 frames and an 80-line server stderr tail) — replace it with a clean deadline assertion before committing.
  - Check: diagnosis and fix are harness-side. The menu render is legal but delayed: while one status `workspace/applyEdit` awaits its reply/echo, `PlainDocument::prepare` (`src/presentation/plain.rs:410-451`) refuses the queued menu render, and `plain_code_actions` (`src/server.rs:3584-3618`) offers no menu until the client text and synced document match a retained snapshot. The old wait queried one line immediately after `open.commit` and missed that render. No production defect was found, so no `src/` change is made; the timeout dumps are removed, `wait_for_commit_menu` and the new `wait_for_status_action` poll structurally with deadlines, and `commit_menu_is_offered_after_a_queued_status_render_is_released` holds an applyEdit reply/echo, proves no menu is offered, releases the client frames, then asserts the structural wait finds the menu and completes a real commit with unchanged Git/HEAD/message/status assertions.
  - Verified: `make test-rust FILTER="--test plain_draft" V=1` @ `d70ee3c` → 10 consecutive runs, 17 passed each.
  - Verified: `make test-rust FILTER="--test plain_draft --test editor_bridge" V=1` @ `d70ee3c` → 27 + 17 passed.
  - Verified: `make test-rust FILTER="--test plain_actions --test plain_sync" V=1` @ `d70ee3c` → 29 + 11 passed.
  - Verification: `make test-rust FILTER="--test plain_draft" V=1`.

- [x] **Repair the five failing UI scenarios (`vim` profile and `process-log`).** `make ui-test` at `5da5d86` reports 44 PASS / 5 FAIL: `common-navigation` and `hunk-body-fold` (vim profile assertions about staged state and hunk bodies), `log-select` and `log-view` (wait for `Log` times out), `process-log` (expects `Staged changes (1)`, screen shows none). A clean extraction of reviewed baseline `2e29535` under `make ui-test` fails the same five scenarios with the same expectations, so this predates the Phase 2 work. Establish whether the `vim` profile setup or the shared fixtures are wrong (a fixture that silently builds an empty or unstageable repository would explain all five), then fix the cause rather than the expectations, or delete a scenario that no longer matches the server-owned contract. Done: f10d8a0.
  - Verified: serial `make test` then `make ui-test` @ f10d8a0 → both exit 0 (Rust, Lua, VSCode, keybindings digests and `build/ui-test`). The success logs were deleted, so no per-suite or scenario counts are claimed here; all five previously failing scenarios pass and no orphan `magit-lsp` processes remain. Per-slice native PASS evidence is recorded on the child Bugfix entries above.
  - Commits: `b888574` log-view/log-select current `l` opener and exact `Select commit(s) to copy` prompt; `1fc9fa3` `two-hunk-siblings` fixture built before client startup; `a8bd4e0` hunk-body-fold on that fixture plus `no_message /stale/` assertions; `8202c9e` basic-mode lone `shift+Tab` retention; `adac686` shifted-Tab terminal-backtab delivery; `f10d8a0` explicit process-log header target with an independent staged-patch oracle.
  - Cause: not an empty or unstageable fixture. `log-view`/`log-select` kept the stale `g l` opener and an inexact `Select` match; `hunk-body-fold` rewrote the repository out of band after client startup, racing the watcher into an unsolicited newer render (lawfully refused as stale); `common-navigation` delivered shifted Tab as CSI-u that the basic terminal did not turn into `<S-Tab>`; `process-log` restored view identity/focus but not the viewport topline. Each was fixed at the cause; no scenario was deleted and no server-owned expectation was weakened.


## Review

- [x] **[P2] Return deferred sibling refreshes from session prompt closes.** Prerequisite for the server-delivery item below. test: after refresh-all is queued during a document prompt, closing either the prompt view or its origin returns fresh renders for every surviving normal-mode view. Text-mode edits and queued refresh remain intact. Verification: `make test-rust FILTER="--test prompts"`. Done: containing commit.
  - Verified: make test-rust FILTER="--test prompts" @ 7fae07c → 34 passed, including `closing_a_document_prompt_returns_every_surviving_refresh` and `closing_a_document_prompt_keeps_text_mode_refreshes_queued`.

- [x] **[P2] Resume deferred sibling refreshes when a prompt or its origin closes.** Server-delivery slice; session/text-mode behavior is owned by the prerequisite above. test: queue refresh-all with multiple repository views, then close the prompt view or its origin through the server `view/close` dispatcher; surviving normal-mode views receive fresh render notifications without a later unrelated refresh, and the response stays JSON null. Use a server unit fixture to queue `Session::refresh_all` deterministically: `common.refresh` bypasses this queue, and the watcher has no observable queued-state barrier. This is dispatcher coverage, not stdio/watcher evidence. Verification: `make test-rust FILTER="--lib --test prompts --test server_async"`. Done: containing commit. Verified: make test-rust FILTER="--lib --test prompts --test server_async" @ ff49240 → ok; `closing_a_prompt_view_or_origin_delivers_resumed_sibling_refreshes` covers both close targets.

- [x] **[P2] Verify Plain close routing of resumed prompt refreshes.** Follow-up verification slice for the shared server-close implementation above. check: trace every `close_plain` and `schedule_plain_open` caller; confirm outcomes are delivered only after releasing `plain_lifecycle`, with no discarded pending outcome. Record source-only limits and run the focused Plain suites. Verification: `make test-rust FILTER="--test plain_actions --test plain_sync --test plain_draft"`. Done: 16d725e.
  - Source audit @ ee220d1: `close_plain` (`src/server.rs:1570`) has exactly three callers — inside `schedule_plain_open` (`:1154`, whose result it returns) and the two Plain `didClose` dispatcher paths (`:2336`, invalid-sync branch; `:2358`, success branch). `schedule_plain_open` has one caller (`:2345`, Plain `didOpen` success branch); no other module or hook calls either. `deliver_plain_close_outcome` (`:1612`) has exactly three callers (`:2340`, `:2347`, `:2360`), each pairing with the close/open immediately above it. All four dispatcher arms take `plain_lifecycle` at `:2278`, call close/open while holding it, drop the guard (`:2339`, `:2346`, `:2359`; `:2354` for the didChange path) and only then deliver, because delivery reacquires the guard through `dispatched_outcome` (`:4777`) and `route_plain_render` (`:788`, guard at `:793`). The didOpen error branch (`:2291-2341`) closes without bootstrapping; `schedule_plain_open`'s early `None` returns (`:1143-1152`) all precede its `close_plain` call, so no pending outcome exists there. No returned `Some` outcome is dropped.
  - Source-only limit: `close_plain` maps `Session::close` errors to `None` (`src/server.rs:1585-1592`); a `resume_pending`/`resume_prompt` error after the prompt was taken (`src/session.rs:906`, `:922`, `:940`) therefore skips delivery. Queued refreshes stay `dirty` (`src/session.rs:1797-1800`) and resume on a later refresh, so no produced result is lost, but static tracing cannot prove runtime interleaving.
  - Coverage limit: `plain_actions`/`plain_sync`/`plain_draft` exercise Plain `didClose` and document lifecycle (`tests/plain_actions.rs:1078`, `tests/plain_sync.rs:104`, `tests/plain_draft.rs:933`), but none closes a Plain prompt view with queued sibling refreshes, so resumed-refresh delivery at Plain transport rests on this source audit. Deterministic rich dispatcher coverage is `src/server.rs::commit_tests::closing_a_prompt_view_or_origin_delivers_resumed_sibling_refreshes`; this is not a stdio or watcher test.
  - Verified: make test-rust FILTER="--test plain_actions --test plain_sync --test plain_draft" @ ee220d1 → 29 + 19 + 11 passed (V=1).

- [x] **[P1] Guard delayed Plain draft delivery failures against accepted or superseded deliveries.** Introduced in `bfb60d1`, `src/server.rs:1071–1085`. Atomically check current delivery identity and unopened draft state before aborting; never check then abort. test: cover both arrival orders; late failure after exact `didOpen` or replacement delivery does not cancel, while failure of current unopened delivery cancels with readable message and no commit. Cover rejection, error and timeout outcomes. Verification: `make test-rust FILTER="--test plain_draft --test editor_bridge"`. Done: containing commit.
  - Verified: make test-rust FILTER="--lib delivery_guard"; make test-rust FILTER="--test plain_draft --test editor_bridge --test editor_drafts" @ 67db167 → ok.
  - Replacement-delivery identity is registry-level coverage (`delivery_guard_aborts_only_the_current_unopened_delivery`), not transport evidence; `plain_draft` covers late rejection/error/timeout after the exact `didOpen` and current-unopened cancellation for all three outcomes.

- [x] **[P1] Match empty-secret cancellation in the shared Rust prompt model.** The new `secret-empty` vector expects cancellation, matching clients, but `tests/common/editor_model/prompts.rs` returns an empty value. Done: containing commit. test: shared vector evaluation cancels empty secret input while preserving ordinary empty text. Verification: `make test-rust FILTER="--test plain_harness shared_prompt_vectors_derive_answers_and_focus"`.
- [x] **[P1] Keep repaired Plain prompt action generations current.** `src/server.rs` captures generation before prompt reconciliation, then ignores the newer repaired render version when rebuilding actions. test: repair protected prompt text, acknowledge the repair, and successfully execute answer/cancel using newly offered actions without a stale-generation rejection. Verification: `make test-rust FILTER="--test plain_sync --test plain_prompt"`. Done: containing commit.
  - Investigation (2026-11-08; source review, not verified implementation): a second defect prevents the repair regression from completing. The repaired branch of `plain_change` does not record the latest observed client text/version before `render_actions`. `PlainDocument::prepare` therefore either sees the old canonical text and emits no repair, or builds an edit against the previous document version. A later code-action response is a valid synchronization barrier; extra waits do not correct the stale controller snapshot.
  - Proposed fix: extract the observation and existing in-flight bookkeeping from `PlainDocument::change` into an observe-only operation, leaving edit preparation in `change`. In the repaired prompt branch, after session reconciliation returns, observe the current document snapshot and render the repaired tree under one controller lock and incarnation check. Use the returned `render.version` for action metadata. Preserve existing in-flight behavior; do not discard flights via `adopt`, emit an obsolete edit before updating authority, or change CRLF policy.
  - Regression plan: replace the CRLF workaround in `tests/plain_actions.rs::repaired_plain_prompt_offers_actions_for_the_repaired_generation` with a valid LF field edit, synchronize through a code-action response while handling any server requests, then damage a protected title. Assert no repair for the valid field edit, repair targeting the damaged notification's document version, successful application preserving the field value, action generation matching the repaired render, and successful Cancel. Guard observation extraction with controller coverage for echo/response ordering and rejected edits using existing infrastructure. Verification: `make test-rust FILTER="--test plain_actions --test plain_sync --test plain_prompt"`; focused diagnosis: `make test-rust FILTER="--test plain_actions repaired_plain_prompt_offers_actions_for_the_repaired_generation"`.
  - Checkpoint: the two-line `src/server.rs` generation fix and unfinished regression in `tests/plain_actions.rs` remain uncommitted. Prior verification failed (28 passed, 1 failed; `build/test-logs/rust.tDK7TH`). Resolved in the containing commit: `PlainDocument::observe` extracts observation bookkeeping, the repaired branch observes the current snapshot before rendering, and the regression plus controller coverage pass.
- [x] **[P2] Resume deferred sibling refreshes after document prompts.** `src/session.rs` queues refreshes for every view while a prompt is active but completes only the origin refresh. test: with multiple repository-backed views, defer refresh-all during a prompt, finish/cancel the prompt, and ensure every affected normal-mode view updates without losing text-mode edits. Verification: `make test-rust FILTER="--test prompts --test server_async"`. Done: `15a7158`. Verified: make test-rust FILTER="--test prompts --test server_async" @ 53de88a → ok: rust.
- [x] **[P1] Report a rejected Plain commit draft edit.** `send_plain_edit_document` in `src/server.rs` sends a versioned whole-buffer `workspace/applyEdit` for an already-open `COMMIT_EDITMSG` buffer, then awaits the reply in a spawned task and discards it. A client that rejects the edit (`applied: false`, an error, or a timeout) leaves the server believing the new template was delivered; the later `didOpen` is then refused by `Draft::did_open` with "Open the commit draft with its current version and exact text" instead of a readable reason. Mirror the existing `send_plain_edit` handling: read `applied`, and on rejection show a readable message and cancel the run instead of leaving a stale-buffer draft pending. test: a Plain client that rejects the draft `applyEdit` sees a readable message, the run is canceled, and no commit happens. Verification: `make test-rust FILTER="--test plain_draft --test editor_bridge"`.

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
