# magit-lsp — Roadmap

Last review: 2e29535237ad2b0f88dabb4b0a049c41d26419d2

Resolved checkpoint detail: immutable `d9ea816:TASKS.md`; not current instructions. `Source-only` labels mark provenance, not implementation commits. Scripted model checks are not real-server conformance; Linux native evidence does not establish Windows/macOS behavior, which remain CI follow-ups. Hygiene remains report-only: one unlisted dependency, 69 unused exports and 34 Lua warnings have owners/reasons. T007.1n had a timeout before a passing rerun; retain this transient timeout/rerun risk. Documentation reconciliation does not close Phase 2; T007 remains open.

## Ad hoc

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
  - Verification provenance: immutable `e464429:TASKS.md` retains historical check commands; completion refs above identify implementation commits.
  - Verified: `make docs-check DOC_FILES="TASKS.md AGENTS.md README.md ui-test-plan.md hygiene-allowlist.md LICENSING.md docs/ARCHITECTURE.md docs/PROTOCOL.md docs/KEYMAPS.md docs/VIEWS.md docs/TESTING.md docs/OPEN_QUESTIONS.md docs/CLIENT_SPEC.md"` → ok: docs-check.


- [x] **Documentation cleanup — Compact T006 and T007 completion records.** Shorten completed prerequisite records without losing live audit evidence, acceptance dependencies or native evidence limits.
  - Verified: `make docs-check DOC_FILES="TASKS.md AGENTS.md README.md ui-test-plan.md hygiene-allowlist.md LICENSING.md docs/ARCHITECTURE.md docs/PROTOCOL.md docs/KEYMAPS.md docs/VIEWS.md docs/TESTING.md docs/OPEN_QUESTIONS.md docs/CLIENT_SPEC.md"` @ `e50d516` → ok: docs-check. Manual preservation audit: compacted checked records through T007.3; verified unchecked blocks unchanged and T007.4 records untouched, including externally updated T007.4a1/a2/a; retained T007.1n native.json paths/timeout risk, T007.5 six-clause native requirement, original clause coverage, model/server and declaration/native evidence limits.
- [ ] **Documentation cleanup — Compact legacy completion records.** Remove obsolete completed-task narratives and dated checkpoints after preserving completion pointers and any forward-relevant constraints.
- [ ] **Documentation cleanup — Condense completed-task records.** Shorten completed ad hoc and roadmap records while retaining completion pointers, open-task prerequisites and evidence limits, then audit the parent cleanup acceptance. Complete after the three preceding compacting slices.
- [ ] **Documentation cleanup — Condense obsolete history and preserve live work.** Remove resolved review/checkpoint narratives and summarize completed-task records. Preserve every unfinished task, acceptance requirement, dependency and authoritative completion pointer; verify the compact queue and documentation links. Complete after the two preceding cleanup slices.

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
  - Done: `c3f5a42`.
  - Verified: `make test-rust FILTER="--test plain_harness --test server_async"` @ `f13dbc1` → ok: rust. One real-stdio scenario covers undeclared support, declared success, and declared rejection; checks correlated request/response, exact URI and flags, fallback, cancellation, unchanged HEAD, no custom effects, and bounded shutdown.
- [ ] **T007.4a — Deliver pending Plain commit editors through LSP.** Route managed edit effects to declared showDocument or readable manual-open instructions. Start the single real-server T007.4 scenario; later children extend that same scenario.
- [ ] **T007.4b — Render pending manual commit paths as structured rows.** Derive pending draft rows from trusted registry state and refresh their lifecycle without parsing rendered text. Extend the existing T007.4 scenario only.
- [ ] **T007.4c — Confirm or abort manually attached commit drafts.** Attach the issued URI through didOpen and expose version-bound Confirm/Abort code actions. Extend the same scenario to prove exact Git completion and abort outcomes; no additional test cases.
- [ ] **T007.4 — Attach manual commit editing without showDocument.** Implement manual COMMIT_EDITMSG instructions/path row, didOpen attachment and Confirm/Abort actions; verify one no-showDocument harness scenario only. Original T007 clause 6.
- [ ] **T007.5 — Audit Plain capability acceptance.** Close T007 only after its original six clauses and native evidence are demonstrated; retain one authoritative completion record.

