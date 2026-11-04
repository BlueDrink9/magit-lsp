# magit-lsp — Roadmap

Last review: 14023109f73f235d35b7631b47c93a005cda4ade

## Ad hoc

- [x] **Record the approved judgment-based review policy and clarify current phase boundaries.**
  Proposed orchestration configuration replacement is recorded in [AGENTS.md](./AGENTS.md#working-conventions); it is not active until orchestration instructions change. Until then, the active higher-priority eight-task review rule remains. Named roadmap phase groups, not tickets, slices or review-fix batches, are actual phase boundaries. Then resume T003.4 and the queue.
- [x] **Limit local verification to Linux and retain cross-platform CI follow-ups.**
  Local execution is Linux-only for now. Record Windows/macOS risks and required CI checks explicitly; do not claim those platforms were executed locally or block Linux progress solely for unavailable non-Linux hosts.
- [x] **Update execution limits and future reviewer selection.**
  Remove the previous deadline and use verified model `openai/gpt-5.6-sol` for future reviewer calls, not worker calls. Skip the additionally discussed review: no review remains pending; retain the prior bounded fallback review and P2 fixes as recorded, and do not advance Last review. Continue the numbered queue; preserve unrelated local changes.
- [ ] **Work through `TASKS_PART_SEVEN.md` without a deadline.** Continue autonomously until interrupted, following dependencies and retaining required validation and reviews; preserve unrelated local changes.
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
- [x] **T002.1a — Build the redacted JSONL recorder and framing tests.**
  Implement a reusable recorder with ordered background file writes and recursive identity/credential redaction. Test direction/order, JSON validity, real framing replay and credential absence before server integration.
- [x] **T002.1b — Wire opt-in recording into the actual server transport.** Actual-binary opt-in, disabled/error paths, drain and collision-safe redaction; `cargo test --test wirelog`.
- [x] **T002.2a — Capture plain Neovim declarations and edit shapes.**
  Done: captured initialize params and three genuine didChange messages from plugin-free Neovim 0.12.6; validated JSON and source equality. Evidence: `tests/fixtures/lsp_traces/`. Partial coverage only: full replacement, insertion and deletion observed; remaining T034 shapes, exact launch/cleanup commands and complete conformance not captured.
- [x] **T002.2b — Capture bundled Neovim declarations and native edits.**
  Done: captured bundled Neovim initialize and authentic native didChange. Evidence, exact commands, observed draft/edit state, source hash and cleanup: `tests/fixtures/lsp_traces/README.md`.
- [x] **T002.2c1 — Forward recorder configuration through the native VSCode launcher.**
  Add only the opt-in wirelog variable to the isolated launcher's environment allowlist; preserve existing isolation and native text-only behavior. The following capture slice supplies actual-host evidence.
- [x] **T002.2c2 — Prevent multi-session wirelog corruption.**
  Native VSCode capture exposed concurrent recorder truncation: `/tmp/opencode/t002-vscode/raw.jsonl` contains 121972 NUL bytes amid valid records. Preserve complete JSONL across recorder instances/processes sharing a path, serialize complete lines off the LSP hot path, and test overlap/restarts without losing prior records.
- [x] **T002.2c — Capture bundled VSCode declarations and native edits.**
  Retain same-PID initialize and first native change with observed omission of `experimental.magit`; native provenance, versions, invocation and cleanup are documented in the trace README. Mismatch remains for T004 audit. Enforced by `cargo test --test wirelog` and JSON/source-equality checks.
- [x] **T002.2d — Capture current Helix declarations and edit shapes or record availability limits.**
  Done: captured Helix 25.07.1 initialize and native edits; documented bounded coverage, provenance and limits in `tests/fixtures/lsp_traces/README.md`. Enforced by `cargo test --test wirelog` and trace JSON parsing.
- [x] **T002.2e — Audit capture provenance and enumerate missing edit shapes.**
  Done: trace README documents Helix reproduction, audited rows and explicit action matrix; no input action inferred from payload shape.
- [x] **T002.2 — Capture real client initialize and change evidence.**
  Done: four client captures and versioned initialize presets retained; recorder implementation `e34ce5a`, client captures `15a8f63`, `1563bbb`. Reconstructed trace remains distinct. Enforced by `cargo test --test wirelog`, JSON parsing and source audit in trace README. T034 six-action Helix/Neovim coverage remains incomplete as enumerated there; no full conformance claim.
- [x] **T001.5b2 — Replace superseded task and history sections.** Replace old §7 with the numbered queue link and explicit gaps; remove §§10–12 and obsolete checkpoint/ad hoc records; retain template and honest limits.
- [x] **T001.5c — Remove duplicate top matter and retired Plain plans.**
  Remove the superseded checkpoint/Contents block left by cutover. Delete both old Plain planning files after confirming their open IDs map to the existing migration ledger; preserve immutable source references and any actual unmatched issue. Plain client scope: T004–T010, T013, T020–T022; keys scope: T034–T039; deferred/speculative scope: T099–T111 where applicable. Immutable sources: `2ff1ee7:magit-lsp-plain-client-tasks.md`, `2ff1ee7:magit-lsp-plain-keys-tasks.md`; mapping and unmatched-requirement audit: [migration ledger](docs/evidence/task-queue-migration.md).
- [x] **T001.4b — Qualify historical visual evidence.**
  Add historical banners to evidence documents that cite images; remove PNG passages from the real-file spike without relabelling visual observations as current text proof. Preserve substantive API/wire/Git evidence.
- [x] **T001.4c — Isolate human-only README image guidance.**
  Make screenshot/releasing guidance explicitly human-only, with AGENTS links and historical caveats. Existing screenshot code is absent; provide a non-capturing human help target and directory documentation rather than adding agent image paths.
- [x] **T001 docs-diet — Complete.**
  Done: `d12d5a4`. Enforced by `git diff --check` and bounded migration assertions. Residuals: A12 ownerless ([ledger](docs/evidence/task-queue-migration.md)); R-EDIT-REAL not green, T104 subset only; binary-platform limits and immutable provenance retained (`cbec0bf:TASKS.md`).
- [ ] **Retained gap — A12 real-dispatch Git failure handling.** Exact scope: real-dispatch Git-failure handling and removal of `server::mutation`; no numbered owner evidenced. Distinct from T103's same-session reader-lock redesign. Immutable provenance: `d04dfa2:TASKS.md`; details: [migration ledger](docs/evidence/task-queue-migration.md).
- [x] **Retained decision — R-NAV.** Four tree-owned commands verified; immutable evidence: `da4d422:TASKS.md`.
- [x] **T001.3b — Compress completed correctness and editor contracts.** Immutable evidence and enforcing checks retained in `da4d422:TASKS.md`.
- [x] **T001.4a — Migrate versioned VSCode host facts and retire NOTES.** Evidence: `docs/evidence/vscode-host-behaviours.md`.
- [x] **T001.5a — Align design and test documents with the replacement queue.**
- [x] **T001.5b1 — Preserve immutable per-ticket scope and verification provenance.** See [migration ledger](docs/evidence/task-queue-migration.md).

Checkpoint (2026-11-05): bounded review through `d90d5da`, not comprehensive audit. `make test FILTER=--no-fail-fast`, rustfmt check and text-native UI tour passed; logs under `/tmp/opencode/queue-checkpoint/`. Fallback peer review was bounded; no comprehensive source-behavior or acceptance audit claimed. T001 documentation migration is complete; broader review remains bounded.

Canonical roadmap and design; see [AGENTS.md](./AGENTS.md). Behaviour claims require current source/tests. Project status: early stage; backward compatibility is not a goal.

## Contents

0. Decisions already made
1. Glossary
2. Completed correctness contracts
3. Design decisions (expanded)
4. Protocol reference
5. Default keymaps (profiles)
6. View catalog
7. Numbered implementation queue ([`TASKS_PART_SEVEN.md`](./TASKS_PART_SEVEN.md))
8. Testing strategy
9. Open questions

---

## Bugfix

- [x] **Checkpoint — Establish the staged-successor failure cause.**
  Capture exact pre/post structured node identities, hunk IDs and rendered cursor for the single failing views case. Distinguish user folding, bounded overflow, metadata identity or cursor re-resolution using concrete output; retain a concise diagnosis and source paths before changing production behavior.
  Diagnosis correction: the command explicitly overrides the generic successor with the next unstaged sibling or source ancestor before staging (`src/commands.rs:1782–1818`, `src/session.rs:2168–2178`). This is intentional source-workflow behavior, not a coordinate bug or hidden-visibility issue. `tests/navigation.rs` and `tests/ui/scenarios/stage-next-hunk.scn` enforce source fallback; external staging separately follows content identity.
- [x] **Checkpoint — Align staging-cursor regression with the source-successor contract.**
  Root cause established: `d755c64` intentionally keeps staging cursor on next unstaged sibling/source ancestor, never staged relocation (`src/commands.rs:1782–1798`, session cursor override at 2168–2178; `tests/navigation.rs` and `stage-next-hunk.scn`). Correct the stale views test to assert exact source-section successor while retaining incremental edits, staged content identity and real Git state. Keep separate external-stage identity-following regression; do not change production behavior.
- [x] **Checkpoint — Restore log-menu key routing and native passthrough.**
  Both profiles bind status `open.log` to `l`; update stale Vim test input from `g l` to `l`. Captured menu, log opening, `q` return and native-key passthrough remain covered. Done: current change; enforced by `cargo test --test log_commands`.
- [x] **Checkpoint — Restore full Git-backend hunk comparisons.**
  Expected staged hunks now use `parse_file_diff`, preserving file metadata in full equality checks. All 41 real-Git backend tests pass; verified `cargo test --test git_backend && git diff --check`.
- [x] **Checkpoint — Resolve Rust formatting-gate drift.**
  `cargo fmt --all -- --check` reports existing workspace-wide formatting differences; log `/tmp/opencode/queue-checkpoint/fmt.log`. Apply rustfmt only to the reported Rust files, preserve unrelated user edits, and verify the full formatting check.

## Review

Checkpoint review (2026-11-05): GPT-5.6 Sol reviewed `464d5e4b06df8d22cacf273f0c458b55ba4bc151..a70df920a165885e73cd315c29bf3c27caf78564`. Coverage was limited to all nine named changed paths, the 222-line raw baseline at `/tmp/opencode/t003-hygiene/baseline.log`, and relevant surrounding VSCode package scripts/import graph, `package.json`/`package-lock.json`, generated-file rules, and T003 acceptance text. Found four P1 and one P2 tasks; no P0. No commands or runtime checks were run during this review. The supplied Linux full-test, rustfmt, UI-tour, five-unit-test, cargo-machete 0.9.2, Knip 5.88.1, and luacheck 1.2.0 results remain worker evidence, not independent reviewer execution. This review does not claim Windows/macOS execution or full repository coverage. P3 deferred: none substantiated.

- [x] **[P1] Report checker failures without calling them findings** (`scripts/hygiene.py:32-48`, `run_check`; `tests/hygiene/test_hygiene.py:43-60`, `test_nonzero_checker_is_reported_but_runner_succeeds`)
  - **Problem:** `run_check` labels every nonzero exit as `FINDINGS` unless stderr happens to contain “no such command” or “not found”; the test even requires an arbitrary cargo exit 7 to be called findings. Invalid options, broken configs, crashes, and other checker failures can therefore produce a reassuring findings-only report instead of an honest failed-invocation status.
  - **Why it matters:** This violates correctness and cleanliness of the report-only contract. A phase gate cannot distinguish analyzable source findings from a checker that never completed successfully.
  - **Change:** Replace the stderr substring heuristic with explicit result semantics. Keep `CLEAN`, `MISSING`, and caught `OSError` statuses; use `FINDINGS` only for exit statuses/output that the specific tool contract identifies as a completed analysis, and otherwise print an unambiguous `CHECK FAILURE`/`INVOCATION FAILURE` with the exit code. If Knip cannot distinguish findings from configuration failure by exit code alone, use an honest neutral `NONZERO` status rather than guessing. Update focused stubs to cover a real findings result, arbitrary exit 7, missing cargo-machete subcommand, and malformed/invalid invocation while retaining overall report exit 0.
  - **Done when:** Output never labels an arbitrary failure as findings; tests prove missing, launch failure, completed-clean, completed-findings where distinguishable, and unknown nonzero statuses remain distinct; `make hygiene` stays report-only.
  - **Constraints:** Do not make source findings fail `make hygiene`, parse prose to infer success, hide checker output, or require network access.

- [x] **[P1] Model the real VSCode entry graph in Knip** (`clients/vscode/knip.json:3-16`, `entry`; `clients/vscode/package.json:15-27`, `main` and `scripts`; `clients/vscode/test/e2e/run.mjs`, host-loaded test modules)
  - **Problem:** Every `test/**/*.js` and `test/**/*.mjs` file is declared as an entry, including helpers reached from `test/test.js`, `test/integration.js`, and `test/e2e/run.mjs`. Treating every helper as independently executable prevents Knip from detecting unreachable test files and imports, while redundant ignore/entry hints confirm the config does not describe the actual package graph.
  - **Why it matters:** This violates correctness and cohesion: the new checker can report a baseline while structurally hiding dead VSCode test code.
  - **Change:** Replace test globs with the actual package/test/script process roots. Include `extension.js` through package metadata or explicitly, package-script roots, documented standalone E2E launchers, and only modules loaded directly by generated VSCode hosts that static analysis cannot reach; add a short reason beside any exceptional host-loaded entry. Remove redundant `node_modules/**` and `out/**` ignores if Knip still identifies them as redundant. Rerun Knip 5.88.1 and update only the Knip counts/list in `hygiene-allowlist.md` from the corrected graph.
  - **Done when:** Removing an otherwise unreachable VSCode helper from all real roots makes Knip report it; all actual package scripts and generated-host extension roots remain represented; configuration hints no longer report redundant patterns; the baseline states the new exact unlisted-dependency and unused-export counts.
  - **Constraints:** Do not blanket-ignore test/E2E directories, suppress unused exports, remove runtime code based only on Knip, or claim native VSCode execution from this static check.

- [x] **[P1] Check Lua against Neovim's actual LuaJIT standard** (`.luacheckrc:1-6`, `std`; `hygiene-allowlist.md:59-83`, `Luacheck`)
  - **Problem:** The comment says the config models Neovim's runtime, but `std = 'lua51'` omits LuaJIT library additions and produces the baseline's `table.unpack` warning. This mixes a real supported-runtime check with a checker-induced compatibility finding.
  - **Why it matters:** This violates cohesion and readability of the Lua baseline. False runtime diagnostics obscure actionable warnings.
  - **Change:** Use luacheck 1.2.0's LuaJIT/Neovim-appropriate standard while retaining the explicit editor-provided `vim` global and current source scope. Rerun the pinned luacheck, remove only diagnostics eliminated by the corrected runtime model, and update the exact warning/error count in `hygiene-allowlist.md`.
  - **Done when:** Standard-library symbols available in the supported Neovim LuaJIT runtime are accepted, undefined project globals remain reported, and the baseline exactly matches fresh luacheck 1.2.0 output.
  - **Constraints:** Do not add project globals to silence source bugs, blanket-disable warning classes, narrow checked source paths, or claim runtime execution.

- [x] **[P1] Complete marker ownership and baseline reconciliation.**
  Split remainder after two partial checkpoints: keep the existing per-export/Lua/size follow-up draft, explicitly disposition every marker, correct the guarded-unpack evidence, and reconcile the report snapshot. Verified `nix shell nixpkgs#cargo-machete nixpkgs#luajitPackages.luacheck --command npm exec --yes --package=knip@5.88.1 -- make hygiene && git diff --check`: exit 0; expected Knip and luacheck findings remain report-only.
- [x] **[P1] Give every retained hygiene finding an owner or reason** (`hygiene-allowlist.md`, `Knip`, `Luacheck`, and `Size and marker report`; `TASKS.md`, `T003.3` and `T003.4`)
  - **Problem:** The baseline itemizes findings but leaves the unlisted dependency, all unused exports, all luacheck warnings, and six over-limit files under generic “validate later,” “follow-up policy,” or “may need treatment” language. T003 acceptance requires each finding to be deleted, retained for a concrete reason, or assigned to a ticket; a pending investigation note is not a disposition or owner.
  - **Why it matters:** This violates correctness and cleanliness of the baseline and makes checked T003.3 overstate completion. Future runs cannot tell accepted debt from unowned work.
  - **Change:** After the Knip-entry and Lua-runtime tasks, rerun the pinned tools and reconcile exact counts. For each remaining finding, record either a concrete evidence-based retention reason or the exact unique unchecked task title that owns its resolution; grouping is allowed only when one reason/action genuinely applies to every named member. Add small unchecked tasks outside `## Review` when no existing ticket owns the work. Resolve or explicitly ticket the direct `vscode-jsonrpc/node` import instead of describing its transitive availability as a future investigation. Give each over-limit file its own rationale or owner.
  - **Done when:** Every finding in the fresh cargo-machete/Knip/luacheck/size/marker report maps to deletion, a specific retention reason, or an exact unchecked task title; raw and summarized counts agree; no entry relies on “legacy,” “review later,” “policy work,” or T003.4 alone as its rationale.
  - **Constraints:** Do after: `Model the real VSCode entry graph in Knip` and `Check Lua against Neovim's actual LuaJIT standard`. Do not suppress findings merely to make the report clean, declare dynamic/public consumers without evidence, change the user's dirty `Cargo.toml`/`README.md`, touch untracked release files, or mark parent T003/T003.4 complete.

Checkpoint closure (2026-11-05): all five hygiene review findings are resolved. Seven focused tests, workspace rustfmt and diff checks pass after fixes; the real pinned-tool report still exits 0 with cargo clean, one Knip dependency, 69 unused exports and 34 Lua warnings, each retained under an explicit owner/reason (`/tmp/opencode/hygiene-checkpoint/after-review.log`). Full Linux tests and text-native tour passed before the tool/config/test-only fixes. Original user manifest checksum is unchanged; no owned server remains. No P0 requires re-review; no P3 was deferred. T003.4 policy/completion remains next; T002 still lacks five native action classes per editor.

### Completed checkpoint review task

- [x] **[P2] Make hygiene tests portable and prove checker working directories** (`tests/hygiene/test_hygiene.py`)
  Done: `caf5588`, `0550f36`, `33d0554`. Seven Linux tests pass, including inherited `HYGIENE_SOFT_LIMIT=37`; real Git supplies tracked/generated state while checker calls are isolated. Exact cargo, Knip and Lua argv/cwd, generated marker/size exclusions and configured threshold are asserted. Windows/macOS execution remains a CI follow-up.
  - **Problem:** The harness assumes Git exists at `/usr/bin/git` or `/bin/git` and creates a symlink, which fails on valid non-FHS Linux environments and is not portable to existing macOS/Windows CI. The cwd test creates `clients/vscode` but never observes `$PWD`, so its name and T003.2a's checked claim are unsupported.
  - **Why it matters:** This violates correctness and readability of the focused tests. A green test neither proves Knip runs from the client root nor reliably runs wherever Git is installed.
  - **Change:** Resolve Git from the original environment before replacing `PATH`, then expose only that executable through a portable wrapper or controlled path without assuming FHS locations or symlink support. Make executable stubs record their argv and cwd; separately test the invalid-executable `OSError` path. Assert cargo-machete and luacheck run at repository root, Knip runs at `clients/vscode`, and luacheck receives only expected tracked nongenerated Lua paths.
  - **Done when:** The five focused tests pass on Linux with Git outside `/usr/bin`/`/bin`; cwd and argv assertions fail if any checker is launched from the wrong directory; platform-specific Windows/macOS risk remains explicit until CI runs.
  - **Constraints:** Do not consult host-installed hygiene tools, weaken PATH isolation, require network access, or claim non-Linux execution.

Checkpoint closure (2026-11-05): all three findings below are resolved (`f123a20`, `7b578b0`, `e08837c`). The final Helix shell recipe passed a fresh Linux execution from a checkout subdirectory; committed fixtures remain unchanged. Linux full tests, rustfmt and text-native tour passed (`/tmp/opencode/helix-checkpoint/`); the subsequent test-only fixture contract passed `cargo test --test wirelog` and rustfmt. Four owned servers left by unsuccessful recipe attempts were identified by isolated capture cwd/environment and closed; no `magit-lsp` remained. No P0 requires re-review; no P3 deferred. T002 remains open for five missing action classes per editor; independent T003 hygiene is next.

Checkpoint review: GPT-5.6 Sol reviewed `8ecba12dc44f321daaa43dec773f374616bb6b67..464d5e4b06df8d22cacf273f0c458b55ba4bc151`. Inspected only `AGENTS.md`, `TASKS.md`, `TASKS_PART_SEVEN.md`, `src/wirelog.rs`, `tests/wirelog.rs`, and `tests/fixtures/lsp_traces/{README.md,bundled-neovim-0.12.6.jsonl,helix-25.07.1.jsonl,initialize-helix-25.07.1.json}` plus fixture-test references under `tests/`. Three findings: one P1 and two P2; no P0. Accepted the checked T002.2 four-client baseline while leaving queue parent T002 open for the five missing T034 native-action classes per editor. Did not rerun tests or claim non-Linux execution; relied on the supplied Linux gate results. Logger changes were reviewed only as prior P1 resolutions, with no speculative expansion. P3 deferred: none substantiated.

- [x] **[P1] Finish native Helix smoke shutdown and recipe verification.**
  Split remainder after two partial checkpoints: preserve the existing recipe improvements, exit insert mode before issuing a real Helix quit command, verify a fresh bounded run and drain, and commit the corrected README. This completes the reviewed recipe task below, not new action-shape coverage.
- [x] **[P1] Make the Helix capture recipe preserve verifiable evidence** (`tests/fixtures/lsp_traces/README.md`, `Native Helix 25.07.1 capture`)
  - **Problem:** The reproduction commands hard-code one checkout path, send input before any bounded readiness check, kill tmux instead of closing Helix cleanly, and delete `$D` before hashing or extracting the raw log. The README gives a source hash but no retained source path or extraction commands, so following the documented recipe cannot reproduce or audit the committed fixture and initialize preset.
  - **Why it matters:** This violates correctness and readability of capture provenance. The existing capture may be authentic, but its documented procedure neither reliably drains the recorder nor shows how the committed evidence was derived.
  - **Change:** Rewrite only the Helix recipe to derive the server path from the repository root, create the fixture repository explicitly, wait with a bounded condition until the initialize row exists before sending native keys, exit Helix gracefully, and wait for the process to close. Add exact commands that hash the raw recorder file, extract the retained same-PID initialize and three `didChange` rows in source order, derive `initialize-helix-25.07.1.json` from the retained initialize params, and compare both generated files with the committed fixtures before optional cleanup. Name the raw source location used for the stated SHA256, or state honestly that only its hash survives.
  - **Done when:** A fresh Linux run can follow the text from an arbitrary checkout, produces a drained raw JSONL, verifies the documented source hash, regenerates byte-equivalent or explicitly normalized committed Helix files, and performs cleanup only after those checks.
  - **Constraints:** Do not recapture with another Helix binary, alter captured payloads, claim Windows/macOS execution, add image evidence, or expand T034 action coverage. Preserve the confirmed Helix 25.07.1 provenance and all explicitly unavailable actions.

- [x] **[P2] Enforce trace and initialize-preset consistency** (`tests/wirelog.rs:1-332`; `tests/plain_harness.rs:242-250`, `reconstructed_trace_is_valid_jsonl`; `TASKS.md:25-30`, `T002.2d`, `T002.2`)
  - **Problem:** `cargo test --test wirelog` never reads `tests/fixtures/lsp_traces/`, while the completed task text cites that test and trace parsing as enforcement. Only `reconstructed-plain.jsonl` has a parsing test, so malformed current-client JSONL or a preset drifting from its retained initialize request passes the named gate.
  - **Why it matters:** This violates correctness and cohesion: versioned evidence is part of T002 acceptance, but its claimed regression gate covers recorder behavior rather than committed evidence.
  - **Change:** Add one focused fixture test, preferably in `tests/wirelog.rs`, that parses every nonempty line of every committed `*.jsonl`, parses every `initialize-*.json`, and asserts each current-client preset exactly equals the `params` of the matching retained initialize request. Keep the reconstructed trace parse-only because it has no real-capture preset. Update the T002 enforcement wording only if the final test command differs.
  - **Done when:** `cargo test --test wirelog` fails for malformed fixture JSON, a missing initialize row, or any mismatch between a current-client initialize preset and its matching trace; all committed fixtures pass.
  - **Constraints:** Do not compare volatile data across different clients, normalize captured declarations, fabricate missing presets, or turn incomplete T034 action evidence into a conformance claim. Do after: Make the Helix capture recipe preserve verifiable evidence.

- [x] **[P2] State that five T034 action classes remain missing** (`tests/fixtures/lsp_traces/README.md:139-160`, `T034 action evidence matrix`)
  - **Problem:** The matrix records native single-insert evidence for both Helix and Neovim, then concludes that all six actions remain incomplete for at least one family. The five other rows are missing for both families; the conclusion contradicts the table and the retained T002 scope.
  - **Why it matters:** This is a correctness and readability defect in the canonical evidence summary and can make workers reopen already captured single-insert evidence.
  - **Change:** Replace the final conclusion with the exact bounded status: single insert is captured for both requested editor families; selection replacement, one-character paste, multi-character paste, rapid typing, and undo immediately after a server repair edit remain uncaptured for each family. Keep parent T002 open and keep the checked T002.2 four-client baseline unchanged.
  - **Done when:** The prose, matrix, `TASKS.md` status, and `TASKS_PART_SEVEN.md` acceptance consistently distinguish the completed four-client baseline from the five missing native-action classes per editor.
  - **Constraints:** Documentation only. Do not mark full T002/T034 conformance complete, recapture clients, or infer native actions from payload shape.

Checkpoint review: Sol 5.6 reviewed `3059565..8ecba12`; three P1 findings below, no P0/P2/P3. Full test, formatting and text-native tour gates passed before fixes; logs `/tmp/opencode/wirelog-captures-checkpoint/`. Tour headings and only `menu-commit` excerpt inspected. The user's earlier review skip does not apply to this newly reached eight-task checkpoint. No full-UI-suite or cross-platform execution claim.

Checkpoint fixes are committed (`e34ce5a`, `f7d4bea`, `6dd4f7e`, `15a8f63`). Full `make test FILTER=--no-fail-fast`, rustfmt and diff checks passed after fixes; final `cargo test --test wirelog`, rustfmt and diff checks pass after adding exact outbound/per-PID row assertions. Log `test-after-fixes.log` in `/tmp/opencode/wirelog-captures-checkpoint/`. No owned server remained. No P0 required re-review and no P3 was deferred. T002 remains unfinished; next slice is current Helix capture/availability, followed by remaining edit-shape evidence.

- [x] **[P1] Serialize shared wirelogs on every tested platform** (`src/wirelog.rs:32-61`, `WireLog::new`; `tests/wirelog.rs:37-64`, `concurrent_servers_append_complete_rows_without_truncating`)
  - **Problem:** Complete-line locking exists only under `cfg(unix)`; Windows uses uncoordinated append even though Windows runs the Rust suite. The concurrency test can also pass after losing one whole session because it checks only that any initialize and shutdown row exist, not exact per-process counts or order.
  - **Why it matters:** This violates correctness and cohesion with the cross-platform CI contract. Concurrent or restarted Windows servers can interleave or lose recorder rows, recreating the corruption that this change claims to fix.
  - **Change:** Give the append writer one cross-process locking abstraction on Unix and Windows. Keep serialization before the lock, then lock only around one complete pre-serialized JSONL line. Strengthen `concurrent_servers_append_complete_rows_without_truncating` with each child PID captured from `TestServer::pid()`, bounded unique message bursts through both live servers, and exact row counts plus expected per-PID message order; prove both sessions are complete rather than checking `any`. Retain the restart test for prior-file preservation.
  - **Done when:** Shared-path tests prove two distinct PIDs each retain every unique row in source order, every line parses, no NUL/interleaved bytes exist, restart rows remain ordered, and the same assertions run on Linux, macOS, and Windows CI.
  - **Constraints:** Do not truncate, disable the Windows test, serialize producers on the LSP thread, or weaken append/restart behavior. Preserve unrelated `Cargo.toml` work if a locking dependency is needed.
  - **Verify:** `cargo test --test wirelog`; cross-platform proof is the existing `native-test` CI matrix.

- [x] **[P1] Open shared logs with Windows lock-compatible access**
  - **Problem:** The portable-lock fix still opens non-Unix files with append-only access. Rust's `File::lock` documentation explicitly says Windows locking fails for append-only handles (`https://doc.rust-lang.org/std/fs/struct.File.html#method.lock`). Linux checks cannot detect this.
  - **Change:** Add read access alongside append/create so the same handle meets the Windows lock precondition; preserve nontruncation and new-file privacy. Keep shared-path tests active on all platforms.
  - **Done when:** The production open options satisfy the documented cross-platform locking precondition and existing recorder tests pass; no claim of locally executed Windows tests.
  - **Verify:** `cargo test --test wirelog && cargo fmt --all -- --check && git diff --check`

- [x] **[P1] Make identity replacement byte-index safe** (`src/wirelog.rs:288-300`, `replace_case_insensitive`; `tests/wirelog.rs:169-194`, `redacts_percent_encoded_home_and_username_forms`)
  - **Problem:** `replace_case_insensitive` finds a byte offset in a Unicode-lowercased copy, then applies that offset and the original needle length to the original string. Unicode lowercase can change byte length, so unrelated text such as `İ` before an identity can produce wrong slicing or panic the recorder worker and drop queued rows.
  - **Why it matters:** This is a correctness and privacy failure in the redaction boundary: valid LSP text can stop recording before sensitive values are drained.
  - **Change:** Replace the lowercase-copy search with offset-preserving matching. The simplest acceptable structure is ASCII case-insensitive byte matching for ASCII/percent-encoded forms and exact matching for non-ASCII identity bytes; never derive original-string offsets from a transformed string. Add cases with length-changing Unicode before and inside surrounding text, lowercase percent escapes, and an exact non-ASCII identity.
  - **Done when:** The regression inputs neither panic nor leak configured plain/encoded identities, unrelated Unicode remains intact, and all queued rows reach disk in order.
  - **Constraints:** Do not apply lossy Unicode normalization, redact unrelated path components, or move redaction onto the background writer. Preserve mixed-case percent-escape matching.
  - **Verify:** `cargo test --test wirelog`

- [x] **[P1] Retain the claimed bundled-Neovim native edit** (`tests/fixtures/lsp_traces/bundled-neovim-0.12.6.jsonl:2`; `tests/fixtures/lsp_traces/README.md:22-30,60-63`; `TASKS.md:13-15`, `T002.2b`)
  - **Problem:** The completed task and README claim an authentic native `COMMIT_EDITMSG` insertion, but the committed JSONL retains only a `magit://status/...` full replacement and omits `Bundled native capture`. Future consumers therefore cannot verify the claimed native-edit evidence from the committed fixture.
  - **Why it matters:** This violates correctness and cleanliness of conformance provenance. A server-render echo must not stand in for retained evidence of a user edit.
  - **Change:** Re-extract the actual commit-draft `textDocument/didChange` from the named recorder source, preserving its exact raw payload and source order with initialize, then update the README to identify exactly which retained row proves the native edit. If the source cannot be verified, leave the status row honestly described and reopen/qualify `T002.2b` instead of claiming retained native-edit evidence.
  - **Done when:** The committed fixture contains a source-equal `COMMIT_EDITMSG` change with the documented native text, or `T002.2b` is explicitly incomplete; JSONL, README, and task status make the same bounded claim.
  - **Constraints:** Do not fabricate or normalize declarations, URIs, versions, or edit payloads. Keep T002 and T034 parent work explicitly unfinished and leave Helix scope unchanged.
  - **Verify:** Parse every `tests/fixtures/lsp_traces/*.jsonl` line as JSON and compare the retained bundled-Neovim rows byte-for-byte or structurally against the recorder source named in the README.

P3 deferred: none substantiated in this review.

- [x] **Checkpoint review — Opt-in wire recorder and completed documentation cutover.**
  Coverage: reviewed `a78349e..77bc34d`, directly inspecting `src/wirelog.rs`, `src/server.rs`, `tests/wirelog.rs`, the T002 ticket, `TASKS_PART_SEVEN.md`, `docs/evidence/task-queue-migration.md`, `docs/evidence/real-file-client-spike.md`, `AGENTS.md` and `Makefile`. Verified queue cutover retains immutable source references; A12 remains ownerless and distinct from T103; R-EDIT-REAL remains not green and T104 is a subset. T002.2 capture/preset acceptance remains pending by design. Found one P2 correlated-response error leak below; no P3 separately substantiated. Coverage does not establish source acceptance or audit every changed path. Configured reviewer unavailable (`openai/gpt-5.5-sol`); fallback peer review only, not independent comprehensive audit. Full tests, rustfmt and text-native UI tour had passed; logs: `/tmp/opencode/wirelog-checkpoint/`.

- [x] **[P2] Correlated secret-prompt errors are recorded without redaction.** (`src/wirelog.rs`)
  - **Problem:** A correlated response error enters `redact_secret_values`, which redacts only `result`; `error.message` and `error.data` remain unchanged. A client error that echoes the submitted secret can therefore persist in the opt-in wire log.
  - **Change:** Redact correlated response error payloads as well as results, without discarding unrelated capability or protocol fields.
  - **Done when:** A test records a secret prompt and matching response error containing the secret in nested error fields, then asserts the secret is absent and non-sensitive fields remain.
  - **Constraints:** Source behavior only; preserve direction/order, correlation identity and legitimate fields.
  - **Verify:** `cargo test --test wirelog`

 - [x] **[P2] Percent-encoded identity characters can bypass wirelog redaction.**
  - **Problem:** `WireLog::new` encodes only `%` and `/`; a home such as `/home/alice smith` is not matched in `file:///home/alice%20smith`. Non-ASCII usernames have the same problem, leaking encoded user/home identity.
  - **Change:** Match proper UTF-8 percent-encoded forms of configured home/username identities, including mixed-case escape digits and usual slash-preserving file URIs; preserve unrelated text.
  - **Done when:** Tests cover space and non-ASCII home/user names, plain and encoded URI forms, and assert sensitive representations absent while other paths and capability objects remain intact.
  - **Constraints:** No new dependencies, no global environment mutation, no unrelated server changes.
  - **Verify:** `cargo test --test wirelog`

- [x] **Checkpoint review — Git mutation identities and structured successors.**
  Review committed changes since `dd92f90cdb11d827606a689a88a280f480d7508d` in Git/patch/dispatch/tree/status/commands and their focused tests. Add only substantiated P0–P2 worker-ready findings here; record exact coverage and P3 deferrals without source edits.
  Coverage: reviewed the requested diff through `d90d5da`; inspected `src/git.rs`, `src/patch.rs`, `src/dispatch.rs`, `src/tree.rs`, `src/status_view.rs`, `src/commands.rs`, and `tests/git_backend.rs`, `tests/dispatch.rs`, `tests/views.rs`, `tests/navigation.rs`. No substantiated P0–P2 findings. P3: none deferred; no speculative findings. Coverage is limited to these paths and this diff; no runtime reruns or broader repository audit.
- [x] **Checkpoint review — Plain protocol versions and process ownership.**
  Review committed server/session/protocol/runner changes since the same baseline, focusing on boundary validation, displayed version/coordinate handling, capability negotiation and secure helper ownership. Add actionable findings and honest coverage limits; do not edit source.
  Coverage: reviewed the requested diff through `d90d5da`; inspected `src/server.rs`, `src/session.rs`, `src/protocol.rs`, `src/runner/owned.rs`, `src/presentation/plain.rs`, `src/presentation/resolve.rs`, and the `tests/plain_render.rs` diff. The requested `tests/plain_providers.rs`, `tests/position_encoding.rs`, and `tests/editor.rs` do not exist. A single glob found related test files `tests/plain_core.rs`, `tests/plain_vis.rs`, `tests/plain_actions.rs`, and `tests/editor_bridge.rs`; these were not inspected. No substantiated P0–P2 findings. Coverage limited to requested diff and listed inspected paths; no runtime reruns or broader audit. No specific P3 finding recorded.
- [x] **Checkpoint review — Roadmap migration preserves open requirements.**
  Audit committed documentation migration and immutable issue provenance since the same baseline; distinguish deliberately unfinished T001 work from lost requirements or false completion claims. Add actionable findings and coverage limits; do not edit source.
- Coverage: reviewed the requested documentation diff through `d90d5da`; inspected the migration ledger, replacement queue index/T001 records, retained A12 and R-NAV records, `REVIEW_REPORT.md` reduction, process-ownership design, VSCode host evidence, and UI test-plan edits. Immutable checks used `d04dfa2:TASKS.md` and `da4d422:TASKS.md`. Older Plain plans and screenshot cleanup remain explicitly unchecked; not treated as regressions. R-NAV is recorded complete; R-EDIT-REAL parent remains explicitly not green and T104 is only a subset. No specific P3 finding recorded. Limit: bounded documentation/provenance review, not independent audit of every source behavior or each A-finding acceptance test.
- [x] **[P2] A12 is marked complete in the queue index while its sole retained issue remains open.** (`TASKS.md`, old §7 index and §12 summary)
  - **Problem:** The index says “A01–A10, A12, and actionable A13–A21/A24–A34 are complete,” but migration ledger and `TASKS.md` retain A12 as an open, ownerless real-dispatch Git-failure/removal-of-`server::mutation` gap. This falsely implies closure and conflicts with immutable source `d04dfa2:TASKS.md`.
  - **Change:** Remove A12 from the completed list or qualify it as open and ownerless; preserve distinction from T103 reader-lock work.
  - **Done when:** Queue index, retained gap, and migration ledger consistently state A12 is unresolved, with no fabricated numbered owner.
  - **Constraints:** Documentation only; retain immutable source references and unfinished T001 work.
  - **Verify:** `grep -n -E 'A01–A10|A12|real-dispatch' TASKS_PART_SEVEN.md TASKS.md docs/evidence/task-queue-migration.md`

Checkpoint review (2026-11-05): GPT-5.6 Sol reviewed `a70df920a165885e73cd315c29bf3c27caf78564..14023109f73f235d35b7631b47c93a005cda4ade`. Coverage was limited to the eight requested changed paths plus `Makefile`, `clients/vscode/package.json`, the actual VSCode E2E roots/imports, live marker sites, and the current review/deferred queues. The supplied Linux full-test, rustfmt, seven hygiene-test, pinned hygiene, and text-tour results remain lead evidence; this review ran no test suite and claims no Windows/macOS execution. T002 native-action gaps and unrelated roadmap debt were not reopened. Found one P1 and three P2 tasks; no P0. P3 deferred: remove the four vacuous generated-file absence assertions in `test_sizes_and_markers_are_deterministic` (`tests/hygiene/test_hygiene.py:89-92`), because the following real-Git test already proves those exclusions.

Checkpoint closure: all four P1–P2 findings are resolved (`973a255`, `9a67645`, `f7acfb3`, `1ca0de6`). Full Linux tests, rustfmt, seven hygiene tests, pinned report-only tooling and text-native tour passed before the final documentation/test/comment-only fixes; targeted hygiene checks (including inherited limit 37), navigation tests, rustfmt and diff checks passed afterward. Logs: `/tmp/opencode/hygiene-policy-checkpoint/`. No owned server remains. No P0 requires re-review; the P3 assertion cleanup above remains deferred. T003 is complete, not Phase 1: T002 still lacks five native action classes per editor. The user cancelled the immediate orchestration-prompt editing task; the judgment-based alternative remains proposed, not activated.

- [x] **[P1] Make the phase-gate schema match the actual queue** (`TASKS_PART_SEVEN.md:5-9`, `How to read this document`; `TASKS.md:1246-1250`, `Task ticket template and UI rule`)
  - **Problem:** The new queue instructions say every ticket has an explicit `Phase gate` field, but no ticket has that field. The same change correctly defines gates through named phase groups, so the field claim makes the completed T003 contract and canonical worker instructions internally false.
  - **Why it matters:** This violates correctness and cohesion. Workers cannot follow a declared ticket schema that the canonical queue does not use, and may mistake tickets or slices for independent gates.
  - **Change:** State in both locations that a ticket inherits its applicable gate from its enclosing named phase and the phase map. Remove the claim that each ticket contains an explicit `Phase gate` field; do not add repetitive fields to every ticket. Preserve the active eight-completed-task checkpoint and the proposed, inactive judgment-based replacement verbatim in substance.
  - **Done when:** The queue describes its existing phase-map/enclosing-section structure truthfully, and nothing says individual tickets, slices, or review-fix batches are phase boundaries.
  - **Constraints:** Documentation only. Do not activate the proposed orchestration policy, mark Phase 1 complete, alter T002/T003 status, or retrofit the entire roadmap.
  - **Verify:** `git diff --check && grep -n -E 'Phase gate|phase gate|eight completed|not active|not.*phase boundar' TASKS.md TASKS_PART_SEVEN.md AGENTS.md`

- [x] **[P2] Restore hygiene-test environment isolation** (`tests/hygiene/test_hygiene.py:19-35`, `HygieneTests.setUp` and `tearDown`; `test_sizes_and_markers_are_deterministic`)
  - **Problem:** `setUp` assigns `HYGIENE_SOFT_LIMIT=20` in the process environment and `tearDown` never restores the caller's value. The committed completion note claims inherited-value isolation, but the test overwrites any inherited value and leaks `20` to later tests.
  - **Why it matters:** This violates correctness and cleanliness of the test host. Test order can change later modules' behavior, while the claimed host-isolation regression is not exercised.
  - **Change:** Save and restore the prior environment state with `patch.dict` or explicit setup/cleanup, and import the runner under the scoped value because `LIMIT` is read at module load. Add an assertion that a pre-existing value such as `37` is unchanged after the test while the imported runner still uses the intended test limit.
  - **Done when:** The seven focused tests pass, both absent and pre-existing `HYGIENE_SOFT_LIMIT` states are restored exactly, and the size test still proves the 20-byte threshold without changing global state after completion.
  - **Constraints:** Do not change production threshold semantics, invoke host hygiene tools, require network access, or claim Windows/macOS execution.
  - **Verify:** `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests/hygiene -p 'test_*.py'`

- [x] **[P2] Remove duplicate Linux verification policy** (`AGENTS.md:65-77`, `Phase end and review`)
  - **Problem:** The new “Local execution is Linux-only” bullet immediately repeats the existing “Local execution and verification target Linux only” bullet, including the same CI and claim limits.
  - **Why it matters:** This violates DRY and readability in the canonical agent instructions. Parallel policy wording can drift and create two apparent sources of truth.
  - **Change:** Keep one consolidated bullet containing Linux-only local execution, explicit Windows/macOS CI follow-ups, portability expectations, and the ban on unsupported cross-platform claims. Delete only the redundant wording.
  - **Done when:** `AGENTS.md` states the complete platform policy once under verification, with no loss of constraints.
  - **Constraints:** Documentation only. Do not weaken CI follow-ups or imply Windows/macOS were executed.
  - **Verify:** `grep -n -E 'Linux-only|Linux only|Windows|macOS|cross-platform' AGENTS.md`

- [x] **[P2] Remove stale planned-navigation marker claims** (`src/commands.rs:643-658`, command entries `common.next-section` and `common.jump`; `hygiene-allowlist.md:100-110`, `Tracked marker matches`)
  - **Problem:** The changed baseline correctly says R-NAV completed these implemented commands and that they are not planned work, but both scanned source comments still say “planned navigation implementation requires roadmap disposition.” The report therefore emits stale retention claims that contradict its own disposition.
  - **Why it matters:** This violates cleanliness and correctness of the hygiene baseline. Marker output should explain live retention, not preserve a resolved plan as active debt.
  - **Change:** Delete the two obsolete `keep-for:` comments from the implemented command entries. Update the baseline marker list and line references to match the fresh report rather than replacing them with another retention marker.
  - **Done when:** `make hygiene` no longer reports navigation markers at `src/commands.rs`, the commands remain implemented and unchanged, and `hygiene-allowlist.md` matches the fresh marker report.
  - **Constraints:** Do not change command behavior, bindings, R-NAV status, unrelated markers, or deferred hygiene owners.
  - **Verify:** `make hygiene && grep -n 'keep-for:' src/commands.rs hygiene-allowlist.md`

## Deferred hygiene follow-ups

- [ ] **Resolve the vscode-jsonrpc/node direct dependency** — `clients/vscode/test/e2e/real-file-native.js:391`; declare it in the owning package and verify with pinned Knip plus VSCode E2E dependency install.
- [ ] **Audit unused VSCode exports: commands and context** — `clients/vscode/lib/commands.js` (`openStatus`, `viewport`, `pressCommand`, `invokeCommand`, `mirrorFoldShortcuts`, `keybindingsPath`); `context.js` (`deriveContext`, `inactiveContext`, `staticScope`). Verify focused VSCode tests and Knip.
- [ ] **Audit unused VSCode exports: draft and edit lifecycle** — `clients/vscode/lib/drafts.js` (`cancel`, `close`, `confirm`, `disconnect`, `handleInvoke`, `onChanged`, `onClosed`, `save`, `setContext`, `setup`); `edit.js` (`KIND`, `SCHEME`, `draftAction`, `isEditEffect`, `languageForKind`); `edits.js` (`onDocumentClosed`, `onActiveEditorChanged`, `confirmEditCommand`, `cancelEditCommand`, `currentEdit`). Verify edit lifecycle tests and Knip.
- [ ] **Audit unused VSCode exports: effects, fold keys and menus** — `clients/vscode/lib/effects.js` (`showMessage`, `replay`); `foldkeys.js` (`parseJsonc`, `mapFoldCommand`, `mapFoldKeys`); `menu.js` (`quickPickItems`, `quickPickItem`, `describeItem`, `routeTyped`, `charToKey`); `menus.js` (`showMenu`, `menuItems`, `hideMenu`, `queueSteps`). Verify menu/fold/effect tests and Knip.
- [ ] **Audit unused VSCode exports: prompt, render and requests** — `clients/vscode/lib/prompt.js` (`CUSTOM_ID`, `promptItems`, `answerFor`, `customChoice`); `render.js` (`applyRender`, `applyEdit`); `requests.js` (`press`, `invoke`, `applyRender`, `handlePrompt`, `resync`). Verify prompt/render/request tests and Knip.
- [ ] **Audit unused VSCode exports: semantic, session and views** — `clients/vscode/lib/semantic.js` (`register`); `session.js` (`activate`, `deactivate`, `createClient`); `views.js` (`stateForUri`, `activeState`, `isMenuKind`, `ensureView`, `showView`, `showDocument`, `applyCursor`, `activateView`, `closeView`, `closeRoot`, `focusView`). Verify VSCode integration tests and Knip.
- [ ] **Resolve Lua warning findings** — `lua/magit-lsp/debug.lua:91`, `edit.lua:98`, `highlights.lua:19`, `hooks.lua:42,49,56,63,70,78,103`, `init.lua:122,167,373`, `keymap.lua:83`, `keys.lua:256`, `requests.lua:179`, `view.lua:626,728`, `scripts/color_spike.lua:57`, `tests/spikes/real_file_nvim.lua:14,27,40,47`, `tests/test_magit_lsp.lua:45,514,894,1143,1526,1550,1553,1555,1556,1557,1743`. Verify pinned luacheck; guarded `table.unpack` fallback is tested at line 1743, but LuaJIT runtime reports `table.unpack == nil`.
- [ ] **Reduce evidence archive size** — `docs/evidence/harness-reassessment-inputs.md`; preserve immutable evidence while reducing tracked file below 100000 bytes. Verify `make hygiene` size report.
- [ ] **Split server implementation below hygiene limit** — `src/server.rs`; preserve behavior and reduce below 100000 bytes. Verify focused Rust tests and `make hygiene`.
- [ ] **Split async server tests below hygiene limit** — `tests/server_async.rs`; preserve coverage and reduce below 100000 bytes. Verify `cargo test --test server_async` and `make hygiene`.
- [ ] **Split Neovim integration tests below hygiene limit** — `tests/test_magit_lsp.lua`; preserve coverage and reduce below 100000 bytes. Verify focused Neovim tests and `make hygiene`.
- [ ] **Split canonical task roadmap below hygiene limit** — `TASKS.md`; retain canonical design while reducing below 100000 bytes. Verify documentation links and `make hygiene`.
- [ ] **Split editor bridge tests below hygiene limit** — `tests/editor_bridge.rs`; preserve coverage and reduce below 100000 bytes. Verify `cargo test --test editor_bridge` and `make hygiene`.

These decisions are canonical unless superseded by an explicit later decision
here. General agent working conventions and verification guidance live in
`AGENTS.md`.

## 0. Decisions already made

| Topic | Decision |
| --- | --- |
| Async runtime | `tokio` |
| Config format | TOML, with `include` of other TOML files (absolute or relative), default location from the OS config dir (XDG on Linux) |
| Neovim default profile | `vim` (evil-style) |
| Non-modal default profile | `standard` (Magit mnemonics, direct keys, no Emacs idioms) |
| Emacs idioms (`C-c C-c`, `M-n`, …) | Not kept. No `emacs` profile is shipped |
| Key notation | W3C `KeyboardEvent.key` names + VSCode-style `ctrl+shift+x` modifiers (see §3.8) |
| Section folding | Server-owned, but triggered by the client's own fold keys wherever possible (§3.9) |
| Unimplemented Magit operations | Shown in menus, greyed out, with their default key and description (Neogit style) (§3.12) |
| VCS boundary | Keep presentation and standard-LSP transport VCS-neutral so future jj support requires effectively zero Plain-client changes. Keep Git discovery, mutation validation, commit-file paths, helper lifecycle and completion lookup in backend/workflow providers. Do not add a jj backend, lowest-common-denominator VCS hierarchy, or speculative wire protocol |
| Diff/patch | Git computes diffs; we parse its output and build patches for `git apply` (this is what Magit does, §3.13) |
| Rendering | Server re-renders fully (as Magit does) and sends line-range edits, so clients stay stable (§3.3) |
| Tests | Mostly Rust, in-process, asserting buffer text and repo state. Neovim tests are kept but few (§8) |
| Detail visibility | Four levels: section (staged/unstaged) → files → hunks → individually visible hunks; profile bindings and tests must match (§5.1) |
| Menu saved arguments | Defer `menus.save_scope` persistence; remove/defer the setting until semantics are understood |
| Magit parity keys | Verify keys against the Magit manual, not memory |
| Recommended Git version | Git 2.30.8 is the tested recommendation, not a hard minimum; older versions are not blocked but may have feature failures |
| Picker (T008) | Server supplies typed labels and stable IDs for single/multi selection; clients return IDs. Clients never filter, parse or resolve Git state. A native widget may filter server-supplied labels. |
| Tests | Feature Rust state tests and conformance scenarios; native Neovim only for new visible behavior, VSCode only for changed declared capabilities or client files. |
| Generated files | Ship only files clients need; mark generated and enforce drift checks. |
| Capability resolution with quirks | Resolve declarations, then client quirks, then user configuration (T005). |
| Rich protocol | The rich protocol stays; v1 preview permits additive changes only. |
| Plain presentation (target) | Standard-LSP presentation is VCS-neutral. T008 serving/stacking and parameterized stable-ID picking; T009 provider-file edits; T010 semantic dispatcher and fake-provider independence test. Intended architecture, not current behavior. |
| Plain support (target) | Generic LSP client selection, sequential URI-keyed sync, safe repository resolution, validated targets and bounded capabilities; deferred/out-of-scope work remains in the numbered queue. |

---

## 1. Glossary

Use these words exactly. Do not invent synonyms.

| Term | Meaning |
| --- | --- |
| **Client** | An editor-side program (Neovim plugin, VSCode extension, TUI). It forwards input to the server and displays what the server sends. It contains no Git logic and no layout logic |
| **Server** | The Rust `magit-lsp` process. Owns all state, rendering, key handling and Git calls |
| **View** | One screen the server can show: status, log, a commit, a menu, a rebase todo, … Each view is a virtual document with a URI like `magit://status/<repo>` |
| **View kind** | The type of a view (`status`, `log`, `revision`, `menu.commit`, …) |
| **Document** | The text of a view as the client displays it, plus structure metadata |
| **Node** | One element in a view's tree: a section, file, hunk, diff line, commit, ref, menu item, … A node knows which text lines it covers |
| **Node ID** | A stable string identifying a node by *what it is*, not where it is, e.g. `unstaged/src/a.rs/hunk:9f3c1a`. Survives refresh. Never a line number or index |
| **Version** | An integer on every render of a view. Incremented on each change |
| **Ack** | The client's `view/rendered` notification saying "I applied version N" |
| **Cursor context** | `{line, column}` plus, for selections, `{startLine, endLine}` and the editor mode (`normal`/`visual`) sent with every key |
| **Command** | A named server operation with an ID such as `status.stage`. Everything a key can do is a command |
| **Command status** | `Implemented`, `Planned` (shown greyed, does nothing yet) or `Unavailable` (backend cannot do it) |
| **Scope** | A named group of key bindings, e.g. `common`, `status`, `log`, `menu.commit`. Scopes inherit from `common` |
| **Binding** | `key sequence → command ID`, within a scope, for some editor modes |
| **Profile** | A named, shipped bundle of default bindings: `standard`, `vim` |
| **Keymap** | The resolved set of bindings active right now for the focused view |
| **Effect** | An instruction from the server to the client: show a message, open a view, ask a question, copy text, … (§3.5) |
| **PassThrough** | An effect meaning "this key is not ours; do what the editor would normally do for it" |
| **Capture mode** | A view flag. While set, the client must send *every* key pressed in that view to the server and must not run native behaviour for it unless the server answers `PassThrough` |
| **Placement** | A hint for where a view should appear: `main`, `side`, `bottom`, `float` |
| **Prompt** | The server asking the user one question (yes/no, text, pick one, pick many) and waiting for the answer (§3.6) |
| **Menu** | A view of kind `menu.*` that lists switches, options and actions (Magit's "transient") (§3.11) |
| **Select mode** | A view (usually a log) opened to let the user pick commits; confirm returns them to a waiting command (§3.7) |
| **Editor bridge** | Git's helper waits until the client confirms or closes Git's real `COMMIT_EDITMSG`, or explicitly aborts (§3.10). |
| **Process log** | A view listing every Git invocation with arguments, exit code, duration and output |
| **Text mode** | A view mode with no ordinary Magit key capture: Neovim retains its existing editable buffer behavior; VSCode browses its read-only virtual document with native editor keys and queued refreshes (§3.14) |
| **Semantic token** | LSP's standard way for a server to label ranges of text with a *type* (and modifiers) so the client colours them using the user's theme |
| **Parity** | Having the same operation available as Magit |

---

## 2. Completed correctness contracts

Completed hands-on fixes, retained here for their behavioural contracts.
These contracts remain canonical; open evidence gaps and follow-ups are tracked
in §7. Historical completion marks are not proof that every original test exists.

### Completed correctness contracts (immutable completion record: `da4d422:TASKS.md`)

- **IMM-1:** Unstage works in unborn repositories and rename paths. Tests: `tests/dispatch.rs`; oldest-Git/fallback evidence gap remains.
- **IMM-2:** Normalize tree-based inclusive selections; retain only changes and safely reverse-apply. Tests: patch/dispatch suites; randomized property evidence per §8.3.
- **IMM-3:** Git failures become readable message effects; protocol faults remain RPC errors. Tests: `tests/errors.rs`; mapped failures enforced, broader UI evidence retained in archive.
- **IMM-4:** Neovim status buffer is render-only except text mode; writes blocked. Tests: `tests/editor_bridge.rs`; native rules and evidence remain in immutable record.
- **IMM-5:** Git diff context determines hunk merge; patch builder does not compute diffs. Tests: patch fixtures and cached diff parity.
- **IMM-6:** Dispatch uses smallest tree node or explicit selection. Tests: `tests/dispatch.rs`; discard execution/wiring remains R-3.6.
- **IMM-7:** Shared deterministic test harness in `tests/common/session.rs`. Tests: harness consumers; completion record retained.
- **IMM-8:** Historical `:TOhtml` spike only; superseded by T028. No rerun; do not treat spike as production evidence.
- **IMM-9:** `AGENTS.md` links roadmap; decisions and working rules have canonical homes. Tests: documentation contract; no runtime test.

---

## 3. Design decisions (expanded)

### 3.1 The server owns the document

- One server-owned node tree produces text, tokens, symbols, link ranges and
  `line → node`; never recover structure by parsing rendered text.
- Node IDs derive from identity (path/hunk content, commit hash, ref name,
  menu key), never position; preserve cursor, visibility and selection.
- Every render has a version; clients ack with `view/rendered`. Positional
  requests (symbols, definition, cursor keys) use the acked tree. Actions carry
  their version/Node ID; vanished nodes re-resolve by content hash or return a
  Message, never target the new occupant of an index.
- Server-owned collapsed bodies are neither rendered nor computed until
  expanded (as in Magit's lazy `magit-insert-section-body`).

### 3.2 Sessions, views and the view stack

- One `Session` per repository root per client. It holds a `ViewStack`.
- A `View` has: `id`, `kind`, `uri`, `title`, `mode` (`interactive`/`text`),
  `placement`, `scope`, `capture`, `viewport {cols, rows}` (sent by the client;
  needed to lay out multi-column menus), its node tree and visibility state.
- Trait: `render(&Ctx) -> Tree`, `scope() -> Scope`, `on_refresh()`, optional
  `load_more()` for lazy lists.
- The server gives a `placement` hint; the **client** decides actual windows.

### 3.3 Rendering and incremental updates (how Magit does it)

Magit rebuilds the buffer (potentially slow in large repos), restores section
identity plus line/column offset and scroll, and computes hidden bodies lazily.
We fully re-render server-side, diff old/new text into line-range edits (common
prefix/suffix or a line diff), and return the Node-ID successor cursor. Clients
retain scroll/marks without identity matching. Collapsed bodies stay lazy;
dirty-subtree rendering is deferred until profiling justifies it.
`view/render` carries `{version,baseVersion,edits}` or full `{version,text}`;
an applied-version/base mismatch requires `view/resync`.

### 3.4 Document delivery and standard positional requests

Standard LSP positional requests only make sense if the client buffer equals
the server's text. Hence the ack scheme above. While a view is in text mode,
position-based requests return `null`. The custom `view/rendered` acknowledgement
identifies an applied custom render version/tree; it differs from Plain synced-text
snapshots, which associate document text with render state without that custom
ack protocol. Plain presentation uses standard LSP documents and providers. The adopted target
uses the declared capability block, logical stack and bounded prompts/placements;
this supersedes the obsolete no-prompts/no-placements/no-editor-bridge target claim. Virtual-content transports and custom URIs remain
out of Plain scope; see §9 for deferred LSP 3.18 work.

### 3.5 Commands and effects

Every key action is a **command** in a registry:

```
Command { id: "status.stage", scope: "status", doc: "Stage the change at point",
          status: Implemented, run: async fn(&mut Ctx) -> Result<()> }
```

Commands are `async fn`s with a `Ui` handle (`ctx.confirm`, `ctx.prompt_text`,
`ctx.pick`, `ctx.pick_ref`, `ctx.pick_commit`, `ctx.edit_document`). A call
suspends the command until the client answers; this makes multi-step flows
ordinary linear code. Tests supply a `ScriptedUi`.

**Effects** (server → client). All have a `type` field:

| `type` | Fields | Client must |
| --- | --- | --- |
| `message` | `level: info/warn/error`, `text` (one line), `hint?`, `detail?`, `processLogId?` | Show a notification at that level. If `detail`/`processLogId` present, offer a way to open it |
| `openView` | `descriptor` (id, kind, uri, title, placement, mode, scope, capture) | Create a window/buffer/panel per `placement`, show it, attach the LSP client to it |
| `pushView` | same as `openView` | As `openView`, remembering the previous view so `popView` returns to it |
| `popView` | `viewId` | Close that view and focus the previous one |
| `focusView` | `viewId` | Focus it |
| `showDocument` | `uri`, `line?`, `column?` | Open a real file (standard `window/showDocument`) |
| `editDocument` | see §3.10 | Open it for editing in the client's own editor |
| `clipboard` | `text` | Put the text on the system clipboard |
| `passThrough` | `key` | Perform the key's normal editor behaviour as if unbound |
| `quit` | — | Close all magit-lsp views of this session |

Rendering updates are not effects; they arrive as `view/render`.

### 3.6 Prompts (what each kind means)

A **prompt** is one question the server asks the user. The server sends a
`view/prompt` *request* and waits for the response. The response always
contains `cancelled: bool`. If `cancelled` is true the command stops quietly
(no error message).

Common request fields: `promptId` (string, unique), `kind`, `title` (the
question, shown prominently, e.g. `"Delete branch feature/x?"`), `detail?`
(extra lines of explanation).

| `kind` | Question it asks | Extra request fields | Response `value` |
| --- | --- | --- | --- |
| `confirm` | Yes or no | `yesLabel?` (default "Yes"), `noLabel?` (default "No"), `destructive: bool` (client should style it as dangerous and make "No" the default focus), `default: bool` (default `false`) | `true` (yes) or `false` (no) |
| `text` | A free-form single line of text | `placeholder?`, `default?` (prefilled text), `secret: bool`, `error?` (shown when the server re-asks after rejecting the previous value) | the string the user typed (may be empty) |
| `choice` | Pick exactly one from a list | `items: [{id, label, description?, detail?}]` (`id` is what the server gets back; `label` is shown), `allowCustom: bool` (the user may type something not in the list), `default?` (an item `id`) | `{id: "<item id>"}` or, if custom, `{custom: "<typed text>"}` |
| `multiChoice` | Pick zero or more from a list | `items` as above, `preselected: [id]`, `min?`, `max?` | `{ids: ["…"]}` |

**`secret` flag.** When `true`, the typed characters must **not** be echoed
(masked or hidden), the value must not be written to any history, log or
completion list, and the server must not log it. Used for Git credential and
SSH passphrase requests arriving through the askpass helper (§3.10).

Prompt presentation should fit the question and user choice. Native widgets may
filter server-supplied labels, but clients never parse, resolve or filter Git
state. Generic `choice` prompts may use the editor's normal picker behavior.

**Intended T008 pick (not implemented):** A `pick` request carries purpose,
prompt, `multi`, and server-supplied `{id,label,description?,detail?}` items.
The response returns selected IDs or cancellation. Commit picking is not a
generic choice prompt; the server owns Git resolution and stable identity.

**Mapping to editors** (default implementations; clients may use richer UI):

| Kind | Neovim | VSCode |
| --- | --- | --- |
| `confirm` | `vim.ui.select({yesLabel, noLabel}, {prompt = title})`. Result maps to `true`/`false`; closing the picker = `cancelled` | `showWarningMessage(title, {modal: true, detail}, yesLabel)`; the returned label = yes, `undefined` = `cancelled` |
| `text`, not secret | `vim.ui.input({prompt = title, default = default})`; `nil` result = `cancelled` | `showInputBox({title, value: default, placeHolder})`; `undefined` = `cancelled` |
| `text`, `secret` | `vim.fn.inputsecret(title)` (`vim.ui.input` cannot mask) | `showInputBox({..., password: true})` |
| `choice` | `vim.ui.select(items, {prompt = title, format_item = label})`. If `allowCustom`, prepend an item "Enter custom value…" that then runs a `text` prompt | `showQuickPick(items, {title})`. If `allowCustom`, use `createQuickPick` and add an item for the typed text |
| `multiChoice` | Loop of `vim.ui.select` that toggles items until the user picks a "Done" item (or a scratch-buffer checklist) | `showQuickPick(items, {canPickMany: true})` |

These map onto the user's own picker plugins automatically in Neovim because
`vim.ui.select`/`vim.ui.input` are overridable.

### 3.7 Select mode (picking commits and other things)

Intended T008 behavior for commands that need the user to pick a commit (rebase onto, cherry-pick,
fixup target, reset to, …). This server-owned log select mode is the normal
commit-selection flow; it is not a client fuzzy picker, QuickPick, or `choice`
prompt. The client displays server-rendered text verbatim and forwards input.

- The server supplies purpose, prompt, multiplicity and stable `{id,label}`
  choices. Clients return IDs; they do not filter, parse or resolve Git state.
  Native widgets may filter server-supplied labels. This is intended behavior,
  not a claim that T008 has shipped.
- Intended extra bindings (scope `log.select`): `common.confirm` returns the
  commit under the cursor (or all marked commits if `multi`),
  `common.cancel` returns without mutation, and `select.toggle-mark` is
  multi-only. Selection identity comes from structured commit nodes, never
  rendered-text parsing or row indices. Confirm returns the selected identity
  to the waiting operation and restores the caller view; cancel restores the
  caller without mutation.
- A free-text option is an optional escape hatch, not the default flow: a key
  (`select.enter-rev`) opens a `text` prompt so the user can type `HEAD~3`, a
  branch, or a tag; the server validates it with `git rev-parse --verify`.
  No client Git, filtering, or commit-selection implementation is required.
- The same pattern selects a branch (refs view), a stash, or files.

**Proposed Enter/o scheme (not approved or implemented):** In a plain log,
`common.confirm`/Enter would open the structured commit under the cursor. In a
commit picker, Enter would remain the primary action and select/confirm the
commit. A proposed `o` action in commit-bearing log, select, and rebase views
would open a read-only commit inspection view without selecting a commit or
mutating the pending operation. `q` would return to the originating picker or
rebase view with its cursor, marks, and pending operation intact. This proposal
does not apply to status, where `o` opens submodules. Use terminal-portable
keys; do not depend on `ctrl+Enter`.

### 3.8 Key notation

No formal standard exists for *textual* shortcut strings. The W3C UI Events
spec standardises the key *values* (`KeyboardEvent.key`: `"Enter"`, `"Escape"`,
`"ArrowUp"`, …) and VSCode/Electron have de-facto readable string forms. We
combine them:

- **Format:** `[modifier+]*key`. Modifiers lowercase, in this order when
  several: `ctrl`, `alt`, `shift`, `meta` (`meta` = Cmd on macOS, Windows/Super
  elsewhere). There is no `mod`/`CommandOrControl` alias: platform differences
  are expressed explicitly in config.
- **Key names:** W3C values: `Enter`, `Escape`, `Tab`, `Backspace`, `Delete`,
  `Insert`, `Home`, `End`, `PageUp`, `PageDown`, `ArrowUp`, `ArrowDown`,
  `ArrowLeft`, `ArrowRight`, `F1`…`F12`. Two spelled-out names for characters
  that are awkward in text: `Space` and `Plus` (the `+` key).
- **Printable characters** are written as the character produced: `s`, `S`,
  `?`, `%`, `$`, `!`, `-`, `=`. **`shift` is not used with printable
  characters** (`S` means shift+s). `shift` is used only with named keys
  (`shift+Tab`).
- **Chords** (several keys in sequence) are separated by a space:
  `g g`, `ctrl+k ctrl+s`, `z a`.
- **Parsing rule:** split on whitespace into steps; split each step on `+`
  (a step that is exactly `Plus` is the plus character).
- **Examples:** `s`, `S`, `Enter`, `ctrl+Enter`, `Escape`, `Delete`, `shift+Tab`,
  `alt+ArrowDown`, `ctrl+c`, `] ]`, `g r`.

**Translators** (one small table per client; unit-tested both directions):

| Canonical | Neovim | VSCode `keybindings.json` |
| --- | --- | --- |
| `s`, `S`, `?` | `s`, `S`, `?` | `s`, `shift+s`, `shift+/` (layout dependent, see §3.9) |
| `Enter` | `<CR>` | `enter` |
| `Escape` | `<Esc>` | `escape` |
| `Tab` / `shift+Tab` | `<Tab>` / `<S-Tab>` | `tab` / `shift+tab` |
| `Delete` / `Backspace` | `<Del>` / `<BS>` | `delete` / `backspace` |
| `ArrowUp` | `<Up>` | `up` |
| `ctrl+c` | `<C-c>` | `ctrl+c` |
| `alt+ArrowDown` | `<A-Down>` | `alt+down` |
| `ctrl+Enter` | `<C-CR>` | `ctrl+enter` |
| `g g` | `gg` | `g g` |

**Terminal limits:** many terminals cannot distinguish `ctrl+Enter`,
`ctrl+shift+<letter>`, or `shift+Enter` from their plain forms. Clients report
`keyModifiers: "full" | "basic"` in the versioned rich declaration during `initialize`. Profiles (§5) avoid these
combinations in the `vim` profile; `standard` assumes a GUI editor.

### 3.9 Keybinding architecture

Goal: the server knows every key; clients only forward; users can customise
per view/menu while sharing analogous actions across views.

**Semantic commands and scopes**
- Bindings target command IDs. IDs are namespaced: `common.*` (shared by all
  views: `common.confirm`, `common.cancel`, `common.quit`, `common.refresh`,
  `common.toggle-section`, `common.toggle-text-mode`, …), view-specific
  (`status.stage`, `log.show-commit`, `rebase.move-up`), and menu openers
  (`open.commit`, `open.branch`, …).
- Scopes inherit: view-kind scope → `common`. A view-kind scope may override a
  `common.*` binding for itself. Resolution order: user config for the scope →
  profile for the scope → user config for `common` → profile for `common`.
- Rebinding `common.confirm` changes confirm in the log selector, the rebase
  todo and the commit message at once (unless a scope overrides it).
- **Editor-mode conditions:** a binding may be restricted to `normal`,
  `visual`, or both (default both). Visual-mode bindings receive the selection.
- The set of keys per scope never depends on repo state (only *enabled/greyed*
  state and labels do). This is what makes the full key space enumerable
  offline.

**Wire shapes** (see §4): `view/keymap` (server → client) carries
`{viewId, scope, version, capture, bindings: [{command, keys, modes, label, doc, group, status}]}`
where `keys` is a list of alternatives, each a canonical key string.

**Client delivery modes** (declared by the client in `initialize`):

1. **`dynamic`** (Neovim, TUI): the client applies each `view/keymap` as the
   complete current keymap for that buffer, atomically.
   Neovim tracks `applied[bufnr] = {lhs…}`: delete exactly those maps, install
   new buffer-local `vim.keymap.set` maps (`nowait=true`), store the new set in
   one synchronous function (no interleaved keys). Maps send canonical
   `view/key`; capture mode additionally forwards every other printable key.
2. **`static`** (VSCode): generate `contributes.keybindings` via
   `magit-lsp keymap --format vscode`; runtime `when` contexts are
   `magit.active/scope/textMode/capture`, refreshed with view state and editor
   focus (static clients use descriptors/renders, not dynamic keymap pushes).
   Native `keybindings.json` customises `magit.invoke` with
   `{"command":"status.stage"}` → `view/invoke`, or `magit.key` with
   `{"key":"s"}` → `view/key`. Editor bindings require the focus/widget guards
   below; do not infer safe capture from the document alone.
3. **`forwardAll`** (fallback): the client sends every key and the server
   answers `passThrough` for anything unbound.

**VSCode capture limits:**
- Gate editor bindings on `magit.active`, editor text focus and exclusion of
  foreign widgets (R-VS-5, completed; archive), not merely the active document.
- Static bindings enumerate specific keys; layout affects printable keys
  (`keyboard.dispatch: keyCode` uses physical codes). IME/dead keys bypass them.
- `type` is window-global, single-owner and incompatible with VSCodeVim;
  routing all typing through `default:type` can be slow. Never own it
  permanently: register only for a focused capture view, dispose on blur/close,
  forward non-Magit typing, and fall back to static/QuickPick if already owned.
- Never shadow workbench shortcuts (`ctrl+p`, `ctrl+shift+p`, `ctrl+w`,
  `ctrl+tab`, F1, `alt+tab`, …). Generate only server-bound keys; deny-list
  tests require explicit reasoned exceptions.
- Standard `ctrl+c` cancel returns `passThrough` for a non-empty text selection
  when there is no menu, prompt or select mode to cancel, preserving copying.

**Fold keys: reuse the client's own fold keys** (so users who customised fold
keys keep them). Implementation per client:
- *Neovim:*
  1. Set `foldmethod=manual`, `nofoldenable` in magit-lsp buffers so native
     folding never interferes.
  2. Buffer-locally map the built-in fold commands to server commands:

     | Native | Command |
     | --- | --- |
     | `za`, `zA` | `common.toggle-section` (`zA`: recursive) |
     | `zo`, `zO` | `common.show-section` (`zO`: recursive) |
     | `zc`, `zC` | `common.hide-section` (`zC`: recursive) |
     | `zR` | `common.show-all` |
     | `zM` | `common.hide-all` |
     | `zr` / `zm` | `common.more-detail` / `common.less-detail` (visibility level ±1) |
     | `zj` / `zk` | `common.next-section` / `common.prev-section` |
  3. At view creation and on `BufEnter`, scan the user's mappings
     (`vim.api.nvim_get_keymap(mode)` and the buffer's) for any whose `rhs` is
     one of the fold commands above (also `:normal! za`, `<Cmd>normal! za<CR>`).
     Re-map each such `lhs` buffer-locally to the matching server command.
     This catches `nnoremap <Tab> za`, which a plain remap of `za` would miss
     (non-recursive mappings ignore other mappings).
  4. Known limit: Lua-function mappings cannot be inspected. Provide
     `fold_keys = { ["<Tab>"] = "common.toggle-section" }` in the Neovim client
     config for those.
- *VSCode:* an extension cannot intercept `editor.toggleFold` (user keybindings
  outrank extension keybindings, and the native command acts on VSCode's own
  folding model, which these views do not use). So:
  1. Ship default bindings for our commands on VSCode's default fold keys
     (`ctrl+shift+[`, `ctrl+shift+]`, `ctrl+k ctrl+l`, `ctrl+k ctrl+0`,
     `ctrl+k ctrl+j`, and macOS variants) with `when: magit.active`.
  2. At activation, best-effort read the user's `keybindings.json` (JSONC) and
     list entries whose command is `editor.fold*`, `editor.unfold*` or
     `editor.toggleFold`. If any, offer a command **"Magit: Mirror my fold
     shortcuts"** that shows (or, with consent, appends) equivalent entries
     targeting `magit.invoke` with `when: magit.active`.

**Server-side hard rules**
- The server validates the merged keymap and reports conflicts when distinct
  commands bind the same key at the same precedence in one scope chain. A more
  specific scope declaration intentionally shadows a broader declaration for
  that scope (including when the commands differ); equal-precedence ambiguity
  remains an error. Conflicts are reported as a `Message` and in
  `magit-lsp keymap --check`.
- `magit-lsp keymap --format json|vscode|neovim|markdown [--profile P]`
  emits the full static manifest (all scopes × bindings). CI snapshots it.

### 3.10 Editor bridge (open Git's editor inside the user's editor)

**Current real-file contract.** Git editor helper opens canonical actual-gitdir
`COMMIT_EDITMSG` as `mode: "realFile"` with a `file:` URI and opaque `runId`.
Validate helper token, owner, kind, URI/path and run before exposing it. No managed
virtual document or draft/history wire protocol remains. Only commit messages are
implemented; history recall is deferred to R-EDIT-HISTORY (T099).

Explicit `common.confirm` accepts only the exact owner/run/URI/current version;
synchronized text is safely written inside the pinned Git directory before helper
release. Saved-file confirmation reads bounded disk bytes and never substitutes
unsaved synchronized text. Replacements must remain regular, owned, and without
symlink/hardlink, inside pinned gitdir. Native Save writes disk but does not finish
editing. Unsaved close honors Save / Don't Save / Cancel: Save confirms saved
bytes; Don't Save confirms last saved bytes (initially Git's template); Cancel
keeps buffer and run open. Empty/comment-only content is Git-owned empty-message
behavior, not explicit abort. Git owns templates, cleanup, hooks and commit result.

Normal close completes helper successfully; explicit abort, progress cancellation,
disconnect/EOF and shutdown cancel at any phase. VSCode confirms only on the
run-aware last owned tab closing across all groups; `didClose` is not confirmation
there. Elsewhere `didClose` saved-file confirmation is an accepted assumption,
not a run-aware guarantee. Registry lock arbitrates terminal outcomes. Progress,
cancellation and disconnect remain owned through completion; one terminal outcome
releases helper/config/progress exactly once. Failures retain the pending run and
recover/reopen editable text; reject late responses. Buffer recovery is best-effort;
drafts do not survive server crash. Token is private and unlogged; inherited-env
same-user/nested-hook spoofing remains a limit. See [process ownership contract](docs/design/process-ownership.md).

**Completed slices** (immutable historical narrative:
[`c6db4cf:TASKS.md`](https://github.com/example/magit-lsp/blob/c6db4cf/TASKS.md)):

| Slice | Completion reference | Enforcing test / evidence |
| --- | --- | --- |
| Git empty-message and saved-file behavior | R-EDIT-REAL, `c6db4cf` | `tests/editor_bridge.rs`; focused real-Git/native scenario |
| Registered confirmation payload validation | R-EDIT-REAL, `c6db4cf` | `tests/editor_bridge.rs` |
| Existing-effects recovery and same-run retry | R-EDIT-REAL, `c6db4cf` | `tests/editor_bridge.rs`, `tests/editor_drafts.rs`; focused UI scenario |
| VSCode lifecycle supervisor slice | R-EDIT-REAL, `c6db4cf` | lifecycle e2e; Linux/Codium single-root evidence only |
| Real-file cutover/integration | R-EDIT-REAL, `c6db4cf` | `tests/editor_bridge.rs`, `tests/editor_drafts.rs`; broad native gap |
| Rich-history route removal | R-EDIT-REAL, `79384c2` | `tests/editor_bridge.rs`; recall deferred |

**Residual gates and limits.** R-EDIT-REAL parent acceptance remains unchecked;
wide native/adversarial, multi-root and broad regression gates are not green.
Spike is evidence only, not production acceptance: see
[real-file client spike](docs/evidence/real-file-client-spike.md). T099 history
is deferred; T100 Windows pipe/ACL/Job feasibility is unverified; T101 macOS
process-group/reaping validation is unverified. T104 lifecycle residual acceptance
is pending: last-window exit, dirty shutdown, other builds, and broader
reload/workspace changes. Process ownership remains subject to its Linux real-Git
and adversarial gate; macOS and Windows guarantees are not claimed. Broad
regression has an inherited failure in
`tests/errors.rs::normal_and_piped_apply_failures_keep_original_details`; no green
gate claim. Plain presentation is separate; see
[Plain-client tasks](./magit-lsp-plain-client-tasks.md).

### 3.11 Menus (transients) and how they appear in each editor

A **menu** is a view of kind `menu.<name>`. The server renders it as text; the
client shows it. No client contains menu logic.

**Content model (server-side)**
```
Menu   { id, title, groups: [Group], savedArgs }
Group  { title, columns?: number, items: [Item] }
Item   = Switch { key: "-a", flag: "--all", label, on: bool, status }
       | Option { key: "=A", flag: "--author=", label, value?: string, reader, status }
       | Action { key: "c",  command: "commit.create", label, status }
status = Implemented | Planned | Unavailable{reason}
reader = text | choice | path | ref | commit | date | number
```
- **Switch** = on/off flag. Key is `-` + letter (two key presses).
- **Option** = flag with a value. Key is `=` + letter. Pressing it triggers a
  prompt (kind chosen by `reader`: `text`→`text`, `choice`/`ref`→`choice`,
  `commit`→select mode, `path`→`text` with completion, etc.). Pressing it again
  while set clears it.
- **Action** = runs a command using the current switch/option values. Single key.
- An **argument preview line** shows the resulting command, e.g.
  `git commit --amend --no-verify`.
 - Saved arguments: `menu.save-args` stores the current switches/options as the
   default for that menu; persistence scope is deferred until semantics are
   understood. Remove/defer `menus.save_scope` meanwhile. `menu.reset-args`
   restores defaults.

**Text rendering (what the server sends)**
```
 Commit                                         <- token menu.title
 Arguments                                      <- menu.group
  -a  Stage all modified and deleted files (--all)   [ ]   <- menu.key, menu.label, menu.flag, menu.switch.off
  -e  Allow empty commit (--allow-empty)             [x]   <- menu.switch.on
  =A  Override the author (--author=)                       <- menu.option, value in menu.value
 Create             Edit HEAD            Edit                       <- groups laid out in columns using viewport.cols
  c  Commit          e  Extend            f  Fixup          <- Planned items use menu.planned (greyed)
```
Layout columns are decided by the server from `viewport.cols`. The client tells
the server its size on open and on resize (`view/resize`).

**Highlighting:** semantic tokens (§3.15). Token types used: `menu.title`,
`menu.group`, `menu.key`, `menu.label`, `menu.flag`, `menu.switch.on`,
`menu.switch.off`, `menu.option`, `menu.value`, `menu.planned`,
`menu.disabled`, `menu.pending`, `menu.preview`. Each maps to a standard theme
group (§3.15).

**Keys inside a menu:** the view has `capture: true`. The client forwards every
key; the server keeps the typed steps as `pending` (`-`, `=`, or a chord prefix),
dims items that cannot complete them, and answers `passThrough`-free (unbound
keys are swallowed with a one-line hint while `pending` is kept). `Backspace`
pops one step. `Escape`, `ctrl+c` and `q` close the menu. `view/render` and
`view/keymap` carry `pending`; `view/keymap` also carries `liveKeys`, so the
client needs no chord logic.

Menus are not LSP hovers (position-anchored Markdown without window placement).
`showDocument` cannot position windows either. The server hints `bottom` or
optional `float`; clients use native windowing:

| Client | `placement: bottom` implementation |
| --- | --- |
| Neovim, default | Floating window: `nvim_open_win(buf, true, {relative="editor", row = vim.o.lines - height - vim.o.cmdheight - 1, col = 0, width = vim.o.columns, height = height, style = "minimal", border = "none"})`. `height` = number of rendered lines (max e.g. 40% of screen) |
| Neovim, alternative (config `menu.style = "split"`) | `botright {height}split` with a scratch buffer |
| VSCode, default | **QuickPick** (like the VSpaceCode which-key extension does for key menus; verify when building): `createQuickPick()` shows items with keys as labels; typed characters arrive in `onDidChangeValue` and are forwarded to the server as keys; the input box is cleared after each key. Uses the structured menu model (below), not the text |
| VSCode, text mode (config) | A read-only virtual document opened in a new editor group below, highlighted with semantic tokens, keys captured as in §3.9 |
| TUI | Bottom pane drawn by the client from the text and tokens |

**Both representations are sent.** `view/render` for a menu view includes the
text *and* the structured `menu` model, so a client can choose: text + tokens
(consistent look everywhere) or native widgets (QuickPick). Text is the
default and the reference; native widgets are an optimisation.

**Menu stack:** opening a menu from a menu (e.g. a "Configure…" action) pushes
a new view; `popView` returns.

### 3.12 Planned (greyed-out) features in menus

Following Neogit: every Magit operation that we have not built yet is still
listed in its menu with its default key and description, rendered in the
`menu.planned` style (greyed). This is how the roadmap is visible in the UI.

- The command registry has `status: Implemented | Planned{roadmapId} | Unavailable{reason}`.
- **Pressing a planned item** does nothing except show
  `Message{info, "Not implemented yet: Rebase onto upstream (roadmap R-7.3)"}`.
- **Unavailable** is different: the item is hidden or greyed with a reason
  because the backend cannot do it (not for Git today).
- Every Magit dispatch menu is created with its **complete** action list from
  the Magit manual (sections Committing, Branching, Fetching, Pulling, Pushing,
  Merging, Rebasing, Stashing, Tagging, Resetting, Cherry Picking, Reverting,
  Bisecting, Remotes, Submodules, Worktrees, Ignoring, Logging, Diffing,
  Applying Patches, Blaming).
- `cargo xtask parity` prints a table of Magit command → our command ID →
  status; `README.md`'s parity table is generated from it.
- A test asserts: every `Planned` command shows the message and changes no
  repo state; every parity-list key exists in some menu.
- Order of work: menu infrastructure first (Phase 2), then populate all menus
  with `Planned` entries, then implement them phase by phase.

### 3.13 Diff and patch layer (what Magit does; what we keep)

Findings (Magit `magit-apply.el`): Magit runs `git diff` and parses its text.
To stage/unstage/discard/reverse it builds a patch consisting of the **file
header** plus the hunk (or a region of the hunk), adjusts the hunk's new-start,
and pipes it to `git apply` (`--cached`, `--reverse`). Whole files use
`git add`, `git reset`, `git rm --cached`. It has no diff algorithm of its own.

What we keep (and nothing more):
1. **Parse** `git diff` / `git diff --cached` output into `File → Hunk → Line`,
   keeping the raw header text and raw hunk text.
2. **Build patches** for whole file, whole hunk, and selected lines, per IMM-2.
3. **Apply** via `git apply --cached [--reverse]` (index) or
   `git apply [--reverse]` (worktree discard).
4. **Whole-file operations** via `git add`/`git reset`/`git update-index`.

Never compute diffs ourselves. Never merge/split hunks ourselves.

`git apply --recount` tells Git to ignore the counts in hunk headers and infer
them. Try it to replace the custom renumbering in `patch.rs`; keep the
proptests as the safety net; keep the old algorithm only if the proptests show
`--recount` is insufficient, with a comment explaining why.

Diff context size is a setting (`diff.context`, default 3) passed as `-U<n>` to
*both* the display diff and any patch-building diff so line numbers agree.

### 3.14 Text mode

`common.toggle-text-mode` (bound in every scope, including text mode itself).
- **Enter:** server sets `mode = text`; sends a `view/keymap` containing only
  the toggle binding to dynamic clients. The server stops answering
  position-based requests for the view and **queues** refreshes (does not
  render them). Static clients receive a reliable mode/context notification
  for entry through invoke, key, menu or setMode.
- **Neovim:** preserve its existing editable text behavior: make the buffer
  modifiable, restore undo (`undolevels` back to the user's value), and stop
  forwarding keys. **Edits** are never parsed or synced to the server model.
- **VSCode:** retain the read-only virtual document and use native browsing
  (navigation, search, selection and copy). Disable all ordinary Magit
  capture/shortcut overrides, including mirrored fold-key overrides, while in
  text mode; retain the explicit return toggle/command. The document remains
  read-only.
- **Exit:** the server re-renders the current tree (with queued refreshes
  applied) as a full replace. Neovim replaces the buffer, resets
  `modified=false` and sets `undolevels=-1` before replacing so the user
  cannot undo into stale content; VSCode refreshes its read-only virtual
  document and restores ordinary Magit bindings.
- Complementary first-class commands avoid needing text mode for common
  copies: `common.copy-value` (hash for a commit, path for a file, diff text
  for a hunk/selection) returning a `clipboard` effect. Yank/copy values are
  server-owned structured-tree/content operations, never client parsing of
  rendered text. Whole-hunk, single-line and visual-selection yanks return raw
  code source: remove exactly the structural leading diff marker (`+`, `-`, or
  context space), preserve actual code/indentation and literal `+`/`-`
  characters and line order, and exclude hunk/file headers and no-newline
  annotations. They do not mutate Git or reconstruct old/new file contents.

### 3.15 Highlighting and theme integration

Goal: elements are visually distinct, readable, and follow the user's colour
scheme. T028 specifies role-based resolved-style tests; this design is intended.

- The server labels text with **semantic tokens**: a custom token *type* plus
  *modifiers*. Clients map each type to the standard theme group(s).
- **Neovim:** LSP semantic tokens create highlight groups named
  `@lsp.type.<type>.<filetype>` / `@lsp.mod.<modifier>.<filetype>`. The client
  defines each with `vim.api.nvim_set_hl(0, group, {link = "<standard group>", default = true})`
  so users can override and colour schemes apply. Never set `fg`/`bg` directly.
- **VSCode:** contribute `semanticTokenTypes` (with `superType` where one
  fits) and `semanticTokenScopes` mapping each type to TextMate scopes
  (`markup.inserted`, `markup.deleted`, …) so any theme colours it (verify exact
  contribution points when implementing).
- Prefer **foreground** colours (works with all themes). Full-line background
  tinting for added/removed lines is an optional later extra (client option),
  because semantic tokens cannot colour whole lines.

T028 tests semantic roles and resolved styles against supported built-in
schemes. Test that roles resolve to readable styles and user overrides work;
do not lock acceptance to a token-to-group table or rendered HTML/image.

| Token type | Meaning | Neovim link | VSCode scope |
| --- | --- | --- | --- |
| `magit.section` | Section titles ("Unstaged changes") | `Title` | `markup.heading` |
| `magit.count` | `(3)` after a title | `Comment` | `comment` |
| `magit.file` | File path in a diff header | `Directory` | `entity.name.tag` |
| `magit.file.modified` / `.added` / `.deleted` / `.renamed` / `.untracked` | status word | `Changed` / `Added` / `Removed` / `Changed` / `Comment` | `markup.changed` / `markup.inserted` / `markup.deleted` / `markup.changed` / `comment` |
| `magit.hunk.header` | `@@ -1,3 +1,4 @@` | `Special` | `meta.diff.range` |
| `magit.diff.added` / `.removed` | `+` / `-` lines | `@diff.plus`→`Added`; `@diff.minus`→`Removed` | `markup.inserted` / `markup.deleted` |
| `magit.diff.context` | context lines | `Normal` (no token) | — |
| `magit.hash` | commit hash | `Constant` | `constant.numeric` |
| `magit.ref.local` / `.remote` / `.tag` / `.head` | refs | `Type` / `Special` / `Label` / `Type` (+ bold) | `entity.name.type` … |
| `magit.author` / `magit.date` | log columns | `Identifier` / `Comment` | `variable` / `comment` |
| `magit.menu.*` | see §3.11 | `Title`, `Statement`, `Keyword`, `Normal`, `Special`, `String`, `Comment`, `String`, `Comment`, `Comment` | as appropriate |
| `magit.message.warn` / `.error` | inline notices | `WarningMsg` / `ErrorMsg` | `markup.deleted` |

Modifiers: `staged`, `selected`, `planned`, `disabled`, `bold`. Neovim applies
`@lsp.mod.<modifier>.<filetype>` above the type group, so modifier links only
add attributes: `staged` / `bold` → `Bold`, `selected` → `Visual`,
`planned` / `disabled` → `Comment`.

### 3.16 Async runner, refresh, parsing

- `tokio`. Replace the synchronous loop. Git invocations go through **one
  runner**: serialises mutations per repository (a queue); lets reads and key
  handling continue during long operations; streams `$/progress`; supports
  cancellation; records command, exit code, duration, stdout, stderr in a ring
  buffer backing the **process log**; never lets child stdout reach the LSP
  pipe; logs via `tracing` to stderr/file only.
- **Refresh:** `common.refresh` plus automatic refresh from a debounced file
  watcher (`notify`) on the worktree and on `.git` (`HEAD`, `index`, `refs`,
  `MERGE_HEAD`, `rebase-*`, `CHERRY_PICK_HEAD`, …). Background reads use
  `--no-optional-locks` (`GIT_OPTIONAL_LOCKS=0`). Handle `index.lock` with a
  short retry then the IMM-3 message. Paused in text mode.
- **Parsing:** `git status --porcelain=v2 -z --branch` and `-z` everywhere
  (spaces, quotes, unicode, renames with both paths). In patches, reuse Git's
  own header lines verbatim.
- Hooks: capture output into the process log. Interactive hooks unsupported.

### 3.17 LSP-native features to use

| LSP feature | Used for |
| --- | --- |
| `textDocument/semanticTokens` | Highlighting (§3.15) |
| `textDocument/documentSymbol` (hierarchical) | Section/file/hunk navigation, outline, breadcrumbs. Replaces `vcs/targets` |
| `textDocument/definition`, `window/showDocument` | "Visit": hunk line → file+line in worktree; hash → revision view; staged-side line → blob view |
| `textDocument/documentLink` | Hashes, paths and refs clickable |
| `textDocument/hover` | Commit summary on a hash, ahead/behind on a ref |
| `textDocument/codeAction` | Context actions at the cursor (stage, unstage, discard, …) |
| `textDocument/publishDiagnostics` | Commit-message lint (summary > 50 chars, non-blank line 2, body > 72), unresolved conflict markers |
| `textDocument/completion` | In commit messages: refs, trailers |
| `$/progress`, `window/showMessage` | Progress, messages |

### 3.18 Configuration

- **Format:** TOML.
- **Default location:** the OS config directory, `<config>/magit-lsp/config.toml`:
  Linux and other Unix `$XDG_CONFIG_HOME` or `~/.config`; Windows `%APPDATA%`;
  macOS: use `~/.config` (XDG-style) unless `$XDG_CONFIG_HOME` is set, because
  that is what terminal-tool users expect (the `etcetera` crate's
  `choose_base_strategy` does this; the `dirs` crate would instead pick
  `~/Library/Application Support` — decide at implementation time, §9).
- **Overrides:** `--config <path>` flag and `MAGIT_LSP_CONFIG` env var.
- **Includes:**
  ```toml
  include = ["keys-work.toml", "/etc/magit-lsp/team.toml", "~/dotfiles/magit/extra.toml"]
  include_optional = ["local.toml"]   # missing file is not an error
  ```
  - Relative paths resolve against the **directory of the file containing the
    `include`**. Absolute paths and `~` are allowed. Environment variables
    (`$VAR`, `${VAR}`) are expanded.
  - Includes load first, in order; the including file's own values override
    them. Later includes override earlier ones.
  - Merge rule: tables merge key by key; **arrays and scalars replace**.
  - Include cycles are detected and reported as an error naming the chain.
- **Layers (lowest → highest):** built-in defaults → profile → user config
  (with includes) → client `initializationOptions` → per-repo
  `.magit-lsp.toml` (only non-executable settings: keys and display; opt-in
  setting `trust_repo_config`).
- **Keys:** `profile = "vim"` or `"standard"`; tables `[keys.common]`,
  `[keys.status]`, `[keys."menu.commit"]`, etc. use short command names and
  arrays of alternatives (`confirm=["Enter"]`, `cancel=["Escape","ctrl+c"]`,
  `stage=["s"]`, `discard=["x","Delete"]`, `amend=["a"]`). Arrays replace
  profile lists; `unbind=["K"]` removes a binding. A scope's `confirm=["Space"]`
  overrides common confirm only there (e.g. `[keys.log]`).
- Other settings (non-exhaustive): `[diff] context, whitespace`, `[sections]
  order, default_visibility`, `[log] default_args, margin`, `[ui]
  confirm_discard = true`. Menu argument persistence semantics must be reviewed
  before adding any persistence setting.
- Hot reload on file change or `workspace/didChangeConfiguration`; invalid
  config keeps the previous one and shows a `Message` with file and line.

### 3.19 Backend boundary and jj

Stance: **keep Git operation semantics intact while making Plain presentation
and standard-LSP transport VCS-neutral.** Git-specific discovery, mutation
validation and workflow details stay behind providers; reusable interactions
take concrete views/data and stable identities. This enables future jj support
with effectively zero Plain-client changes, without requiring a jj backend,
generic VCS trait hierarchy, or speculative wire protocol. This does not
promise a low-cost backend fork; keep Git-specific CLI/domain details where
they are meaningful. See §0 and the Plain phase task list.

---

## 4. Protocol reference

No backward compatibility: delete the old `vcs/*` methods as their
replacements land.

### 4.1 Client → server (current contract; Plain presentation selection is intended, not yet implemented)

| Method | Params | Returns |
| --- | --- | --- |
| `initialize` | `clientInfo`; rich clients declare `capabilities.experimental.magit` v1. Absence selects Plain. `profile?` remains in initializationOptions. | server capabilities |
| `view/open` | `{kind, args?, placement?, viewport}` (`status` carries `args.repo_path`) | `ViewDescriptor` |
| `view/close` | `{viewId}` | — |
| `view/key` | `{viewId, version, key, editorMode, cursor: {line, column}, selection?: {startLine, endLine}}` | `{effects: [...]}` |
| `view/invoke` | `{viewId, command, args?, cursor?, selection?}` | `{effects: [...]}` |
| `view/rendered` (notification) | `{viewId, version}` | — |
| `view/resync` | `{viewId}` | full render |
| `view/resize` (notification) | `{viewId, cols, rows}` | — |
| `view/setMode` | `{viewId, mode: "interactive"/"text"}` | `{effects}` |
| `view/promptRespond` | `{promptId, cancelled, value?}` | — |

### 4.2 Server → client

| Method | Params |
| --- | --- |
| `view/render` (notification) | `{viewId, version, baseVersion?, text?, edits?: [{startLine, endLine, lines}], cursor?: {line, column}, mode, pending?, menu?: Menu}` |
| `view/keymap` (notification) | `{viewId, scope, version, capture, bindings: [{command, keys: [string], modes: ["normal","visual"], label, doc, group, status}], mode, pending?, liveKeys?: [string]}` |
| `view/effects` (notification) | `{effects: [...]}` for asynchronous effects |
| `view/prompt` (request) | See §3.6; returns `{cancelled, value?}` |
| standard | `window/showDocument`, `window/showMessage`, `$/progress`, semantic tokens, symbols, definition, links, hover, code actions, diagnostics, completion |

Plain mode uses standard document synchronization and code actions/commands for
semantic action dispatch; concrete views/data carry stable identities and are
never reconstructed by parsing rendered text. Git-specific discovery,
mutation validation, commit-file paths, helper lifecycle and completion
candidates belong to Git backend/workflow providers. Reusable server
interaction primitives must support serving/stacking views, single and
multi-item in-buffer selection with stable identities and caller restoration,
message-edit sessions using a provider-supplied real file and finish/cancel
callbacks, and semantic action dispatch. Git wrappers retain operation-specific
semantics. Require a minimal fake-provider independence test, not a production
jj backend or new wire protocol. Plain message editing reuses the implemented
real-file/ring contract in §3.10; broader Plain dispatch remains a target.

### 4.3 Example

Client presses `s` with the cursor on a `+` line in an unstaged hunk:
```json
{"method":"view/key","params":{"viewId":"v1","version":12,"key":"s","editorMode":"normal","cursor":{"line":18,"column":4}}}
```
Server stages the hunk and answers:
```json
{"result":{"effects":[]}}
```
followed by a notification:
```json
{"method":"view/render","params":{"viewId":"v1","version":13,"baseVersion":12,
 "edits":[{"startLine":9,"endLine":22,"lines":["  Staged changes (1)", "…"]}],
 "cursor":{"line":10,"column":4}}}
```

---

## 5. Default keymaps (profiles)

Profiles are data files shipped in the binary (`profiles/standard.toml`,
`profiles/vim.toml`) and validated by tests. Values below are **proposals**
meant to be committed as the first version; edit freely.

**Rules for every binding in every profile**
- R1. No Emacs prefix keys (`ctrl+c ctrl+*`, `ctrl+x …`) and no `alt+<letter>`
  meant to mimic `M-<letter>`.
- R2. Prefer Magit's mnemonic letters where the letter matches the Git
  operation name (`c` commit, `b` branch, `l` log, `d` diff, `f` fetch,
  `F` pull, `P` push, `m` merge, `r` rebase, `t` tag, `s`/`u` stage/unstage).
- R3. `standard` uses direct, widely-known keys: `Enter` confirms, `Escape`
  cancels, `Delete` discards, arrows navigate.
- R4. `vim` never rebinds visual-mode keys (`v`, `V`, `ctrl+v`), counts
  (`0`–`9` as counts), `j k`, `gg`, `G`, `ctrl+d/u/f/b`, `/`, `H`, `L`.
  Structured views may bind `h`/`l`; text mode retains native horizontal motions.
- R5. Every command has a binding (or an explicit "unbound" entry) in every
  profile; a test enforces this.
- R6. `ctrl+c` always means cancel in non-editable views (with the
  copy-passthrough rule from §3.9). Editable scopes use the keys below.

### 5.1 `common` scope

| Command | `standard` | `vim` |
| --- | --- | --- |
| `common.confirm` | `Enter` | `Enter` |
| `common.cancel` | `Escape`, `ctrl+c` | `Escape`, `ctrl+c` |
| `common.quit` | `q` | `q` |
| `common.refresh` | `g`, `F5` | `g r` |
| `common.toggle-section` | `Tab` | `Tab`, `z a` |
| `common.show-section` / `hide-section` | — | `z o` / `z c` |
| `common.cycle-visibility` (all sections) | `shift+Tab` | `shift+Tab` |
| `common.show-all` / `hide-all` | unbound | `z R` / `z M` |
| `common.more-detail` / `less-detail` | unbound | `z r` / `z m` |
| `common.detail-level-1` … `detail-level-4` | `alt+1` … `alt+4` | unbound |
| `common.next-section` / `prev-section` | `n` / `p` | `] ]` / `[ [` |
| `common.next-sibling` / `prev-sibling` | `alt+ArrowDown` / `alt+ArrowUp` | `g j` / `g k` |
| `common.parent-section` | `alt+ArrowLeft` | `g u` |
| `common.toggle-text-mode` | `ctrl+t` | `ctrl+t` |
| `common.copy-value` | `ctrl+shift+c` | `y y` |
| `common.visit` | `Enter` | `Enter` |
| `open.dispatch` (help / menu of menus) | `?` | `g ?` |

Visibility has four levels: section headers → file headers → hunk headers
(without body lines) → individually expanded hunk bodies. The four
`common.detail-level-N` actions set an absolute level; `standard` binds
`alt+1` … `alt+4` to these actions. `vim` uses `z m`/`z r` to step down/up
and `z M`/`z R` to hide/show extremes. These are server-owned visibility
actions; clients do not infer structure or key-specific state.

The four R-NAV bindings are Implemented. `shift+Tab` cycles the global detail
level 4 → 1 → 2 → 3 → 4. Sibling motion stays within the immediate parent
(root sections are siblings), never wraps, and treats a diff body as its hunk.
Parent motion goes to the immediate containing header; roots stay put.

### 5.2 `status` scope (also used by diff-like views for staging)

| Command | `standard` | `vim` |
| --- | --- | --- |
| `status.stage` | `s` | `s` |
| `status.unstage` | `u` | `u` |
| `status.stage-all` | `S` | `S` |
| `status.unstage-all` | `U` | `U` |
| `status.discard` | `Delete` | `x` |
| `status.reverse` | `v` | `R` |
| `status.apply` | `a` | `a` |
| `open.commit` | `c` | `c` |
| `open.branch` | `b` | `b` |
| `open.log` | `l` | `l` |
| `open.diff` | `d` | `d` |
| `open.fetch` | `f` | `f` |
| `open.pull` | `F` | `F` |
| `open.push` | `P` | `P` |
| `open.merge` | `m` | `m` |
| `open.rebase` | `r` | `r` |
| `open.stash` | `z` | `Z` |
| `open.tag` | `t` | `t` |
| `open.reset` | `x` | `X` |
| `open.revert` | `V` | `-` |
| `open.cherry-pick` | `A` | `A` |
| `open.bisect` | `B` | `B` |
| `open.remote` | `M` | `M` |
| `open.submodule` | `o` | `o` |
| `open.worktree` | `%` | `g %` |
| `open.ignore` | `i` | `i` |
| `open.refs` | `y` | `g y` |
| `open.cherries` | `Y` | `g Y` |
| `open.patch-apply` | `w` | `w` |
| `open.patch-create` | `W` | `W` |
| `open.run` | `!` | `!` |
| `open.process-log` | `$` | `g $` |

`vim` keeps the single letters `f F t T m r M` (shadowing the Vim find/till/
mark/replace motions) because they match Magit and are useless in a read-only
buffer. `v V ctrl+v` remain Vim's visual modes, which is why reverse is `R`,
revert is `-`, stash is `Z` (since `z` is the fold prefix), reset is `X`
(since `x` is discard) and a few menu keys gain a `g` prefix.

### 5.3 Select mode (`log.select`, `refs.select`)

| Command | `standard` | `vim` |
| --- | --- | --- |
| `common.confirm` | `Enter` | `Enter` |
| `select.toggle-mark` (multi) | `Space` | `Space` |
| `select.enter-rev` | `i` | `i` |
| `common.cancel` | `Escape`, `ctrl+c` | `Escape`, `ctrl+c` |

### 5.4 Menu scopes (`menu.*`)

| Command | `standard` | `vim` |
| --- | --- | --- |
| Toggle a switch | `-` then its letter | same |
| Set an option | `=` then its letter | same |
| Run an action | its letter | same |
| `common.cancel` / close | `Escape`, `ctrl+c`, `q` | same |
| `menu.save-args` | `ctrl+s` | `ctrl+s` |
| `menu.reset-args` | `ctrl+r` | `ctrl+r` |
| `open.dispatch` (toggle help) | `?` | `?` |

### 5.5 Commit-message scope

| Command | `standard` | `vim` |
| --- | --- | --- |
| `common.confirm` | `ctrl+Enter` | `Z Z` (and `ctrl+Enter` where the terminal supports it) |
| `common.cancel` | `ctrl+shift+x` | `Z Q` |
| `commit-message.history-prev` / `next` | `alt+ArrowUp` / `alt+ArrowDown` | `[ m` / `] m` (normal mode) |
| `commit-message.add-signoff` | `ctrl+alt+s` | `g s` |

Closing the buffer is always cancel (§3.10). `ctrl+c` is **not** cancel here
because it is copy in non-modal editors.
The current bridge emits this scope for commit messages; merge/tag message
support is not emitted yet, and `rebase-todo` has no implemented edit scope.

### 5.6 Rebase todo (structured view)

| Command | `standard` | `vim` |
| --- | --- | --- |
| `rebase.pick/reword/edit/squash/fixup/drop` | `p`/`r`/`e`/`s`/`f`/`Delete` | `p`/`r`/`e`/`s`/`f`/`x` |
| `rebase.move-up` / `move-down` | `alt+ArrowUp` / `alt+ArrowDown` | `ctrl+k` / `ctrl+j` |
| `common.confirm` / `common.cancel` | `ctrl+Enter` / `Escape` (prompts if changed) | `Z Z` / `Z Q` |

(Other views define their bindings when implemented; they must follow R1–R6.)

---

## 6. View catalog

The final column references stable parent task IDs in §7; completed status/process
work is summarized in the archive, managed editor work is R-EDIT, and todo is R7.3.

Status common hierarchy navigation and four-level visibility cycling are
implemented and verified by R-NAV (§7); these are server-tree operations.

| View | Magit analog | Contents | Key commands | §7 parent task |
| --- | --- | --- | --- | --- |
| **Status** | `magit-status` | Header lines (Head, Merge, Push, Tag); sections: in-progress operation, Untracked, Unstaged, Staged, Unmerged, Stashes, Unpulled, Unpushed, Recent commits, Submodules | stage/unstage/discard/apply/reverse, visit, toggle, refresh, all menus | Completed (§10/archive); R-3.6; R-6 |
| **Log** | `magit-log` | Commit list with graph, decorations, author, date; options (`-n`, `--graph`, `--decorate`, `--author`, `--since`, `--grep`, `-S`, `-G`, path limits); lazy "load more"; **select mode** | show commit, checkout/reset/branch/rebase/cherry-pick/revert from the commit at point, confirm/cancel | R-5.1 (completed; archive) |
| **Log variants** | `l` menu | current, other, all, local branches, reflog, file log, line-range log (`-L`), `HEAD..upstream`, `upstream..HEAD` | same | R-5.3 |
| **Revision** | `magit-revision` | Header (hash, parents, author, committer, dates, refs, message), diffstat, full diff with hunks; parent/child navigation; combined diff for merges | visit file, apply/reverse hunk, copy hash | R-5.2 |
| **Diff** | `magit-diff` | Any range `a..b`, `a...b`, worktree vs index vs revision, path-limited, stash vs parent, stat-only; context/whitespace/rename/word-diff options | visit, stage/unstage when applicable | R-5.2 |
| **Refs** | `magit-show-refs` | Local branches, remotes, tags with ahead/behind vs a base; also select mode | checkout, create, delete, rename, reset, merge, rebase onto, log | R-5.3 |
| **Cherry** | `magit-cherry` | `git cherry` between two refs | cherry-pick, show | R-7.2 |
| **Stash list / show** | `magit-stash-list` | Stashes; show = diff view of a stash | apply, pop, drop, branch | R-6.5 |
| **Rebase todo** | `git-rebase-mode` | Planned structured interactive todo: pick/reword/edit/squash/fixup/drop/exec/label/merge lines with commit summaries; reorder | move up/down, set action, show commit, confirm, abort | R-TODO / R-7.3 |
| **Rebase in progress** | status section | Todo position, stopped commit, conflicts | continue, skip, abort, edit todo | R-OP-STATE / R-7.3 |
| **Merge / conflicts** | `magit-merge` + unmerged | Unmerged section; per-conflict actions | take ours/theirs, mark resolved, open merge tool (client effect `openMergeTool`), continue, abort | R-7.1 |
| **Sequencer** | `A`, `V` | In-progress cherry-pick/revert | continue, skip, abort | 7 |
| **Commit message** | with-editor buffer | Native real Git file; companion staged diff, history recall (R-EDIT-HISTORY), trailers and lint remain roadmap | confirm, cancel; planned history prev/next, add trailer | R-EDIT |
| **Process log** | `magit-process-buffer` | Every Git invocation: args, exit code, duration, output, hook output | visit, copy command, clear | Completed (archive) |
| **Blame** | `magit-blame` | Annotated file (hash, author, date, summary); also inlay hints/code lens in real file buffers | show commit, blame at parent, copy hash | R-8.1 |
| **File at revision** | `magit-find-file` | Virtual doc `magit://blob/<rev>/<path>` | diff against worktree | R-5.3 |
| **Bisect** | `magit-bisect` | In-progress section: good/bad/skip log, candidate | good, bad, skip, run, reset | R-8.2 |
| **Worktrees** | `magit-worktree` | List with branch/path/state | add, checkout, move, delete, visit | R-8.2f |
| **Submodules** | `magit-submodule` | Modules section and list | add, register, update, sync, fetch, populate, unpopulate | R-8.2 |
| **Remotes / config** | `magit-remote`, `magit-gitconfig` | Remote list; per-branch/remote config | add, rename, remove, prune, set upstream/pushRemote | R-6.3 |
| **Patches** | `w`, `W` | `git am`/`apply`, `format-patch`; in-progress am | apply, continue, skip, abort | R-8.2 |
| **Notes / ignore / run** | `T`, `i`, `!` | Notes; gitignore targets; run arbitrary git command | per command | R-8.2a |
| **Repository list** | `magit-list-repositories` | Multi-repo overview | open status | R-9.3 |
| **Dispatch / help** | `magit-dispatch` | Generated from the command registry and keymap manifest | invoke by key | Completed (§10/archive) |
| **Menus** | transients | commit, branch, fetch, pull, push, merge, rebase, stash, tag, reset, cherry-pick, revert, bisect, remote, submodule, worktree, diff, log, ignore, patch | §3.11 | R-5.1 / R-5.2 |

---

## 7. Numbered implementation queue

The sole active numbered backlog is [`TASKS_PART_SEVEN.md`](./TASKS_PART_SEVEN.md). Old ticket scope and Verify text remain recoverable from immutable `d04dfa2:TASKS.md`; provenance is not completion evidence. See [migration ledger](docs/evidence/task-queue-migration.md).

### Retained gaps

- **A12:** real-dispatch Git-failure handling and removal of `server::mutation`; no numbered owner evidenced. Do not conflate with T103's separate same-session reader-lock redesign.
- **R-EDIT-REAL:** parent acceptance is not green. T104 is only a subset; last-window exit, dirty shutdown, cross-build, reload, workspace and platform limits remain. Broad fallback review is not comprehensive acceptance.
- Binary files remain whole-file-only; T093 layout work remains open.
- §9 retains unresolved config-directory/root-edit and Escape caveats.

### Task ticket template and UI rule

Every ticket states **Target Files** (exclusive write boundary), **Reference Files**, **Architectural Constraints**, **Acceptance Criteria**, and exact **Verification Command**; also state dependencies and size. Each ticket inherits its applicable phase gate from its enclosing named phase and the phase map. The phase gate applies when the last unchecked item in a named phase group completes, not when an individual ticket, slice or review-fix batch completes. The active higher-priority review checkpoint remains every eight completed tasks since `Last review:`; see the proposed orchestration configuration in [AGENTS.md](./AGENTS.md#working-conventions). Apply shared rules in [AGENTS.md](./AGENTS.md#working-conventions). User-visible changes require observable text/structured outcomes and focused native evidence; never images.

---

## 8. Testing strategy

**Target distribution:** ~80% Rust in-process, ~15% protocol-level (spawned
binary over stdio), ~5% Neovim. The Neovim tests that exist stay as
integration tests; their *intent* for logic moves to Rust.

### 8.1 In-process `TestSession` harness (main tool)

The harness is implemented in `tests/common/session.rs` (IMM-7; completed
history in the archive); extend it there rather than adding another session
type, with visibility/keymap/effects inspection as needed. Drive the real key/invoke path,
locate cursor targets through tree metadata, snapshot buffers (`insta` or
`expect-test`) and assert repo state with Git. `ScriptedUi` queues text,
confirm, choice and `edit_document(|text| …)` replies.

### 8.2 Determinism

Fixtures set `GIT_AUTHOR_DATE`, `GIT_COMMITTER_DATE`, `GIT_CONFIG_NOSYSTEM=1`,
a temp `HOME`, `XDG_CONFIG_HOME` to a temp dir, `LC_ALL=C`, fixed author and
committer, so hashes are stable and appear verbatim in snapshots. Normalise
only temp paths and timings.

### 8.3 Other Rust layers

- **Patch engine:** `proptest` — random file pairs and random selections;
  `git apply --cached --check` passes; staging everything selected gives the
  expected index; stage-then-unstage is identity; selections containing context
  or header lines never produce a corrupt patch (IMM-2).
- **Fixture matrix:** renames, copies, mode changes, symlinks, binary, CRLF,
  no trailing newline, spaces/unicode/quote paths, unborn HEAD, detached HEAD,
  merge conflicts, in-progress rebase/merge/cherry-pick, worktrees, submodules,
  empty repo, large diffs, changes 4 lines apart and 20 lines apart (hunk
  merging, IMM-5).
- **Keymap:** no duplicate keys within a scope chain; every command has a doc
  and a binding in each profile (R5); manifest snapshot per profile; text-mode
  keymap has only the toggle; workbench deny-list; key-notation round trips
  for every translator.
- **Config:** include order, relative/absolute/`~`/env expansion, optional
  includes, cycle detection, array-replace/table-merge, error messages with
  file and line.
- **Document model:** stable Node IDs across refresh; cursor successor;
  stale-action rejection; lazy bodies not computed while collapsed.
- **Menus:** render snapshots at several widths; switch/option/action flows
  through `ScriptedUi`; `Planned` entries show the message and change nothing.
- **Editor bridge/askpass:** fake `GIT_EDITOR` flow end to end through the
  socket helper; client disconnect aborts Git.
- **Error mapping:** one test per row of the IMM-3 table.

### 8.4 Colour and presentation tests (T028)

Test token names and role coverage. Add a Rust token-response test for a fixture,
client data tests that map each legend type to a Neovim link or VS Code scope,
and one ANSI smoke test per client using one colour scheme. The smoke test checks
that added and removed cells have different foreground colours. Do not test
contrast, multiple schemes, theme changes, or HTML rendering. `:TOhtml` cannot
see semantic-token namespaces; the historical IMM-8 spike is not acceptance.

### 8.5 Protocol-level tests

A few tests spawn the real binary over stdio: `initialize` capabilities,
`view/open` → `view/render`, `view/key` round trip, ack/resync, prompt request
and response, standard LSP requests (`documentSymbol`, `semanticTokens`).

### 8.6 Neovim integration (kept, few)


Attach and no orphan processes; atomic keymap replace (old keys gone, new
present, nothing between); visual-selection forwarding; fold-key mirroring
(including a user `nnoremap <Tab> za`); `passThrough` of an unbound key;
read-only enforcement (IMM-4); text-mode toggle; error notifications (IMM-3);
prompt effects via stubbed `vim.ui.input`/`vim.ui.select`; the editor bridge
opens the commit-message buffer in the same instance; bottom menu placement.

### 8.7 CI

`cargo test` on Linux/macOS/Windows; Neovim tests on stable and nightly;
manifest-snapshot job; generated VSCode keybindings up to date; run IMM-1 tests
on the oldest supported git version; a `ui-test` job (tmux + Neovim) that runs
`make ui-test` and uploads `build/ui/` on failure (R-UI-6).

### 8.8 Real-UI tests (L3)

L3 drives a real Neovim inside tmux and asserts on the captured screen. It
catches what the other layers cannot: timing, chord timeouts, key encoding,
the command line and real colours.

| Layer | Tool | Use it for |
| --- | --- | --- |
| L1 | Rust `TestSession` | Menu content, pending-chord state, view-stack depth, messages, token attributes |
| L2 | mini.test child Neovim | Window counts, floats, keymaps applied, buffer options, `:messages` |
| L3 | Real Neovim in tmux | What the user sees: timing, chord timeouts, key encoding, the command line, real colours |

- `scripts/ui start <fixture>` builds a deterministic repo (§8.2 environment)
  and opens the client in a dedicated tmux server; `keys`, `wait`, `screen`,
  `state`, `trace` and `stop` form the "inspect, act, inspect" loop described
  in `AGENTS.md`.
- `lua/magit-lsp/debug.lua` exposes `snapshot()` (windows, view stack, pending
  chord, render versions, keymap, messages) and `idle()`; `MAGIT_LSP_TRACE=<path>`
  records per-hop timings for `ui trace`.
- Scenarios are `.scn` files in `tests/ui/` using the DSL in
  `ui-test-plan.md` §A.5. Artefacts land in `build/ui/<scenario>/` as
  `.screen` and `.ansi`.
- **Never sleep:** poll `idle()` with `ui wait`.
- A known bug is an `xfail: <bug id>` scenario. The runner reports XFAIL, and a
  passing `xfail` fails the run until the tag is removed.
- `make ui-test` runs every scenario; `make ui-tour` writes
  `build/ui/tour.md`; `make ui-unit` runs the harness's own Python tests.
- Every menu scenario runs under both profiles (`standard`, `vim`) at 100x30
  and 80x24; `size-only`/`profile-only` opt out for latency and the tour.

---

## 9. Open questions

1. **macOS config dir:** `~/.config` via `etcetera` (proposed) or
   `~/Library/Application Support` via `dirs`.
    -- Use dirs. We should have a root command for editing the config file in the client though.
2. **Escape behavior:** Escape cancels most views. Commit-message and rebase
   buffers may need client-specific exit behavior; retain this caveat.
3. **VSCode menu default:** QuickPick (proposed) vs. text document in a lower
   editor group. Revisit after the Phase 2 spike.
4. **Per-repo config trust:** is `.magit-lsp.toml` worth the risk surface?
   Currently limited to keys and display, behind `trust_repo_config`.
5. **Minimum supported Git version** (decide by running IMM-1 tests on old
   versions).
6. **Full-line background tinting:** wanted; T094 owns the feature.
7. **LSP 3.18 `workspace/textDocumentContent`** adoption when clients support it.
8. **Fold keys in VSCode** for users with customised `editor.toggleFold`:
   the "mirror my shortcuts" command (proposed) vs. any better mechanism found
   during the spike.

---
