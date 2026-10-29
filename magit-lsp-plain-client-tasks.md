# magit-lsp Plain-client re-architecture: task list

*Formerly "Level 0". Current phase working list linked from canonical TASKS.md §7, ahead of "Feature: release prerequisites". TASKS.md remains the canonical roadmap and design; this file preserves this phase's ticket boundaries, acceptance gates, dependencies and verification. Paths marked (new) are proposals; agents may rename files but must keep task boundaries. Updated 29 October 2026.*

**Status:** R-PLAIN-DOC is complete in TASKS.md. This file is a concise working
index; TASKS.md §7 is the canonical scanner-visible queue and this file retains
ticket bodies, decisions, risks and deferred notes. All implementation, spike,
harness, phase and release work remains pending unless explicitly marked
otherwise in TASKS.md. R-EDIT-REAL's production cutover is implemented with focused
evidence; its broader parent acceptance remains incomplete. R-ARGS and the stated
R-4.1-head-edit criteria (including reset-author review correction and both-client
native evidence) are complete; see TASKS §7 and
`docs/evidence/commit-head-operations.md`.
**Authorization (2026-10-29):** the user explicitly chose “Defer and continue
Plain”: defer the known R-EDIT-REAL Rust errors/log_commands/views and UI
log-select/log-view/process-log gate failures, leave R-EDIT-REAL unchecked, and
advance to R-PLAIN-HARNESS. This is scheduling authorization, not acceptance of
the failed gates or release acceptance. HARNESS is now complete as test infrastructure;
R-PLAIN-PREREQ's scoped acceptance and R-PLAIN-SYNC are complete; R-PLAIN-CORE is the first unchecked active feature task. The scoped R-EDIT-REAL lifecycle limits, native semantic,
recovery and two-repository evidence, and finite uncovered criteria remain
recorded below. The ring's pinned parity differences remain documented policy,
not a change request or a missing source-research blocker.
Decision: assume ordinary `didClose` confirms saved-file content by default;
standard LSP lacks run identity and reliability/late-event limitations are
accepted assumptions, not protocol guarantees. VS Code is the known exception.
The 2026-10-29 native spike observed Codium did not emit `didClose` after
ordinary real-file tab close within five seconds in either host, while Neovim
passed historically (not rerun). Subsequent core tab fallback/dirty-choice evidence passed; the
2026-10-29 clean lifecycle observer follow-up found zero qualifying draft closed
events for native close-window/reload/same-window folder-change. These scoped
observations do not complete step 0 or the parent; see
`docs/evidence/real-file-client-spike.md`.

## 1. Goal and resolved decisions

**Goal:** any general-purpose LSP client that implements the standard methods listed in the Plain-mode spec section can drive status, stage/unstage/discard, menus, log navigation and commit with no magit-specific editor code. The bundled Neovim and VS Code clients keep the custom `view/*` path.

| Topic | Decision |
|---|---|
| Naming | "Plain clients" / "Plain mode" = a general LSP client, spec only. Presentation adapter name: `plain`. |
| Client model | Spec-only. No editor-specific features (including Helix expansions such as `%{buffer_name}`). Helix is a reference configuration, not a target. |
| Architecture | One semantic dispatcher; two presentation adapters: `customRender` (existing) and `plain` (new). |
| Adapter selection | Sole selector is `capabilities.experimental.magit`: absent Plain; present rich. `initializationOptions` carries profile only. |
| Sync | Server advertises Incremental and accepts ranged and rangeless changes through one document store. |
| Repository resolution | Workspace root plus document path. No launcher. Ambiguity renders a choose-repository view; the server never guesses among several repositories for a mutating command. Presentation documents are excluded from Untracked. |
| VCS-neutral interactions | Presentation and standard-LSP transport are VCS-neutral. Server primitives for serving/stacking concrete views, stable-identity single/multi-selection with caller restoration, provider-file message-edit sessions with finish/cancel callbacks, and semantic action dispatch are reusable. Git providers own discovery, mutation validation, COMMIT_EDITMSG paths, helper lifecycle and completion lookup. Require a minimal fake-provider independence test; no production jj backend, VCS trait hierarchy, or speculative wire protocol. |
| Action targets | Content-addressed: existing node ID plus a hash of what the action will mutate, validated at execution. Stale means refresh and no mutation. |
| Rendering | Fully expanded by default. Sections above a size cap auto-collapse with an expand action. Toggle/expand are code actions. `foldingRange` is still served. |
| Prompts | One document-backed prompt renderer for confirm/text/choice/multiChoice, with editable fields and `textDocument/completion`. |
| Capability model | Declared once per effect type and prompt kind (section 2). No per-command annotations. |
| Commit editing (implemented; Plain presentation remains target) | Git's real `COMMIT_EDITMSG` template and canonical file URI/runId. Default didClose confirms saved-file text by accepted assumption, not run-identity/reliability proof. VS Code uses owned last-matching-tab all-group run-aware `magit.command` confirmation, ignoring language-change didClose. Explicit confirm validates synchronized exact version and writes it back. Native dirty Save saves/confirms, Discard submits prior saved text, Cancel remains open. Explicit abort cancels. |
| Message ring | Saved eligible finish text and best-effort abort file text feed the bounded server ring, not didSave. Retain existing eligibility, dedup and caps; pinned Magit source comparison documents differences in effective-message normalization, global dedup, scope and caps. These are documented policy differences, not a migration requirement. See `docs/evidence/commit-message-ring-parity.md` and TASKS §3.10. |
| Close without saving | On an amend or reword, committing the unchanged message is Git's own behaviour and is kept. Abort is the explicit way out. |
| History prev/next | Deferred to R-EDIT-HISTORY (server code action issuing a versioned `applyEdit`, identical for every client). |
| Out of scope | Level 1 character input, launcher, diagnostics channel, discard recovery refs, direct `workspace/executeCommand` bindings (arguments come only from code actions), custom URIs, LSP 3.18 virtual content, the grace timer (section 6). |

**Assumptions to confirm (cheap to reverse):**

1. "Pull features out of the client-dependent stage" is read as: prompts, select mode, menus and commit get a Plain-mode implementation before the release sprint via the shared dispatcher.
2. Evidence rule: Plain-mode acceptance is the fake-editor harness plus native runs of a plain Neovim LSP setup (no magit-lsp plugin) through the existing `scripts/ui`. Helix 25.07.1 is a manual smoke checklist, not a harness.
3. BUG-READ-1's minimal first step is pulled forward (R-PLAIN-PREREQ), because `applyEdit` round-trips need an unblocked intake loop. The larger deferred design stays deferred.
4. R-DIFF-SAFETY is pulled forward unchanged as a dependency of R-PLAIN-TARGET.
5. Best-effort saved-file retention at abort is implemented; full Magit parity remains a gated R-EDIT-REAL acceptance requirement.