- [x] **Roadmap reconciliation — Restore one authoritative queue.**
  Make this file the sole status queue. Preserve unfinished numbered work in a compact linked index; retain Part Seven as scope/provenance, not a competing completion ledger. Remove stale execution directives without closing genuine gaps.
  Done: `6d08d85`. Verified: authority-pointer sweep finds no live instruction to execute Part Seven as a queue; its detailed scopes remain mandatory references. No feature acceptance was closed.
- [x] **Roadmap reconciliation — Index individual unfinished numbered tickets.**
  Give each unfinished heading/table ticket its own checkbox and valid specification link. Retain phase order and owner-request gates; keep T105 only in Phase 6A and T005 child status in Ad hoc.
  Done: `6d08d85`. Verified: the §7 scan finds all 114 unique T006–T119 entries, no missing or duplicate IDs, and no invalid specification anchors. T005 retains its existing open children; deferred owner-request guards remain. Documentation-only `make ui-unit` passed (`ok: ui-unit`).
- [x] **Roadmap reconciliation — Refresh protocol and evidence guidance.**
  Correct current Plain selection and quirk-test references. Condense obsolete trace/formatter narratives into immutable provenance and current workflow links; preserve unproved T005 acceptance and deferred debt.
  Done: `6d08d85`. Verified: current Plain initialize selection and the named didOpen regression were checked against source/tests; outgoing quirk variants and appendix acceptance remain unchecked. Historical recorder evidence links replace obsolete current-enforcement claims.
- [x] **Roadmap reconciliation — Route active verification through Make.**
  Replace live direct test/UI command instructions with existing Make targets. Preserve historical command records as labeled provenance and identify any missing target explicitly.
  Done: `6d08d85`. Verified: unfinished specification command sweep uses Make targets; T011 owns missing schema-drift integration and T025 owns missing native-host integration. Native F5 verification requires a suitable Make target; unit/integration results are not native evidence. `git diff --check` passed.

- [x] **Investigate and rationalize trace/capture test infrastructure.**
  Done: `5025937` (cleanup implementation and verification linked in immutable `5025937:TASKS_PART_SEVEN.md`). Current retained coverage and native workflow: [fixture guide](tests/fixtures/lsp_traces/README.md) and [UI test plan](ui-test-plan.md). Static trace/parser gates and embedded capture programs are retired; native causality limits remain explicit.
- [x] **Autofix staged formatting through shared hook and Make commands.**
  Done: `9702973`, including final index-publication safety fix `fa5c3de`. Current staged-only behavior and contributor commands: [formatting guide](docs/FORMATTING.md). Disposable real-hook verification is recorded at immutable `9702973:TASKS_PART_SEVEN.md`; no permanent formatter tests.

- [x] **Remove permanent formatter tests and retain one-time verification.**
  Remove the formatter/hook test suite as requested; verify the tools and staged-safe hook once in disposable repositories. Update T120 acceptance and CI scope accordingly; finish formatter setup, then stop without advancing unrelated roadmap work.

Historical trace/capture evidence and implementation records below describe the state at immutable Git revision `83f79f5cbf2e272a8be92673ef3c8bd41118c23e`; they are not active capture workflows. Current cleanup status is tracked in this file's [Ad hoc queue](#ad-hoc).

- [x] **T120.1 — Configure pinned Lua and JavaScript/TypeScript formatters.**
  Implement the requested hook tooling foundation with locked StyLua and Prettier dependencies and repository-owned configurations. Cover maintained Lua and JS/TS sources; preserve unrelated release changes and immutable fixtures.
- [x] **T120.2 — Implement staged-safe formatter checks.**
  Done: added `python3 scripts/format.py --staged` and `--base <commit>` with isolated index/HEAD snapshots, changed-source checks, pinned StyLua/Prettier and workspace Rust formatting. One-time disposable verification confirms staged-content checks preserve the index and working copy.
- [x] **T120.3a — Add opt-in formatter hook installation.**
  Done: added `scripts/install-hooks.sh` install/remove commands and `.githooks/pre-commit`; one-time disposable verification checked install safety and real staged hook behavior.
