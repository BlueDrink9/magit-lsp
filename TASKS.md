# magit-lsp — Roadmap
Overnight: until none | parked 0/6
Stop after: user-requested Phase 6 completion as far as possible, plus release tasks and blockers (2026-11-19). This supersedes the documentation-only execution restriction; Phase 6A still requires T022 sign-off and release still requires explicit human approval. Do not modify pre-existing untracked user files.
Last review: `18ba3df` (2026-11-19, base `4045289`); both P1 findings resolved (`fd4fe5d`, `c850bd0`, `e42301c`, `fc34fe9`, `abd67c3`), no P0/P2/P3. Full `make test` passed after user provided ps; Phase 6 evidence below. Review fixes do not trigger another heavy gate; no release approval.
Gate rationale (2026-11-19): revalidate Phase 6 closure after accumulated 51-file changes since review `4045289` (VSCode lifecycle/export cleanup, native harness teardown, docs and hygiene); user accepted combined commit `b96e132` and continuation without history rewriting.

## Audit findings and work order (2026-11-18)

This file alone owns checkbox states and Done evidence. [TASKS_PART_SEVEN.md](./TASKS_PART_SEVEN.md) owns detailed numbered scopes/dependencies, not duplicate completion status. Current contracts live in maintained architecture/protocol references, not historical attempt logs.

Recommended order (not execution authorization): reopened P1 Review; ready independent work; blocked prerequisites; gated features/release; optional/unscheduled work; completed ledger. Revalidate old export inventories before deletion. T105 has one Phase 6A scope; T111 routes menu/fold and minimum-Git decisions to T113/T119. T065/T066/T078 may overlap delivered work, but overlap is INFERRED, not completion; compare remaining acceptance when scheduled.

Policy decisions: release does not require zero hygiene debt; findings must instead be fixed/deleted, excepted with a concrete reason, or assigned an exact owner task, with missing tools disclosed honestly. Readable prose/source size is maintainability guidance, not an absolute byte gate; intact logs/program/wire transcripts need not be split solely for size. Before project version 1.0, breaking protocol/API/config changes are allowed without compatibility shims or additive-only guarantees; the rich wire's `v=1` label does not itself promise stability. Negotiation and validation invariants remain. T022 remains unchecked pending its actual evidence, finding dispositions and review; explicit human release go/no-go remains mandatory. Optional prototypes, stable freeze and post-release features are not silently added to the release prerequisite subset.

Provenance: **Base** means immutable `a366409749edf0e5591dff34d70500b73ab595b9:TASKS.md`. Ledger `B<number>` maps a retained checkbox occurrence to its exact base line, including its original title, nesting, full acceptance, verification and correction context. Historical closed prose and superseded gates are consolidated into that source, not another archive. `Refs` are historical references, not inferred implementation Done commits. Ambiguous `Done: containing commit` and source-only evidence remain ambiguous; no verification SHA is substituted for implementation evidence. Separate occurrences remain separate ledger rows, even when they concern the same umbrella ID. No unchecked record is consolidated into a checked one.

Durable risks: R-EDIT-REAL's parent is not green (T104 covers only a subset); last-window exit, dirty shutdown, cross-build, reload, workspace and platform acceptance remain. Binary files remain whole-file-only; T093 layout remains open. R-PLAIN-EDIT-EOF-RISK retains intermediate-echo/terminator uncertainty; R-T020-STALE's original cause is unknown, and its investigation authorizes no retry. T007.1n's native timeout followed by a diagnostic pass is not a proven fix. Linux-native evidence never establishes Windows/macOS behavior. T004.5c has no producing-command proof for multiChoice. Scripted models are not native/real-server conformance. The initial T021 Helix smoke failed commit/abort; later attachment/lifecycle evidence is scoped to R-T021-ATTACH/DRAFT, not universal Helix support.

Historical gates (details in Base): Phase 2 `5da5d86`/`ebf9ed1`; Phase 3 `b4dae94`; schema `92c78fb`; Phase 4 `84fb4bd`; regression pass `43318a2`. Superseded failed gates, deleted success logs, missing-tool reports and bounded reviews are provenance, not current instructions or evidence that unrun suites passed. Current T022 evidence and holds are below.

Historical reference mapping (not additional tasks or completion claims): R-VS/A06 identifies the native routing boundary in Base line 98; T008.8 names the native-pick emitter grouping at lines 165–166; IMM-6 remains part of T008.9's recorded contract at line 177; T008.9c names the pick grouping used by T008.9d/e at lines 198–201. R-4.1 and R-4.1a are the old fixup/squash references at line 722, with instant autosquash routed to T019 at line 851; R-5.1 is the retained registry availability reason at line 1214. T120 is the formatter umbrella at lines 1185 and 1813–1816; its child records remain in the ledger.

## Contents