**Contract status:** Plain presentation and provider-neutral interactions remain
targets. The real-file cutover is implemented in the server and both rich clients;
TASKS §3.10 and CLIENT_SPEC's current real-file section specify today's contract.
Focused production evidence is not parent acceptance or verified Magit parity.

## 2. Plain-mode effect and prompt support (the one capability table)

Commands inherit support from the effects they emit. A CI test (R-PLAIN-PROMPT) runs every registered command against a recording Plain `Ui` and generates the supported/unsupported table.

| Effect / prompt | Plain mapping | Needs client capability | If missing |
|---|---|---|---|
| `message` | `window/showMessage`; detail/process log as a view row | none | n/a |
| `openView`/`pushView`/`popView`/`focusView` | re-render the active logical view in the same document | `workspace.applyEdit` | document inert, readable message |
| `visitFile` | `window/showDocument` (file URI, selection) | `window.showDocument.support` | `Unavailable`; a `path:line` message is not equivalent support |
| `editDocument` (commit message) | `window/showDocument` on Git's `COMMIT_EDITMSG`; if already open, a versioned whole-buffer `applyEdit` with the fresh template first | `showDocument`; `applyEdit` for the refresh | commit-producing commands `Unavailable` |
| `clipboard` | none | none | message showing the text; copy commands `Unavailable` |
| `passThrough`, `quit` | no key capture; `quit` returns to a closed notice | none | n/a |
| progress | `$/progress` | `window.workDoneProgress` | dropped |
| prompt `confirm`/`text`/`choice`/`multiChoice` | document prompt renderer | none | n/a |
| prompt `secret`, askpass | unsupported | none | clean cancel plus message; credential-requiring network operations fail readably |

## 3. Order of work

1. R-ARGS and R-4.1-head-edit (amend, extend and reword) are complete. R-EDIT-REAL parent acceptance remains unchecked and deferred by explicit user authorization while Plain proceeds; it is not a prerequisite to starting R-PLAIN-HARNESS or R-PLAIN-PREREQ. R-PLAIN-DOC is complete. R-EDIT-REAL and R-PLAIN-PREREQ both touch `src/server.rs`; coordinate merges.
2. R-PLAIN-SYNC (after R-EDIT-REAL), then R-PLAIN-CORE.
3. R-DIFF-SAFETY, then R-PLAIN-TARGET. R-PLAIN-REPO after R-PLAIN-CORE.
4. R-PLAIN-RENDER, then R-PLAIN-ACTIONS, then R-PLAIN-STACK, R-PLAIN-VIS and R-PLAIN-PROMPT (parallel), then R-PLAIN-SELECT.
5. R-PLAIN-DRAFT (after R-EDIT-REAL and R-PLAIN-ACTIONS).
6. R-PLAIN-E2E, R-PLAIN-HELIX, R-PLAIN-SPEC, then R-PLAIN-GATE.

R-ARGS is complete and R-4.1-head-edit is complete after both-client native reset-author review; they precede R-EDIT-REAL parent acceptance. The remaining release-slice chain (R-SELECT-OPS, R-EDIT-KINDS, R-OP-STATE, R-TODO, then R-4.1a and R-7.3a) is unchanged and can run in parallel. R-PLAIN-GATE re-runs those slices through Plain mode. The release sprint (R-REL-1 to R-REL-4) starts after R-PLAIN-GATE.

Verification follows AGENTS.md: narrowest relevant check per task; full `make test` at R-EDIT-REAL, R-PLAIN-CORE and R-PLAIN-GATE.

## 4. Tasks

### R-EDIT-REAL: implemented cutover; parent acceptance pending (L)

**Current implementation:** Git supplies comments/status/configured template and
owns hooks, cleanup and commit. `editDocument` announces the canonical file URI,
separate `runId`, exact text/version and `mode:"realFile"`. One bounded full-sync
stream retains live text. `workspace/executeCommand` registers `magit.command`
with one argument `{command:"common.confirm",runId,uri,version,source}`;
`source:"synchronized"` validates the exact latest version and writes it safely;
`source:"saved-file"` reads disk without substituting dirty synced text. Result
`{type:"confirmed",runId,uri,version}` means helper release, not Git success.
Default didClose confirms saved-file text by accepted assumption despite its lack
of run identity/reliability guarantees. VS Code's clientInfo exception detaches
only and uses the actual owned last URI tab's all-group closed event instead.
Registry locking arbitrates terminal/abort outcomes; no new draft protocol exists.

**Historical step 0 evidence (not production acceptance):**
**Scoped follow-up (2026-10-29):** clean permanent draft, isolated normal Codium
hosts: native close-window/reload-window/same-window folder-change produced no
qualifying draft `event.closed` event. External supervisor survived exit/restart;
actual deactivate/reactivate, all-group counts, API/wire logs and native
pre-action screenshots retained and inspected. Core fallback and dirty choices
passed earlier. Only this observer slice is complete; parent/overall step 0
remain pending, without production changes or general lifecycle guarantees.
See the evidence report for exact commands, versions, paths and scope limits;
this supersedes the historical unverified-shutdown statements below only for
those three clean cases.
**Historical spike result:** Codium 1.140.06880 removed real-file tabs without wire
didClose within five seconds and emitted misleading close/open on language change.
Core last-tab fallback and dirty choices passed; scoped clean lifecycle runs
observed zero qualifying closed events. See
[`real-file-client-spike.md`](docs/evidence/real-file-client-spike.md) for limits.
Production migration is now implemented; these spike checks do not complete parent
acceptance or establish general shutdown behavior.

- VS Code: fallback/dirty-choice checks are complete; known exception to default didClose assumption uses verified tabGroups last-matching-tab run-aware executeCommand fallback and ignores language-change didClose.
- Neovim: a buffer with `bufhidden=wipe` and explicit LSP attach; `:q` on a modified buffer is refused natively and a real close sends `didClose`.
- For each client: what `showDocument` does on an already-open file whose contents changed on disk (reload, prompt, or stale text). Git rewrites the template on every commit and the path is reused.
- For each client: what reaches the server after a language-mode change (VS Code `setTextDocumentLanguage` fires document close then open; Neovim and Helix unverified).
- Confirm that rich clients close their own buffer on abort.
- Scoped clean shutdown/reload/workspace-change observation is complete with no qualifying closed event for those cases; its limits are in the evidence report. Keep explicit confirm and abort available. Do not treat standard didClose as run-correlated; explicit run-aware signals validate run/session.