- [x] **T120.3a1 — Isolate missing-tool tests and preserve existing hook locations.**
  Fix the missing-formatter test so it never deletes a binary through shared node_modules; restore locked dependencies. Resolve default hooks through Git for worktrees, preserve custom hooks, and give actionable missing-Python guidance.
- [x] **T120.3b — Integrate formatter CI checks and contributor commands.**
  Add shared Make targets, committed-range CI checking and focused contributor documentation, without permanent formatter tests. Keep legacy-source normalization out of scope, close T120 after its children pass, and preserve concurrent work.
- [x] **T120.3 — Wire formatter hooks, installation and CI.**
  Add an opt-in repository-owned pre-commit hook, safe installation/removal, Make targets and contributor documentation. Run staged-content formatter checks in the hook and CI; use one-time disposable verification, not permanent formatter tests. Close T120 after all slices pass.

- [x] **Record formatter pre-commit hooks and resolve reported Rust formatting errors.**
  Historical formatting checkpoint; superseded by completed T120 and the staged-only hook follow-up above. Use [current formatting guidance](docs/FORMATTING.md), not this record as a workspace-wide formatting claim.

- [x] **T005.1 — Define the evidence-backed quirk registry and resolver.**
  Done: evidence-bounded registry, validated overrides and precedence resolver; Helix showDocument remains evidence-gated. Enforced by `make test-rust FILTER="--test quirks"` against retained initialize declarations, version/name boundaries, capability matching and override validation.
- [x] **T005.2a — Add validated layered quirk settings.**
  Extend existing configuration loading with a quirks table of known registry ids to booleans, including TOML layers and initializationOptions. Test precedence and rejection of invalid ids or values; preserve existing configuration recovery conventions.
- [x] **T005.2b1 — Finish and commit initialize quirk resolution.**
  Finalize the existing production integration and typed override seam, restore unrelated formatting changes and define effective report shape for both presentations. Commit after existing regressions pass; new production acceptance cases remain explicitly owned by T005.2b2.
  Done: effective Plain and Rich capability metadata, validated overrides, and unrelated formatting restored. Enforced by `cargo test --test quirks --test plain_core --test server_async && cargo fmt --all -- --check && git diff --check`; real-server acceptance tests remain in T005.2b2.
- [x] **T005.2b2a — Commit real initialize quirk acceptance tests.**
  Commit the already-passing real-server identity, override, metadata, invalid-config recovery and strict Rich schema tests. Outgoing edit-format proof remains explicitly owned by T005.2b2b.
- [x] **T005.2b2b — Prove quirk-controlled outgoing edit format.**
  Done: real Plain server test proves Neovim 0.12.6 receives `documentChanges` automatically and `changes` with the quirk disabled, despite neither initialize declaring documentChanges. FakeEditor applies each response and sends response/echo frames; final editor text and unchanged `.magit` presentation file are asserted, with no custom render. Verified: `make test-rust FILTER="--test plain_render --test quirks --test plain_core --test server_async" @ d2faf01 → ok: rust`.
- [x] **T005.2b2 — Prove effective quirk resolution through the real server.**
  Add real initialize/config recovery and behavior tests for known and unknown clients, explicit on/off overrides and strict rich negotiation. Complete T005.2b and T005.2 only after these production acceptance cases pass.
- [x] **T005.2b — Resolve and report production initialize quirks.**
  Consume layered settings and the registry at initialize, apply capability overrides and expose effective results; enforce production server behavior and strict rich negotiation in focused tests. Complete T005.2 after both children pass.
- [x] **T005.2 — Integrate quirk configuration and effective initialize reporting.**
  Resolve the registry at the production initialize boundary using layered user configuration, apply capability overrides and report effective results under experimental.magit.effective. Test real server responses, invalid configuration and unknown clients without weakening rich declaration validation.
- [x] **T005.3a — Drive fake-editor behavior from quirk registry models.**
  Add a registry-driven fake-editor configuration seam and permanent behavioral coverage for every modeled quirk. Preserve existing profiles and keep the modeled evidence limits explicit.
  - Verified: `make test-rust FILTER="--test plain_harness --test plain_render --test quirks"` @ f224f94 → passed