0. [Decisions already made](docs/ARCHITECTURE.md#0-decisions-already-made)
1. [Glossary](docs/ARCHITECTURE.md#1-glossary)
2. [Completed correctness contracts](docs/ARCHITECTURE.md#2-completed-correctness-contracts)
3. [Design decisions](docs/ARCHITECTURE.md#3-design-decisions-expanded)
4. [Protocol reference](docs/PROTOCOL.md#4-protocol-reference)
5. [Default keymaps (profiles)](docs/KEYMAPS.md#5-default-keymaps-profiles)
6. [View catalog](docs/VIEWS.md#6-view-catalog)
7. [Numbered implementation queue](#7-numbered-implementation-queue) and [scope specifications](./TASKS_PART_SEVEN.md)
8. [Testing strategy](docs/TESTING.md#8-testing-strategy)
9. [Open questions](docs/OPEN_QUESTIONS.md#9-open-questions)

## Bugfix

- [x] **T022 hygiene — retire unused pickFor export**
  - check: remove only the redundant external export, preserving module-local picker behavior and its real `handlePick` caller. Verify VSCode suite, then hygiene separately; do not add suppressions or artificial consumers.
  - Research (2026-11-19, `54c3e66`): requests.js:259 calls module-local pickFor (:285); only export is :333. Repository search finds no external consumer. Earlier note that integration.js consumes pickFor is historical and no longer true after its concurrent cleanup.
  - Phase gate evidence at `245316c`: narrow Plain/dispatch tests, `make ui-test`, `make ui-tour`, `make registry-docs-check` passed. `make hygiene` report-only exit 0: cargo-machete/Luacheck clean; Knip only unused pickFor. Size reports remain soft-policy exceptions; marker inventory needs final review disposition. Full `make test` still blocked on missing ps.
  - Verified A1 (2026-11-19, base `54c3e66`): only the `exports.pickFor = pickFor;` binding was removed. Repository sweep `rg -n -F pickFor clients/vscode test tests docs lua src Makefile` retains just `clients/vscode/lib/requests.js:259` (real `handlePick` call) and `:285` (module-local declaration); no external consumer or export remains, with no suppression or artificial consumer added.
  - Verified A2: `CI=true make vscode-test` exited 0 (`ok: build`, `ok: vscode-deps`, `ok: vscode-typecheck`, `ok: vscode`, `ok: keybindings`), preserving `view/pick` handling through module-local `pickFor` and its `view/pickRespond` reply.
  - Verified A3: evidence recorded here and committed with the source change by explicit path; the T022 parent checkbox stays unchecked (Part 1/2).
  - Verified A1 (Part 2, 2026-11-19, HEAD `1cc349a`): `make hygiene` → exit 0; `knip: CLEAN (exit 0)` with no findings at all, so `pickFor` is absent from the report. At HEAD the picker is module-local only (`clients/vscode/lib/requests.js:259` real call, `:285` declaration, no export after `fd8a85f`). `cargo machete: CLEAN (exit 0)`; `luacheck: CLEAN (exit 0)`. Honest scope: the run's working tree also carried the concurrent session's unstaged `clients/vscode/lib/requests.js` inlining; no suppression, allowlist entry or artificial consumer was added.
  - Verified A2 (Part 2): exit 0 is not claimed as a clean-size or clean-marker result. Soft over-limit files remain `docs/client-exchanges.md` (257454 bytes), `TASKS.md` (111202), `tests/server_async.rs` (101203) and `TASKS_PART_SEVEN.md` (100197). Dispositions cite existing records: *Waive size-only authored transcript splitting* keeps the complete `docs/client-exchanges.md` exchanges; *Waive size-only canonical roadmap obligation* covers `TASKS.md`/`TASKS_PART_SEVEN.md` ("readable prose/source size is maintainability guidance, not an absolute byte gate"); `tests/server_async.rs` is covered by the completed "Split async server tests below hygiene limit" ledger entry. No new waiver is invented. Marker inventory: 45 report lines over 23 paths, all accounted for — live Rust `keep-for:` markers name their existing architecture sections (`src/config.rs` §3.18 ×6, `src/keymap.rs` §3.9, `src/menu.rs` §3.11 plus `:141`/`:151` schema-only models with inline reasons, `src/semantic_tokens.rs` §3.15 ×2, `src/ui.rs` §3.6/R-3.1, `src/view.rs` §3.2 ×2); the same §3.2/§3.11/§3.15 text is carried into generated artifacts (`docs/schema/*` 7, `docs/client-payloads.md` 1, `clients/vscode/types/protocol.d.ts` 4) and `lua/magit-lsp` types/highlights; `docs/evidence/harness-reassessment-inputs-0{6,7,8}` hits are historical evidence quotes; `tests/common/mod.rs:6` is the test-only helper allowance recorded in `hygiene-allowlist.md:125`; `scripts/hygiene.py:84,:89` are the scanner needles themselves.
  - Verified A3 (Part 2): ticked with `Done: fd8a85f` (Part 1 code commit). The bookkeeping commit stages only this item's hunks from a HEAD-based copy, leaving the concurrent session's unstaged queue hunks in the worktree; T022 gate/Phase 6A holds and explicit human release approval are unchanged.
  - Done: `fd8a85f`.

- [x] **T022 gate — restore process-check prerequisite**
  - Done: `1cc349a` (user environment provision). Verified: `ps` available at `/opt/env/default/bin/ps`; full `make test` at base `18ba3df` passed all Rust, Lua and VSCode stages, including existing ps-based ownership/orphan assertions. Successful logs are deleted by Make; no separate survivor report was emitted. Prior missing-tool evidence below is historical.
  - check: `make test` can execute its existing process ownership assertions; install/expose `ps` on PATH (human environment prerequisite; workers must not install or substitute tools).
  - Verified: gate at `a5d8e27`, `make test` failed in `build/test-logs/rust.OGGcWl`; two server_async process checks cannot spawn `ps`. `command -v ps` returns no path. Do not weaken orphan assertions.
- [x] **T022 gate — diagnose instant-rebase bounded wait**
  - Done: `5f70a34`.
  - test: `dispatched_instant_fixup_conflict_can_be_resolved_and_continued` must observe the actual stopped rebase and complete resolve/continue without timeout-only changes or weaker assertions.
  - Verified: `make test` at `a5d8e27` failed at `tests/server_async.rs:401`, `instant rebase did not stop in bounded wait`; log `build/test-logs/rust.OGGcWl`. Independence from missing `ps` is not established.
  - Verified: `make test-rust FILTER="--test server_async dispatched_instant_fixup_conflict_can_be_resolved_and_continued"` → `ok: rust`; three consecutive runs of `--test server_async instant` → `ok: rust`. The bounded wait now drains queued and incoming frames and requires both the caller render and the exact `Instant autosquash stopped during autosquash; marker ...` error effect under one deadline; a timeout reports the confirm response, observed frames and render versions, HEAD, status, unmerged paths and rebase directories. The original 20s stall did not reproduce and no source defect was found; this path has no `ps` dependency (`ps` appears only in process-check tests, not `src`). Remaining risk: the original stall cause is unproven and may be load-related.

## Review

Priority work before further feature execution. Earlier checked Review items are in the completion ledger; their priority labels are historical.

- [x] **[P1] Synchronize the TERM-ignoring descendant fixture before teardown** (`tests/ui/test_driver.py:752-777`, `NvimHelperCommandTest.test_plain_preset_teardown_escalates_term_ignoring_descendant`)
  - **Problem:** VERIFIED: the parent publishes `ready` before installing its own SIGTERM handler, and before its descendant confirms installing any handler. The test can therefore pass after SIGTERM kills the descendant (only the parent's SIGKILL return code is asserted), or fail because SIGTERM races the parent's setup; it does not reliably exercise the claimed descendant escalation.
  - **Why it matters:** Correctness and test validity — the completed teardown review explicitly requires a TERM-ignoring descendant, not just a TERM-ignoring direct child.
  - **Change:** Install the parent's ignore handler before spawning/publishing readiness. Have the descendant publish its own PID/readiness only after installing SIGTERM ignore; wait for that marker under the existing deadline before calling `teardown`. Retain direct-child SIGKILL and descendant-disappearance assertions and unconditional `finally` cleanup. Extend this existing regression rather than adding a parallel scenario.
  - **Done when:** **test:** `make ui-unit` passes with readiness proving both handlers are installed before the first teardown signal; the existing real-process regression still verifies SIGKILL escalation and disappearance. **check:** `make plain-presets` retains all five graceful-shutdown assertions and leaves no owned server processes.
  - **Constraints:** Keep the change in `tests/ui/test_driver.py`; do not weaken survivor assertions, substitute a fixed sleep for readiness, change production teardown, or claim non-Linux verification.
  - Verified (2026-11-19, base `18ba3df`): `make ui-unit` → `ok: ui-unit`; `V=1 make ui-unit` → `Ran 273 tests in 3.126s` / `OK`. The parent now installs its SIGTERM ignore handler before spawning, and the descendant publishes its PID only after installing its own handler, so the bounded marker wait proves both before the first teardown signal. Direct-child SIGKILL and descendant-disappearance assertions are unchanged; the unconditional `finally` cleanup still tears down the owned group when the readiness assertion fails. The native `make plain-presets` check is Part 2 (below).
  - Verified (Part 2/2 native, 2026-11-19, HEAD `fd4fe5d`): `make plain-presets` → exit 0 (`ok: build`, `ok: plain-presets`); `V=1 make plain-presets` printed `PASS` for all five presets (`minimal`, `showDocument`, `completion`, `progress`, `full`), each ending "cancel restores document and Git state, clean shutdown" (`scripts/test_plain_presets.py:161`). Process-state evidence: a `/proc` scan by `comm`/cmdline identity found no surviving `magit-lsp` or preset-harness process after the run (the only match was the scanner itself). Linux only.
  - Done: `fd4fe5d` (Part 1/2 fixture readiness synchronization; Part 2 native preset verification above).

- [x] **[P1] Make native VSCode prerequisite checks match the launcher requirements** (`Makefile:120-129`, `vscode-prereqs`; `clients/vscode/test/e2e/run.mjs:44-71`, prerequisite checks; `clients/vscode/lib/version.js:7-33`, `resolveCommand`)
  - Done: `e42301c`, `fc34fe9`. Part 3/3: reused the recorded passing `make vscode-test` from Part 2 (source unchanged); all acceptance satisfied. No repeat gate or release approval implied.
  - **Problem:** VERIFIED: `vscode-prereqs` unconditionally requires Xvfb and recognizes only `code` or an explicit path, although the launcher requires an existing DISPLAY and accepts `codium`; a valid desktop/codium setup is rejected before `vscode-version-startup`. Separately, the new A06 xdotool check cannot reject a missing tool: `resolveCommand('xdotool')` returns the truthy string `'xdotool'` when resolution fails.
  - **Why it matters:** Correctness and cohesion — the new preflight rejects supported environments and fails to detect the missing prerequisite it claims to check, pushing failure into a running native host.
  - **Change:** Align `vscode-prereqs` with the actual launcher: require DISPLAY, accept executable `MAGIT_VSCODE_BIN`, `codium`, or `code`, and remove the unused Xvfb requirement (the runner does not start Xvfb). In the A06 launcher check, verify that the resolved xdotool path identifies an executable rather than testing string truthiness. Preserve `resolveCommand`'s existing diagnostic fallback contract and strict focused-run failure behavior.
  - **Done when:** **check:** through `make vscode-prereqs`, record acceptance of DISPLAY plus executable codium without Xvfb/code, and rejection of missing DISPLAY/editor. **check:** through `make vscode-e2e`, a controlled PATH without xdotool fails with the prerequisite diagnostic before launching an editor; the normal equipped environment still completes A06. **test:** `make vscode-test` passes. Record these environment checks rather than adding a one-off test suite.
  - **Constraints:** Touch only `Makefile` and `clients/vscode/test/e2e/run.mjs`; do not install tools, launch an implicit display, weaken native evidence assertions, or alter declared client capabilities.
  - Verified (Part 1/3): `make vscode-prereqs` at `e42301c` with controlled PATH/symlink/sh-stub fixtures: `DISPLAY=:0` plus codium-only PATH (no `code`/`Xvfb`) → exit 0; `code`-only PATH → exit 0; executable explicit `MAGIT_VSCODE_BIN` sh stub → exit 0; unset DISPLAY → `MISSING: DISPLAY(set it or run under Xvfb)`; empty editor PATH → `MISSING: codium-or-code(set MAGIT_VSCODE_BIN in local.mk)`; mode-644 explicit editor → `MISSING: MAGIT_VSCODE_BIN=...(not executable; fix local.mk)`. Runner xdotool requirement and `vscode-e2e` checks remain Parts 2/3. Done: `e42301c` (Part 1/3; no tick).
  - Verified (Part 2/3, 2026-11-19, base `8a190d6`): `run.mjs:69-72` now requires an absolute resolved `xdotool` that passes `accessSync(..., X_OK)` instead of string truthiness, preserving `resolveCommand`'s bare-name fallback (`clients/vscode/lib/version.js:33`). `make vscode-e2e` under `xvfb-run` with a PATH whose only editor is a `codium` shim and which contains no `xdotool` → `FAIL e2e: xdotool is required for the A06 physical refresh check; install it or add it to PATH`, exiting before any editor launch (`ok: build`, `ok: vscode-deps`). Normal equipped `make vscode-e2e` → `ok: vscode-e2e` exit 0, with `V=1` printing `PASS native A06 root quit closes menu/tabs and reopens status` and structured `/tmp/vscode-review-XWDPbz/a06-evidence.json` (`affectedTabsAfterQuit: 0`, `preservedRoot: "v2"`, `reopenedRoot: "v3"`, `quickPickHiddenAfterQuit: true`); `ps -ef` showed no surviving `magit-lsp`, `codium` or `Xvfb` process. `make vscode-test` → `ok: build`, `ok: vscode-deps`, `ok: vscode-typecheck`, `ok: vscode`, `ok: keybindings` (exit 0). Linux only; Part 3/3 remains.

- [x] **[P1] Reap every Plain preset process on failure** (`scripts/test_plain_presets.py:25-57`, `teardown`; `tests/ui/test_driver.py:650-665`, `test_plain_preset_teardown_reaps_owned_descendant`)
  - **Problem:** The preset harness owns only the direct Neovim child. Its `finally` block terminates and waits for Neovim, but on timeout, assertion failure, or forced editor termination it neither owns nor verifies the descendant `magit-lsp` process, so a failed native run can return with an orphan server.
  - **Why it matters:** Correctness and cleanliness — native verification must leave no owned server behind on precisely the failure paths where Neovim does not perform its graceful LSP shutdown.
  - **Change:** Launch each preset editor in its own owned process group/session and centralize bounded teardown that terminates the whole group, escalates to kill only after the deadline, waits/reaps the direct child, and verifies the group is empty while preserving the original test failure and useful stderr. Add a controlled `ui-unit` regression using a child that spawns a descendant: force the failure cleanup path and prove both owned processes are gone; also retain the healthy graceful-shutdown path. Use readiness files or process state with deadlines, not blind sleeps.
  - **Done when:** Success still proves graceful Neovim/LSP shutdown for all five presets; forced failure cannot leave either the editor or descendant alive; cleanup errors do not mask the primary assertion; `make ui-unit` and `make plain-presets` pass with no orphan `magit-lsp` process.
  - **Constraints:** Do not scan or signal by process name, signal unrelated processes, add tmux to this preset harness, weaken native assertions, use unbounded waits, or claim Windows/macOS verification.
  - Verified (historical): `make ui-unit` and `make plain-presets` exited 0; owned child/descendant cleanup and primary-exception preservation were checked; all five presets retained graceful-shutdown assertions. Linux only. The tag-history oracle was corrected to the fixture's `tag-only` and `v1` tags, not weakened.
  - Done: `64767e0`, `0065277` (coverage correction).
  - **Reopened at `cbe63f5` (source review):** The cleanup implementation still has an incomplete wait boundary. It never polls/reaps the direct child during the TERM grace period, so a successfully terminated but unreaped child keeps `killpg(pgid, 0)` succeeding until the full deadline. After SIGKILL it waits only for the direct child and immediately probes the group once; a descendant that has not yet exited or been reaped causes a premature cleanup failure. The existing real-process regression uses TERM-responsive processes and does not directly cover escalation.
  - **Problem:** `teardown` confuses an unreaped direct child with a still-running group during graceful shutdown and treats signal delivery as completed descendant termination after escalation. This adds an unconditional grace-period delay on failure and can report surviving processes before asynchronous SIGKILL cleanup has completed.
  - **Why it matters:** Correctness — bounded cleanup must observe completion, not merely deliver signals; otherwise the new failure-path guarantee and its diagnostics are timing-dependent.
  - **Change:** In `teardown`, reap/poll the direct child while observing group disappearance during the TERM grace period. If escalation is required, tolerate the group disappearing between the probe and SIGKILL, then use a separate bounded completion wait for both direct-child reaping and group disappearance before reporting survivors. Extend the existing cleanup tests with controlled process-state transitions proving delayed post-KILL disappearance is awaited, plus a TERM-ignoring owned descendant to exercise escalation. Put fixture cleanup in `finally` so a readiness/assertion failure cannot leak the test's own processes.
  - **Done when:** TERM-responsive cleanup returns without exhausting the grace period solely because the direct child is a zombie; escalation waits for observable group disappearance and reports a genuine timeout only after the completion deadline; primary exceptions remain preserved; `make ui-unit` and `make plain-presets` pass.
  - **Constraints:** Keep this follow-up within `scripts/test_plain_presets.py` and `tests/ui/test_driver.py`. Do not replace group ownership with name-based signaling, remove survivor checks, use fixed sleeps as proof of completion, or claim non-Linux verification. Earlier Done/Verified entries above are historical evidence, not completion of this reopened boundary.
  - Verified (reopened follow-up): `make ui-unit` → exit 0 (`ok: ui-unit`; 273 tests). Covers reaping the TERM-responsive direct child during grace (no full-deadline zombie wait), delayed post-KILL group disappearance after a probe/SIGKILL race, genuine timeout only after the completion deadline, and real SIGKILL escalation for a TERM-ignoring descendant with fixtures cleaned in `finally`. Linux only; native preset verification remains Part 2.

  - Done: `b940fc8` (Part 1/2, teardown completion regression coverage; native verification pending).
  - Verified (Part 2/2 native): `make plain-presets` → exit 0; all five presets (`minimal`, `showDocument`, `completion`, `progress`, `full`) passed with graceful editor/LSP shutdown assertions. Process-state evidence: a `/proc` scan by executable/`comm` identity found no surviving `magit-lsp` process and no preset-harness leftovers after the run. Linux only.

- [x] **[P1] Correct the audit's single-outstanding-render claim** (`docs/design/rich-protocol-audit.md:114-138,349-356`, item 4)
  - Done: `bee06ba`.
  - **Problem:** VERIFIED at `f15c026`: item 4 equates one retained acknowledgement slot with at most one outstanding delivered version. `OpenView::render_tree_with_delivery` (`src/session.rs:418-460`) advances and returns a render without waiting for an ack; if the previous version is unacked it sends full text, rather than preventing another delivery. `Session::ack` (`:991-1003`) accepts only the current version.
  - **Why it matters:** Correctness — the proposed documentation-only rule contradicts the implementation and could cause T113 to adopt nonexistent flow control.
  - **Change:** Rewrite item 4 and its summary/classification to distinguish retained snapshots from in-flight notifications. Describe successive unacked renders, the full-text fallback, and ignored stale acks. Include a short sequence (ack N, deliver N+1, deliver N+2 before ack) grounded in the baseline. If single-outstanding delivery is proposed, label it a behavior change requiring implementation, not an existing rule to document.
  - **Done when:** No audit section claims that a single ack slot bounds outstanding deliveries; status, risk, effort and wire/behavior classification agree with the baseline sequence.
  - **Constraints:** Audit correction only in `docs/design/rich-protocol-audit.md`; use `f15c026` blobs, not concurrent source edits. Do not change code, run tests/builds, decide T113 policy, or tick T112.
  - Verified: `git diff --check -- docs/design/rich-protocol-audit.md TASKS.md` → exit 0; item 4 and the status table/classification now distinguish the one retained ack snapshot from several in-flight renders, with the baseline ack N → deliver N+1 → deliver N+2 → stale/full-text sequence; single-outstanding delivery labeled a behavior change, not a documented rule.

- [x] **[P1] Audit standard positional reads separately from versioned commands** (`docs/design/rich-protocol-audit.md:235-254`, item 9)
  - Done: `205029e`.
  - **Problem:** Item 9 substitutes versioned command dispatch for the stated last-acked-tree concern. Its cited `Session::dispatch` branch (`f15c026:src/session.rs:1117-1126`) runs only for `action_target: Some`, whereas rich invoke supplies `None` (`:1069-1080`); standard LSP reads instead use `tree_for_positional_request` and `PublishedViews`.
  - **Why it matters:** Correctness and acceptance substitution — the audit omits the required standard-request behavior and conflates it with a different version-selection policy.
  - **Change:** Document the standard read path using `f15c026:src/server.rs:804-815,1534-1536`, its callers in `src/server/requests.rs`, and `src/session.rs:97-125,172-179`: last acknowledged tree, current fallback before any ack, no positional tree in text mode. Separately describe rich command resolution from `src/session/commands.rs:17-90`: exact retained version, current acceptance not limited to the first-ack window, and the acknowledgement requirement for commit-selection commands (including the validated-offer exception). Correct the proposal and cite the maintained specification gap rather than treating all positional operations alike.
  - **Done when:** Item 9 directly covers standard LSP reads and distinguishes them from invoke/key commands, with baseline evidence and accurate exceptions; the summary agrees.
  - **Constraints:** Change only the audit document; no source changes, tests/builds, new policy or T112 completion mark. Preserve the `f15c026` evidence baseline.
  - Verified: `git diff --check -- docs/design/rich-protocol-audit.md TASKS.md` → exit 0; item 9 now documents the standard LSP read path (published acked tree, current fallback before the first ack, `null` in text mode) separately from rich invoke/key resolution (exact acknowledged or exact current version, commit-selection/`select.toggle-mark` acknowledgement, validated-offer exception, stale-view refusal), and the summary row and W1 citation agree.

- [x] **[P1] Distinguish existing version rejection from missing negotiation in the audit** (`docs/design/rich-protocol-audit.md:140-158`, item 5)
  - Verified: `824cf8b` separates strict v1 rejection/readable initialize error from missing bidirectional handshake; summary and wire/risk/effort match. `git diff --check -- docs/design/rich-protocol-audit.md` passed.
  - Done: `824cf8b`.
  - **Problem:** The unqualified finding that no protocol version check or readable mismatch error exists contradicts `f15c026:src/presentation/mod.rs:163-171,227-229`: `deserialize_v1` explicitly rejects other versions with `unsupported rich protocol version`. Initialization propagates that text (`src/server/requests.rs:186-187`) through `error_response` (`src/server.rs:1746-1760`); missing server protocol advertisement/minimum-version negotiation is a narrower real gap.
  - **Why it matters:** Correctness — T112 must revalidate the historical premise, not label an already enforced invariant absent.
  - **Change:** Narrow item 5's status, heading and summary to the missing bidirectional handshake. Record the existing strict v1 validation and readable RPC error, distinguish package-version warnings from protocol negotiation, and avoid claiming all mismatches are silent. Keep any proposed server advertisement/client minimum as future wire work with its own risk and effort.
  - **Done when:** The audit explicitly separates the implemented client-declaration check from the unimplemented handshake and no longer denies the existing error message.
  - **Constraints:** Audit document only, based on `f15c026`; do not implement negotiation, change source, run tests/builds or tick T112.

- [x] **[P1] Remove modifier downgrading as a replay fix in the audit** (`docs/design/rich-protocol-audit.md:181-202`, item 7)
  - **Problem:** Option (b) recommends `modifiers: basic` as an honest fix for VSCode's replay limitation, but modifier-reporting fidelity is not replay capability (`f15c026:src/protocol.rs:342-355`). `basic_step` still permits unshifted named keys and Ctrl/Alt chords (`src/keymap.rs:287-297`), and static clients do not receive dynamic bindings; filtering server lookup does not remove their installed shortcuts.
  - **Why it matters:** Correctness — this proposal cannot solve non-printable replay and can turn formerly handled static shortcuts into unbound/pass-through input without adding a replay implementation.
  - **Change:** Remove (b) as a solution, or explicitly mark it rejected with the above reasons. Retain bounded explicit replay support or documented limitations as options; distinguish input fidelity, static key capture and fallback replay. Reconcile the risk/effort discussion without deciding T113 policy.
  - **Done when:** Item 7 does not recommend reducing modifier fidelity to fix replay; its alternatives directly address or explicitly accept the actual limitation in `clients/vscode/lib/effects.js:7-35`.
  - **Constraints:** Change only `docs/design/rich-protocol-audit.md`; do not alter capabilities, static bindings or source, run tests/builds, or tick T112. Use baseline `f15c026`.
  - Verified: `git diff --check -- docs/design/rich-protocol-audit.md` → exit 0. Item 7 now separates modifier reporting fidelity (`f15c026:src/protocol.rs:342-355`), static key capture and fallback replay; `modifiers: 'basic'` is recorded as rejected (it admits unshifted named keys and Ctrl/Alt at `src/keymap.rs:287-297`, installs no replay and can only add pass-throughs), and the remaining options are bounded replay support in `effects.js:7-35` or a documented client limit.
  - Done: `49d41bc`.

- [x] **[P1] Make the T113 version handshake an unambiguous compatibility check** (`docs/design/rich-protocol-v1.md:96-123`, decision 3; `docs/PROTOCOL.md:32-39`, planned negotiation)
  - **Problem:** VERIFIED: the plan checks `minimumVersion` against the server's advertised version, but speaks the separately accepted `v`, without requiring that the session version satisfies the minimum. A future server advertising 2 and accepting `v=1, minimumVersion=2` passes both written checks while speaking version 1. The missing-advertisement fallback to 1 also silently accepts a peer that has not completed the planned handshake, despite the stated no-shims policy.
  - **Why it matters:** Correctness — T114 cannot implement a compatibility guarantee from checks that can accept a session below its required version; boundary behavior must not be guessed.
  - **Change:** In decision 3 and the protocol summary, define whether `protocolVersion` is the accepted session version or a server maximum, and explicitly constrain `minimumVersion`, `v`, and the accepted version. Prefer the current-client exact-version rule (server supports only 1; positive integer minimum must not exceed the selected `v`; no implicit downgrade or retry) rather than inventing multi-version support. Reject a missing/malformed server advertisement with a readable client error and no rich-view opening, instead of substituting 1. Include a compact outcome table for matching versions, unsupported `v`, minimum above selected/server version, invalid numeric values and missing advertisement. State how both clients surface an initialize rejection as well as a post-initialize validation failure; retain the distinction from the existing strict-v1 rejection.
  - **Done when:** The documented checks cannot accept a selected session version below the client's minimum; each mismatch has an explicit readable failure and no rich traffic; T114 needs no inferred range or legacy-fallback policy.
  - **Constraints:** Documentation only in these two files; do not implement negotiation, add compatibility shims, require multi-version support, run builds/tests, or tick T113.
  - Verified: `git diff --check -- docs/design/rich-protocol-v1.md docs/PROTOCOL.md TASKS.md` → exit 0. `protocolVersion` is the accepted session version (not a maximum); the acceptance rule is `minimumVersion <= v == protocolVersion` with `v` strictly `1`, so an accepted session cannot fall below the minimum. The outcome table covers match, unsupported/invalid `v`, minimum above the selected version, invalid `minimumVersion`, and missing/malformed/different server advertisement; every mismatch is a readable error with no session or rich traffic, and both clients' initialize-rejection and post-initialize paths are named. Decision 3 and PROTOCOL §4.1 state the same rule and reject the fallback to `1`; T113 stays unticked.
  - Done: `0dd1616`

- [x] **[P1] Include empty edit arrays in the documented render contract** (`docs/design/rich-protocol-v1.md:27,133,145`, core render forms and wart dispositions; `docs/PROTOCOL.md:51-55`, render forms)
  - **Problem:** VERIFIED: both new descriptions require “one or more” edits, excluding a currently emitted render. `OpenView::render_tree_with_delivery` (`src/session.rs:428-455`) advances the version and sends `Some(line_edits(...))`; equal lines return an empty vector (`src/diff_text.rs:13-16`). Thus an acknowledged unchanged refresh sends `edits: []` with a base and a new version, not a mode-only render.
  - **Why it matters:** Correctness — a client following this reference could reject a valid update or fail to advance/ack its version, confusing it with the no-text/no-edits form.
  - **Change:** Describe the incremental form as a present edit array containing zero or more non-overlapping edits. Explicitly distinguish `edits: []` (validate base, retain text, adopt and acknowledge the delivered version) from absent `text` and absent `edits` (mode/state-only, unchanged version). Reconcile the core table and finding 2/W3 descriptions with the protocol paragraph; preserve the current at-most-one emitted edit and bottom-up multi-edit rules.
  - **Done when:** An acked unchanged refresh is covered by the three-form contract and cannot be mistaken for a mode-only notification; no “one or more” restriction remains in the changed render descriptions.
  - **Constraints:** Documentation only in these two files; do not alter rendering, add tests, run builds/tests, or tick T113.
  - Verified: `git diff --check -- docs/design/rich-protocol-v1.md docs/PROTOCOL.md TASKS.md` → exit 0. PROTOCOL §4.2 and the design record's core table now describe a present `edits` array with zero or more non-overlapping edits; both state that `edits: []` on an acknowledged unchanged refresh carries a base and a new version, validates the base, retains text and adopts/acknowledges the delivered version, while absent `text` and absent `edits` remain mode/state-only with an unchanged version (finding 2 and W3 name the distinction; no "one or more" restriction remains). The at-most-one current emission and bottom-up multi-edit application are preserved verbatim.
  - Done: `270ee71`

- [x] **[P1] Align the fold-mirroring decision with the existing explicit preview workflow** (`docs/design/rich-protocol-v1.md:176-185`, decision 6; `docs/ARCHITECTURE.md:413-421`, VSCode fold mirroring)
  - **Problem:** VERIFIED: decision 6 says the command appends entries, while Architecture says both “with consent, appends” and “never ... rewrites the user's file.” The existing `mirrorFoldShortcuts` (`clients/vscode/lib/commands.js:173-206`) only opens a preview and offers clipboard copying; it does not append or refresh previously installed mirrors. Saying only the next mirror run reflects a changed binding omits the user's required installation/removal step.
  - **Why it matters:** Correctness and simplicity — the supposedly smallest current-client decision implies an unimplemented configuration writer and leaves stale shortcut handling unspecified.
  - **Change:** Choose and document the already implemented on-demand preview/copy workflow in both sections: user runs the command, reviews/copies scoped entries, and manually replaces obsolete mirrors in `keybindings.json` after changing native shortcuts. Remove the automatic append and activation-scan implications from this selected workflow; explicitly say the command never edits the file and rerunning alone does not install/update bindings. Keep QuickPick as the menu default and the reason that user bindings outrank extension defaults.
  - **Done when:** Both sections describe one implementable workflow consistent with `mirrorFoldShortcuts`, including how the user updates/removes an old mirror, without implying a writer or watcher exists or is required by T114.
  - **Constraints:** Documentation only in these two files; do not modify VSCode code, introduce a configuration-editing subsystem, run builds/tests, or tick T113.
  - Verified: `git diff --check -- docs/design/rich-protocol-v1.md docs/ARCHITECTURE.md TASKS.md` → exit 0. Both sections now describe the implemented `mirrorFoldShortcuts` (`clients/vscode/lib/commands.js:173-206`) workflow: the command reads `keybindings.json`, opens a preview document of `magit.invoke` entries scoped to `magit.active` and offers "Copy to clipboard"; the user pastes them into `keybindings.json` and, after changing a native fold shortcut, removes or replaces the obsolete mirrors there by hand. Both say the command never edits or writes the file, rerunning only shows the current mapping and installs/updates nothing, and no activation scan or watcher exists; QuickPick stays the menu default and the user-bindings-outrank-extension-defaults reason is kept (design §6, Architecture §3.9 VSCode steps 1–4).
  - Done: `a00aa6c`

### Documentation audit review (2026-11-18; base `bec27b3`)

- [x] **[P2] Describe the implemented command continuation model** (`docs/ARCHITECTURE.md:142-154`, `Commands and effects`; `src/commands.rs:100-101,275-302,368-372`, `RunFn`, `PromptAction`, `resume_prompt`)
  - **Problem:** The maintained architecture describes commands as async functions that await a UI handle in ordinary linear code. The implementation instead uses synchronous handlers returning `CommandOutcome`, typed prompt actions and session-owned continuation state; this is not merely different syntax for the documented extension point.
  - **Why it matters:** Readability and cohesion — contributors following the canonical design would introduce a second command execution model or misunderstand which layer owns a suspended interaction.
  - **Change:** Update only the command execution explanation and illustrative signature in Architecture §3.5 to describe `RunFn`, `CommandOutcome`, `PromptAction`, and the session's prepare/resume/driver boundary. Use `src/commands.rs:1-5,100-101,250-302,368-372` and `src/session.rs:1357-1376,1421-1431,1509` as evidence. Keep `ScriptedUi` described as the test-side answer provider rather than a UI awaited inside every handler; retain the existing effects table and wire contract.
  - **Done when:** The maintained section distinguishes synchronous preparation from waiting/delivery and continuation resumption, contains no claim that registry handlers are async, and `make docs-check` passes when documentation verification is authorized.
  - **Constraints:** Documentation only; do not redesign handlers, add an abstraction, change the protocol or claim runtime verification. Preserve all queue records and statuses, T022's hold, the Phase 6A prohibition and explicit human release approval.
  - Verified: `make docs-check` → exit 0 (`ok: docs-check`); §3.5 now names synchronous preparation, session-owned waiting/delivery and typed resumption.
  - Done: `58f27d4`.

### P3

No open P3 item. Historical P3 completion is in the ledger.

## Ready independent work

Readiness means no known missing Make entry point, not completion or permission to execute during this documentation-only session. Phase 7 does not gate 0.2. T024's spike decision still gates its dependents.

### Feature — Phase 7: Test stabilization (does not gate 0.2)

- [ ] **T024 — host-spike.** [Scope](./TASKS_PART_SEVEN.md#t024-host-spike-s).
  - The obsolete missing-Make-target blocker is removed: `make vscode-e2e` exists, with native A06 evidence in the completed E2E split. That evidence does not satisfy the ten-run spike or choose the host. T025/T026 depend on this decision; T027/T028 retain their native VSCode verification requirements.

## Blocked prerequisites

### Deferred hygiene follow-ups

Historical inventories are inputs to revalidation, not deletion instructions. The roadmap's historical order/header freeze is superseded only by this user-authorized documentation compaction/reordering; no other restriction is waived.

- [x] **Provision reproducible Knip checker** — check: `make hygiene` executes `knip`. Resolution order: plain `knip` from `PATH` when available, else the workspace-local `clients/vscode/node_modules/.bin/knip`, else the existing missing-tool report; never a `DISABLED` status line. Findings remain report-only and belong to the existing audit items below.
  - Provisioning: `knip` is provided on `PATH` by the container/dev environment (user statement 2026-11-19), currently resolving to the workspace-local `clients/vscode/node_modules/.bin/knip`, which the user is repairing; `scripts/hygiene.py` keeps that path only as the `PATH` fallback (`f029a4c`). Historically the workspace-local binary launched but could not run because its `oxc-parser` native binding was unavailable; no repo evidence is expected.
  - Verified 2026-11-18 (`f029a4c`): `make hygiene` → `knip: NONZERO (exit 1)` with and without the workspace `.bin` on `PATH`; this checkout's Knip exits before analysis because the `oxc-parser` native binding is unavailable. An earlier `PATH` run reported real findings (1 unused file, 1 unused devDependency, 1 unlisted dependency, 74 unused exports). No clean-hygiene result is claimed.
  - Verified 2026-11-19 (user repaired the workspace-local Knip): `make hygiene` → `knip: NONZERO (exit 1)` now completing full analysis — findings: 1 unused file (`test/types.js`), 1 unused dependency (`@oxc-parser/binding-linux-x64-gnu`), 1 unused devDependency (`prettier`), 1 unlisted dependency (`vscode-jsonrpc/node`), 74 unused exports; each owned by the audit items below. No clean-hygiene result is claimed.
  - Done: `make hygiene` invokes Knip reproducibly from `PATH`.
- [x] **Update the stale Knip assertion in the hygiene unit tests** — `tests/hygiene/test_hygiene.py` asserted the then-current plain-`PATH` `knip` invocation (command `knip`, working directory `clients/vscode`, no workspace-local `node_modules` binary path) through its existing subprocess stub instead of the removed `knip: DISABLED` output; `make hygiene-test` ran the suite.
  - Verified: `make hygiene-test` → exit 0 (`ok: hygiene-test`); before the fix the same target failed on the stale `knip: DISABLED` assertions.
  - Superseded (`f029a4c`): by user decision `tests/hygiene/test_hygiene.py` and the `hygiene-test` make target were removed; unit-testing the hygiene report process itself was judged waste of code and complexity. The `make hygiene` check above is the current enforcement.
  - Done: `3e893b6`.
- [x] **Resolve the vscode-jsonrpc/node direct dependency** — `clients/vscode/test/e2e/real-file-native.js:391`; declare it in the owning package and verify with pinned Knip plus VSCode E2E dependency install.
  - Change: `vscode-jsonrpc: ^8.2.0` declared in `clients/vscode/package.json` `devDependencies` (the consumer is a test file; the range matches the `vscode-jsonrpc@8.2.0` entry already resolved in `clients/vscode/pnpm-lock.yaml`, so no new package enters the lockfile).
  - Change: new `vscode-deps-refresh` Make target (registered in `.PHONY`) — the `vscode-deps` recipe with `--no-frozen-lockfile` and the same `PNPM_STORE_PATH`/`XDG_CACHE_HOME` store-dir convention, so a dependency declaration can regenerate `clients/vscode/pnpm-lock.yaml` without hand-editing it.
  - Verified: `CI=true make vscode-deps-refresh` → exit 0 (`ok: vscode-deps-refresh`); before the target, `--frozen-lockfile` refused with `ERR_PNPM_OUTDATED_LOCKFILE` ("1 dependencies were added: vscode-jsonrpc@^8.2.0"). The lockfile diff adds the `vscode-jsonrpc` importer specifier and, as generated metadata from the local pnpm 10.34.5, `libc:` fields on the existing `@oxc-parser` package entries.
  - Verified: `CI=true make vscode-test` → exit 0 (`ok: build`, `ok: vscode-deps`, `ok: vscode-typecheck`, `ok: vscode`, `ok: keybindings`).
  - Verified: `make hygiene` → exit 0, `knip: NONZERO (exit 1)`; the unlisted-dependency finding `vscode-jsonrpc/node` is gone (`rg -c vscode-jsonrpc` on the report → 0 matches; the report's only remaining findings are `Unused files (1) test/types.js` and `Unused exports (58)`, both owned by their sibling audit items). `cargo machete: CLEAN (exit 0)`, `luacheck: CLEAN (exit 0)`.
  - Done: `74eef84`.
- [x] **Audit unused VSCode exports: commands and context** — `clients/vscode/lib/commands.js` (`openStatus`, `viewport`, `pressCommand`, `invokeCommand`, `mirrorFoldShortcuts`, `keybindingsPath`); `context.js` (`deriveContext`, `inactiveContext`, `staticScope`). Verify focused VSCode tests and Knip.
  - Dispositions: retained with real consumers — `openStatus` (`clients/vscode/lib/session.js:100`), `pressCommand` (`session.js:101`), `invokeCommand` (`session.js:104`), `mirrorFoldShortcuts` (`session.js:112`), `deriveContext` (`clients/vscode/lib/views.js:172`; also `test/test.js:128`), `inactiveContext` (`views.js:173`; also `test/test.js:167`); removed from the export surface and kept module-local with in-module consumers — `viewport` (`commands.js:75`), `keybindingsPath` (`commands.js:174`), `staticScope` (`context.js:33`).
  - Verified: `CI=true make vscode-test` → exit 0 (`ok: build`, `ok: vscode-deps`, `ok: vscode-typecheck`, `ok: vscode`, `ok: keybindings`).
  - Verified: `make hygiene` → `knip: NONZERO (exit 1)`; unused exports fell 74→71 and `viewport`, `keybindingsPath`, `staticScope` no longer appear. The six retained exports remain listed because Knip's `knip.json` entry graph (`test/e2e/*`) never reaches `extension.js:3` → `lib/session.js`, so their `commands.*`/`contextKeys.*` consumers are untraced; other findings are unchanged and owned by the sibling audit items. No clean-hygiene result is claimed.
  - Verified: `rg -n "viewport|keybindingsPath|staticScope" clients/vscode` → no `commands.viewport`, `commands.keybindingsPath` or `staticScope` consumer remains; surviving hits are the module-local definitions, the unrelated protocol `viewport` descriptor field and the `magit-lsp.keybindingsPath` setting name (`clients/vscode/package.json:2069`, `README.md:46`), not exports.
  - Done: `dbb3991`.
- [x] **Audit unused VSCode exports: draft and edit lifecycle** — `clients/vscode/lib/drafts.js` (`cancel`, `close`, `confirm`, `disconnect`, `handleInvoke`, `onChanged`, `onClosed`, `save`, `setContext`, `setup`); `edit.js` (`KIND`, `SCHEME`, `draftAction`, `isEditEffect`, `languageForKind`); `edits.js` (`onDocumentClosed`, `onActiveEditorChanged`, `confirmEditCommand`, `cancelEditCommand`, `currentEdit`). Verify edit lifecycle tests and Knip.
  - Dispositions: retained with real consumers — `cancel` (`clients/vscode/lib/edits.js:53`), `close` (`lib/session.js:110`), `confirm` (`lib/edits.js:47`), `disconnect` (`lib/session.js:130`, `lib/session.js:158`), `handleInvoke` (`lib/session.js:103`), `onChanged` (`lib/session.js:114`), `onClosed` (`lib/edits.js:11`), `setContext` (`lib/edits.js:32`, `lib/edits.js:35`), `setup` (`lib/session.js:118`); (`edit.js`) `SCHEME` (`lib/drafts.js:84`), `draftAction` (`lib/drafts.js:692`), `isEditEffect` (`lib/drafts.js:595`), `languageForKind` (`lib/drafts.js:647`); (`edits.js`) `onDocumentClosed` (`lib/session.js:113`), `onActiveEditorChanged` (`lib/session.js:115`), `confirmEditCommand` (`lib/commands.js:146`; also `lib/session.js:106`), `cancelEditCommand` (`lib/commands.js:147`; also `lib/session.js:107`), `currentEdit` (`lib/commands.js:144`). Removed from the export surface — `save` (`drafts.js`: no consumer in `clients/vscode`; its only helper `checkpoint` was orphaned and deleted with it), `KIND` (`edit.js`: module-local constant with no external consumer). No test-only consumers remained.
  - Verified: `CI=true make vscode-test` → exit 0 (`ok: build`, `ok: vscode-deps`, `ok: vscode-typecheck`, `ok: vscode`, `ok: keybindings`).
  - Verified: `make hygiene` → `knip: NONZERO (exit 1)`; unused exports fell 71→69 and `save`/`KIND` no longer appear. Retained exports remain listed because `knip.json`'s entry graph never reaches `extension.js` → `lib/session.js`, so their `session.js`/`edits.js`/`commands.js` consumers are untraced; other findings are unchanged and owned by the sibling audit items. No clean-hygiene result is claimed.
  - Verified: `rg -n "drafts\.save|edit\.KIND|\bcheckpoint\(" clients/vscode` → no `drafts.save` or `edit.KIND` reference remains; the `checkpoint(` hits are the unrelated E2E capture helper in `test/e2e/*.js`.
  - Done: `32106f1`.
- [x] **Audit unused VSCode exports: effects, fold keys and menus** — `clients/vscode/lib/effects.js` (`showMessage`, `replay`); `foldkeys.js` (`parseJsonc`, `mapFoldCommand`, `mapFoldKeys`); `menu.js` (`quickPickItems`, `quickPickItem`, `describeItem`, `routeTyped`, `charToKey`); `menus.js` (`showMenu`, `menuItems`, `hideMenu`, `queueSteps`). Verify menu/fold/effect tests and Knip.
  - Dispositions: removed from the export surface and kept module-local with in-module consumers — (`effects.js`) `showMessage` (`effects.js:69`), `replay` (`effects.js:76`); (`foldkeys.js`) `mapFoldCommand` (`foldkeys.js:68`); (`menu.js`) `describeItem` (`menu.js:65`), `charToKey` (`menu.js:129`); (`menus.js`) `menuItems` (`menus.js:45`, `menus.js:91`). Retained with real consumers — `mapFoldKeys` (`lib/commands.js:185`; also `test/e2e/run.mjs:77`), `quickPickItems` (`lib/menus.js:101`), `routeTyped` (`lib/menus.js:53`), `showMenu` (`lib/views.js:106`, `lib/views.js:256`), `hideMenu` (`lib/views.js:191`, `lib/views.js:231`), `queueSteps` (`lib/commands.js:120`). Retained as test-support with recorded reason — `parseJsonc` (`test/test.js:440,446,448` assert JSONC comment/trailing-comma tolerance and the reported error line; the production consumer of the same function is `foldkeys.js:58`, so exporting it only adds a test seam), `quickPickItem` (`test/types.js:84` is a compile-time fixture asserting `activationSteps` is required on every serialized item; the fixture is a declared `knip.json` entry for the `typecheck` gate, yet the namespace member access stays uncredited by the CJS limitation; the production consumer is `menu.js:94`).
  - Verified: `CI=true make vscode-test` → exit 0 (`ok: build`, `ok: vscode-deps`, `ok: vscode-typecheck`, `ok: vscode`, `ok: keybindings`).
  - Verified: `make hygiene` → `knip: NONZERO (exit 1)`; unused exports fell 69→63 and `showMessage`, `replay`, `mapFoldCommand`, `describeItem`, `charToKey`, `menuItems` no longer appear. The retained exports that remain listed (`parseJsonc`, `quickPickItems`, `quickPickItem`, `routeTyped`, `showMenu`, `hideMenu`, `queueSteps`) stay because `knip.json`'s entry graph never reaches `extension.js` → `lib/session.js`, so their `menus.js`/`views.js`/`commands.js` consumers are untraced, and `parseJsonc`/`quickPickItem` are reached only from tests; other findings are unchanged and owned by the sibling audit items. No clean-hygiene result is claimed.
  - Verified: `rg -n "effects\.(showMessage|replay)|foldkeys\.mapFoldCommand|menu\.(describeItem|charToKey)|menus\.menuItems" clients/vscode` → no consumer reference remains; surviving hits are the module-local definitions and their in-module calls in `lib/effects.js`, `lib/foldkeys.js` and `lib/menu.js`.
  - Done: `476dc86`.
- [x] **Audit unused VSCode exports: prompt, render and requests** — `clients/vscode/lib/prompt.js` (`CUSTOM_ID`, `promptItems`, `answerFor`, `customChoice`); `render.js` (`applyRender`, `applyEdit`); `requests.js` (`press`, `invoke`, `applyRender`, `handlePrompt`, `resync`). Verify prompt/render/request tests and Knip.
  - Dispositions: retained with real consumers — (`prompt.js`) `promptItems` (`lib/requests.js:208`, `lib/requests.js:227`), `answerFor` (`lib/requests.js:204`; also `test/test.js:493`), `customChoice` (`lib/requests.js:222`; also `test/test.js:499`); (`render.js`) `applyRender` (`lib/requests.js:109`; also `test/test.js:174`); (`requests.js`) `press` (`lib/menus.js:149`, `lib/commands.js:125`), `invoke` (`lib/commands.js:157`; also `test/e2e/quit-reopen.js:34`, `test/e2e/real-file-native.js:320`), `applyRender` (`lib/session.js:124`, `lib/views.js:90`), `handlePrompt` (`lib/session.js:125`). Removed from the export surface and kept module-local with in-module consumers — `CUSTOM_ID` (`prompt.js:38`, `prompt.js:65`), `applyEdit` (`render.js:53`), `resync` (`requests.js:111`). No test-only external consumers among the audited names.
  - Verified: `CI=true make vscode-test` → exit 0 (`ok: build`, `ok: vscode-deps`, `ok: vscode-typecheck`, `ok: vscode`, `ok: keybindings`).
  - Verified: `make hygiene` → `knip: NONZERO (exit 1)`; unused exports fell 63→60 and `CUSTOM_ID`, `applyEdit` and `resync` no longer appear. The retained audited exports stay listed because `knip.json`'s entry graph never reaches `extension.js` → `lib/session.js`, so their `requests.js`/`commands.js`/`menus.js`/`views.js` consumers are untraced, and `promptItems`/`answerFor`/`customChoice`/`render.applyRender` are reached only from tests; other findings are unchanged and owned by the sibling audit items. No clean-hygiene result is claimed.
  - Verified: `rg -n "CUSTOM_ID|render\.applyEdit|requests\.resync" clients/vscode` → no dangling consumer reference remains; surviving hits are the module-local `CUSTOM_ID` definition and its two in-module uses in `lib/prompt.js`.
  - Done: `31e58a0`.
- [x] **Audit unused VSCode exports: semantic, session and views** — `clients/vscode/lib/semantic.js` (`register`); `session.js` (`activate`, `deactivate`, `createClient`); `views.js` (`stateForUri`, `activeState`, `isMenuKind`, `ensureView`, `showView`, `showDocument`, `applyCursor`, `activateView`, `closeView`, `closeRoot`, `focusView`). Verify VSCode integration tests and Knip.
  - Dispositions: retained with real consumers — `register` (`lib/session.js:139`), `activate` (`extension.js:5`), `deactivate` (`extension.js:5`), `stateForUri` (`lib/edits.js:12`, `lib/edits.js:40`, `lib/session.js:39`, `lib/session.js:93`; also `test/e2e/quit-reopen.js:19`), `activeState` (`lib/commands.js:114`, `lib/commands.js:150`; also `test/e2e/quit-reopen.js:76`), `isMenuKind` (`lib/commands.js:119`, `lib/requests.js:136`), `ensureView` (`lib/commands.js:77`, `lib/effects.js:80`; also `test/e2e/quit-reopen.js:15`, `test/e2e/real-file-native.js:314`, `test/e2e/suite.js:666`), `showView` (`lib/commands.js:80`, `lib/effects.js:83`; also `test/e2e/quit-reopen.js:18`, `test/e2e/real-file-native.js:317`), `applyCursor` (`lib/requests.js:143`), `activateView` (`lib/edits.js:31`, `lib/edits.js:37`, `lib/edits.js:41`, `lib/menus.js:35`, `lib/menus.js:78`, `lib/requests.js:140`, `lib/drafts.js:478`, `lib/session.js:150`), `closeView` (`lib/edits.js:15`, `lib/effects.js:86`), `closeRoot` (`lib/effects.js:92`), `focusView` (`lib/commands.js:68`, `lib/effects.js:89`, `lib/drafts.js:476`; also `test/e2e/quit-reopen.js:20`, `test/e2e/quit-reopen.js:26`). Removed from the export surface and kept module-local with in-module consumers — `createClient` (`session.js:120`), `showDocument` (`views.js:109`, `views.js:260`). No test-only external consumers among the audited names.
  - Verified: `CI=true make vscode-test` → exit 0 (`ok: build`, `ok: vscode-deps`, `ok: vscode-typecheck`, `ok: vscode`, `ok: keybindings`).
  - Verified: `make hygiene` → `knip: NONZERO (exit 1)`; unused exports fell 60→58 and `createClient`/`showDocument` no longer appear. The retained audited exports stay listed because `knip.json`'s entry graph never reaches `extension.js` → `lib/session.js`, so their `views.js`/`session.js` consumers and the `extension.js` activation entrypoints are untraced; other findings are unchanged and owned by the sibling audit items. No clean-hygiene result is claimed.
  - Verified: `rg -n "views\.showDocument|session\.createClient|createClient|showDocument" clients/vscode` → no dangling export reference remains; surviving hits are the module-local definition/calls (`session.js:26,120`, `views.js:109,117,260`) and the unrelated LSP `window/showDocument` middleware in `test/e2e/real-file-spike.js`.
  - Done: `44e0916`.
- [x] **Audit the unused-file finding: test/types.js** — Knip reports `clients/vscode/test/types.js` as an unused file. Verify no real consumers (imports/requires/JSDoc type references across source and tests); delete it with any orphaned references, or record the deliberate-keep reason under this item. Verify focused VSCode tests and Knip.
  - Role (A1): `test/types.js` is a compile-time fixture, not a runtime test. `clients/vscode/jsconfig.json:14` includes it, `clients/vscode/package.json:26` defines `"typecheck": "tsc -p jsconfig.json"`, and `Makefile:131` runs `pnpm --dir clients/vscode run typecheck` as the first step of `vscode-test`; the file's own header (`test/types.js:12-13`) records that no runtime suite requires it. Verified no require/import path exists: `rg -n "require\(['\"].*types|from ['\"].*types|import\(['\"].*types" clients/vscode --glob '!node_modules'` returns no `test/types` hit, and `rg -n "test/types|types\.js" clients/vscode --glob '!node_modules'` finds only `jsconfig.json:14`.
  - Disposition (A2): kept — deleting it would drop the wire-declaration compile-time guards that `vscode-typecheck` enforces. `clients/vscode/knip.json` now lists `test/types.js` in `entry` (the honest mechanism, mirroring how `test/open.test.js` is kept): tsc consumes it as a root and knip does not auto-detect it, so no redundant-pattern hint appears. `lib/menu.js` is untouched; its `quickPickItem` export remains test-support with the reason recorded in the menu audit above.
  - Verified (A4): `CI=true make hygiene` → `knip: NONZERO (exit 1)`; the `Unused files` section is gone (`rg -c "types.js"` on the report → 0), no `Configuration hints`, remaining findings are the 58 limitation-backed unused exports; `cargo machete: CLEAN (exit 0)`, `luacheck: CLEAN (exit 0)`.
  - Verified (A3): `CI=true make vscode-test` → exit 0 (`ok: vscode-typecheck`, `ok: vscode`, `ok: keybindings`).
  - Done: `620bff7`.
- [x] **Make Knip reflect verified usage and dispose deliberate findings** — after the five export audits, the retained exports stay flagged even though each has a recorded consumer in this file: knip's CommonJS member-access usage tracing does not credit them (VERIFIED: knip auto-detects the real entries — configuring them yields "Remove redundant pattern" hints and changes nothing — so the entry-gap theory is falsified). Root-cause with a minimal scratch reproduction whether this is a knip 6.45.1 limitation or a config cause; fix our config if it is ours, otherwise record the limitation and leave the flags backed by their recorded consumer dispositions. Dispose deliberate keeps with evidence: `prettier` (invoked by `scripts/format.py:118-119` via `.githooks/pre-commit`) and `@oxc-parser/binding-linux-x64-gnu` (user's deliberate declaration for knip's native parser). Record the final honest findings list with an owner for each. Verify with Knip via `make hygiene`.
  - Root cause (A1): knip 6.45.1 limitation, not a config cause. Scratch reproduction in `clients/vscode/node_modules/.bin/knip` against a throwaway `/tmp` dir (own `package.json`, producer `module.exports = { foo, bar }`, consumer `const m = require('./mod'); m.foo()`) reports **both** `foo` and `bar` as unused exports; the control `const { foo } = require('./mod')` reports only `bar`. So knip credits destructured `require` but not namespace member access. Our code uses exactly the uncredited shape (`lib/session.js:8` `const commands = require('./commands')`, `:100` `commands.openStatus`; same for `lib/views.js:15`), so the files do not diverge from the plain pattern. The 58 flagged exports therefore stay and are explicitly backed by their recorded consumers in the five completed export audits above (lines 106–121); they are not mass-ignored.
  - Config final state (A2): the prior worker's redundant `entry` additions are reverted — `extension.js`, `test/test.js`, `test/integration.js` and `test/e2e/run.mjs` are auto-detected through package.json `main`/`scripts` and produced "Remove redundant pattern" hints. `test/open.test.js` is kept: knip does not auto-detect it and reports no hint for it. Schema is `knip@6`, matching the installed 6.45.1. `vscode` is removed from `ignoreDependencies` (knip hinted it and recognizes the module as host-provided through `engines.vscode`): `make hygiene` with it removed produced no new finding. `ignoreDependencies` now holds only the two deliberate keeps.
  - Dispositions (A3): `prettier` is deliberate — invoked by `scripts/format.py:118-119` (`clients/vscode/node_modules/.bin/prettier`), run from `.githooks/pre-commit` (`python3 scripts/format.py --staged --fix`). `@oxc-parser/binding-linux-x64-gnu` is deliberate — the user's declaration so the native parser binding knip needs is installed (platform-specific, unreachable by static import analysis).
  - Verified: `make hygiene` → `knip: NONZERO (exit 1)`; `Configuration hints` section is absent (no redundant patterns, no stale `ignoreDependencies`). Remaining findings, each with an owner: 58 unused exports (knip CJS limitation above; consumers recorded in the audit items above), 1 unused file `clients/vscode/test/types.js` (own item, line 123), 1 unlisted dependency `vscode-jsonrpc/node` at `test/e2e/real-file-native.js:391` (own item, line 92). `cargo machete` and `luacheck` unchanged.
  - Verified: `CI=true make vscode-test` → pass.
  - Done: `93a113c`.
- [x] **Waive size-only canonical roadmap obligation** — policy resolution, not implementation: readable prose/source size is maintainability guidance, not an absolute byte gate. No artificial splitting or content removal is required; preserve canonical design and obligations. The hygiene size report is soft, not a release gate.
  - Prior HALT (2026-11-17): orientation and stronger compaction attempts were PARTIAL, then failure-triage halted retries. Base `3125ee6`; 543 checked records; protected skeleton 67879 bytes before Done/contracts. Safe history diet 373253→370614; later measurement `b84baa2ee09839676b2c92598c157aadeac3ef2b` 371887 bytes. Then-classified candidates saved only 30523 bytes, not proof of feasibility. Full measurements/attempts remain in Base B519–522.
  - User now authorizes this bounded audit/compaction/reordering, superseding only first-three-lines/order freeze. Preserve identity/state/Done/corrections, unresolved acceptance/dependencies and anchors; no permanent archive or deletion of obligations to meet the cap. Current contracts remain in `docs/ARCHITECTURE.md`; earlier immutable sources `da4d422:TASKS.md`, `d9ea816:TASKS.md` remain provenance. This audit does not run `make hygiene` or close this broader hygiene item; all unwaived verification and the prior hold remain for any further work.
- [x] **Waive size-only authored transcript splitting** — policy resolution, not implementation: retain the complete `docs/client-exchanges.md` worked exchanges and their evidence; intact transcripts need not be split solely for size. No generated references or captures were hand-edited.
  - Verified: `make docs-check` → exit 0 (`ok: docs-check`); maintained policy and links pass documentation checks. No runtime/hygiene gate was run or claimed.
  - Done: `4fa191c` (policy record); follow-up scope reconciliation is recorded separately below.
  - Verified: follow-up `make docs-check` → exit 0; T113/T118 scope now aligns with pre-1.0 breaking freedom and requires an explicit future stability decision.

### Host-dependent stabilization

- [ ] **T025 — e2e-runner.** [Scope](./TASKS_PART_SEVEN.md#t025-e2e-runner-m). T024's host decision remains required; existing target alone does not prove hermetic/non-US-layout acceptance.
- [ ] **T026 — host-cases.** [Scope](./TASKS_PART_SEVEN.md#t026-host-cases-l). Keep coverage migration and host dependencies.
- [ ] **T027 — keybinding-compaction.** [Scope](./TASKS_PART_SEVEN.md#t027-keybinding-compaction-m). Native host/keybinding evidence remains required.
- [ ] **T028 — colour-roles.** [Scope](./TASKS_PART_SEVEN.md#t028-colour-roles-s). Native host verification remains required.
- [ ] **T029 — scenario-slimming.** [Scope](./TASKS_PART_SEVEN.md#t029-scenario-slimming-m).
  - Part 1/2 independently verified scenario matrix/artifact defaults through `make ui-unit`; parent stays unchecked. Part 2 retains shared conformance interpreters, fake/native comparisons and driver coverage matrix, including real-host requirements. Defer its integrated driver outcome until T024/T025 supply the host decision/hermetic dependency boundary; a Make target now exists, but existing seams do not establish full acceptance.
  - Part 1 evidence: `make ui-unit` → exit 0 (`ok: ui-unit`). `tests/ui/test_scenario.py` checks one default combination, four `matrix: true` pairs, explicit size/profile and size-only/profile-only compatibility, no `.screen`/`.ansi` on unrequested success, both for explicit snapshots and failures (including quiet XFAIL). Done: `790d2ce` (Part 1 only).

## 7. Numbered implementation queue

Detailed acceptance/dependencies: [scope specifications](./TASKS_PART_SEVEN.md#phase-map). Completed umbrellas/children retain separate ledger occurrences. A checkbox is not permission to bypass dependencies or owner approval.

### Feature — Phase 6: Plain acceptance

- [x] **T022 — plain-gate.** [Scope](./TASKS_PART_SEVEN.md#t022-plain-gate-s). Phase 6 sign-off reconciles current evidence and dispositions below; inherited `0c667d1` alone was not completion.
  - Current verification (2026-11-19): full `make test` passed at base `18ba3df` after `1cc349a` exposed ps; `make test-rust FILTER="--test plain_harness --test dispatch"`, `make ui-test`, `make ui-tour`, `make registry-docs-check` passed at `245316c`. Exact-state/non-Git proofs independently confirmed in `tests/plain_harness.rs:979-1172` and `tests/dispatch.rs:13-70`; all ten Unavailable rows have reasons. Final sign-off verification: `make registry-docs-generate` (worker must confirm no artifact drift before committing this tick).
  - Dispositions: hygiene `18ba3df` records all three tools CLEAN plus concrete soft-size/marker dispositions (not debt-free); prior native-binding failure superseded, remaining waivers/owners retained. Both review P1s fixed and verified: synchronized TERM fixture (`fd4fe5d`, `c850bd0`, ui-unit + all five plain-presets, no survivors); Make/xdotool preflight (`e42301c`, `fc34fe9`, `abd67c3`, controlled prerequisite failures, native A06 and vscode-test, no survivors). No P0/P2/P3. Rebase stall cause remains unproven; `5f70a34` adds effect-driven bounded wait/diagnostics without weaker oracles. Linux evidence only; T021 scoped Helix and other listed native risks remain, not universal support claims.
  - Done: containing sign-off commit (implementation/proof and review-fix commits above). T006/T013/T015/T019/T020/T021 are checked in the ledger. This authorizes only the specified Phase 6A dependency order, never publication or optional prototypes/stable freeze.
  - Documented HEAD `ec0bbbc`: serial final `make test` in clean `/home/opencode/.local/share/opencode/worktree/c43be0/t022-gate-verify` exited 0 (`build`, `rust`, `lua`, `vscode-deps`, `vscode-typecheck`, `vscode`, `keybindings` all OK). `make ui-test`, `make ui-tour`, `make registry-docs-check` passed. `make hygiene` exited 0 report-only: cargo-machete CLEAN, Luacheck CLEAN (0 warnings/errors, 31 files), Knip NONZERO (oxc-parser native binding unavailable); size output was a soft-limit report, not a release gate. This audit does not rerun or upgrade any of that evidence.
  - Earlier attempts: instant-rebase bounded-wait assertion failed (`build/test-logs/rust.e8LBtb`); rerun passed Rust then pnpm refused protected main-worktree `node_modules/` removal without TTY (`ERR_PNPM_ABORTED_REMOVE_MODULES_DIR_NO_TTY`, `build/test-logs/vscode-deps.9exi03`). Root cause of setup stop: a verification worktree shared the protected dependency directory. No permission to modify protected files or bypass setup was granted. At historical `0c667d1`, `make test` was incomplete (`exec82` wrapper missing `../node_modules/@pnpm/exe/pnpm`); later final pass does not turn those attempts into passes.
  - State proofs: `9ba3c0d` extends existing `tests/plain_harness.rs` for exact T015/T019 workflows and `tests/dispatch.rs` for real dispatch/refresh/ack/copy of a non-Git provider; no duplicate harness. Support rows: `docs/plain-support.md:52,55-56,62,67,89,254,256,258`. Native fixup `6066f46`; `plain-rebase-interactive.scn` passed in `make ui-test` at `0d70795`; T019 records native rebase proofs. At `ec0bbbc`, tour includes `menu-rebase`, `f Fixup`, `F Instant fixup`, and no `magit-lsp` process remained. No native defect is inferred from an obsolete tour note.
  - Review provenance: base `2001ec4`, range to then-HEAD: 81 files, +4950/-1000; full path inventory retained in Base B1024. Roadmap and transcript size-only obligations are waived by policy, not implemented splits. VSCode `suite.js` 97605 bytes was not an open size finding. Aggregate passing tests do not disposition findings or approve sign-off. T022 stays unchecked pending its acceptance evidence, finding dispositions and review.

### Feature — Phase 6A: Rich protocol v1 and client prototypes

T022 sign-off above opens this phase in the specification's internal dependency order. Release requires only its explicitly defined protocol subset, not every prototype or stable freeze. Pre-1.0 breaking protocol changes are allowed; no compatibility shim or additive-only promise applies merely because the wire is labeled `v=1`. Preserve negotiation/validation invariants. T113/T119 still own their designated protocol/minimum-Git decisions; no Phase 6A implementation or sign-off is inferred from opening the phase.

- [x] **T112 — rich-protocol-audit.** [Scope](./TASKS_PART_SEVEN.md#t112-rich-protocol-audit-s).
  - Audit: `docs/design/rich-protocol-audit.md` covers items 1–11 plus W1–W3. Each entry records status (real / not real / unknown), baseline evidence at `f15c026`, a proposal, wire-change classification, risk and effort; the status summary table agrees. No item is unknown, and no source file was changed. The audit is facts-only: T113 owns every decision (`Out of scope` in the audit doc).
  - Independent review: reviewer `ses_e0cb8e993ffeGg317aEX58yhzM` confirmed all 11+3 item fields and the no-code claim, and found four P1 corrections. All four were fixed in the audit and checked against the baseline: item 4 one retained ack snapshot vs. several in-flight deliveries (`bee06ba`); item 9 standard LSP reads vs. rich versioned commands (`205029e`); item 5 existing strict `v1` rejection vs. the missing bidirectional handshake (`824cf8b`); item 7 modifier reporting fidelity vs. replay capability (`49d41bc`, replacing the rejected `modifiers: 'basic'` proposal).
  - Verification scope (honest): each correction's queue record cites `git diff --check -- docs/design/rich-protocol-audit.md [TASKS.md]` → exit 0; the audit diff is whitespace-clean only. `make docs-check` passed earlier in this documentation session, but its `DOC_FILES` (`Makefile:37`) does not list `docs/design/rich-protocol-audit.md`, so that target did not check the audit and no audit content lint is claimed. No runtime test, build, UI run or new contract decision is claimed; source and tests were untouched.
  - Done: `d71b5ac` (audit), with review corrections `bee06ba`, `205029e`, `824cf8b`, `49d41bc`.
- [x] **T113 — rich-protocol-decisions.** [Scope](./TASKS_PART_SEVEN.md#t113-rich-protocol-decisions-s).
  - Decisions: `docs/design/rich-protocol-v1.md` §1 core/optional parts (frozen core; declared optional surfaces; reason: a small frozen core lets new clients attach safely), §2 evolution (pre-1.0 breaks without shims plus an explicit post-1.0 major-version rule; reason: one stable minimum to target), §3 version handshake (`minimumVersion <= v == protocolVersion`, `v` strictly `1`, readable outcomes; reason: rejects a session below the client's minimum without negotiation or fallback), §4 a fix/document/reject disposition with reason and wire classification for every T112 finding including W1–W3 (no false dead-field removal), §5 prompt as notification plus `view/promptRespond`, §6 QuickPick menu default and explicit preview/copy fold mirroring. Each decision has a reason.
  - Independent review: reviewer `ses_e0ca332b0ffel8nqNVV2aqgUKK` confirmed the six decisions, reasons and dispositions and found three P1 corrections; all three were fixed and diff-checked: handshake `0dd1616` (record `0a899a6`), empty `edits: []` render form `270ee71`, fold preview/copy workflow `a00aa6c` (record `5c994c5`).
  - Verification scope (honest): `make docs-check` exited 0 at `e673854`, but `DOC_FILES` (`Makefile:37`) omits `docs/design/rich-protocol-v1.md`, so it checked the edited reference files and the link only, not the new record's content; it was not rerun for the correction batch. Each correction cites `git diff --check -- <files>` → exit 0 (whitespace only). No runtime test, build, UI run or runtime verification of negotiation is claimed; the handshake is planned for T114 and adds no shim or multi-version support.
  - Verified: `git diff --check -- TASKS.md` → exit 0; each acceptance decision and audit disposition maps to a decision and reason in the record, and the review corrections are recorded above.
  - Done: `e673854` (decision record and reference corrections), with review corrections `0dd1616`, `0a899a6`, `270ee71`, `a00aa6c`, `5c994c5`.
- [ ] **T114 — rich-protocol-cleanup.** [Scope](./TASKS_PART_SEVEN.md#t114-rich-protocol-cleanup-m).
- [ ] **T105 — vscode-f5-cause.** [Scope](./TASKS_PART_SEVEN.md#t105-vscode-f5-cause-s); rescheduled here, not duplicated under Deferred.
- [ ] **T119 — min-git-version.** [Scope](./TASKS_PART_SEVEN.md#t119-min-git-version-s).
- [ ] **T115 — visual-studio-client.** [Scope](./TASKS_PART_SEVEN.md#t115-visual-studio-client-l).
- [ ] **T116 — zed-client.** [Scope](./TASKS_PART_SEVEN.md#t116-zed-client-l).
- [ ] **T117 — jetbrains-client.** [Scope](./TASKS_PART_SEVEN.md#t117-jetbrains-client-l).
- [ ] **T118 — rich-protocol-freeze.** [Scope](./TASKS_PART_SEVEN.md#t118-rich-protocol-freeze-s).

### Feature — Phase 8: Release

Release prerequisites include phases 1–6 and the specified Phase 6A protocol subset; preserve [the phase map](./TASKS_PART_SEVEN.md#phase-map). Human approval is mandatory; this documentation audit cannot grant it.

- [ ] **T030 — release-artifacts.** [Scope](./TASKS_PART_SEVEN.md#t030-release-artifacts-m).
- [ ] **T031 — client-install-docs.** [Scope](./TASKS_PART_SEVEN.md#t031-client-install-docs-s).
- [ ] **T032 — release-readme.** [Scope](./TASKS_PART_SEVEN.md#t032-release-readme-m).
- [ ] **T033 — release-go-no-go.** [Scope](./TASKS_PART_SEVEN.md#t033-release-go-no-go-m); explicit human approval remains required.

## Optional and unscheduled

### Feature — User-requested small features (2026-11-19; scheduled after Phase 6)

User-requested 2026-11-19; work once the Phase 6 push completes. The commits-sections ticket deliberately delivers only a display-first subset — the full interactive scopes remain T060 (status-history-sections) and T061 (unpulled-and-headers), and per queue precedent a delivered subset does not close those scopes.

- [ ] **Status view: unpushed and recent commits sections.** Magit-style server-rendered sections in the main status view: commits the remote lacks ("Unpushed to <upstream>") and a short recent-commits list; an "Unpulled from <upstream>" sibling may ride along if the same plumbing makes it trivial. New reads behind `src/git.rs`; rendering/structure in `src/status_view.rs` under the server-owned tree rules (no text re-parsing, no client recomposition, identity never by index); labeled fallbacks for no-upstream, detached and unborn repositories. Extend the nearest existing status-view tests/snapshots; update VIEWS.md §6; KEYMAPS.md §5 only if keys are added. Out of scope (stays in T060/T061): commit navigation into a revision view, lazy `More` groups, fold persistence, header lines.
- [ ] **Backspace in a submenu returns to the previous menu.** `?F<BS>` ends back at the `?` popup: opening a submenu pushes the parent onto a menu stack; `<BS>` in a submenu pops one level and reopens the parent in its prior state. First map every current `<BS>` binding and keep it — flag toggling/clearing in transient menus and multi-key command sequences retain their existing meaning; the pop applies only where `<BS>` is otherwise unbound in a submenu. Likely menu-stack state in `src/session.rs`, handling in `src/menu.rs`/`src/keyhandler.rs`. Menu state tests for push/pop plus regressions proving flag-`<BS>` and multi-key-`<BS>` unchanged; KEYMAPS.md §5 update.

### Feature — Phase 9: Plain keys (optional; starts after T022)

- [ ] **T034 — key-edit-shapes.** [Scope](./TASKS_PART_SEVEN.md#t034-key-edit-shapes-s-gate).
- [ ] **T035 — key-classifier.** [Scope](./TASKS_PART_SEVEN.md#t035-key-classifier-m).
- [ ] **T036 — key-policy-settings.** [Scope](./TASKS_PART_SEVEN.md#t036-key-policy-settings-m).
- [ ] **T037 — key-execution.** [Scope](./TASKS_PART_SEVEN.md#t037-key-execution-m).
- [ ] **T038 — key-safety-e2e.** [Scope](./TASKS_PART_SEVEN.md#t038-key-safety-e2e-m).
- [ ] **T039 — plain-keys-docs.** [Scope](./TASKS_PART_SEVEN.md#t039-plain-keys-docs-s).

### Feature — Phase 10: Small features

Post-release; scopes for phases 10–13 remain mandatory in [specifications](./TASKS_PART_SEVEN.md#phases-10-to-13-post-release-tickets), not fresh completion claims. T040/T041 are already in the ledger; their delivered subsets do not close other feature scopes.

- [ ] **T042 — remote-config-tags.**
- [ ] **T043 — branch-basics.**
- [ ] **T044 — notes-ignore.**
- [ ] **T045 — theming-docs.**
- [ ] **T046 — forking-docs.**
- [ ] **T047 — config-location-edit.**

### Feature — Phase 11: Read views

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
- [x] **T070 — branch-reset.** Delivered early as Early T070 (see that ad hoc entry).
- [ ] **T071 — annotated-tags.**
- [ ] **T072 — stash-views.**
- [ ] **T073 — stash-operations.**

### Feature — Phase 13: Advanced workflows

- [ ] **T074 — merge-conflicts.**
- [ ] **T075 — mergetool-effect.**
- [ ] **T076 — sequencer-state.**
- [ ] **T077 — cherry-view.**
- [ ] **T078 — rebase-options.**
- [ ] **T079 — rebase-merges.**
- [ ] **T080 — scripted-sequence-edit.**
  - Editing/synchronization review (2026-11-13): reuse the existing commit-message `EditRegistry`/`Draft` lifecycle for reword/squash message callbacks; do not infer a production native todo editor from `DraftKind::RebaseTodo`. VERIFIED at review base `1ccb038`: the helper announces `commit-message` (`src/editor.rs:1086-1102`), while paused/prepared todo editing owns a structured model (`src/rebase_todo_view.rs:15-35,80-94`) and explicit submit (`src/commands.rs:1193-1247`). Instant autosquash already admits message callbacks through scoped editor registration (`src/editor.rs:943-977,1271-1333`); verify reachability again before choosing an adapter.
  - Binding boundaries: keep prepared launch authority separate from paused-todo save authority; render generations separate from synchronized editor versions and Plain applyEdit response/echo identities; todo submit separate from commit-helper release. A todo must not inherit commit-style close-to-confirm or commit history. Reuse acknowledged run/URI/version confirmation and Abort when a managed message editor is involved, retaining unsaved text on failed or mismatched acknowledgement. Rich/Plain differences belong at delivery adapters, not duplicate domain validation or cancellation policy.
  - Capability constraint: prepared rebase currently rejects `Reword`/`Squash` (`src/rebase_plan.rs:72-75`, statically VERIFIED at review base); instant autosquash message support does not establish prepared-launch or Plain todo-editing parity. Keep the refusal until registered message editing is actually wired and verified; no terminal-editor or `GIT_EDITOR=true` fallback.
  - Acceptance to retain when splitting: test: scripted editing preserves exact intended todo identities/order and repository state on cancellation or rejection; reword/squash uses the registered message bridge and its correlated completion/Abort; stale versions, rejected acknowledgements, and transport failure preserve editable text and ownership. Extend existing `tests/rebase_interactive.rs`, `tests/rebase_todo.rs`, and `tests/editor_bridge.rs` coverage where fixtures match. Verification: `make test-rust FILTER="--test rebase_interactive --test rebase_todo --test editor_bridge"`; changed visible behavior additionally needs an existing text-only native scenario through Make. These are future acceptance requirements, not executed evidence or authorization to cross T019.
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

Owner-request-only; not independent-ready work. [Scopes](./TASKS_PART_SEVEN.md#phase-14-deferred-not-scheduled). T105 is scheduled only under gated Phase 6A. T096 needs profiling; T106 needs an actual command need before scheduling.

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

### Retained gaps

- A12 real-dispatch Git-failure handling/removal of `server::mutation` is documented closed (ledger); T103 remains the distinct same-session reader-lock redesign. The old “ownerless A12” statement was stale, not a new implementation finding.
- R-EDIT-REAL remains ungreen beyond T104, as above. Binary/platform limits remain; macOS config-directory choice is resolved, but root config-edit command and runtime location verification remain open. Escape caveat remains.

### Task ticket template and UI rule

Every ticket states **Target Files**, **Reference Files**, **Architectural Constraints**, **Acceptance Criteria**, exact **Verification Command**, dependencies and size. Enclosing named phase/phase map supplies the phase gate when its last unchecked item completes, not at every slice/review batch. Follow [AGENTS.md](./AGENTS.md#working-conventions). User-visible changes require observable text/structured outcomes and focused native evidence; never images.

## Ad hoc

### Documentation-only backlog audit (2026-11-18)

- [x] **DOC-AUDIT-COMPACT — Audit, compact and prioritize the backlog without executing it.** check: preserve all base checkbox occurrences/states, unique IDs, exact authoritative Done/corrections and unresolved obligations; compare inventories and sizes; retain anchors and scope ownership; `make docs-check`. Four attempts total (two default, two strong): reconciliation; stalled orientation; substantive compaction/inventory; final reference reconciliation and verification. No gate approval, runtime work or protected untracked files included. Done: bec27b33839a19f03d0d58cc878605f1b49d70c0.
  - Verified A1 — base/final inventory against `a366409749edf0e5591dff34d70500b73ab595b9`: all 672 original checkbox occurrences retained (567 closed, unique B-line mappings; 105 original unchecked identities/states); only this audit adds a checkbox. Numbered IDs 401/401; all eight historical references restored; all 268 distinct SHA references in 289 original Done-bearing rows retained. Original B-line context preserves exact Done/correction ownership and ambiguous evidence; B1572's trailing parent `09119a7` is recorded at parent B1570, not claimed as child implementation.
  - Verified A2 — UTF-8 bytes: TASKS.md 418966 → 74872; TASKS_PART_SEVEN.md 96642 → 99062. Closed narratives/gate chronologies consolidate into explicit immutable Base mappings, not another archive; open scopes/dependencies remain here or in linked specifications. Priority Review → ready → blocked → gated → optional → completed ordering is documentation-only; neither hygiene-size checkbox is closed.
  - Verified A3 — base/final heading inventories retain every original heading text and duplicate occurrence in both targets, including the superseded Phase 3 landing anchor. Seven stale design/protocol references and T022 harness ownership now name maintained files; T105/T111 routing and the release subset have one documented owner. No runtime completion inferred.
  - Verified A4 — T022 remains unchecked; Phase 6A is prohibited; explicit human release approval is required. The temporary plan is replaced by durable risks, decision ownership and recommended order. Policy questions remain unresolved; Stop applies after this audit, prior authorization/restrictions remain separate, and no Overnight directive remains.
  - Verified A5 — `make docs-check` → exit 0 (`Ran 3 tests`, `OK`, `ok: docs-check`); it also passed before compaction, so the A1–A4 before/after inventory checks above separately exercise the new preservation/reconciliation work. Only the two sprint-owned Markdown paths are eligible for this audit commit; user-owned untracked paths remain untouched.

### Destructured local requires (2026-11-19) — clears the 58 limitation-backed Knip flags

Knip 6.45.1 does not credit CommonJS member-access usage (`const m = require("./mod"); m.foo()`), so 58 used exports stay flagged. Converting consumer requires to destructured form credits them; verified by scratch repro. Parts are mechanical rename-class refactors: no behavior, export surface or knip.json changes; each part verifies `CI=true make vscode-test` and records the Knip unused-exports count via `CI=true make hygiene` (count must not increase; expected to fall where all member-access consumers of a flagged export sit in the part's files).

- [x] **Destructure local requires: session layer** — `clients/vscode/extension.js`, `lib/session.js`, `lib/commands.js`: every local-module require consumed by member access becomes a destructured require of exactly the members used; module objects passed wholesale may stay bound, with the reason recorded. Sweep: no member-access pattern on local modules remains in these files. Done: 3167447.
  - Verified A1 — `grep -nE "\b(commands|edits|drafts|effects|requests|semantic|shared|views|version|foldkeys|menus)\."` over the three files plus `grep -nE "require\('\.\.?/"`: all 19 local requires are destructured (extension.js 1, session.js 11, commands.js 7); the only remaining dotted hits are `vscode.commands`, `store.views` and prose. No wholesale-pass exception was needed. Local decisions: `../types/shared-lists` is included as a local types module (2 members, `optionalEffects`/`optionalPromptKinds`); local `const warning` renamed `versionWarning` and local `const currentEdit` renamed `edit` because the destructured member names would otherwise shadow them inside their own initializers.
  - Verified A2 — `CI=true make vscode-test` → exit 0 (`ok: build`, `ok: vscode-deps`, `ok: vscode-typecheck`, `ok: vscode`, `ok: keybindings`).
  - Verified A3 — `CI=true make hygiene` → `knip: NONZERO (exit 1)` with `Unused exports (28)`; the 58 baseline fell to 28 because the destructured consumers credit the member exports, and 28 ≤ 58.
- [x] **Destructure local requires: view/menu/effects layer** — `clients/vscode/lib/views.js`, `lib/menus.js`, `lib/effects.js`, `lib/edits.js`, `lib/drafts.js`, same rules as the session layer. Done: 3f18c81.
  - Verified A1 — `grep -nE "require\('\.\.?/"` over the five files plus `grep -nE "\b(views|menus|effects|edits|drafts|edit|menu|requests|context)\."`: all 22 local requires are destructured (views.js 5, menus.js 5, effects.js 3, edits.js 5, drafts.js 4); the only remaining dotted hits are `store.views`/`store.drafts` Map access, `state.menu` properties, the `context` ExtensionContext parameter, a `'menu.'` string literal and prose. No wholesale-pass exception was needed. Local decisions: `effects.js` resolves `drafts`/`views` inside `applyEffects` and `menus.js` resolves `views` inside `showMenu`, because the out-of-scope `requests.js` requires all three wholesale and a top-level destructure would capture partial exports in the `views → menus → requests → effects → drafts → views` cycle.
  - Verified A2 — `CI=true make vscode-test` → exit 0 (`ok: build`, `ok: vscode-deps`, `ok: vscode-typecheck`, `ok: vscode`, `ok: keybindings`).
  - Verified A3 — `CI=true make hygiene` → `knip: NONZERO (exit 1)` with `Unused exports (11)`; the 28 post-session-layer baseline fell to 11 and 11 ≤ 28.
- [x] **Destructure local requires: requests layer** — `clients/vscode/lib/requests.js`, same rules as the session layer. Done: b96e132.
  - Verified A1 — `grep -nE "require\('\.\.?/"` over `lib/requests.js` finds 8 local requires, all destructured (6 top-level: `./render`, `./prompt`, `./pick`, `./effects`, `./store`, `./util`; `./menus` and `./views` in-function inside `applyRenderNotification`); `grep -nE "\b(render|prompt|pick|effects|menus|views|store|util)\.[a-zA-Z]"` leaves only `store.*` property access on the destructured `store` export plus prose/string hits. Local decisions: `prompt.answerFor` and `pick.answerFor` alias to `answerPrompt`/`answerPick` (same member name used twice in one file); the local function `applyRender` is renamed `applyRenderNotification` because the destructured `render` member now binds that name, and `exports.applyRender = applyRenderNotification` keeps the export surface; `menus`/`views` resolve in-function because `views → menus → requests → effects → drafts → views` means a top-level destructure would capture partial exports.
  - Verified A2 — `CI=true make vscode-test` → exit 0 (`ok: build`, `ok: vscode-deps`, `ok: vscode-typecheck`, `ok: vscode`, `ok: keybindings`).
  - Verified A3 — `CI=true make hygiene` → `knip: NONZERO (exit 1)` with `Unused exports (4)`; the 11 post-Part-2 baseline fell to 4 and 4 ≤ 11.
  - Verified A5 — the verified `requests.js` blob landed inside concurrent commit `b96e132`: a parallel session committed the shared index while this ticket's staged files were pending, so the commit identity is conflated with unrelated Plain-preset work. A2/A3 above ran against that exact blob at HEAD; recorded honestly rather than claimed as a clean path-limited commit.
- [x] **Destructure local requires: unit and type tests** — `clients/vscode/test/test.js`, `test/types.js`; JSDoc type-only references (`render.RenderState`) switch to the destructured binding.
  - Verified A1 — `grep -nE "require\('\.\.?/"` over both files: `test.js` now has 9 destructured local-module requires (`../lib/context`, `../lib/render`, `../lib/menu`, `../lib/foldkeys`, `../lib/prompt`, `../lib/pick`, `../lib/edit`, `../lib/version`, `./e2e/launcher-result`) and `types.js` 3 (`../lib/render`, `../lib/menu`, `../lib/prompt`); `grep -nE "\b(context|render|menu|foldkeys|prompt|pick|edit|versions|launcherResult)\.[a-zA-Z]"` over both files leaves only section comments (`// lib/render.js ---`), the `'menu.commit'`/`'menu.rebase'` scope string literals, test-title prose and `vector.render` payload fields — no member access on a local-module binding remains. Exceptions: `./no-vision` stays whole because its export is the single function passed wholesale at suite registration; the three `client-contracts.json` requires keep inline `.prompts`/`.picks`/`.renders` access and the four `../package.json` sites keep inline `.contributes` access (no module binding to destructure). No JSDoc reference of the `render.RenderState` form exists in either file: the only type references are `import('../types/protocol')` paths in `types.js`, so that clause needed no change. Local decisions: `prompt.answerFor`/`pick.answerFor` alias to `answerPrompt`/`answerPick` (same member name in two modules, matching the requests-layer precedent); `types.js` destructures `quickPickItems` and `quickPickItem` separately because both are used. Test files are require-graph leaves (nothing requires them), so top-level destructuring is cycle-safe.
  - Verified A2 — `CI=true make vscode-test` → exit 0 (`ok: build`, `ok: vscode-deps`, `ok: vscode-typecheck`, `ok: vscode`, `ok: keybindings`), run after the last code edit.
  - Verified A3 — `CI=true make hygiene` → `knip: NONZERO (exit 1)` with `Unused exports (1)`; the 4 post-requests-layer baseline fell to 1 and 1 ≤ 4. The one survivor is `pickFor lib/requests.js:333:9`, whose remaining consumer sits only in the out-of-scope `test/integration.js`.
  - Done: `f491560`.
- [x] **Destructure local requires: integration harness** — `clients/vscode/test/integration.js`; the 15-member `draft` list destructures multi-line; fake-`vscode` site destructures its six members.
  - Verified A1 — `grep -nE "\b(vscode|shared)\." clients/vscode/test/integration.js` → no matches. The only local-module binding consumed by member access was `./fake/vscode`, now destructured multi-line into its six members at `test/integration.js:20-28` (`Position`, `Selection`, `__test`, `window`, `workspace`, `commands: vscodeCommands`); the module object stays bound as `vscodeModule` only for wholesale use at `:196` (`Module._load('vscode')`) and `:292`/`:941` (selection-integration). `commands` aliases to `vscodeCommands` because the function-local `commands` Map at `:232` would otherwise shadow the fake registry; `grep -nE "\bvscode\b"` now leaves only prose, `vscodeModule` and the `'./fake/vscode'`/`'vscode'` strings. Premise recomputed as directed: no 15-member `draft` module binding exists at HEAD — `draft` (`:89`) is `require('../lib/drafts').current()` and `record`/`binding` are data objects — so the surviving local requires are the inline knip-credited `require('./x').member` forms left untouched: drafts `.current`/`.forUri`/`.open`×2/`.onTabsChanged`×3/`.disconnect`, extension, store `.store`×5, effects `.applyEffects`, menus `.updateQuickPick`, requests `.promptFor`, package.json `.contributes`, client-contracts `.prompts`, selection-integration `.checkShapes`/`.checkGitSelections`.
  - Verified A2 — `CI=true make vscode-test` → exit 0 (`ok: build`, `ok: vscode-deps`, `ok: vscode-typecheck`, `ok: vscode`, `ok: keybindings`), run once after the last edit.
  - Verified A3 — `CI=true make hygiene` → exit 0 with `cargo machete: CLEAN (exit 0)`, `knip: NONZERO (exit 1)` `Unused exports (1)` (`pickFor lib/requests.js:333:9`), `luacheck: CLEAN (exit 0)`; unchanged from the 1-flag baseline (no increase), and the destructure introduces no new dead export.
  - Verified A5 — code commit `245316c` changes only `clients/vscode/test/integration.js` (73+/65−); this bookkeeping commit stages only the rebuilt TASKS.md hunk, leaving the concurrent session's unstaged TASKS.md hunks untouched in the worktree.
  - Done: `245316c`.

### Cheap missing commands (2026-11-19; user-directed early delivery)

User-directed 2026-11-19: implement now, on current infrastructure, the cheapest magit-parity commands still marked Planned — pulled forward from their post-release phases. Identification (scout at base `e42301c`): template `status.stage` — registration `src/commands.rs:793-799`, handler/menu binding `src/menus/dispatch.rs:48`, state tests `tests/commands/cases.rs`, parity rows auto-flip from menu roadmap markers (`src/commands.rs:1154-1172`, `src/menus/mod.rs:217`) and regenerate via `make registry-docs-generate`. Deliberately not ad hoc, despite trivial-looking parity rows (each is sequenced behind real design): stash (T072/T073 need a stable stash identity first — `stash@{n}` is forbidden index addressing), revert/cherry-pick (T076 sequencer state; a bare revert can strand a repository mid-sequence), bisect (T087 is a workflow, not bare commands), branch checkout (T067 unsafe-operation reporting), fetch/pull/push/merge (T068/T069 network seam). Unverified candidates for follow-up tickets: `clean`, discard-all (absence SUSPECTED, plumbing unproven).

- [x] **Early T043 — branch-basics (cheap commands).** Deliver the full T043 scope now (TASKS_PART_SEVEN.md:770): branch configure, create, rename and delete; checkout stays in T067. Per command: one `src/git.rs` method, handler + menu/key wiring per the `status.stage` template, state tests in `tests/commands/`, a conformance scenario for menu-driven application, KEYMAPS.md §5 rows for any bound keys; flip the parity rows to Implemented and regenerate via `make registry-docs-generate`, committing generated docs separately with the contract change and command named. On full verified scope, also tick the Phase 10 T043 line with a Done cross-reference to this item.
  - Verified A1 — `grep -n 'id: "branch\.\(create\|rename\)"' src/commands.rs` → `command!` registrations at `:844`/`:845`; `src/menus/branch.rs:138`/`:171` carry empty markers on the `n` and `m` rows, and the four checkout rows keep `R-6.1` (`branch.checkout` `:79`, `branch.checkout-local` `:86`, `branch.orphan` `:93`, `branch.checkout-remote` `:100`); KEYMAPS.md §5.4.2 documents `n` (new branch) and `m` (rename).
  - Verified A2 — `make test-rust FILTER="--test commands"` → exit 0; create-then-no-switch and rename-two-prompt cover the new prompt actions and `src/git.rs` methods.
  - Verified A3 — `make ui-file FILE=tests/ui/scenarios/branch-create.scn` → exit 0, menu-driven `b n` creation leaves `main` checked out.
  - Verified A4 — this implementation commit plus the separately generated registry-docs commit flipping the create and rename parity rows; `make registry-docs-check` exit 0.
  - Verified (delete) — `make test-rust FILTER="--test commands"` → exit 0 (51 tests); `delete_branch` in `src/git.rs`, `PromptAction::BranchDelete` and `branch.delete` (`src/commands.rs:855`), and the `k` row marker cleared (`src/menus/branch.rs:175-181`) while the four checkout rows keep `R-6.1`.
  - Verified (delete, native) — `make ui-file FILE=tests/ui/scenarios/branch-delete.scn` → exit 0; menu-driven `b k` with `C-u feature Enter` clears the pre-filled default and deletes the branch.
  - Verified (delete, docs) — `make registry-docs-generate` then `make registry-docs-check` → exit 0; the diff is exactly the `branch.delete` row in `docs/parity.md` and its `docs/plain-support.md` sibling, committed separately from this implementation commit.
  - Verified (description, A1) — `grep -n 'id: "branch.configure-description"' src/commands.rs` → one `command!` registration; the `src/menus/branch.rs:30-36` `d` row marker is empty while the four checkout rows and the `b C` configure popup keep `R-6.1`; KEYMAPS.md §5.4.2 documents `b d` and keeps the configure popup planned.
  - Verified (description, A2) — `make test-rust FILTER="--test commands"` → exit 0; two new tests set a description on `main` and on a `feature` side branch through the two-prompt chain.
  - Verified (description, A3) — `make ui-file FILE=tests/ui/scenarios/branch-description.scn` → exit 0; menu-driven `b d` accepts the pre-filled `main` default and writes `git config branch.main.description`.
  - Verified (description, A4) — implementation, registry-docs and Done commits; `make registry-docs-check` exit 0 with the diff exactly the `branch.configure-description` row in `docs/parity.md` and its `docs/plain-support.md` sibling.
  - Verified (description, A5) — final `git status --porcelain` leaves only the concurrent session's ` M TASKS.md` and ` M clients/vscode/lib/requests.js` plus the five user-owned untracked paths.
  - Early-delivery boundary: the `b C` configure... popup remains Planned R-6.1; the Phase 10 T043 line stays unticked until the full configure scope lands; checkout remains T067.
  - Done: `a6e5eb8`.
- [x] **Early T042 — remote-config-tags (cheap commands).** Deliver the full T042 scope now (TASKS_PART_SEVEN.md:769): lightweight tag create/delete plus remote configuration — verify the exact remote rows in the parity table (add/rename/remove/prune) and implement those. Same template and evidence pattern; tick the Phase 10 T042 line on full verified scope.
  - Verified (tags, part 1/3, A1) — `grep -n 'id: "tag\.' src/commands.rs` → `command!` registrations at `:910`/`:911`; `src/menus/tag.rs:26`/`:33` clear the `t`/`k` markers while `tag.release` (`:27`) and `tag.prune` (`:34`) keep `R-6.3`; KEYMAPS.md §5.4.3 documents `t t` and `t k`.
  - Verified (tags, part 1/3, A2) — `make test-rust FILTER="--test commands"` → exit 0; `make test-rust FILTER="--test commands tag_"` → 3 passed, covering create-at-HEAD without switching, delete of an existing tag, and Git's "not found" refusal for a missing tag.
  - Verified (tags, part 1/3, A3) — `make ui-file FILE=tests/ui/scenarios/tag-create.scn` → exit 0; menu-driven `t t` creates `v1` while `main` stays checked out; no snapshot reflects tag rows, so none changed.
  - Verified (remotes add/remove, part 2/3, A1) — `grep -n 'id: "remote\.' src/commands.rs` → `command!` registrations at `:956`/`:957`; `src/menus/remote.rs:36`/`:44` clear the `a`/`k` markers while the six other remote rows keep `R-6.3`; KEYMAPS.md §5.4.4 documents `M a` and `M k`.
  - Verified (remotes add/remove, part 2/3, A2) — `make test-rust FILTER="--test commands"` → exit 0; `V=1 make test-rust FILTER="--test commands remote_"` → 3 passed, covering the two-prompt add whose `git remote get-url origin` equals the answered path, removal of an existing remote, and Git's "No such remote" refusal.
  - Verified (remotes add/remove, part 2/3, A3) — `make ui-file FILE=tests/ui/scenarios/remote-add.scn` → exit 0; menu-driven `M a` prompts for `origin` then the URL and `git remote get-url origin` matches; no snapshot reflects remote rows, so none changed.
  - Part 2/3 remote add and remove landed; remote rename/prune and part 3/3 remain; the Phase 10 T042 line stays unticked.
  - Verified (remotes rename/prune, part 3/3, A1) — `grep -n 'id: "remote\.\(rename\|prune\)"' src/commands.rs` → `command!` registrations at `:1006`/`:1007`; `src/menus/remote.rs:37`/`:46` clear the `r`/`p` markers while the four other planned rows (`:44` configure, `:52` prune-refspecs, `:59` unshallow, `:66` update-default) keep `R-6.3`; KEYMAPS.md §5.4.4 documents `M r` and `M p`.
  - Verified (remotes rename/prune, part 3/3, A2) — `make test-rust FILTER="--test commands"` → exit 0; the three new tests cover the two-prompt rename, prune dropping a stale tracking ref while keeping a live one, and the missing-remote prune refusal surfacing as Git's warn message.
  - Verified (remotes rename/prune, part 3/3, A3) — `make ui-file FILE=tests/ui/scenarios/remote-rename.scn` → exit 0; menu-driven `M r` prompts for `origin` then `upstream` and `git remote` reports `upstream` only; no snapshot reflects remote rows, so none changed.
  - Early T042 remotes complete; remaining Planned R-6.3: remote.configure, prune-refspecs, unshallow, update-default, tag.release, tag.prune; the Phase 10 T042 line stays unticked.
  - Done: `0f64083`.
- [x] **Early T070 — branch-reset (cheap commands).** Deliver the full T070 scope now (TASKS_PART_SEVEN.md:807): branch reset, kept separate from file checkout at a revision, with the target chosen through the existing prompt/pick infrastructure. Same template and evidence pattern; tick the Phase 12 T070 line on full verified scope.
  - Reset is exactly `git branch -f <name> <target>`: it never checks out the branch and never touches the working tree. That separation is deliberate; file checkout at a revision stays T067.
  - Verified (Early T070, A1) — `grep -n 'id: "branch\.reset"' src/commands.rs` → `command!` registration at `:1032`; `src/menus/branch.rs:173` clears the `x` marker to `""` while the `b C` configure row and the four checkout rows keep `R-6.1`; KEYMAPS.md §5.4.2 documents `b x` at `:155` and its remaining-planned note no longer lists reset.
  - Verified (Early T070, A2) — `make test-rust FILTER="--test commands"` → exit 0; the two new tests cover a side branch moving to the target without switching, and Git's refusal of the checked-out branch surfacing as a warn message.
  - Verified (Early T070, A3) — `make ui-file FILE=tests/ui/scenarios/branch-reset.scn` → exit 0; `b x` prompts for `feature` then `HEAD`, and `git rev-parse feature` equals `git rev-parse HEAD` with `main` still checked out.
  - Done: `c6d8379`.

## Completed work

Completion ledger: identity/state plus exact Done evidence or Base provenance. Each B marker maps one original checkbox, not a new claim of implementation acceptance. Titles shortened to stable IDs retain their full identity at that base line. Unlabeled historical refs are not upgrades to Done. For nested slices, the original indentation/context at B remains authoritative; a trailing parent's historical evidence is not a new child implementation claim. Duplicate checked narratives are consolidated only through their explicit B mapping; every original checkbox occurrence remains present.

- [x] SERVER-MODULE-PLAIN — Done: b7c8e56. (B48)
- [x] SERVER-MODULE-DRAFTS — Done: containing commit; 79ea498. Refs: b7c8e56. (B51)
- [x] SERVER-MODULE-VIEW — Done: containing commit; 4c25e8d. Refs: 79ea498. (B55)
- [x] SERVER-MODULE-REQUESTS — Done: b7a27f3. Refs: 71a2e12. (B59)
- [x] PNPM1 — Done: containing commit. Refs: 5a76c92, e2a06cf, 4b3605d9e4e20d484aa6b60befcaf5cb0fa29e34. (B64)
- [x] PNPM2 Refs: 14e986a. (B66)
- [x] VER1 — Done: containing commit. Refs: 001aa40. (B70)
- [x] VER2 — Done: containing commit. Refs: 9eb5001. (B72)
- [x] VER2h — Done: containing commit. Refs: efad539. (B74)
- [x] VER3 — Done: `6c5f26d`. (B76)
- [x] VER3a — Done: containing commit. Refs: 64b721e. (B77)
- [x] VER3c — Done: `9daa386`. Refs: 14e986a, 6954cd7. (B79)
- [x] VER3b — Done: `6c5f26d`. (B81)
- [x] VER3b1 (B82)
- [x] VER3b1a Refs: 0f1adc8. (B83)
- [x] VER3b1b (B85)
- [x] VER3b1b1 — Done: containing commit. Refs: a49f123. (B86)
- [x] VER3b1b2 (B89)
- [x] VER3b1b2a — Done: c7a4505. Refs: 1445158. (B90)
- [x] VER3b1b2b — Done: 63016f7. Refs: cdadead, 86215ae, 615a743. (B92)
- [x] VER3b2 — Done: `6c5f26d`. (B99)
- [x] VER3b2a (B100)
- [x] VER3b2a1 (B101)
- [x] VER3b2a2 Refs: 89dc62d. (B103)
- [x] VER3b2b — Done: `6c5f26d`. Refs: 50704ce, 89dc62d. (B107)
- [x] PORT1 — Done: `c974f8c`. Refs: 185bc9e. (B111)
- [x] PORT2 Refs: 89e5bd6. (B113)
- [x] DOC1 — Done: `bbc97db`. Refs: 3c31d80. (B116)
- [x] DOC2 — Done: `001aa40` plus Makefile correction. (B118)
- [x] DOC2a — Done: `185bc9e`. Refs: 9729f50. (B119)
- [x] DOC2b — Done: `001aa40` plus Makefile correction (`001aa40` mistakenly included baseline Makefile hunks; corrected here). Refs: 185bc9e. (B121)
- [x] DOC3 — Done: `613fb5c`. (B123)
- [x] DOC4 — Done: `ece5b10`. Refs: 9c1ab8c. (B124)
- [x] DOC5 Refs: 10400c6. (B126)
- [x] T008.1 — Done: `6957647`. (B129)
- [x] T008.2 — Done: `5cde72b`. (B131)
- [x] T008.3 — Done: `fbb1aa3`. (B133)
- [x] T008.4a (B135)
- [x] T008.4b — Done: `ec5d90c`. Refs: 85821b5. (B136)
- [x] T008.4c — Done: `5e4dfb5`. (B138)
- [x] T008.5a1 — Done: `0cab12f`. (B140)
- [x] T008.5a2 — Done: `1396871`. (B142)
- [x] T008.5a3 — Done: `1ffc72f`. (B144)
- [x] T008.5b1 — Done: `26ebd80`. (B146)
- [x] T008.5b2a — Done: `3e95ee6`. (B148)
- [x] T008.5b2b — Done: `ebfa0fc`. (B150)
- [x] T008.5b2c — Done: `5564a03`. (B152)
- [x] T008.5c1 — Done: `514422d`. (B155)
- [x] T008.5c2 — Done: `8a02a7e`. (B157)
- [x] T008.6a1 — Done: `3223e7d`. (B159)
- [x] T008.7a — Done: `d8b36fd`. (B162)
- [x] T008.7b — Done: `6887df2`. (B165)
- [x] T008.8a — Done: `d4cfc43`. (B168)
- [x] T008.8b — Done: `6fcf2c5`. (B171)
- [x] T008.10 — Done: `8ffb45d18c29cfc05441de200fa518ef979caa92`. (B174)
- [x] T008.9 — Done: `2c3cf0f5f61159cd7a39272616ecf0abceb9ee44` (via T008.9a–e). (B177)
- [x] T008.9a — Done: `e177b11`. (B178)
- [x] T008.9b — Done: `309c83f`. (B180)
- [x] T008.9b2 — Done: `60edee5`. (B182)
- [x] T008.9c1 — Done: `200c5532b4c4ce6b59bb712c7bab82fcaada7c04`. (B184)
- [x] T008.9c2 — Done: `0b54901`. (B185)
- [x] T008.9c3 — Done: `fb721e576dfb3d7027fece1856a0c020f3a9f32e` (via T008.9c3a–b2). (B188)
- [x] T008.9c3a — Done: `c7dadc3`. (B189)
- [x] T008.9c3b1 — Done: `5f3de54`. (B192)
- [x] T008.9c3b2 — Done: `fb721e576dfb3d7027fece1856a0c020f3a9f32e`. (B195)
- [x] T008.9d — Done: `9f0a70e20eb7a60c6049dc271c860fcd51db4f36`. (B198)
- [x] T008.9e — Done: `2c3cf0f5f61159cd7a39272616ecf0abceb9ee44`. (B201)
- [x] T008.11 — Done: `98a0a4c`. (B204)
- [x] T008.12 — Done: `ebe11b7` (via T008.12a–c). (B207)
- [x] T008.12a — Done: `f717778`. (B208)
- [x] T008.12b — Done: `e87a50c0eaabaa35384eb50628aabde5ab8ecc88`. Refs: e87a50c. (B211)
- [x] T008.12c — Done: `ebe11b7`. (B212)
- [x] T008.13 — Done: `b549480` (T008.13a), `6179b54` (T008.13b). (B214)
- [x] T008.13a — Done: `b549480`. Refs: b01ce98. (B215)
- [x] T008.13b — Done: `6179b54`. (B218)
- [x] T009.1 (B221)
- [x] T009.2 — Done: `plain_log_select_offers_commit_code_actions` in `tests/plain_select.rs`; full original assertion evidence at B222. Refs: 8280e85. (B222)
- [x] T009.3 — Done: `plain_log_select_revision_field_drives_confirm` and `plain_log_select_invalid_revision_changes_nothing_and_keeps_the_picker` in `tests/plain_select.rs`. (B225)
- [x] T009.4 — Done: `plain_log_select_confirm_returns_the_caller_menu_with_the_chosen_commit` and `plain_log_select_cancel_returns_the_caller_without_changes` in `tests/plain_select.rs`. Refs: 70d617d. (B228)
- [x] T009.5 (B231)
- [x] T009.6 — Done: `plain_log_select_fixup_target_survives_a_cancelled_second_run` in `tests/plain_select.rs`. Refs: 838203a. (B232)
- [x] T010.1 Refs: d4f8e09. (B236)
- [x] T010.2 — Done: `9537e52`. (B238)
- [x] T010.3 — Done: `stale_explicit_confirm_is_refused_and_keeps_the_draft_pending`, `explicit_abort_creates_no_commit_and_restores_the_status_view`, `close_after_abort_is_ignored`, `unrelated_document_notifications_are_rejected_without_touching_the_draft` in `tests/plain_draft.rs`. Refs: 51da1b7. (B239)
- [x] T010.4a — Done: `saved_file_close_commits_the_disk_text` and `dirty_close_commits_the_prior_saved_disk_text` in `tests/plain_draft.rs`. Refs: f930ff7. (B242)
- [x] T010.4b — Done: `dirty_save_creates_no_commit_and_keeps_the_draft_pending` and `dirty_cancel_keeps_the_draft_pending_and_the_editor_open` in `tests/plain_draft.rs`. Refs: 829eceb. (B245)
- [x] T010.4c — Done: `empty_saved_message_is_refused_by_git_and_reported` in `tests/plain_draft.rs`. Refs: aeb7ec7. (B248)
- [x] T010.4d — Done: `second_session_gets_no_draft_actions_and_cannot_forge_confirm` in `tests/plain_draft.rs`. Refs: 0345fff. (B251)
- [x] T007.4c2 — Done: `2fd268371f92fbdec3b8ef1db60668d7a66ebd83`. (B255)
- [x] T007.4c — Done: `2fd268371f92fbdec3b8ef1db60668d7a66ebd83`; exact Confirm body/stale-version correction `ccf6cbbe055ccb157b59aed525537cdef48f6171`. (B256)
- [x] T007.4n1 — Done: `41ff608` (runner foundations: `7e1d814`). (B257)
- [x] T007.4n1a — Done: `c361960`. (B260)
- [x] T007.4n1b — Done: `eb56e16`. (B261)
- [x] T007.4n1c — Done: `49cc25f`; recovery follow-up: `82d59c8`. (B262)
- [x] T007.4n1d — Done: `78f98eb`. Refs: f6a7832. (B265)
- [x] T007.4n1e — Done: `431af86`. Refs: 78f98eb. (B267)
- [x] T007.4n1f — Done: `9187cae`. (B269)
- [x] T007.4n1g — Done: `20c7f9c`. (B271)
- [x] T007.4n2 — Done: `c407a1d`. (B273)
- [x] T007.4 — Done: `2fd2683`. (B275)
- [x] T007.5 — Done: containing commit. Refs: 70eca61, 6646086, 11d1c8b, 33230ec, 53483b9, 5209f6e, 338b9de, 0e9223a, c3f5a42, 2264616, ccf6cbb, 2fd2683, 20c7f9c, c407a1d. (B277)

## Bugfix

Closed fixes from both former Bugfix sections share this ledger; no distinct identity is merged away.

- [x] BUG-LUALS-PROMPT-UNION — Done: 6ce08f4. Refs: 43572aa. (B291)
- [x] BUG-PLAIN-COMMIT-BARRIER — Done: f1a43d1. (B296)
- [x] BUG-PHASE6-INSTANT-REBASE — Done: `dc43535`. Refs: 9320f09. (B300)
- [x] BUG-PHASE6-PLAIN-COMMIT — Done: `0d70795`; inherited Done: `dc43535`. Refs: 9320f09. (B303)
- [x] BUG-PHASE6-PNPM — Done: `4045289`. Refs: 43318a2. (B307)
- [x] BUG-PHASE6-LOG-ARGUMENT-STATUS — Done: `43318a2`. Refs: 0763e38. (B310)
- [x] BUG-PHASE6-VISIBILITY — Done: containing commit; inherited Done: `53942dc`; `9320f09`. (B313)
- [x] BUG-PHASE6-VIEWS — Done: containing commit; premise correction: `35a7dfd`; inherited Phase 6 Done: `b48187d`. (B318)
- [x] BUG-PHASE6-SERVER-ASYNC — Done: containing commit; inherited Phase 6 Done: `f15fa55`; `b48187d`. (B322)
- [x] BUG-PHASE6-PROMPT-RENDER — Done: containing commit; inherited Phase 6 Done: `b63fe3a`; `f15fa55`. (B330)
- [x] BUG-PHASE6-PLAIN-OFFER-BOUND — Done: containing commit; inherited Phase 6 Done: `a0e9e91`; `b63fe3a`. (B334)
- [x] BUG-PHASE6-NAVIGATION — Done: containing commit; `a0e9e91`. Refs: e2472ac. (B338)
- [x] BUG-PHASE6-LOG-SELECTION — Done: `e2472ac`. Refs: c4e3c2b. (B342)
- [x] BUG-PHASE6-KEY-VERSION — Done: d2ad536. Refs: 6fdb9a2. (B345)
- [x] BUG-PHASE6-SECOND-DRAFT — Done: `6fdb9a2`. (B349)
- [x] BUG-PHASE6-EXAMPLE-TIMEOUT — Done: `100c541`. Refs: 315b905. (B353)
- [x] BUG-PHASE6-STALE-RENDER — Done: containing commit. Refs: e0566d1, 9a8372a. (B357)
- [x] BUG-PHASE6-BRIDGE — Done: `b4f5d87`. (B361)
- [x] BUG-PHASE6-PROC — Done: `04907d4`; focused owned-runner test passed through Make, and subsequent full run passed all 68 library tests. (B363)
- [x] Refresh instant-autosquash generated references. Refs: 63c8bd3, f2f7cd0. (B365)
- [x] Refresh instant-autosquash recorded registry responses. (B368)
- [x] Refresh instant abort recording. Refs: f3767f3. (B369)
- [x] Refresh instant confirm recording. Refs: f3767f3. (B371)
- [x] Stabilize the instant-conflict readiness assertion. — Done: containing commit. Refs: 4da5eb0. (B375)
- [x] Refresh initial-rebase generated references. — Done: `b2c99d3`. Refs: 0680cdb. (B378)
- [x] Repair initial-rebase command replay fixtures. — Done: `3ebfadc`. Refs: cea53d3. (B382)
- [x] Repair initial-rebase abort recording. Refs: cea53d3. (B385)
- [x] Complete initial-rebase recording replay. — Done: `3ebfadc`. Refs: 23bbd5f. (B387)
- [x] Refresh generated references for scoped todo commands. — Done: `8beb859`. Refs: 46dc360. (B391)
- [x] Regenerate VSCode todo keybindings through Make. Refs: 656ddd7, 760ad60. (B396)
- [x] Align recorded actions with todo editor scope. Refs: e30a080. (B403)
- [x] Refresh snapshots after todo scope correction. Refs: e30a080, 30ba485. (B408)
- [x] Reconcile todo keymap manifests and shortcut policy. — Done: containing commit. Refs: 8b9f2f6. (B413)
- [x] Restore menu registry parity for contextual todo actions. — Done: containing commit. Refs: 61c9022. (B420)
- [x] Repair todo command registry example replay. Refs: 180dfce. (B425)
- [x] Repair commit abort registry frame. (B428)
- [x] Complete remaining todo registry replay. (B432)
- [x] Refresh worked exchanges for initialize identity. — Done: `f393248`. Refs: 4e7521a, efad539, ebe9ac6. (B436)
- [x] Repair initialize serverInfo replay fixtures. — Done: containing commit. Refs: 6d626a5, 4e7521a. (B441)
- [x] Repair Phase 5 recorded example drift. — Done: `d34d9ac`, `c21eda6`. Refs: 4e25db9. (B446)
- [x] Split client specification below hygiene limit — Done: 32be945. (B462)
- [x] Split session implementation below hygiene limit — Done: 2d72f7b. (B466)
- [x] Split confirm recording below hygiene limit — Done: 5625c94; coverage completion e522e31. (B471)
- [x] Split abort recording below hygiene limit — Done: 520d811. (B478)
- [x] Disposition newly reported dead-code suppressions — Done: e0a5317. (B484)
- [x] Resolve Lua warning findings — Done: 1116c65. (B496)
- [x] Reduce evidence archive size — Done: 6175df8; containing commit. (B501)
- [x] Split server implementation below hygiene limit — Done: containing commit. Refs: b7c8e56, 79ea498, 4c25e8d, b7a27f3. (B506)
- [x] Split async server tests below hygiene limit — Done: containing commit; cb3bb4c. Refs: 89dc88c. (B509)
- [x] Split Neovim integration tests below hygiene limit — Done: b84baa2; containing commit. Inherited async-test `Done: cb3bb4c` preserved, not Neovim implementation. Refs: 32521932. (B514)
- [x] Split editor bridge tests below hygiene limit — Done: 8a35f9a. (B524)
- [x] Split generated worked-exchanges reference below hygiene limit — Done: 4d0c29d. (B527)
- [x] Split command tests below hygiene limit — Done: dd44fb2. (B531)
- [x] Split VSCode E2E suite below hygiene limit — native A06 via `make vscode-e2e`; no Done SHA supplied in Base. (B534)
- [x] Resolve new Luacheck findings from T022 hygiene run — Done: containing commit; implementation `60f439a`, generated annotations `2a583fe`. (B538)

### Feature — Phase 2 (acceptance and recorded final gate complete)

- [x] T006 — Done: containing commit. (B581)
- [x] T007 — Done: containing commit; all six original clauses and native evidence audited under T007.5. (B583)

### Feature — Phase 3: Plain interaction (complete; gate recorded)

- [x] T008 — Done: `6957647`..`6179b54` (via T008.1–T008.13b). (B587)
- [x] T009 — Done: `8280e85`..`cb8423c` (via T009.1–T009.6). (B588)
- [x] T010 — Done: `7c62e6c`..`600411d` (via T010.1–T010.4d). (B589)

### Feature — Phase 4: Specification

- [x] T011 — Done: 92c78fb (containing commit of the last slice). Child implementation mapping: T011.1 `e6db680`, .2 `f5cd1dc`, .3 `1fcfce5`, .4 `ba42672`, .5 `5887917`, .6 `c7771d5`/`66b8f9f`, .7 `2b968a6`, .8 `d5573c2`, .9 `ff550b0`, .10 `f33bf3c`, .11 `1abac39`, .12 `18c9d4a`, .13 `395541a`, .14 `db870f0`, .15 `92c78fb`. Acceptance 5's example-file half belongs to T012. (B593)
- [x] T011.11 (B595)
- [x] T011.12 — Done: containing commit. Refs: 1abac39, f33bf3c. (B596)
- [x] T011.13 Refs: 18c9d4a. (B604)
- [x] T011.15 Refs: db870f0. (B607)
- [x] T011.14 — Done: containing commit. Refs: 395541a. (B611)
- [x] T011.10 Refs: 2b968a6. (B616)
- [x] T011.9 Refs: d5573c2. (B620)
- [x] T011.8 Refs: 66b8f9f. (B623)
- [x] T011.7 — Done: containing commit. Refs: ff550b0. (B627)
- [x] T011.6 — Done: containing commit. Refs: 5887917. (B632)
- [x] T011.5 — Done: containing commit. Refs: ba42672. (B637)
- [x] T011.4 — Done: containing commit. Refs: 1fcfce5. (B642)
- [x] T011.3 — Done: containing commit. Refs: f5cd1dc. (B647)
- [x] T011.2 — Done: containing commit. Refs: e6db680. (B652)
- [x] T011.1 — Done: containing commit. Refs: 86d12d3. (B657)
- [x] T012 — Done: 1e83443 (containing commit of the last slice). Child mapping: .1 `f118882`, .2 `da056e3`, .3 `153d57b`, .4 `a2e06c0`, .5 `5f305aa`, .6 `1e83443`. (B662)
- [x] T012.1 Refs: 92c78fb. (B664)
- [x] T012.2 Refs: f118882. (B668)
- [x] T012.3 Refs: da056e3. (B672)
- [x] T012.4 Refs: 153d57b. (B676)
- [x] T012.5 Refs: a2e06c0. (B680)
- [x] T012.6 Refs: 5f305aa. (B686)
- [x] T013 — Done: 7169349 (containing commit of the last slice). Child mapping: .1 `a62d659`, .2 `648b5e3`, .3 `7169349`. (B690)
- [x] T013.1 Refs: 3bc7a3d. (B692)
- [x] T013.2 Refs: a62d659. (B697)
- [x] T013.3 Refs: 648b5e3. (B702)

### Feature — Phase 5: Commit selection and rebase

- [x] T014 — Done: c9571c6 (containing commit of the last slice). Child mapping: .1 `d479920`, .2 `c9571c6`. (B710)
- [x] T014.1 Refs: d479920. (B712)
- [x] T014.2 Refs: c9571c6. (B717)
- [x] T015 (B722)
- [x] T015.1 (B724)
- [x] T015.1a Refs: ba58ceb, 37a4e57. (B727)
- [x] T015.1b Refs: ba58ceb. (B731)
- [x] T015.2 — Done: containing commit. Refs: 8c25d27. (B735)
- [x] T016 — Done: 241d292 plus T016.2. (B740)
- [x] T016.1 Refs: 2f3c7e1. (B741)
- [x] T016.2 — Done: containing commit. Refs: df88c46. (B745)
- [x] T017 (B749)
- [x] T017.1 — Done: containing commit. Refs: 02e5e01. (B750)
- [x] T017.2 (B754)
- [x] T017.2a — Done: `1db0220`, `d31087b`. (B757)
- [x] T017.2a1 — Done: containing commit. (B760)
- [x] T017.2a2 — Done: containing commit. Refs: 1db0220. (B763)
- [x] T017.2b Refs: 7be7622. (B767)
- [x] T017.3 Refs: 12bdfbb. (B771)
- [x] T018 (B775)
- [x] T018.1 Refs: 2e0cb73. (B777)
- [x] T018.2 Refs: a3ae47f. (B781)
- [x] T018.3 (B785)
- [x] T018.3a — Done: containing commit. Refs: a1c141e. (B788)
- [x] T018.3b (B792)
- [x] T018.3b1 (B795)
- [x] T018.3b1a Refs: 3141c6b. (B798)
- [x] T018.3b1b Refs: ef318e1. (B803)
- [x] T018.3b2 (B807)
- [x] T018.3b2a Refs: 4a3eb72. (B811)
- [x] T018.3b2b Refs: 7893c2b. (B815)
- [x] T018.3c Refs: 6ff3713. (B819)
- [x] T018.3c1 Refs: 6ff3713. (B824)
- [x] T018.3c2 (B828)
- [x] T018.3c2a Refs: 97cf821. (B832)
- [x] T018.3c2b Refs: 42bf123. (B836)
- [x] T018.4 (B841)
- [x] T018.4a Refs: 15b41e9. (B844)
- [x] T018.4b Refs: 14c5ce5. (B848)
- [x] T019 — Done: `6dcebc9`. (B851)
- [x] T019.1 Refs: 73cdbcd. (B855)
- [x] T019.2 (B859)
- [x] T019.2a Refs: 97b361e. (B862)
- [x] T019.2b (B865)
- [x] T019.2b1 Refs: 817e314. (B868)
- [x] T019.2b2 (B871)
- [x] T019.2b2a Refs: 632dc68. (B874)
- [x] T019.2b2b (B877)
- [x] T019.2b2b1 — Done: `cf051cb`. (B880)
- [x] T019.2b2b2 (B885)
- [x] T019.3 (B888)
- [x] T019.3a Refs: a72aaed. (B891)
- [x] T019.3b Refs: 8701ed8. (B894)
- [x] T019.3c (B897)
- [x] T019.3c1 (B899)
- [x] T019.3c1a (B902)
- [x] T019.3c1b Refs: 7312e65. (B904)
- [x] T019.3c2 (B907)
- [x] T019.3c2a Refs: d7b4563. (B909)
- [x] T019.3c2b (B912)
- [x] T019.3c2b1 Refs: e3a2bab, 24c5ceb. (B914)
- [x] T019.3c2b2 Refs: d13b123. (B918)
- [x] T019.4 (B921)
- [x] T019.4a (B923)
- [x] T019.4a1 Refs: 186931a. (B925)
- [x] T019.4a2 (B928)
- [x] T019.4a2a (B931)
- [x] T019.4a2b (B933)
- [x] T019.4a2b1 Refs: 67b81ba. (B936)
- [x] T019.4a2b2 Refs: 67b81ba, da51e1d. (B939)
- [x] T019.4b — Done: `67b81ba`. (B942)
- [x] T019.5 Refs: 58aedcd. (B945)
- [x] T019.5a (B949)
- [x] T019.5a1 Refs: 58aedcd. (B951)
- [x] T019.5a2 Refs: 12683de. (B953)
- [x] T019.5b (B955)
- [x] T019.5c (B956)
- [x] T019.5c1 Refs: 95fce1f. (B957)
- [x] T019.5c2 (B959)
- [x] T019.5c2a Refs: fc697b4. (B960)
- [x] T019.5c2b (B962)
- [x] T019.5d (B964)
- [x] T019.5d1 (B965)
- [x] T019.5d1a (B966)
- [x] T019.5d1b (B967)
- [x] T019.5d2 Refs: b88cf0d. (B969)
- [x] T019.5d3 Refs: b88cf0d. (B971)
- [x] T019.5e (B974)
- [x] T019.5e1 Refs: a51d840. (B975)
- [x] T019.5e1a Refs: a026823. (B977)
- [x] T019.5e1b Refs: a51d840. (B979)
- [x] T019.5e2 (B981)
- [x] T019.5e2a Refs: 94d965a. (B982)
- [x] T019.5e2a1 (B984)
- [x] T019.5e2a2 (B986)
- [x] T019.5e2b Refs: 4da5eb0. (B989)
- [x] T019.5f (B991)
- [x] T019.5f1 (B992)
- [x] T019.5f1a Refs: 32b951e. (B993)
- [x] T019.5f1b Refs: 6375fc8. (B995)
- [x] T019.5f2 (B997)
- [x] T019.5f2a Refs: 26b5801. (B998)
- [x] T019.5f2b (B1000)
- [x] T019.5f2b1 Refs: ae1f998. (B1001)
- [x] T019.5f2b2 Refs: 2307325. (B1003)
- [x] T020 — Done: containing commit (with `a2fc50e`). Standalone script retained because focused scenarios do not replace all assertions. (B1010)
- [x] T020.1 — Done: `a2fc50e`. (B1011)
- [x] T020.2 — Done: `a2fc50e`. (B1013)
- [x] T020.3 — Done: containing commit. (B1015)
- [x] T021 — Done: `6f98336`. Scoped initial/follow-up limits remain in risks and [manual evidence](docs/evidence/plain-helix.md). (B1017)
- [x] T023 — Done: `9e6111e` (Part 1: `9933272`; Part 2 implementation: `dcdabfe`; generated table: `6d1c37d`). Completion declaration is not completionProvider support. (B1042)
- [x] T040 — Done: `7ce9dd3`. Deterministic signer evidence only; no real-key/native pinentry/cryptographic claim. (B1082)
- [x] T041 — Done: `c84d1ae`. Existing branch/ref variants and filters only; remaining stubs/new views excluded. (B1086)

## Bugfix

Legacy second Bugfix landing anchor retained; all closed fixes are in this completion ledger.

- [x] BUG-PHASE6-REGISTRY-DOCS — Done: `d1a3591`; queue evidence: `3620357`. (B1212)
- [x] BUG-PHASE6-EXAMPLE-COMMAND-ORDER — Done: b902634, 944d571, 39bccb2; queue completion: `0763e38`. Refs: c84d1ae. (B1218)
- [x] Target restored status content explicitly in process-log UI regression. — Done: containing commit. Refs: adac686. (B1225)
- [x] Preserve terminal backtab for basic modifier clients. — Done: containing commit. Refs: b02d98f. (B1229)
- [x] Deliver shifted Tab as terminal backtab in native UI harness. — Done: containing commit. (B1233)
- [x] Build two-hunk UI repository before client startup. — Done: 1fc9fa3. Refs: b888574. (B1238)
- [x] Repair vim hunk-body folding UI regression. — Done: a8bd4e0. Refs: 1fc9fa3, 1794310251020. (B1241)
- [x] Repair log UI scenarios for current vim bindings and selection behavior. — Done: b888574. Refs: 16d725e. (B1245)
- [x] Pin queued Plain renders across echo/response ordering. — Done: containing commit. Refs: 555560a. (B1250)
- [x] Phase 2 gate — Declare a rich client in editor-bridge commit tests. — Done: 5da5d86. Refs: eb8bb1e. (B1254)
- [x] Fix the flaky `tests/plain_draft.rs` suite (commit menu never offered). Refs: bfb60d1, f53b4b6, d70ee3c. (B1256)
- [x] Repair the five failing UI scenarios (`vim` profile and `process-log`). — Done: f10d8a0. Refs: 5da5d86, 2e29535, b888574, 1fc9fa3, a8bd4e0, 8202c9e, adac686. (B1271)
- [x] [P1] Reject ambiguous logical recording inputs — Done: 9081251. (B1279)
- [x] [P1] Restore the stage identity matrix test registration — Done: containing commit; 009e610. (B1289)
- [x] [P2] Point extracted rich-client links back to the client specification — Done: afa7ec1. (B1299)
- [x] R-PLAIN-EDIT-EOF-RISK — Done: investigation complete with this evidence boundary and residual echo uncertainty; sprint-owned Done note retained. (B1308)
- [x] R-T020-STALE — Done: `3e29d72`. Refs: 69bd3dd, 9a8372a. (B1310)
- [x] R-T021-ATTACH — Done: `69bd3dd`. (B1318)
- [x] R-T021-DRAFT — Done: `69bd3dd`. Native Confirm/Abort follow-up in [evidence](docs/evidence/plain-helix.md#follow-up-verification-after-lifecycle-fixes). (B1320)
- [x] R-PHASE6-ORACLES — Done: `a2c1ae3`. (B1322)
- [x] R-PHASE6-CLEANUP — Done: `be609d5`. Non-Linux fallback unverified. (B1323)
- [x] BUG-PHASE6-HOOK — Done: `67cc416`. Refs: 9ba3c0d, 1ccb038adc2b9e9633f394ca1fea789aed2eabf8. (B1324)
- [x] [P0] Retain VSCode draft text until Abort is positively acknowledged Refs: 3125ee6, 275bcc2, 5739f0c, e627b7a. (B1328)
- [x] [P1] Bind todo saves to the exact rebase operation instance Refs: 9a1b69e. (B1339)
- [x] [P1] Serialize todo replacement with the Git lock path Refs: 9cc6a17. (B1348)
- [x] [P1] Batch todo object resolution into one Git invocation (B1356)
- [x] Todo batch response correctness Refs: 972148b. (B1358)
- [x] Todo batch single-process evidence Refs: 955a8b7. (B1360)
- [x] [P1] Load each todo view exactly once and surface factory failures Refs: 48f17a9. (B1368)
- [x] [P1] Restore scoped Makefile ownership without discarding external work Refs: a3ae47f, 7ac0630, a8089a3, 130f8eb, 2b68dda. (B1377)
- [x] [P1] Check Unix executable access instead of any execute bit Refs: 89979d8. (B1385)
- [x] [P1] Record truthful VER3c completion evidence Refs: 473b1de, a3ae47f, 7ac0630, 9daa386, 14e986a, 6954cd7. (B1393)
- [x] [P1] Resolve the real Git test binary through PATH Refs: a242deb. (B1401)
- [x] [P1] Record reproducible PATH-runner verification Refs: 0122a77, aeed37f, 14e986a, 6954cd7. (B1409)
- [x] [P1] Discover optional VSCode hosts through PATH — Done: `aeed37f`. Refs: 14e986a, 6954cd7. (B1417)
- [x] [P2] Exercise Abort synchronization failure and outstanding confirmation directly Refs: 275bcc2. (B1425)
- [x] [P2] Replace the retired Abort envelope in the current client contract Refs: cc73c59. (B1433)
- [x] [P2] Restore profile coverage for contextual todo commands Refs: 0a98afe, dfd6415. (B1441)
- [x] [P2] Pin the VSCode adapter version to the server package version Refs: dfd6415. (B1450)
- [x] [P2] Make the portability audit claims self-consistent Refs: 50f6aac. (B1458)
- [x] [P2] Retire the source-spelling portability checker Refs: aeed37f. (B1466)
- [x] [P2] Delete the stale temporary handoff transcript — Done: containing commit. Refs: 326cced, 130f8eb. (B1475)
- [x] [P2] Return deferred sibling refreshes from session prompt closes. — Done: containing commit. Refs: 7fae07c. (B1484)
- [x] [P2] Resume deferred sibling refreshes when a prompt or its origin closes. — Done: containing commit. Refs: ff49240. (B1487)
- [x] [P2] Verify Plain close routing of resumed prompt refreshes. — Done: 16d725e. Refs: ee220d1. (B1489)
- [x] [P1] Guard delayed Plain draft delivery failures against accepted or superseded deliveries. — Done: containing commit. Refs: bfb60d1, 67db167. (B1495)
- [x] [P1] Match empty-secret cancellation in the shared Rust prompt model. — Done: containing commit. (B1499)
- [x] [P1] Keep repaired Plain prompt action generations current. — Done: containing commit. (B1500)
- [x] [P2] Resume deferred sibling refreshes after document prompts. — Done: `15a7158`. Refs: 53de88a. (B1505)
- [x] [P1] Report a rejected Plain commit draft edit. (B1506)
- [x] [P1] Reconcile relocated architecture with the current wire contract. — Done: `e666f27`. (B1508)
- [x] [P1] Make bare docs-check validate every maintained reference. — Done: `0474efc`; evidence: `9df2075`. (B1509)
- [x] [P2] Stop calling the resolved config-directory choice open. — Done: containing commit. (B1510)
- [x] [P1] Route Plain draft-sync failures through standard notifications, not `view/effects` — Done: `5fb7977`. (B1512)
- [x] [P2] Document the registry-lock-during-emit invariant behind the detached refresh thread — Done: `e70f748`. (B1519)
- [x] [P1] Bind prepared rebase launch to the captured HEAD attachment Refs: 7f5f74f. (B1526)
- [x] [P1] Make prepared-plan edit failures transactional Refs: 7f5f74f. (B1534)
- [x] [P1] Refuse unsupported prepared message-edit actions before launch (B1542)
- [x] Reject prepared message actions at the model boundary. Refs: efa984d. (B1549)
- [x] Verify prepared message refusals through server submit. Refs: 09fbbda. (B1551)
- [x] [P2] Assert native cancellation invariants before the next rebase starts Refs: 7be7ff2. (B1554)
- [x] [P2] Reconcile focused version-startup documentation with the shared runner Refs: 27ad2de. (B1562)
- [x] [P1] Keep deferred repository ownership until the worker exits — Done: `09119a7`; reopened publication/progress boundary corrected after `1e50954`. Refs: b097155, 2001ec405e5501ffca40031244bcca7be46a0c8e. (B1570)
- [x] Deferred reservation and Rich routing regression. (B1571)
- [x] Plain deferred mutation refusal regression. (B1572)
- [x] [P1] Verify the complete autosquash marker subject before replay Refs: 3274dbf. (B1588)
- [x] [P1] Preserve operation cancellation across the marker-to-rebase boundary (B1597)
- [x] Persistent cancellation intent regression. Refs: 1ccb038. (B1598)
- [x] Post-marker cancellation regression. Refs: 1ccb038. (B1600)
- [x] [P1] Restore the non-Unix scoped-editor compilation boundary Refs: d7b656f. (B1608)
- [x] [P2] Stage synchronized changes for only the addressed document (B1616)
- [x] Addressed-document staging structure. (B1617)
- [x] Real draft multi-document isolation regression. Refs: 46322ce. (B1618)
- [x] [P1] Guard every effective Plain repository mutation during deferred work — Done: containing commit; d83fcd2. (B1626)
- [x] [P1] Preserve native teardown identity when owned processes survive — Done: 0c667d1. (B1636)
- [x] [P1] Honor ref-wide logs when the current branch is unborn — Done: `00ef0f4`. (B1645)
- [x] [P1] Reject contradictory merge filters at the Git boundary — Done: `0384b01`. (B1654)
- [x] [P1] Enforce the exact bounded worked-exchange artifact set — Done: `255f2e9` (implementation), `e302639` (completion). (B1663)
- [x] [P2] Align UI matrix documentation with opt-in execution — Done: `04b4834`. (B1690)
- [x] [P3] Remove the duplicate tempfile dependency declaration Refs: 6c00f60. (B1701)
- [x] Documentation cleanup — Compact capability and recorder completion records. — Done: containing commit. (B1711)
- [x] Documentation cleanup — Compact legacy completion records. — Done: containing commit. (B1712)
- [x] Documentation cleanup — Condense completed-task records. — Done: containing commit. (B1713)
- [x] Documentation cleanup — Condense obsolete history and preserve live work. — Done: containing commit. (B1714)
- [x] Documentation cleanup — Restore moved completion records. — Done: `16f4d11`. (B1715)
- [x] Documentation cleanup — Finish queue navigation and preservation audit. — Done: `2e051e3`. Refs: 16f4d11. (B1716)
- [x] Documentation cleanup — Add focused documentation verification. — Done: `93bb9ab` (Makefile ownership correction `4c83d0d`). (B1721)
- [x] Documentation cleanup — Move decisions and glossary reference. — Done: `3186a69`. (B1722)
- [x] Documentation cleanup — Move core design reference. — Done: `9f8f28f`. (B1723)
- [x] Documentation cleanup — Move workflow design reference. — Done: `338b9de`. (B1724)
- [x] Documentation cleanup — Move expanded design reference. — Done: `338b9de`. (B1725)
- [x] Documentation cleanup — Move architecture and glossary reference. — Done: `338b9de`. (B1726)
- [x] Documentation cleanup — Move protocol reference. — Done: `0d663c7`. (B1727)
- [x] Documentation cleanup — Preserve inline-code heading anchors in link checks. — Done: `7d4bee4`. (B1728)
- [x] Documentation cleanup — Move keymaps and view reference. — Done: `e4f259d`. (B1729)
- [x] Documentation cleanup — Move testing and open questions reference. — Done: `4537ef1`. (B1730)
- [x] Documentation cleanup — Finish maintained reference navigation. — Done: `d9ea816`. (B1731)
- [x] Documentation cleanup — Move durable reference material into docs. — Done: `d9ea816`. (B1732)
- [x] Documentation cleanup — Repair recent review completion pointers. — Done: `f13dbc1`. (B1733)
- [x] Documentation cleanup — Repair older review completion pointers. — Done: `e464429`. (B1734)
- [x] Documentation cleanup — Remove resolved review and checkpoint narratives. — Done: `3ec4751`; provenance correction: `f13dbc1`, `e464429`. (B1735)
- [x] Documentation cleanup — Compact documentation completion records. — Done: `a45fcde`. (B1736)
- [x] Documentation cleanup — Compact T006 and T007 completion records. — Done: `bceb82d`. (B1737)
- [x] Documentation cleanup — Compact formatter completion records. — Done: `eb87a14`. (B1738)
- [x] Documentation cleanup — Compact quirk completion records. — Done: `5f5f308`. (B1739)
- [x] Documentation cleanup — Compact typed-capability completion records. — Done: `3787c66`. (B1740)
- [x] Documentation cleanup — Compact recorder and migration completion records. — Done: `408148f`. (B1741)
- [x] T006.1a Refs: 1570ec0. (B1742)
- [x] T006.1b Refs: 2e60210. (B1743)
- [x] T006.1 Refs: 2e60210, 1570ec0. (B1744)
- [x] T006.2a1 Refs: 2e1f392. (B1745)
- [x] T006.2a2 Refs: 219f8e8, 2e1f392. (B1746)
- [x] T006.2a Refs: 219f8e8, 2e1f392. (B1747)
- [x] T006.2b1 Refs: 27c8a70790926b0ebdd6c807db9977b477cc9c90, 219f8e8. (B1748)
- [x] T006.2b2 Refs: 96504c60fde13587bb69f38e2d66f8dd0758366a, 27c8a70. (B1749)
- [x] T006.2b Refs: 96504c60fde13587bb69f38e2d66f8dd0758366a. (B1750)
- [x] T006.2 Refs: 96504c60fde13587bb69f38e2d66f8dd0758366a. (B1751)
- [x] T006.3a Refs: 53483b9. (B1752)
- [x] T006.3b Refs: a4578fe956998619de708c2aee8ee93da20a17dc, 7e4c26cf15b95c3e9715b45b3876e4fa92c05f58. (B1753)
- [x] T006.3c — Done: source-only provenance `e50d516:TASKS.md`; verified at `a4578fe`; dynamic IDs, cancellation, empty/default/custom/secret cases. (B1754)
- [x] T006.3 — Done: source-only provenance `e50d516:TASKS.md`; original clause 5. (B1755)
- [x] T006.4a Refs: cf74f8911d474b5f63c5def41b10864248c8f67e, 158692e8c5e09ea3f24c0daf225fcf977152f1ef. (B1756)
- [x] T006.4b — Done: source-only provenance `e50d516:TASKS.md`; verified at `cf74f89`. Refs: 864f58c, 37fb07d. (B1757)
- [x] T006.4 — Done: source-only provenance `e50d516:TASKS.md`; original clause 6 and final audit. (B1758)
- [x] T007.1 — Done: source-only provenance `e50d516:TASKS.md`; verified on `70eca6154ce29951c05e1f5ae632cfd146e8e6b8`; original clauses 1–3, no executeCommand declaration requirement. (B1762)
- [x] T007.1n — Done: source-only provenance `e50d516:TASKS.md`; `make plain-neovim` on `1d0422e9fa600c50b5015de4798b3269b856779d` passed; both variants prove declaration absence, action/render/Git state and cleanup. (B1763)
- [x] T007.2a — Done: source-only provenance `e50d516:TASKS.md`; `make test-rust FILTER="--test plain_render"` on `53483b9` passed. (B1764)
- [x] T007.2b1 — Done: source-only provenance `e50d516:TASKS.md`; focused Rust suite passed; real-server silent-stale success without echo triggers fresh edit; one-second grace and lifecycle guards covered. (B1765)
- [x] T007.2b2 Refs: 5209f6e76623db5965983b202f0708fa8e09eb6e, 33230ec. (B1766)
- [x] T007.2b Refs: 5209f6e76623db5965983b202f0708fa8e09eb6e. (B1767)
- [x] T007.2 Refs: 5209f6e76623db5965983b202f0708fa8e09eb6e. (B1768)
- [x] T007.3 Refs: 338b9de. (B1769)
- [x] T007.4a1 — Done: `0e9223a`. (B1770)
- [x] T007.4a2 — Done: `c3f5a422c6c1eac2066c44ee815f66ab392c3105`. (B1773)
- [x] T007.4a — Done: `c3f5a422c6c1eac2066c44ee815f66ab392c3105`; enforcing scenario recorded under T007.4a1–T007.4a2. (B1776)
- [x] T007.4b1 — Done: `8dd8bcf`. (B1777)
- [x] T007.4b2 — Done: `2264616`. (B1780)
- [x] T007.4c1 Refs: cd2d152. (B1783)
- [x] T007.4b — Done: `2264616`; enforcing snapshot and lifecycle checks recorded under T007.4b1–T007.4b2. (B1785)
- [x] Roadmap reconciliation — Restore one authoritative queue. — Done: `6d08d85`. (B1786)
- [x] Roadmap reconciliation — Index individual unfinished numbered tickets. — Done: `6d08d85`. (B1788)
- [x] Roadmap reconciliation — Refresh protocol and evidence guidance. — Done: `6d08d85`. (B1790)
- [x] Roadmap reconciliation — Route active verification through Make. — Done: `6d08d85`. (B1792)
- [x] Investigate and rationalize trace/capture test infrastructure. — Done: `5025937`. [Fixture guide](tests/fixtures/lsp_traces/README.md), [UI test plan](ui-test-plan.md). (B1795)
- [x] Autofix staged formatting through shared hook and Make commands. — Done: `9702973`; final index-publication fix `fa5c3de`. [Formatting guide](docs/FORMATTING.md). (B1797)
- [x] Remove permanent formatter tests and retain one-time verification. Refs: bceb82d. (B1800)
- [x] T120.1 (B1806)
- [x] T120.2 (B1807)
- [x] T120.3a (B1808)
- [x] T120.3a1 (B1809)
- [x] T120.3b (B1810)
- [x] T120.3 Refs: bceb82d. (B1811)
- [x] Record formatter pre-commit hooks and resolve reported Rust formatting errors. (B1815)
- [x] T005.1 Refs: eb87a14. (B1818)
- [x] T005.2a Refs: eb87a14. (B1819)
- [x] T005.2b1 Refs: eb87a14. (B1820)
- [x] T005.2b2a Refs: eb87a14. (B1821)
- [x] T005.2b2b Refs: eb87a14, d2faf01. (B1822)
- [x] T005.2b2 Refs: eb87a14. (B1823)
- [x] T005.2b Refs: eb87a14. (B1824)
- [x] T005.2 Refs: eb87a14. (B1825)
- [x] T005.3a Refs: eb87a14, f224f94. (B1826)
- [x] T005.3b — Done: `65ee6d9`. (B1827)
- [x] T005.3c — Done: `6c95b50`. Refs: 65ee6d9. (B1828)
- [x] T005.3 Refs: eb87a14. (B1829)
- [x] Record the approved judgment-based review policy and clarify current phase boundaries. (B1830)
- [x] Limit local verification to Linux and retain cross-platform CI follow-ups. (B1831)
- [x] Update execution limits and future reviewer selection. (B1833)
- [x] T004.final Refs: 5f5f308. (B1834)
- [x] T004.7 Refs: 5f5f308. (B1836)
- [x] T004.7a Refs: 5f5f308. (B1838)
- [x] T004.7b — Done: `b89bf60`; review correction: `78b6f49`. (B1840)
- [x] T004.6 Refs: 5f5f308. (B1842)
- [x] T004.6a Refs: 5f5f308. (B1844)
- [x] T004.6b Refs: 5f5f308. (B1846)
- [x] T004.1 Refs: 5f5f308. (B1848)
- [x] T004.2 Refs: 5f5f308. (B1850)
- [x] T004.2a Refs: 5f5f308. (B1852)
- [x] T004.3 Refs: 5f5f308, 7678242. (B1854)
- [x] T004.3a Refs: 5f5f308. (B1856)
- [x] T004.3b Refs: 5f5f308. (B1858)
- [x] T004.4 Refs: 5f5f308. (B1860)
- [x] T004.4a Refs: 5f5f308. (B1862)
- [x] T004.4b Refs: 5f5f308. (B1864)
- [x] T004.4b1 Refs: 5f5f308. (B1866)
- [x] T004.4b2 Refs: 5f5f308. (B1868)
- [x] T004.5a Refs: 5f5f308. (B1870)
- [x] T004.5b — Done: `3627f46` (implementation: `b16685d`, `f6a859d`). (B1872)
- [x] T004.5b1 — Done: `b16685d`. (B1874)
- [x] T004.5b2 — Done: `3627f46`. (B1876)
- [x] T004.5b2a — Done: `f6a859d`. (B1878)
- [x] T004.5b2b — Done: source-only `5f5f308:TASKS.md`; native evidence provenance retained. (B1880)
- [x] T004.5c — Done: `b5eb865`. Producing-command/native limits remain above. (B1882)
- [x] T002 — Done: `77bc34d`; portable logger `e34ce5a`/`f7d4bea`; Unicode redaction `6dd4f7e`. Refs: 61370ad, 83f79f5, 8476a8e. (B1884)
- [x] T003.1 Refs: 3787c66. (B1885)
- [x] T003.2 Refs: 3787c66. (B1887)
- [x] T003.3 Refs: 3787c66. (B1889)
- [x] T003.2a Refs: 3787c66. (B1891)
- [x] T003.4 Refs: 3787c66, 83f79f5, 15a8f63, 1563bbb. (B1893)
- [x] T001.5b2 Refs: 3787c66. (B1896)
- [x] T001.5c Refs: 3787c66, 2ff1ee7. (B1897)
- [x] T001.4b Refs: 3787c66. (B1898)
- [x] T001.4c Refs: 3787c66. (B1899)
- [x] T001 — Done: `d12d5a4`. Refs: cbec0bf. A12's old ownerless wording superseded by its retained closure below. (B1900)
- [x] Retained gap — A12 real-dispatch Git failure handling. Refs: da4d422, d04dfa2, 4395f73, 5bb9903, 6688ed1. (B1902)
- [x] Retained decision — R-NAV. Refs: da4d422. (B1904)
- [x] T001.3b Refs: da4d422. (B1905)
- [x] T001.4a (B1906)
- [x] T001.5a Refs: 3787c66. (B1907)
- [x] T001.5b1 Refs: 3787c66. (B1908)

## Resolved review and bugfix index

Historical source-only records retain that classification; no source SHA below is reclassified as implementation Done.

- [x] T006 gate — Wire shared editor models into all fake-editor consumers. Refs: d9ea816, 2e29535. (B1915)
- [x] Checkpoint — Revalidate VSCode regressions without overlapping server rebuilds. Refs: d9ea816. (B1916)
- [x] Checkpoint — Restore full regressions after strict wire migration. — Done: `d996e19`. (B1917)
- [x] Checkpoint — Establish the staged-successor failure cause. Refs: d9ea816. (B1918)
- [x] Checkpoint — Align staging-cursor regression with the source-successor contract. Refs: d9ea816. (B1919)
- [x] Checkpoint — Restore log-menu key routing and native passthrough. Refs: d9ea816. (B1920)
- [x] Checkpoint — Restore full Git-backend hunk comparisons. Refs: d9ea816. (B1921)
- [x] Checkpoint — Resolve Rust formatting-gate drift. Refs: d9ea816. (B1922)
- [x] [P1] Correlate scripted prompt responses with real server replies Refs: d9ea816, 2e29535. (B1923)
- [x] [P1] Enforce static keymap delivery in the strict checker Refs: d9ea816, 02b7d19. (B1924)
- [x] [P2] Put metadata-only rich rendering in the shared contract vectors Refs: d9ea816, 43480bc. (B1925)
- [x] [P2] Refresh the T006 accumulated-work gate totals Refs: d9ea816, 2e29535. (B1926)
- [x] [P1] Pin Neovim stale-apply entry ordering Refs: d9ea816, 6c95b50. (B1927)
- [x] [P1] Prove the real Plain apply pipeline becomes ready Refs: d9ea816, e46632b. (B1928)
- [x] [P2] Repair T005 completion provenance and open-phase wording Refs: d9ea816, ae5b78d. (B1929)
- [x] [P2] Make capability-match appendix generation exhaustive Refs: d9ea816, 5e61d9f. (B1930)
- [x] [P1] Make the secret-only declaration test exercise prompt capabilities — Done: `78b6f49`. (B1931)
- [x] [P1] Remove retired capability modes from current client contracts — Done: `db288e4`. (B1932)
- [x] [P1] Report checker failures without calling them findings Refs: d9ea816. (B1933)
- [x] [P1] Model the real VSCode entry graph in Knip Refs: d9ea816. (B1934)
- [x] [P1] Check Lua against Neovim's actual LuaJIT standard Refs: d9ea816. (B1935)
- [x] [P1] Complete marker ownership and baseline reconciliation. Refs: d9ea816. (B1936)
- [x] [P1] Give every retained hygiene finding an owner or reason Refs: d9ea816. (B1937)
- [x] [P2] Make hygiene tests portable and prove checker working directories — Done: `caf5588`, `0550f36`, `33d0554`. (B1938)
- [x] [P1] Finish native Helix smoke shutdown and recipe verification. Refs: d9ea816. (B1939)
- [x] [P1] Make the Helix capture recipe preserve verifiable evidence Refs: d9ea816. (B1940)
- [x] [P2] Enforce trace and initialize-preset consistency Refs: d9ea816. (B1941)
- [x] [P2] State that five T034 action classes remain missing Refs: e34ce5a. (B1942)
- [x] [P1] Serialize shared wirelogs on every tested platform Refs: d9ea816. (B1943)
- [x] [P1] Open shared logs with Windows lock-compatible access Refs: d9ea816. (B1944)
- [x] [P1] Make identity replacement byte-index safe Refs: d9ea816. (B1945)
- [x] [P1] Retain the claimed bundled-Neovim native edit Refs: d9ea816. (B1946)
- [x] Checkpoint review — Opt-in wire recorder and completed documentation cutover. Refs: d9ea816. (B1947)
- [x] [P2] Correlated secret-prompt errors are recorded without redaction. Refs: d9ea816. (B1948)
- [x] Checkpoint review — Git mutation identities and structured successors. Refs: dd92f90cdb11d827606a689a88a280f480d7508d. (B1949)
- [x] Checkpoint review — Plain protocol versions and process ownership. Refs: d9ea816. (B1950)
- [x] Checkpoint review — Roadmap migration preserves open requirements. Refs: d9ea816. (B1951)
- [x] [P2] A12 is marked complete in the queue index while its sole retained issue remains open. Refs: d9ea816. (B1952)
- [x] [P1] Enforce exact action-fixture metadata and source-row mappings Refs: d9ea816. (B1953)
- [x] [P1] Add a self-contained Neovim selection-and-paste recipe Refs: d9ea816. (B1954)
- [x] [P2] Harden the Helix action recipe cleanup. Refs: d9ea816. (B1955)
- [x] [P2] Harden the Neovim selection recipe cleanup. Refs: d9ea816. (B1956)
- [x] [P2] Repair rapid-recipe syntax without claiming cleanup acceptance. Refs: d9ea816. (B1957)
- [x] [P2] Harden the Neovim rapid-repair recipe cleanup. Refs: d9ea816. (B1958)
- [x] [P2] Make capture-recipe cleanup survive failures Refs: d9ea816. (B1959)
- [x] [P1] Make the phase-gate schema match the actual queue Refs: d9ea816. (B1960)
- [x] [P2] Restore hygiene-test environment isolation Refs: d9ea816. (B1961)
- [x] [P2] Remove duplicate Linux verification policy Refs: d9ea816. (B1962)
- [x] [P2] Remove stale planned-navigation marker claims Refs: d9ea816. (B1963)
- [x] [P1] Finish terminating the owned Helix recipe server Refs: d9ea816. (B1964)
- [x] [P2] Isolate Helix recipe repository setup from host Git config Refs: d9ea816. (B1965)
- [x] [P1] Finish migrating shared test wire fixtures to the strict schema Refs: d9ea816. (B1966)
- [x] [P2] Test duplicate entries in every typed capability set Refs: d9ea816. (B1967)
- [x] [P2] Anchor the fake StaticFeature APIs to real VSCode evidence Refs: d9ea816. (B1968)
- [x] [P1] Remove retired fields from the public rich-protocol contract Refs: d9ea816. (B1969)
- [x] [P2] Pin the bundled Neovim declaration at its producer boundary Refs: d9ea816. (B1970)

## 3. Design decisions (expanded)

[Maintained design](docs/ARCHITECTURE.md#3-design-decisions-expanded); general conventions in [AGENTS.md](./AGENTS.md).

## 4. Protocol reference

[Maintained protocol](docs/PROTOCOL.md#4-protocol-reference).

## Keymaps and views

- [Keymap profiles](docs/KEYMAPS.md#5-default-keymaps-profiles)
- [View catalog](docs/VIEWS.md#6-view-catalog)

## 8. Testing strategy

[Maintained strategy](docs/TESTING.md#8-testing-strategy).

## 9. Open questions

[Maintained questions](docs/OPEN_QUESTIONS.md#9-open-questions); this audit does not resolve policy questions.