**Target Files:** `src/{protocol,server,editor,commands,session}.rs`, `src/editor/draft.rs`, `lua/magit-lsp/edit.lua`, `clients/vscode/lib/{drafts,edit,edits}.js`, their tests (`tests/editor_drafts.rs`, `editor_bridge.rs`, `server_async.rs`, `tests/test_magit_lsp.lua`, `clients/vscode/test/*`), `tests/ui/scenarios/commit-*.scn`, `TASKS.md`, `docs/CLIENT_SPEC.md`.

**Reference Files:** TASKS §3.10; `src/editor.rs` helper validation; git history before the R-EDIT-3 cutover for the old real-file adapter (reference only, no blind revert).

**Implemented changes (focused slices, not parent checkoff):**
- **Effect/routing:** canonical file URI/runId and registered `magit.command` confirmation as above; explicit abort remains `view/invoke common.cancel` with `args:{runId}`. The general Plain dispatcher is still pending.
- **Run events:**
  - `didSave`: no handler.
  - Shared session/owner/run-aware idempotent terminal path arbitrates saved-file close versus exact-version synchronized-buffer confirmation; the latter has sync/version and writeback barriers. Atomic abort/terminal race arbitration prevents duplicate helper release or commit.
  - `didClose` or verified last-tab close: read saved file text; never use this to confirm unsaved synced text. `common.cancel {runId}` explicitly aborts and is distinct from native Discard-unsaved. Do not delete the file.
  - Disconnect, EOF, shutdown and progress-cancel cancel the run; ring write is best-effort and skipped if the file is unreadable. Unchanged otherwise.
- **Content sources:**
  | Path | Confirmation source |
  |---|---|
  | Saved-file close | Read saved Git file; never substitute unsaved buffer text |
  | Explicit confirm | Exact synchronized buffer version, validated and written back before helper success |
  | Abort | No confirmation; atomic helper abort |
  | VS Code explicit action | Native close/save workflow; source explicitly identified |
- **Dirty actions:**
  | Action | Meaning |
  |---|---|
  | Save/close | Save does not finish; close confirms saved file |
  | Dirty Save | Save, then close confirms saved content |
  | Native Discard-unsaved | Close using prior saved file text |
  | Dirty Cancel | Keep editor open; no confirmation |
  | Cleared and saved empty | Git evaluates effective-empty message and aborts |
  | Explicit Abort | Cancel helper; not a close/discard synonym |
  | VS Code Git Discard | Clear and save; Git evaluates effective-empty message |