- [x] **T005.3b — Prove named server workarounds and universal safety.**
  Cover every named registry workaround through server behavior, including unreliable apply results, echo/hash reconciliation with quirks disabled or absent, and single-document applyEdit requests.
  Done: `65ee6d9`.
  - Verified: `make test-rust FILTER="--test plain_render --test quirks"` @ 65ee6d9 → ok: rust
- [x] **T005.3c — Generate the known-client appendix and audit T005.**
  Generate the client-spec appendix from registry metadata with a permanent drift check through Make. Audit all original T005 acceptance clauses and close T005.3/T005 only when evidence is complete.
  Done: `6c95b50`.
  - Verified: `make update-client-behaviors && make test-rust FILTER="--test quirks --test plain_harness --test plain_render --test plain_core --test server_async"` → ok. T005 clause 1: `tests/quirks.rs` authentic initialize declarations and `src/quirks.rs` registry cover Neovim 0.12.6 and Helix 25.07.1 entries; Helix `showDocument` remains explicit-override-only pending positive evidence. Clause 2: `tests/plain_harness.rs::all_documented_profiles_have_assertable_behavior` and registry-driven model coverage; `tests/plain_render.rs::named_workaround_reconciles_silent_stale_success_and_preserves_universal_safety` covers workaround and universal echo/hash plus single-document applyEdit safety. Unknown clients have zero matches (`tests/quirks.rs::captured_declarations_and_authentic_identity_match`). Clause 3: generated appendix drift check. No new native execution claimed.
- [x] **T005.3 — Enforce quirk behaviors and generate the client appendix.**
  Drive fake-editor models from registry metadata, cover every modeled behavior and named server workaround, generate the Known client behaviors appendix and audit all T005 acceptance clauses. Preserve universal echo/hash safety and one-document applyEdit requests; retain explicit native-evidence limits.
- [x] **Record the approved judgment-based review policy and clarify current phase boundaries.**
  Apply current review and orchestration instructions from [AGENTS.md](./AGENTS.md#working-conventions). Named roadmap phase groups, not tickets, slices or review-fix batches, are actual phase boundaries. Then resume T003.4 and the queue.
- [x] **Limit local verification to Linux and retain cross-platform CI follow-ups.**
  Local execution is Linux-only for now. Record Windows/macOS risks and required CI checks explicitly; do not claim those platforms were executed locally or block Linux progress solely for unavailable non-Linux hosts.
- [x] **Update execution limits and future reviewer selection.**
  Remove the previous deadline and use verified model `openai/gpt-5.6-sol` for future reviewer calls, not worker calls. Skip the additionally discussed review: no review remains pending; retain the prior bounded fallback review and P2 fixes as recorded, and do not advance Last review. Continue the numbered queue; preserve unrelated local changes.
- [x] **T004.final — Audit and close typed capability declaration acceptance.**
  Reconcile all seven T004 acceptance clauses with current implementation, genuine client evidence and enforcing tests. Run the full serialized regression gate, compress the completed numbered ticket with immutable original acceptance and honest evidence limits, and leave actual Phase 2 open for T005–T007.
- [x] **T004.7 — Check presentation emissions against negotiated declarations.**
  Fail library test-build emission of undeclared optional effects, prompt kinds/secret text and placements before fallback can mask the violation. Preserve production warning-message and main-placement fallbacks, document them and cover strict rejection plus actual production serialization; do not turn capability probes into failures or expose a production test-only mode.
- [x] **T004.7a — Complete strict-emitter rejection and placement-fallback tests.**
  Done: strict unit tests reject undeclared effects, prompts, secret text and placements while accepting declared emissions; production tests verify warning and Main-placement fallbacks preserve descriptor identity. Enforced by `cargo test --lib presentation::custom && cargo test --test plain_core --test server_async && cargo fmt --all -- --check && git diff --check`.
- [x] **T004.7b — Cover both secret dependencies and declared optional effects.**
  Supervision found the strict unit suite covers text without secret, but not secret without text; declared-effect success only covers visitFile. Add the missing valid-declaration cases for secret-only rejection and successful editDocument/clipboard emission; preserve existing strict and production coverage.
  Done: `b89bf60`, with secret-only declaration corrected in review commit `78b6f49`; library custom-emitter tests enforce both prompt dependencies and declared-effect success.
- [x] **T004.6 — Derive capability requirements from command metadata.**
  Replace the presentation-layer command-name match with canonical command-owned optional-effect requirements. Cover actual direct and deferred effect-producing paths before closing this parent; retain preflight rejection before Git or editor work.
- [x] **T004.6a — Derive the optional-effect requirement table from the registry.**
  Put requirements alongside registered command metadata and consume that single source in RichCapabilities. Preserve all seven existing initiating-command requirements, including deferred commit editing and selection copying; do not make generic confirmation require clipboard access.
- [x] **T004.6b — Cover actual optional-effect producers against command requirements.**
  Exercise direct visit/copy, selection-copy completion and commit editor initiation with real repository/session paths. Attribute deferred effects to their initiating commands, verify registry requirements and preserve unsupported-capability preflight protections; do not claim exhaustive coverage from a duplicated list alone.
  Done: asserted real VisitFile, Clipboard and shared editor-initiation effect paths against registry metadata; existing rich missing-capability tests retain preflight/no-side-effect coverage. Enforced by focused command, log-selection and editor-bridge tests; this covers exercised producers, not every possible optional-effect path.
- [x] **T004.1 — Use camelCase content-action targets on the wire.**
  Change `ActionTarget` serialization/deserialization to `nodeId` and `mutationHash`, update live Plain test payloads and assert old snake_case payloads are rejected. Preserve content-addressed identity, selection and stale-target protections; do not alter historical native trace fixtures.
- [x] **T004.2 — Remove unconsumed rich rendering and key-delivery modes.**
  Remove runtime support for `forwardAll`, render-only-full and native-fold presentation after confirming neither shipped client declares them. Retain supported dynamic/static keys, acknowledged incremental rendering, server-fold semantics and their real-server tests; the following typed-schema slice removes singleton wire fields.
- [x] **T004.2a — Finish server mode removal and validate supported-mode coverage.**
  Prior slices removed session/capability modes and rewrote tests without committing. Restore the accidentally reverted `ForwardAll` server branch deletion, use workspace rustfmt (not standalone defaults), and verify/commit the existing scoped implementation before checking this child and T004.2.
- [x] **T004.3 — Deserialize strict typed rich capability declarations.**
  Replace hand parsing with serde `MagitCapabilities` and typed key/modifier modes; reject unknown fields, duplicates, invalid values and missing main placement. Remove singleton render/fold fields from live client declarations and tests, remove obsolete Lua capability keys and fallback parsers, and preserve immutable historical captures.
  Done: strict serde deserialization and client/test migration committed. Enforced by focused rich-schema serde/negotiation checks and the listed verification command; Lua suite passed separately in `7678242`. T004 parent and T004.4 remain open.
- [x] **T004.3a — Pin Lua verification to the server built by Make.**
  Lua negotiation failures came from stale `target/release/magit-lsp` selected by production lookup, while Make built debug. Pin `test-lua` and `test-file` subprocesses to the freshly built debug server without changing production lookup; then finish strict-schema verification.
- [x] **T004.3b — Pin native UI verification to the freshly built server.**
  The isolated native driver drops parent environment variables and currently permits the same stale-release lookup already diagnosed for Lua tests. Set its default server to this checkout's debug binary, retain explicit scenario overrides and make native test/tour targets build first; verify harness tests and text-native tour against the strict protocol.
  Done: pinned isolated driver environment, retained `extra_env` override and made `ui-test`/`ui-tour` depend on `build`. Enforced by `make ui-unit && timeout --kill-after=10s 900s make ui-tour && git diff --check`.
- [x] **T004.4 — Verify real client declarations and repair VSCode initialization wiring.**
  Confirm both actual clients send the new typed block, including the bundled VSCode omission observed in T002. Use supported languageclient hooks and only honored VSCode placements; retain focused native Linux text/wire evidence and test both actual declarations against server validation.
- [x] **T004.4a — Register and natively verify the real VSCode declaration feature.**
  Replace the unsupported initialize middleware with the installed languageclient StaticFeature API, declare only honored main placement and correct fake-client initialization to model that real API. Verify the actual isolated VSCodium extension host sends the strict block and receives rich service; preserve authentic client identity and immutable older captures.
- [x] **T004.4b — Validate both current native client declarations against the typed schema.**
  Capture the current bundled Neovim declaration and retain focused native text/wire evidence for both clients. Test actual emitted blocks against strict server deserialization, keeping historical trace presets immutable and T004 open for its remaining surface/command/emitter audit.
- [x] **T004.4b1 — Retain current native Neovim declaration evidence.**
  Use the isolated bundled native driver with the freshly built server and opt-in wirelog. Record the exact emitted strict block, client/source identity, rich-render response, text inspection and owned cleanup in the existing capability evidence document; preserve historical captures.
- [x] **T004.4b2 — Test the actual native client blocks against strict deserialization.**
  Extract current Neovim and VSCode declarations from their genuine recorder sources into one new provenance-bearing fixture. Enforce both with production serde/negotiation tests and reject schema drift mutations; document that source equality and native causality are manually verified, not live CI recapture.
- [x] **T004.5a — Remove unused process-log identifiers from readable errors.**
  Audit actual Rust, client and test consumers, then remove reserved `process_log_id`/`processLogId` error metadata that is never populated or consumed. Preserve readable Git failure messages, hints/details and process-log functionality; keep a regression proving no internal identifier leaks onto the wire.
- [x] **T004.5b — Remove the unproduced OpenView effect alias.**
  Confirm real producers use PushView, then remove OpenView from protocol, presentation/server matches and both client dispatchers without changing push/root semantics. Preserve real prompt consumers and defer unrelated UI surface changes.
  Done: `3627f46` (implementation `b16685d`/`f6a859d`); enforced by Rust stack/schema checks, awaited VSCode root propagation, both focused native clients and serialized full regressions.
- [x] **T004.5b1 — Remove the unproduced Rust OpenView effect variant.**
  Delete only `Effect::OpenView`, its dead matching branches and obsolete variant documentation; preserve the unrelated session `OpenView` state type and real PushView producers. Assert that PushView remains valid and the removed wire alias is rejected; client/documentation cleanup follows separately.
  Done: `b16685d`; enforced by `cargo test --lib presentation::custom` and the plain_stack/plain_core/server_async suites.
- [x] **T004.5b2 — Remove client OpenView aliases and reconcile the public contract.**
  Keep existing PushView root/return semantics while removing both client aliases and normative OpenView effect documentation. Verify focused native Linux stack behavior in both clients; preserve unrelated dirty README changes and immutable historical captures.
  Done: `3627f46`; enforced by the preceding client/native checks and the current contract audit.
- [x] **T004.5b2a — Delete client effect aliases and verify native stack behavior.**
  Remove the two client OpenView dispatch aliases without changing existing PushView root propagation. Run the focused Neovim close/reopen scenario and VSCode A06 native stack checks; commit the already staged README-only alias correction without staging user release edits.
  Done: `f6a859d`; enforced by VSCode unit/integration tests, `close-reopen-views.scn` and native A06 evidence recorded above.
- [x] **T004.5b2b — Reconcile remaining public view-effect documentation.**
  Remove the retired alias from CLIENT_SPEC and this effect table. Root creation uses `view/open` and its returned descriptor; PushView/pop retain stack semantics. Done: docs cleanup and native Neovim close/reopen plus VSCode A06 evidence (`/tmp/opencode/openview-alias-checkpoint/a06.log`, `/tmp/opencode/vscode-review-dQ43aA/a06-evidence.json`). Enforced by this check and `git diff --check`.
- [x] **T004.5c — Consolidate the duplicate recording UI test helper.**
  Retain one shared `tests/common/recording_ui.rs` recorder with ordered structured render/effect/prompt payload assertions. The `multiChoice` prompt remains: Lua `prompt.lua` and VSCode `prompt.js` consume it; `tests/data/client-contracts.json` retains selection, empty and cancel contracts. This is client/contract coverage, not evidence of a producing command. CustomRender's notification-wire coverage remains in both focused suites.
  Done: `b5eb865`; enforced by `cargo test --lib presentation::custom`, `cargo test --test plain_core` and serialized full Linux regression gates recorded above.
- [x] **T002 wire-recorder — Complete.**
  Done: `77bc34d`; portable logger `e34ce5a`/`f7d4bea`; Unicode redaction `6dd4f7e`. Runtime recorder/security regressions remain in `tests/wirelog.rs`, exercised through `make test-rust FILTER="--test wirelog"`.
  Original acceptance: immutable `61370ad:TASKS_PART_SEVEN.md`. T002 capture slices, source hashes, native action limits and retired fixture-gate records: immutable `83f79f5:TASKS.md`; final historical Helix fixture work: `8476a8e`. These are provenance, not current enforcement or reproducible capture instructions.
  Current coverage replaces static trace assertions with live server/fake-editor tests, including stale rapid repair and undo (`83f79f5`), and retained native Neovim acceptance. See completed trace cleanup above and the [fixture guide](tests/fixtures/lsp_traces/README.md). Linux evidence does not establish Windows/macOS or full native-client conformance.
- [x] **T003.1 — Configure Lua and VSCode hygiene entry points.**
  Add scoped luacheck and knip configurations for actual shipped code, tests and scripts. Declare editor-provided globals/modules explicitly; do not hide findings with blanket exclusions. Keep T003 open until runner, reasoned baselines and policy verification are complete.
- [x] **T003.2 — Add the report-only hygiene runner and focused tests.**
  Add `make hygiene` running cargo-machete, knip and luacheck, reporting missing tools distinctly. Include the twenty largest nongenerated tracked files, a soft growth threshold, and `allow(dead_code)`/`keep-for:` searches; test report-only behavior and failure reporting.
- [x] **T003.3 — Audit hygiene findings and validate unused-dependency detection.**
  Run actual tools in an isolated tool environment, retain each baseline finding with a reason, and prove an intentional unused dependency is reported before removing it. Preserve the user's dirty Cargo.toml exactly outside any explicitly scoped metadata change.
- [x] **T003.2a — Enforce generated-file filtering and hermetic reporter tests.**
  Correct the runner's Git attribute parsing: current text splitting retains `: linguist-generated` in keys and therefore never excludes generated paths. Add actual Git-attribute coverage and make missing-tool tests independent of installed host tools; exercise invocation failures and real test cwd layout explicitly.
- [x] **T003.4 — Document phase hygiene and complete the tooling contract.**
  Add AGENTS phase-end/review guidance, ticket-template and T033 hygiene gates. Verify the completed report-only baseline and narrow tests; record Linux evidence and non-Linux CI follow-ups before marking T003 complete.
Historical T002.1/T002.2 recorder integration, concurrent append/restart fixes and native capture records remain in immutable `83f79f5:TASKS.md` (client capture commits `15a8f63`, `1563bbb`). Current fixture consumers retain only three quirk initialize inputs; current client declarations are tested separately. No old JSONL/sidecar parser gate remains an acceptance requirement.
- [x] **T001.5b2 — Replace superseded task and history sections.** Replace old §7 with the numbered queue link and explicit gaps; remove §§10–12 and obsolete checkpoint/ad hoc records; retain template and honest limits.
- [x] **T001.5c — Remove duplicate top matter and retired Plain plans.**
  Remove the superseded checkpoint/Contents block left by cutover. Delete both old Plain planning files after confirming their open IDs map to the existing migration ledger; preserve immutable source references and any actual unmatched issue. Plain client scope: T004–T010, T013, T020–T022; keys scope: T034–T039; deferred/speculative scope: T099–T111 where applicable. Immutable sources: `2ff1ee7:magit-lsp-plain-client-tasks.md`, `2ff1ee7:magit-lsp-plain-keys-tasks.md`; mapping and unmatched-requirement audit: [migration ledger](docs/evidence/task-queue-migration.md).
- [x] **T001.4b — Qualify historical visual evidence.**
  Add historical banners to evidence documents that cite images; remove PNG passages from the real-file spike without relabelling visual observations as current text proof. Preserve substantive API/wire/Git evidence.
- [x] **T001.4c — Isolate human-only README image guidance.**
  Make screenshot/releasing guidance explicitly human-only, with AGENTS links and historical caveats. Existing screenshot code is absent; provide a non-capturing human help target and directory documentation rather than adding agent image paths.
- [x] **T001 docs-diet — Complete.**
  Done: `d12d5a4`. Enforced by `git diff --check` and bounded migration assertions. Residuals: A12 ownerless ([ledger](docs/evidence/task-queue-migration.md)); R-EDIT-REAL not green, T104 subset only; binary-platform limits and immutable provenance retained (`cbec0bf:TASKS.md`).
- [x] **Retained gap — A12 real-dispatch Git failure handling.** Original scope and acceptance: `da4d422^:REVIEW_REPORT.md` A12. Migrated to `Session`/stdio seams; `server::mutation` and its callers are absent. `tests/server_async.rs::a12_selection_stage_reports_index_lock_as_successful_error_effect` verifies readable real-dispatch failure effects, unchanged Git state, malformed-input JSON-RPC errors and stderr; `tests/errors.rs` and `tests/dispatch.rs` cover error conversion and dispatch. T103 remains the distinct deferred same-session reader-lock redesign. Immutable provenance: `d04dfa2:TASKS.md`; implementation: `4395f73`, `5bb9903`.
  - Verified: `make test-rust FILTER="--test errors --test dispatch --test views --test server_async"` @ `6688ed1` → ok: rust
- [x] **Retained decision — R-NAV.** Four tree-owned commands verified; immutable evidence: `da4d422:TASKS.md`.
- [x] **T001.3b — Compress completed correctness and editor contracts.** Immutable evidence and enforcing checks retained in `da4d422:TASKS.md`.
- [x] **T001.4a — Migrate versioned VSCode host facts and retire NOTES.** Evidence: `docs/evidence/vscode-host-behaviours.md`.
- [x] **T001.5a — Align design and test documents with the replacement queue.**
- [x] **T001.5b1 — Preserve immutable per-ticket scope and verification provenance.** See [migration ledger](docs/evidence/task-queue-migration.md).

Checkpoint (2026-11-05): bounded review through `d90d5da`, not comprehensive audit. `make test FILTER=--no-fail-fast`, rustfmt check and text-native UI tour passed; logs under `/tmp/opencode/queue-checkpoint/`. Fallback peer review was bounded; no comprehensive source-behavior or acceptance audit claimed. T001 documentation migration is complete; broader review remains bounded.

Canonical roadmap and queue; architecture and glossary: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). See [AGENTS.md](./AGENTS.md). Behaviour claims require current source/tests. Project status: early stage; backward compatibility is not a goal.

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