- **Git validation:** effective-empty semantics belong to Git, not raw length or unconditional template equality. Preserve comments/status/configured-template semantics and test configured templates.
- **Ring:** retain existing eligibility, dedup and caps; verify parity rather than assuming raw nonempty text is eligible.
- **Removed:** obsolete managed-draft wire APIs/error notification, Neovim close-command interception/custom picker, VS Code writable provider and rich history recall. Document bounds, explicit-confirm version/writeback validation and server ring remain. History bindings are `planned` until R-EDIT-HISTORY.
- **Clients:** Follow upstream [gitEditor.ts](https://github.com/microsoft/vscode/blob/main/extensions/git/src/gitEditor.ts) and [commands.ts](https://github.com/microsoft/vscode/blob/main/extensions/git/src/commands.ts). Distinguish native Discard-unsaved from explicit Abort and VS Code Git Discard-clear/save.
  - Neovim: open the real file in the current window, attach explicitly, set `bufhidden=wipe`. LSP code action `common.confirm` confirms latest synchronized exact-version buffer; close confirms saved file.
  - VS Code: open permanently (`preview:false`, including `showDocument` middleware). Existing language-client `workspace/executeCommand` routes `common.confirm` to registered `magit.command`; native save/close uses the saved-file source. Owned last-tab fallback is implemented; pure-LSP close reliability is not assumed for this client. Never synthesize `didClose`.
  - Abort is `view/invoke common.cancel {runId}` in both.
- **IMM-3:** map "Aborting commit due to empty commit message" to the info message "Commit aborted: empty message".

**Constraints:**
- Git owns hooks and cleanup; never `git commit -F`.
- Keep helper token, owner and run validation, and the entire R-EDIT-2 process lifecycle (process groups, TERM/KILL escalation, reaping, bounded drains, lock warnings, cancel on disconnect), untouched. Only the draft-content layer is removed.
- Status read-only, A29, text mode and askpass are unchanged.
- `didClose` is never abort.
- Delete rather than deprecate; no shims.

**Recovery and remaining gates:** Rejected sync reports a readable message and
retains edits; rejected ordinary close re-presents the same pending run and
text/version, restoring editable buffer/sync/keymaps for repair/retry. No helper
release on unreadable/unsafe saved file or synchronized version mismatch. No
transport shutdown/deactivation confirmation. Scoped clean lifecycle evidence
does not establish general dirty/multi-root/cross-build behavior.

Focused production evidence is recorded in TASKS §3.10, including Rust/Lua/VSCode
integration and native Codium runs. HEAD-edit is no longer a prerequisite; its stated
criteria are complete in both clients (see TASKS §7 and `docs/evidence/commit-head-operations.md`). The canonical Neovim scenario is now
`commit-real-file.scn` (both profiles at 100x30 and 80x24), retaining empty/template/
save-alone, dirty refusal/:wq, prior-saved :q!, exact-state abort and same-run
rejected-close recovery. Old close/history scenarios are removed; cancellation
scenarios remain. **Scoped native checklist:** multi-root isolation passed the
dedicated Codium EDIT_ONLY slice below; same-URI fresh-run/stale-run handling is
covered by `clients/vscode/test/e2e/real-file-lifecycle.js`. Rust unreadable saved
file and exact-version rejection/recovery are covered by
`tests/editor_drafts.rs::saved_close_reads_native_safe_save_and_retains_helper_on_unsafe_read`
and `confirm_requires_exact_current_version_and_writes_without_a_checkpoint`,
with additional client evidence below. Native saved-close/dirty choice is covered by
`tests/ui/scenarios/commit-real-file.scn` and
`clients/vscode/test/e2e/real-file-native.js`. Semantic duplicate/late protocol
probes are covered, but are not evidence of duplicate native events; do not
fabricate such events. Native nonlast/last-tab behavior and unsafe-read recovery
are covered. Broader dirty shutdown/reload/workspace-change/cross-build and
last-window exit remain unverified; scoped clean Codium observer results and
Neovim extra-window close evidence are not universal lifecycle guarantees.
Ring differences documented in `docs/evidence/commit-message-ring-parity.md`
remain documented differences under the retained policy, not an instruction to
change implementation. Full-phase gates are not green. Historical
phase failures: `make test` failed at
`tests/errors.rs::normal_and_piped_apply_failures_keep_original_details` line 477
(`build/test-logs/rust.P4rm85`); no-fail-fast Rust also failed `errors`,
`log_commands`, and `views` (`build/test-logs/real-file-phase-rust.log`). UI gate42
was 42 PASS/6 FAIL (`build/test-logs/ui-test.R6BK0X`): log-select, log-view,
process-log and three obsolete commit scenarios. The three commit scenarios
subsequently passed focused repair; the other UI failures remain out of scope.
Fresh `make ui-tour` succeeded; inspected `menu-commit`, `clean-commit-menu` and
`clean-commit` excerpts in `build/ui/tour.md`. This is tour evidence only; parent
remains unchecked and no full gate is green.

**Focused rejection/retry evidence (2026-10-29):** the Neovim canonical scenario
already covers unsafe-file same-run recovery. Normal-host Codium EDIT_ONLY now
also covers a wrong-version protocol rejection with exact HEAD/index/worktree/
disk/live-text invariants followed by exact-saved-version native close, and an
actual native unsafe-file close rejected by the secure read. Restoring the regular
target and reopening/reclosing a real owned tab retains the same pending run and
commits the meaningful saved message once, leaving a neighboring sentinel intact.
The dangling symlink deliberately separates server secure-read validation from
native editor following/reloading an external target. Duplicate terminal and stale
old-run confirm/abort requests before and after same-URI reuse are labelled
**protocol probes**, not native duplicate tab events; the new run remains intact
and subsequently commits once. Inspected PNGs and identity-bearing JSON traces:
`/tmp/opencode/vscode-review-VQvOzT/real-file-native-{trace,evidence}.json`,
`real-file-native-4-wrong-version-retained.png`, `real-file-native-5-unsafe-{rejected,recovered}.png`,
`real-file-native-6-stale-probes-retained.png` and commit-result screens.
The preceding unsafe retry timed out because its restored bytes were template-only,
not because ownership blocked retry (the trace already contained two confirmations).
No production adapter change or weakened guards was needed. Focused verification:
`make vscode-test && npm --prefix clients/vscode run test:integration && MAGIT_VSCODE_BIN=/home/william/.nix-profile/bin/codium MAGIT_VSCODE_EDIT_ONLY=1 npm --prefix clients/vscode run test:e2e && git diff --check`.

**Multi-root native isolation: [x] scoped Codium slice complete (2026-10-29).**
Two canonical repository sessions under one unchanged production host retain
simultaneous distinct runIds/URIs/messages. Bidirectional cross-root run/URI
confirmation and independent-connection owner misuse are explicitly semantic
protocol probes, not fabricated native events. All reject without changing
either root's exact HEAD/tree/index/worktree/count/message/status or either
pending draft's disk/live text. Actual saved last-tab close commits A once
(count 7→8); B stays editable/pending and unchanged (count 1). Production Abort
B preserves exact state/file, and actual reopen/late close emits no new
confirmation or commit. Inspected all four two-repository PNGs and persisted
before/after/request JSON in `/tmp/opencode/vscode-review-S81D41/`.
The second-root view/open/session/invoke route was correct: the original clean
Create had no staged delta, so refused before opening any draft/helper. Both
roots now stage distinct deltas. A second workbench LanguageClient collided
with the existing `magit.command` registration; owner probes instead use a
separate labelled stdio connection and await its shutdown/exit. No production
changes, reload/deactivation, synthetic effects/contexts/tab events, sleeps or
deadline increases. No magit-lsp helpers remained. Exact ticket verification
above passed. Parent and broader lifecycle/platform/full gates remain unchecked.

**Native acceptance limits:** Same-URI/late/duplicate
semantic protocol probes, native nonlast/last-tab behavior, and unsafe-read
recovery are already covered; do not fabricate duplicate native events. Preserve
separate shutdown/reload/workspace-change assertions only for specified
clients/platforms; do not infer universal lifecycle guarantees. Existing Rust
coverage is not a substitute for native cases. Exact existing coverage and limits
are listed above.

**Acceptance:**
- Save alone never commits.
- Save then close commits exactly the saved text (assert the HEAD message).
- Close without saving on an amend commits the unchanged message (Git parity).
- Empty or template-only text gives an info message and no commit.
- Abort never commits, leaves the file in place, focuses status, and a late close after abort is ignored.
- The ring receives the file text at finish and at abort, and not at `didSave`.
- Existing disconnect and adversarial tests still pass.
- No `draft/` method or `managedDraft` remains in `src/` or the clients.
- In each client, after `:wq` or closing the commit tab, the status view is clean and shows the new commit.
- Verify explicit confirm against synchronized buffer/version; close confirmation against saved-file text; Discard uses prior saved text and Cancel retains the session. Verify shutdown/reload/workspace-change and whether shutdown emits `event.closed`.

**Docs:** TASKS §3.10 and CLIENT_SPEC describe the implemented real-file contract;
obsolete managed-wire/picker/history designs are superseded, not current claims.
Plain capabilities and broader presentation/dispatcher work remain targets.

**Focused scenario/docs verification:** `scripts/ui run tests/ui/scenarios/commit-real-file.scn && git diff --check`.
**Parent gates (not claimed complete):** `cargo test --test editor_bridge --test editor_drafts --test server_async && make test-lua && npm --prefix clients/vscode run test:integration && npm --prefix clients/vscode run test:e2e && make test`, plus broader native/adversarial acceptance above. Historical full-phase failures and current scoped repair are recorded above; no green full gate is claimed.

### R-EDIT-HISTORY (S, deferred stub)

History prev/next as a server code action issuing a versioned `applyEdit` from the ring, identical for every client. Deferred and not scheduled until separately requested; depends on R-EDIT-REAL and, for Plain clients, R-PLAIN-ACTIONS.

### R-PLAIN-DOC: record decisions and amend the roadmap (S)

- **Acceptance update:** document the adopted experimental capability block and standard-capability matrix in `docs/CLIENT_SPEC.md`; distinguish current bundled wire preferences from the target selector.

**Status: [x] Complete (documentation only).** R-EDIT-REAL production cutover is
implemented; broader parent acceptance remains pending.

- **Target Files:** `TASKS.md`, `AGENTS.md`.
- **Reference Files:** this document; TASKS §0, §3.4, §4.1, §7 (R-4.1a, R-7.3a, R-REL-2, R-REL-3, R-CLIENT-SPEC).
- **Constraints:** docs only. Do not duplicate edits owned by R-EDIT-REAL.
- **Acceptance:**
  - Section 1 decisions (except Commit editing and Message ring, owned by R-EDIT-REAL) added to TASKS §0.
  - §3.4 and §4.1 distinguish current wire behavior from the adopted target block `capabilities.experimental.magit`; keep `profile` in initializationOptions. Do not delete target prompt/placement/editor interaction design.
  - This phase inserted before "Feature: release prerequisites"; R-4.1a and R-7.3a explicitly require acceptance through the Plain harness at R-PLAIN-GATE; R-REL-2 wording becomes "general LSP clients supported in Plain mode per the spec; no first-party promise beyond the bundled clients".
  - AGENTS.md gains the Plain evidence rule from assumption 2.
  - The completed R-CLIENT-SPEC core-payload sub-task (fourth review recorded complete) is ticked.
- **Verify:** `git diff --check`; grep TASKS.md for contradictions with the new decisions.

### R-PLAIN-SPEC documentation design substep

- [x] Drafted the Plain target-design section in `docs/CLIENT_SPEC.md` and
  linked the single effect/prompt capability table here.
- [ ] Source/runtime cross-check against protocol and implementation, harness
  conformance across all seven profiles, native evidence, and Helix smoke remain
  pending. R-PLAIN-SPEC remains unchecked until those later gates are complete.

### R-PLAIN-HARNESS: fake-editor conformance harness (M)

**Completed (2026-10-29):** 12 harness tests cover all seven profiles, lifecycle
operations, negotiated position encoding, 26 sequential ranged changes in one
notification, atomic edit validation and captured emission order. The bounded
real-binary stdio pump acknowledges actual `window/workDoneProgress/create`
requests during successful staging and commit cancellation, awaiting progress
end. Cancelling on begin previously killed setup `git rev-parse`; the test now
waits for the actual commit editor before cancelling. Local scripted applyEdit
and showDocument requests use that same dispatcher/send path and capture actual
pipe writes, but are **not evidence of production Plain server support**.
The fixture is explicitly reconstructed, not a raw Helix capture. Future Plain
server conformance tasks remain unchecked. Verification: 12 harness tests and
23 server_async tests passed, plus `git diff --check`.

- **Target Files:** `tests/common/fake_editor.rs` (new), `tests/fixtures/lsp_traces/` (new), `tests/plain_harness.rs` (new).
- **Reference Files:** `tests/common/session.rs`, `tests/server_async.rs`, the recorded Helix traces from the Plain handoff (§G).
- **Constraints:** test code only; no production test hooks; no sleeps (await message conditions).
- **Acceptance:**
  - A scripted LSP client drives the real binary over stdio and keeps a document model (text, version, negotiated position encoding).
  - It implements `workspace/applyEdit` (versioned; stale rejected with `applied:false` and `failureReason`), `window/showDocument`, and `window/workDoneProgress/create`. Do not attribute reconstructed fixture behavior to Helix evidence.
  - Operations: open, edit, save, close, `code_action(range)`, `execute_command`, `text()`.
  - Quirk profiles, each with a self-test: `decomposed-changes` (a full replace arrives as many incremental changes in one notification), `debounced-didchange`, `reorder-echo` (`didChange` after the `applyEdit` response), `no-documentChanges`, `no-showDocument`, `crlf-normalise`, `utf16`.
  - Trace fixtures come from the user's raw Helix JSONL. If unavailable, reconstruct from the handoff's quoted examples and mark fixtures "reconstructed".
- **Verify:** `make test-rust FILTER="--test plain_harness" && cargo test --test server_async && git diff --check`.

### R-PLAIN-PREREQ: outbound requests, progress gating, minimal BUG-READ-1 (M)

**Complete (2026-10-29):** see TASKS.md §7 for exact evidence and deferred gate limits. Per-repository handles release the global registry before session/Git work; published resync/positional snapshots and prompt routing avoid unrelated-session waits. Watchers are terminal-gated and drained/joined outside their registry, breaking shutdown ownership cycles. Focused verification passed (11 server unit, 27 async, 26 editor bridge, 12 plain harness); blocked-A/same-view resync and independent-B resize/resync/outline/mutation pass alongside outbound/progress and real-file cancellation regressions. No owned magit-lsp processes remained. Full `make test-rust` still fails at the user-deferred errors case; explicit errors/log_commands/views verification reconfirms the five deferred failures (`build/test-logs/rust.rUL2j1`, `rust.puCxlW`). No broad acceptance or larger same-session ack/resize redesign is claimed.

- **Target Files:** `src/server.rs`, `src/session.rs`, `tests/server_async.rs`.
- **Reference Files:** TASKS BUG-READ-1; CLIENT_SPEC progress section.
- **Constraints:** escalate before implementing if this needs a View/`CommandCtx` rewrite or multiple unacked versions per view. The full BUG-READ-1 design stays deferred.
- **Acceptance:**
  - Progress create and `$/progress` are sent only when the client advertises `window.workDoneProgress`. Begin waits for the create response, or progress is dropped. The initialize result stops claiming the capability unless true.
  - `invoke_view`, `key_view` and `set_mode_view` no longer hold `Shared.sessions` across the Git run. The BUG-READ-1 probe is un-ignored and passes. `maybe_test_delay` and `MAGIT_LSP_TEST_DELAY_MS` are removed.
  - A generic outbound-request helper (applyEdit, showDocument, progress create) correlates responses with a timeout and never blocks inbound intake. Tests cover delayed and never-answered responses.
- **Verify:** `make test-rust FILTER="--test server_async"`; `make test-rust`.

### R-PLAIN-SYNC: document store and sync mode (M)

**Complete (2026-10-29):** generic URI/open
store, atomic sequential ranged/rangeless updates, UTF-8/16/32 negotiation,
CRLF/Unicode bounds, skipped versions and trustworthy full-text/reopen repair.
Invalid/stale/malformed changes cannot resume relative synchronization or confirm
an old snapshot; rejected registry updates never install store snapshots.
Versionless `didSave.text` validates rather than replacing same-version live text
or clearing repair. Real stdio commit tests exercise accepted incremental edits,
registry byte-limit rejection, malformed ranges, same-run rejected/valid reopen,
and save/confirmation barriers. `make test-rust FILTER="--test plain_sync --test
editor_bridge" && make test-lua && git diff --check` passed (6 sync, 27 editor).
The two focused native scenarios `commit-real-file.scn` and `commit-confirm.scn`
passed both profiles/sizes; inspected Save/Discard/recovery screens under
`build/ui/commit-real-file/{standard,vim}-{100x30,80x24}/`.

**VSCode scripted regression verified:** the initial integration run failed at a
premature Discard assertion: the fake advertised empty capabilities, so its
progress-based `idle()` returned immediately; the later progress-cancel case
also lacked a token. Diagnostic observation after the actual draft queue
completed confirmed the correct saved message. The earlier attribution of a
user-authorized upcoming-overhaul deferral was unsupported and is withdrawn.
The integration-local fixture now explicitly advertises its implemented
work-done progress support, acknowledges and validates create/begin/end, requires
an observed active token for each commit, and waits for that draft's queue and
terminal progress. Saved Discard, dirty Save, Cancel and actual progress
cancellation assertions remain. `npm --prefix clients/vscode run test:integration
&& git diff --check` passed, including 240 real-Git selection cases. Shared
initialize helpers, production capability gating/synchronization and native
Save/Discard/Cancel semantics are unchanged. This is not native VSCode or full
regression acceptance; the existing user-authorized broad errors/log/views/UI
gate deferrals remain unchanged.

- **Target Files:** `src/docstore.rs` (new), `src/server.rs`, `src/protocol.rs`, `tests/plain_sync.rs` (new).
- **Reference Files:** `general.positionEncodings`; R-EDIT-REAL's resulting server code.
- **Constraints:** no shims. Versions are not assumed to increase by 1. The store tracks which URIs are open (used by R-PLAIN-DRAFT's refresh); it carries no draft barrier logic.
- **Acceptance:**
  - The store is keyed by URI and applies `contentChanges` sequentially, ranged and rangeless, under the negotiated encoding (utf-8, utf-16, utf-32), with bounds validation.
  - Server advertises `textDocumentSync {openClose, change: 2, save: {includeText: true}}`.
  - Tests: multi-change notifications, multibyte text, CRLF, out-of-range changes (flagged for resync/repair), and rangeless full-text changes from the bundled clients.
- **Verify:** `make test-rust FILTER="--test plain_sync"`; `make test-lua`; `npm --prefix clients/vscode run test:integration` (sync mode changed, so client regression is required).

### R-PLAIN-CORE: presentation seam and unified dispatcher (L)

**Capability-model prerequisite (2026-10-29; scoped complete, parent remains pending):**
`src/presentation/mod.rs` now validates initialize's sole selector: absent selects
Plain, while a present `experimental.magit` must be a complete supported v1
declaration. It separately represents mandatory core behavior and declared
optional effects/prompts/placements, and extracts standard Plain capabilities
with support reasons. `tests/plain_core.rs` covers negotiation, allowed values,
invalid declarations and missing standard capabilities. This does not route
production requests, enforce rich declarations, deliver a Plain document, change
clients, or implement the seam/dispatcher; none of those behaviors are claimed.

- **Target Files:** `src/presentation/{mod,custom}.rs` (new), `src/session.rs`, `src/server.rs`, `src/protocol.rs`, `lua/magit-lsp/init.lua`, `clients/vscode/lib/session.js` (init options only).
- **Reference Files:** AGENTS.md architecture flow; TASKS §3.2, §3.5.
- **Constraints:** no wire or behaviour change for rich clients; no shims.
- **Acceptance:**
  - A `Presentation` seam carries a view's render output, effects and prompts to the client. Current behaviour moves behind `CustomRender` unchanged.
  - The seam serves and stacks concrete views/interactions without client VCS recognition; a minimal fake provider proves dispatcher and view interaction reuse independently of Git.
  - Adapter chosen solely from `capabilities.experimental.magit`; absent means Plain, present means rich. Migrate keymapDelivery/keyModifiers into this block in server and bundled clients.
  - One `dispatch(DispatchRequest{command, target, args})` entry serves `view/key`, `view/invoke` and later `executeCommand`.
  - Sessions keyed by canonical repository identity (state by git-dir, locks by common-dir).
  - All existing suites green; unit tests cover adapter selection.
- **Verify:** `make test`; `make ui-test` (broad shared-lifecycle change).

### R-DIFF-SAFETY (pulled forward, scope unchanged)

Owned by the existing roadmap entry: mutations target the displayed patch identity, not a freshly reconstructed selection. R-PLAIN-TARGET depends on it.

### R-PLAIN-TARGET: content-addressed action targets (M)

- **Target Files:** `src/dispatch.rs` (`ActionTarget`), `src/tree.rs`, `src/commands.rs`, `src/patch.rs`, `src/status_view.rs`.
- **Reference Files:** TASKS §3.1, IMM-6, IMM-2.
- **Constraints:** the node ID is the existing identity (path and content); never a counter or token registry. Identical-content sibling hunks use a positional ordinal as disambiguator only and are still validated against Git: this is the one documented exception to "never address hunks by index" and needs reviewer sign-off.
- **Acceptance:**
  - `ActionTarget{node_id, mutation_hash}`; the hash covers the patch text (stage/unstage) or worktree content (discard).
  - At execution the server re-derives the target from fresh Git state. A mismatch produces a "changed since displayed; refreshed" message, a refresh, and no mutation.
  - Rich-client paths use the same validation.
  - Tests: double-fire, edit between offer and execute, identical hunks, renames, unborn repository.
- **Verify:** `make test-rust FILTER="--test commands"` plus the patch proptests.

### R-PLAIN-REPO: repository and document resolution (M)

- **Target Files:** `src/presentation/resolve.rs` (new), `src/server.rs`, `src/session.rs`, `src/git.rs`, `tests/plain_repo.rs` (new).
- **Constraints:** file content never selects the repository; never guess among candidates for any mutating command.
- **Acceptance:**
  - Resolution order: explicit `repo_path` (rich, unchanged); a single unambiguous Git root from workspace folders; the document path if inside a repository; otherwise a choose-repository view whose rows are code actions over candidates.
  - Backend-owned resolution supplies repository identity and mutation semantics; the presentation/client layer performs no VCS recognition or launcher discovery.
  - Canonical identity from `git rev-parse --git-dir --git-common-dir --show-toplevel`, handling linked worktrees (`.git` file) and symlinks.
  - Presentation documents (`.magit`, configurable) are excluded from Untracked and watcher noise.
  - Tests: no root, `/tmp` root, multi-root, nested, worktree, two repositories in one editor, and a mutating command refused while ambiguous.
- **Verify:** `make test-rust FILTER="--test plain_repo"`.

### R-PLAIN-RENDER: bootstrap and versioned applyEdit pipeline (L)

- **Target Files:** `src/presentation/plain.rs` (new), `src/diff_text.rs`, `src/session.rs`, `tests/plain_render.rs` (new).
- **Reference Files:** the handoff's state-machine and applyEdit results; TASKS §3.3.
- **Constraints:** never write the presentation file from the server; no parsing rendered text for structure; render is spawned after the `didOpen` handler returns.
- **Acceptance:**
  - `didOpen` of a recognised document creates the session and status view and renders. Stale on-disk content is replaced.
  - Tree-to-text uses minimal `TextEdit`s from `diff_text` with correct end positions under the negotiated encoding. Versioned `documentChanges` are used when advertised, else unversioned `changes`.
  - States Detached, Ready, Applying: one `applyEdit` in flight, latest render coalesced, echo reconciled by resulting text hash (before or after the response), no V+1 assumption.
  - A user edit, undo or formatter divergence is repaired back to authoritative text. A stale `applied:false` recomputes and retries. A few recent snapshots are retained, keyed by text hash.
  - A client without `workspace.applyEdit` gets a readable message and an inert document.
  - All harness quirk profiles pass.
- **Verify:** `make test-rust FILTER="--test plain_render"`.

### R-PLAIN-ACTIONS: code actions and executeCommand (M)

- **Target Files:** `src/server.rs`, `src/commands.rs`, `src/protocol.rs`, `src/presentation/plain.rs`, `tests/plain_actions.rs` (new).
- **Reference Files:** TASKS §3.18 (config layers).
- **Constraints:** `codeAction` is side-effect free and does no Git subprocess; actions are discoverable without relying on custom kinds.
- **Acceptance:**
  - Server advertises `codeActionProvider` (custom `magit.*` kinds as hints) and `executeCommandProvider` (`magit.command`).
  - `codeAction` resolves the range against the snapshot whose text hash equals the synced document; if none matches it returns nothing. It returns one action per applicable registered command, with `Planned` and `Unavailable` entries disabled with a reason.
  - Titles follow `actions.title_style` (`plain`, `mnemonic`, `path`, `mnemonicPath`), set per user and per repository. Stable command IDs are unaffected by titles.
  - Arguments are `{command, documentUri, target: ActionTarget, viewGeneration}`. `executeCommand` re-validates through the dispatcher.
  - Selection conversion from half-open LSP ranges to inclusive selections (end at column 0 means the previous line).
  - Tests: every IMM-6 row through code actions, stale or out-of-sync returns none, a latency smoke test on a large view.
- **Verify:** `make test-rust FILTER="--test plain_actions"`.

### R-PLAIN-STACK: view stack and menus in one document (M)

- **Target Files:** `src/presentation/plain.rs`, `src/session.rs`, `src/menu.rs`.
- **Constraints:** no assumption that the cursor moves after a transition; every action in a menu or prompt view is offered from any line.
- **Acceptance:**
  - `openView`, `pushView`, `popView` and `focusView` re-render the active logical view in the same document while the ViewStack keeps view identities. A Back action is present on every non-root view.
  - Menus render switches, options and actions as rows with actions, plus the argument preview line. A switch toggle re-renders; an option opens the prompt renderer.
  - Scenario: status, commit menu, toggle `-a`, back; log, then commit; undo after a transition is repaired.
- **Verify:** `make test-rust FILTER="--test plain_stack"`.

### R-PLAIN-VIS: visibility, folding fallback, standard providers (S)

- **Target Files:** `src/status_view.rs`, `src/tree.rs`, `src/presentation/plain.rs`, `src/config.rs`.
- **Acceptance:**
  - Plain mode renders fully expanded by default. `plain.max_section_lines` (default 400) auto-collapses larger sections with an "N more lines: expand" row.
  - `common.toggle-section`, `show-section`, `hide-section` and `detail-level-N` are code actions on any node. Expansion survives refresh via node ID.
  - `documentSymbol`, `definition`, `documentLink`, `foldingRange` and `semanticTokens/full` are served for Plain documents from the matching snapshot, and return null when out of sync. Fold ranges are valid and bounded.
- **Verify:** `make test-rust FILTER="--test plain_vis"`.

### R-PLAIN-PROMPT: document-backed prompts, completion, support table (L)

- **Target Files:** `src/presentation/prompt.rs` (new), `src/tree.rs` (Field node), `src/server.rs` (`completionProvider`), `src/commands.rs`, `docs/plain-support.md` (generated), `tests/plain_prompt.rs` (new).
- **Reference Files:** TASKS §3.5, §3.6.
- **Constraints:**
  - A `Field` is a tree node with a tracked range. Only the user-authored value is read from the document store; structure is never parsed from text. This does not relax the "never parse rendered text" rule.
  - Edits inside a field are accepted and recorded; edits elsewhere in a prompt view are repaired. Fields are single-line and bounded.
- **Acceptance:**
  - A Plain implementation of the `Ui` interface: `confirm` as Yes/No rows; `text` as a field plus Submit/Cancel; `choice` as item rows, plus a field when `allowCustom`; `multiChoice` as toggle rows plus Done.
  - `textDocument/completion` on a field returns branches, tags, remotes or revisions by field type. Checkout works through a prompt.
  - Candidate lookup is supplied by typed backend/workflow providers; the generic prompt renderer does not inspect Git or infer VCS from candidate text.
  - `secret` prompts and askpass cancel cleanly with a message.
  - A CI test runs every registered command against a recording Plain `Ui` and generates `docs/plain-support.md` keyed by effective capabilities and unsupported reasons, snapshot-tested with `UPDATE_EXPECT=1`. If the artifact is absent, record the gap here rather than generating it manually. Commands emitting unsupported effects become `Unavailable{reason}`. A runtime unsupported effect gives a readable message and a clean cancel.
- **Verify:** `make test-rust FILTER="--test plain_prompt --test manifest --test parity"`.

### R-PLAIN-SELECT: select mode over the document (S)

- **Target Files:** `src/presentation/plain.rs`, `src/commands.rs`, `src/session.rs`.
- **Reference Files:** TASKS §3.7; R-5.1 (completed).
- **Acceptance:** the log select view works as an ordinary document view. The picker is parameterized for single or multi selection and carries stable identities, not client-parsed labels. Confirm, cancel and toggle-mark are actions; free-text revision uses a field. Commit identity comes from `ActionTarget`; the caller is restored on confirm and cancel; stale selection is rejected. Scenario: fixup target selected and cancelled with no mutation.
- **Verify:** `make test-rust FILTER="--test plain_select"`.

### R-PLAIN-DRAFT: commit editing for plain clients (S)

- **Target Files:** `src/presentation/plain.rs`, `src/editor.rs`, `tests/plain_draft.rs` (new).
- **Reference Files:** R-EDIT-REAL (resulting run events); TASKS §3.10.
- **Depends on:** R-EDIT-REAL, R-PLAIN-ACTIONS, R-PLAIN-SYNC.
- **Constraints:** reuse the implemented R-EDIT-REAL workflow above. Git's own `COMMIT_EDITMSG` is identified by canonical path and URI, never `languageId`; Confirm uses registered `magit.command` exact-version synchronized confirmation. General Plain dispatch/Abort code actions remain pending; existing abort is owning-view `common.cancel {runId}`. Preserve run identity/source discrimination and distinct saved-file close.
- **Acceptance:**
  - On `editDocument`: if the document store shows the URI already open, send a versioned whole-buffer `applyEdit` with the fresh template, then `window/showDocument` (`takeFocus`). If not open, `showDocument` only.
  - Provider-file sessions are reusable interactions with explicit finish/cancel callbacks; Git-specific canonical-path and helper-security validation remains in the backend/editor provider, never in client VCS recognition.
  - If the client lacks `showDocument`, or no `didOpen` arrives within a bounded time, cancel the run with a message and mark commit commands `Unavailable`.
  - Abort is a code action running `common.cancel {runId}`; it focuses status via `showDocument`.
  - Explicit Confirm code action uses latest synchronized exact-version content; close confirms saved-file content.
  - Acceptance requires both explicit Confirm and Abort code actions.
  - Harness/native tests: save/close, dirty Save, dirty Discard uses prior saved content, dirty Cancel stays open/no confirm, cleared-and-saved empty Git abort, explicit Confirm exact version, explicit Abort no commit; stale buffer refresh; no `applyEdit` message; second session no actions; comments/status/configured-template/whitespace effective-empty behavior.
- **Verify:** `make test-rust FILTER="--test plain_draft --test editor_bridge"`.

### R-PLAIN-E2E: vertical slice and native evidence (L)

- **Target Files:** `tests/plain_e2e.rs` (new), `tests/ui/plain-nvim/init.lua` (new), `scripts/ui` (client option), `tests/ui/scenarios/plain-*.scn` (new), `ui-test-plan.md`.
- **Reference Files:** TASKS §8.8, AGENTS.md verification.
- **Constraints:** the plain-Neovim init starts magit-lsp with `vim.lsp.start` and no `lua/magit-lsp` code. Neovim's built-in filetype is `gitcommit`, so this also exercises the URI-based draft rule.
- **Acceptance:**
  - Harness slice on a real disposable repository: open, status, stage file, stage hunk, stale action rejected, unstage, commit menu, commit file, save, close, commit exists with exact message.
  - Native plain-Neovim scenarios for status/stage and for commit editing; screens inspected; `scripts/ui stop` run; no orphan processes.
- **Verify:** `make test-rust FILTER="--test plain_e2e"`; `scripts/ui run tests/ui/scenarios/plain-status-stage.scn`; `scripts/ui run tests/ui/scenarios/plain-commit.scn`.

### R-PLAIN-HELIX: manual smoke on Helix 25.07.1 (S)

- **Target Files:** `docs/evidence/plain-helix-25.07.1.md` (new).
- **Acceptance:** a checklist using only the generic Plain configuration (no editor-specific expansions): open, status, stage, menu, log, commit with save and close, abort. Results, Helix version and date recorded; failures filed as tasks, not silently skipped.
- **Verify:** manual; evidence file reviewed.

### R-PLAIN-SPEC: client spec Plain section, README, release wording (M)

- **Target Files:** `docs/CLIENT_SPEC.md`, `README.md`, `TASKS.md` (R-REL-2, R-REL-3).
- **Reference Files:** harness quirk profiles; `docs/plain-support.md`.
- **Acceptance:**
  - CLIENT_SPEC gains a "Plain mode: standard-LSP clients" section: required, recommended and optional capabilities with degradation behaviour; sync and echo rules; code action and `executeCommand` flow; commit editing (open Git's file; save checkpoints; close finishes; abort action); known limits (undo and dirty state, no close-buffer request, cursor not controlled, hidden-buffer clients that never send `didClose` stay pending until closed or aborted); a generic configuration example (Helix TOML as one reference); a conformance checklist keyed to the harness profiles.
  - README states that general-purpose LSP clients are supported in Plain mode (status, staging, menus, log, commit) provided they implement the specification in `docs/CLIENT_SPEC.md`, and names only the clients actually tested.
  - Every claim cross-checked against `src/protocol.rs` and the Plain tests.
- **Verify:** documentation review; link check; `git diff --check`.

### R-PLAIN-GATE: release-slice acceptance through Plain mode (S)

- **Depends on:** R-4.1a, R-7.3a, R-PLAIN-PROMPT, R-PLAIN-SELECT, R-PLAIN-DRAFT, R-PLAIN-E2E.
- **Acceptance:** the R-4.1a (targeted fixup via select mode) and R-7.3a (interactive rebase base selection and structured todo) scenarios run through the fake-editor harness with exact repository-state assertions. `docs/plain-support.md` is regenerated and shows no unexpected `Unavailable` commands. Full `make test` and phase evidence recorded.
- **Verify:** `make test`; `make test-rust FILTER="--test plain_e2e"`.

## 5. Deferred (not release gates)

- **R-EDIT-HISTORY:** see section 4.
- **Level 1 character input:** pending-deletion pairing, armed mode, no destructive keys.
- **Optional launcher:** `magit-lsp open`, only if resolution proves unreliable in practice.
- **Diagnostics channel:** persistent Git and hook errors, with `quickfix` refresh.
- **Discard recovery refs:** snapshot under `refs/magit-lsp/` before irreversible discards.
- **Per-user daemon:** only if second-server-process failures appear in a non-Helix editor.
- **Direct `workspace/executeCommand` bindings:** need a document-identity story that is not editor-specific.
- **Custom URIs and LSP 3.18 `workspace/textDocumentContent`:** tracked under existing §9 item 7. Note: `workspace/textDocumentContent/refresh` exists in 3.18 but is for server-provided content under custom schemes, not `file:` documents, so it is not used for the template refresh.

## 6. Speculative: recorded, NOT to be implemented

- **Close grace timer.** A short delay (about 1 s) after `didClose`, cancelled by a same-URI `didOpen`, so that a language-mode change (VS Code sends document close then open) does not release the helper early. Shutdown, EOF and progress-cancel would cancel immediately. Rationale for deferral: rare scenario. Known limits if ever built: it only helps when the client reopens the file within the selector, and the VS Code client should then select by filename pattern. Reinstate only if real users hit it; the acceptance rows would be "close then same-URI reopen within the period stays pending" and "shutdown or EOF during the period cancels with no commit".
- **Per-run copy of the commit file, copied back at close.** Only if the R-EDIT-REAL spike shows the `applyEdit` refresh (R-PLAIN-DRAFT) misbehaves in a client. It would re-add write-back and helper path validation work.

## 7. Top risks to watch during implementation

1. **Echo versus user edit.** An echo of a server edit must never be classified as input. Reconcile by resulting text hash, not by flags.
2. **Stale targets.** Both defences must stay in place: resolve against the snapshot matching synced text, and validate the mutation hash against Git.
3. **`didClose` is the finish signal.** Delivery is client-dependent (VS Code may keep documents alive after the tab closes; Neovim hidden buffers never send it). The R-EDIT-REAL spike is the gate. Plain clients that never send `didClose` stay pending until the user closes the buffer properly or runs Abort; this is a documented limit.
4. **Stale template buffer.** Git rewrites `COMMIT_EDITMSG` per commit at a reused path; a client holding the old buffer may show stale text. Mitigated by the open-document `applyEdit` refresh and verified in the spike.
5. **Second server process.** An editor may start another instance for the commit document; refuse cleanly, never guess.
6. **Sync-mode regression.** Changing to Incremental must not break the bundled clients' rangeless full-text changes; R-PLAIN-SYNC's client regressions exist for this reason.