### Feature — Phase 2 (open)

T005 is complete through its authoritative [Ad hoc children](#ad-hoc); full scope and original acceptance: [T005](./TASKS_PART_SEVEN.md#t005-quirks-table-m).
- [x] **T006 — fake-editor-model.** [Specification](./TASKS_PART_SEVEN.md#t006-fake-editor-model-l).
  - Done: containing commit.
- [ ] **T007 — plain-capability-fixes.** [Specification](./TASKS_PART_SEVEN.md#t007-plain-capability-fixes-m).

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

- **A12:** real-dispatch Git-failure handling and removal of `server::mutation`; no numbered owner evidenced. Do not conflate with T103's separate same-session reader-lock redesign.
- **R-EDIT-REAL:** parent acceptance is not green. T104 is only a subset; last-window exit, dirty shutdown, cross-build, reload, workspace and platform limits remain. Broad fallback review is not comprehensive acceptance.
- Binary files remain whole-file-only; T093 layout work remains open.
- §9 retains unresolved config-directory/root-edit and Escape caveats.

### Task ticket template and UI rule

Every ticket states **Target Files** (exclusive write boundary), **Reference Files**, **Architectural Constraints**, **Acceptance Criteria**, and exact **Verification Command**; also state dependencies and size. Each ticket inherits its applicable phase gate from its enclosing named phase and the phase map. The phase gate applies when the last unchecked item in a named phase group completes, not when an individual ticket, slice or review-fix batch completes. Follow current review and orchestration instructions in [AGENTS.md](./AGENTS.md#working-conventions). User-visible changes require observable text/structured outcomes and focused native evidence; never images.

---

## 8. Testing strategy

See [Testing strategy](docs/TESTING.md#8-testing-strategy).

## 9. Open questions

See [Open questions](docs/OPEN_QUESTIONS.md#9-open-questions).

---
