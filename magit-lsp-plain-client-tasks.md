# magit-lsp Plain-client re-architecture: task list

*Formerly "Level 0". Current phase working list linked from canonical TASKS.md §7, ahead of "Feature: release prerequisites". TASKS.md remains the canonical roadmap and design; this file preserves this phase's ticket boundaries, acceptance gates, dependencies and verification. Paths marked (new) are proposals; agents may rename files but must keep task boundaries. Updated 29 October 2026.*

**Mandatory no-vision policy:** all agents, delegated workers and subagents must
never use vision, inspect/render images, invoke image/screenshot tools, or capture
screenshots. Native UI verification remains real, using text/structured state
only (`.screen`, accessibility/DOM text, JSON, wire/API and Git state). Text
snapshots remain allowed. Do not add image-snapshot tests or capture paths, or
require image-based acceptance/inspection. Historical evidence descriptions are
preserved as historical and do not assert that the original evidence was text.

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
R-PLAIN-PREREQ's scoped acceptance, R-PLAIN-SYNC, R-DIFF-SAFETY and R-PLAIN-TARGET are complete; R-PLAIN-REPO is the first unchecked executable task; R-PLAIN-CORE remains an unchecked umbrella after its concrete prerequisites. The scoped R-EDIT-REAL lifecycle limits, native semantic,
recovery and two-repository evidence, and finite uncovered criteria remain
recorded below. The ring's pinned parity differences remain documented policy,
not a change request or a missing source-research blocker.
The typed dispatcher slice is now complete: `Session::dispatch` accepts typed
command/target/args and is shared by rich invocation, resolved keys and menu
actions. Plain delivery, capability enforcement and later content-addressed
action targets remain subsequent work; R-PLAIN-CORE parent acceptance is pending.
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
2. Evidence rule: Plain-mode acceptance is the fake-editor harness plus native runs of a plain Neovim LSP setup (no magit-lsp plugin) through the existing `scripts/ui`, checked with text/structured state only. Helix 25.07.1 is a manual smoke checklist, not a harness. No image capture or inspection.
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
2. R-PLAIN-SYNC (after R-EDIT-REAL); its seam/negotiation/dispatch/rich-enforcement slices are groundwork, not umbrella acceptance.
3. R-DIFF-SAFETY and R-PLAIN-TARGET are complete (2026-10-30; TASKS §7 evidence); next R-PLAIN-REPO. R-PLAIN-CORE remains unchecked until concrete Plain rendering, actions, stack, prompt, picker and draft dependencies are complete; place its umbrella after R-PLAIN-DRAFT and before R-PLAIN-E2E. Preserve the user-authorized R-EDIT-REAL/broad-gate deferral. After concrete, well-implemented Plain protocol work, pause and summarize before the read-only evidence report/planning; do not expand speculative provider abstractions before that checkpoint.
4. R-PLAIN-RENDER, then R-PLAIN-STACK, R-PLAIN-VIS and R-PLAIN-PROMPT (parallel), then R-PLAIN-SELECT. These consume the implemented R-PLAIN-ACTIONS foundation; they do not depend on its final umbrella applicability acceptance.
5. R-PLAIN-DRAFT (after R-EDIT-REAL; it also consumes the action foundation).
6. R-PLAIN-ACTIONS final umbrella applicability acceptance, after STACK/VIS/PROMPT/SELECT/DRAFT; then R-PLAIN-CORE, R-PLAIN-E2E, R-PLAIN-HELIX, R-PLAIN-SPEC, then R-PLAIN-GATE.

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
invalid declarations, missing standard capabilities, transport-independent
recording delivery, and unchanged custom wire shapes. **Scoped typed-seam slice
complete:** production render/effects/prompt notifications pass through `Ui` and
`CustomRender`, including watcher, resync/resize, editor/recovery and outcome
paths; prompt pending registration remains before send. This does not implement
a Plain renderer, client changes, or the
unified dispatcher. The R-PLAIN-CORE parent remains unchecked.

**Rich optional-feature enforcement (scoped complete):** delivery now reads the
shared negotiated declaration; command preflight refuses unsupported visit,
clipboard/selection and commit-worker operations before mutation/helper startup.
Unsupported confirmation/menu prompts return feedback without pending waiters;
secret text requires both text and secret. Unsupported bridge edits abort the
helper, and undeclared root/nested placements become main while preserving
push/pop and caller identity. Actual stdio tests check refusal, no Git mutation
subprocess, unchanged HEAD/index/draft, prompt-free continuation, nested caller
restoration and full-capability delivery, including a real edited commit.
Verified with `cargo test --lib presentation:: && cargo test --test plain_core
--test server_async --test editor_bridge --test plain_harness && git diff --check`.
Native bundled menu regression passed both profiles/sizes via
`scripts/ui run tests/ui/scenarios/commit-menu-shows-actions.scn`; inspect text
evidence in `build/ui/commit-menu-shows-actions/{standard,vim}-{100x30,80x24}/commit-menu.screen`.
**Rich render/fold/modifier enforcement (scoped complete):** audited b3967e6
with new real-binary stdio regressions, not just negotiation tests. Full render
omits edits/baseVersion even after consecutive acks, while provider snapshots
still advance on ack; edits mode reconstructs the same resized menu text from
its acknowledged base. Native folds keep initial, refreshed and newly staged
file/hunk text expanded; visibility calls do not truncate it and standard fold
ranges remain usable. Server folds still collapse and preserve collapse across
refresh. Basic mode filters entire explicit-shift/meta chords and prefixes from
delivery/matching, retains ctrl/alt and literal uppercase/shifted printables,
and keeps empty-key command metadata, explicit invocation and menu switches.
Fidelity reselection now restores full keys directly from parsed chords, with
unit boundary coverage and no hot-path canonical reparsing.
Verified: `cargo test --lib presentation:: && cargo test --test plain_core
--test keymap --test server_async --test editor_bridge && scripts/ui run
tests/ui/scenarios/commit-menu-shows-actions.scn && git diff --check` (2 unit,
91 integration tests; native scenario PASS across both profiles/sizes).
Text-only native evidence inspected:
`build/ui/commit-menu-shows-actions/standard-100x30/commit-menu.screen`;
no owned server/UI processes remain. Concrete Plain implementation and
downstream acceptance are still pending. R-PLAIN-CORE remains unchecked; no
provider/VCS abstraction is introduced before the user checkpoint.

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

- [x] **Completed 2026-10-30, including both reopened review tasks.** Render-bound whole-file snapshots cover HEAD/index blobs/modes, worktree bytes/types/modes, rename endpoints and recursive untracked-directory membership. Confirmed untracked discard and safe symlink deletion are preserved; unstage ignores worktree-only changes. Exact `cargo test --test commands --test dispatch --test patch_cases --test status_render && scripts/ui run tests/ui/scenarios/status-file-identity.scn && git diff --check` passed (89 Rust tests; native standard/vim at 100x30/80x24). Inspected focused `.screen` evidence; owned processes stopped. See TASKS §7 for independent exact-state matrices, evidence paths and limits: conservative permissions, special-file capture refusal, no atomic guarantee against edits racing after validation, deliberate IMM-6 bulk unchanged. No new Plain protocol or broad-gate closure is claimed.

### R-PLAIN-TARGET: content-addressed action targets (M)

- [x] **Review corrections completed 2026-10-30.** Both TASKS §7 P0/P1 corrections are complete; the identical-content ordinal exception is narrowly approved. Hunk authorization checks the actual mutated Git side and render-bound modes/type/create/delete/rename/copy semantics, excluding blob IDs and coordinates. Selection execution compares captured metadata with the fresh headers it applies. The exact gate `cargo test --test commands --test dispatch --test patch_cases --test status_render && cargo test --lib patch:: && git diff --check` passed (100 integration + 16 patch tests). An independent 96-fixture matrix proves exact stale HEAD/index/worktree preservation and successful already-displayed mode application through typed start/dispatch, rich header/line cursors and visual header/changed-line selections, for stage/unstage and executable-bit additions/removals. Positive unstage tolerates worktree-only content/mode edits; existing shift/rename/header/sibling/retained-version tests pass. Release-built native `status-file-identity.scn` passed, including wrong-side discard refusal; an additional interactive native hunk-selection stale-mode checkpoint verified readable refusal, unchanged index/HEAD, preserved executable worktree and idle acknowledged refresh (TASKS §7 evidence). Native unstage/all matrix combinations are not claimed. Plain providers, atomic external-race guarantees and inherited broad-gate deferrals remain out of scope. The original completion entry below is historical evidence; its sign-off request is resolved by the scoped approval in TASKS §7.

- [x] **Completed 2026-10-30.** Exact displayed-version metadata now produces action-bound targets and fresh Git validation in unified rich invoke/key/selection dispatch. Typed `Node.mutation` replaces file-identity JSON in copy payloads. Later Plain actions will reuse `dispatch::offer_target`, `DispatchRequest.action_target` and `Session::start_dispatch` plus its existing prompt continuation; this does not expose executeCommand or claim a Plain code-action provider. Independent exact-state tests cover double-fire, edits before execute/during discard confirmation, both identical siblings (typed and rich), rename endpoints, unborn HEAD and forged targets. The exact ticket gate passed: `cargo test --test commands --test dispatch --test status_render && cargo test --lib patch:: && git diff --check` (88 integration + 15 patch tests); focused native `status-file-identity.scn` also passed with text evidence inspected. **Reviewer sign-off requested for the existing identical-sibling ordinal exception:** it only disambiguates equal content IDs; ambiguous targets additionally bind sibling count and complete file state, so consuming one cannot retarget another. Unique hunk coordinate shifts remain valid. Whole-file/discard authorization remains conservative, unstage ignores worktree-only changes, deliberate section-bulk behavior is unchanged, and no atomic guarantee is claimed for external edits racing after validation. See TASKS §7 for exact-state/index-cache evidence and unchanged broad-gate deferrals.

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

**Complete:** bounded Git-owned discovery supplies canonical root, per-worktree
git-dir and common-dir; initialize captures deduplicated workspace candidates
using strictly validated local file URIs. Resolution has explicit/workspace/
document precedence and returns typed `Choose` rather than guessing. Config
validates `presentation.files` basename policy (default `.magit`); status and
watcher filters use that policy. Actual-binary initialize/rich-open, UTF-8 and
escaped URI cases, and real-Git resolution/status tests pass. No presentation
layer performs Git discovery or `.git` recognition.

**Downstream only:** Plain didOpen rendering, chooser rows/code actions and
selection require R-PLAIN-RENDER/ACTIONS. No chooser UI or Plain mutation route is
claimed here.

- **Target Files:** `src/presentation/resolve.rs` (new), `src/presentation/mod.rs`, `src/server.rs`, `src/session.rs`, `src/git.rs`, `src/config.rs`, `src/watcher.rs`, `tests/plain_repo.rs` (new), `TASKS.md`, `magit-lsp-plain-client-tasks.md`.
- **Constraints:** file content never selects the repository; never guess among candidates for any mutating command.
- **Acceptance:**
  - Resolution order: explicit `repo_path` (rich, unchanged); a single unambiguous Git root from workspace folders; the document path if inside a repository; otherwise a choose-repository view whose rows are code actions over candidates.
  - Backend-owned resolution supplies repository identity and mutation semantics; the presentation/client layer performs no VCS recognition or launcher discovery.
  - Canonical identity from `git rev-parse --git-dir --git-common-dir --show-toplevel`, handling linked worktrees (`.git` file) and symlinks.
  - Presentation documents (`.magit`, configurable) are excluded from Untracked and watcher noise.
  - Tests: no root, `/tmp` root, multi-root, nested, worktree, two repositories in one editor, and a mutating command refused while ambiguous.
- **Verify:** `make test-rust FILTER="--test plain_repo --test plain_core --test server_async --test config --test watcher" && git diff --check`.

### R-PLAIN-RENDER: bootstrap and versioned applyEdit pipeline (L)

**Completed bootstrap/lifecycle and native acceptance (2026-10-30):** Recognized didOpen now
resolves a repository and opens its Session status Tree, delivering standard
applyEdit requests from accepted DocStore snapshots. Captured runtime ownership
supports native watcher callbacks; serialized incarnation transitions cover
install/send, accepted changes, replies, close/reopen and terminal detachment.
Every server render path now negotiates Plain delivery. Actual-binary tests
exercise all eight fake transport profiles (including decomposition, queued
debounce and reordered replies), both divergent echo/response orders, native
watcher-only changes, pending close/reopen/disconnect and absent applyEdit.
Disk bytes remain untouched and custom render notifications are rejected.
The focused binary matrix passed. Plugin-free native Neovim then ran in an owned
tmux server with `--listen` and standalone `vim.lsp.start`, without project Lua
modules/runtimepath. Evidence shows stale disk presentation replacement,
watcher-only worktree refresh, native typing and undo with increasing accepted
LSP buffer versions, and final exact restoration to authoritative text. The
presentation sentinel bytes remained unchanged; no `view/render` notification
or project Lua module was observed. Owned tmux, Neovim, socket and server-child
cleanup was verified. Retained text/JSON: `build/ui/plain-render/native.screen`
and `build/ui/plain-render/native.json`. Exact verification passed:
`cargo build --release && python3 scripts/test_plain_neovim.py && cargo test
--test plain_render && git diff --check` (10 plain-render tests).

**Controller slice:** `PlainDocument` is
a reusable Detached/Ready/Applying controller with bounded exact-text-hash
snapshots, one in-flight edit and latest-render coalescing. It emits minimal
UTF-8/16/32-positioned edits in versioned `documentChanges` or unversioned
`changes`, reconciles exact text echoes on either side of the response, and
recomputes after stale rejection or divergent edits. Closing invalidates the
pending request. Deterministic fake-editor tests cover the applicable profile,
encoding, CRLF, version, echo-order and detach cases; those tests complement,
and do not substitute for, the binary and native transport evidence above.
Repeated `applied:false` for an unchanged authoritative/client snapshot is held
inert to avoid an unbounded retry loop; a fresh client change or new render
makes the controller eligible again.

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
- **Verify:** `cargo test --test plain_render --test plain_harness --test plain_sync && git diff --check`.

### R-PLAIN-ACTIONS: code actions and executeCommand (M)

**Scheduling note (2026-10-30):** The foundation (snapshot-matched dispatch, IMM-6 content safety, configured titles, native actions and latency) is implemented. STACK/VIS/PROMPT/SELECT/DRAFT consume that foundation; the remaining umbrella acceptance is final downstream adapter applicability, scheduled after those tasks. This does not mark the acceptance complete or alter the historical evidence below.

**STACK child — retained menu switches (2026-10-30):** The completed
`Toggle Plain menu switches through retained semantic actions` child is recorded
in TASKS §7. Plain render snapshots now atomically retain typed menu metadata;
switch offers bind the exact menu, item, view and generation and call the shared
Session switch operation. Binary coverage toggles commit `-a` on/off from
distinct rows, checks exact previews, URI/caller restoration and refuses stale,
forged, wrong-menu and unsupported targets without Git mutation or prompt waits.
Native toggle evidence remains pending; options remain unavailable pending
R-PLAIN-PROMPT, and broader STACK acceptance remains open. The child gate passed:
`cargo test --test plain_stack --test plain_actions --test plain_render --test
menu --test server_async && git diff --check`.

**Plugin-free native content-action child complete (2026-10-30; parent remains open):** Stock Neovim exposed a missing server `codeActionProvider` advertisement that raw binary requests had hidden. The only production correction advertises the existing route; binary initialize assertions cover that provider and unchanged `magit.command` advertisement. The standalone setup declares stock static execute-command support, without overriding LSP/UI functions. Native cursor/file Stage and Unstage plus visual selection of the first replacement in a two-replacement hunk execute through unmodified `vim.lsp.buf.code_action`/`vim.ui.select`/`inputlist`: Space pages through all offered titles, then literal actual numbers select the action. Independent HEAD/index/worktree assertions prove whole-file stage, exact unstage restoration, then index `ONE\ntwo\nthree\n` versus worktree `ONE\ntwo\nTHREE\n`, with HEAD and unrelated untracked bytes preserved. Discard stays visibly disabled; cancellation and a subsequent native request preserve exact Git state. Inspected `build/ui/plain-render/native.json`, `actions-final.screen`, and per-page/closed picker text; unchanged presentation disk bytes, no project Lua/runtimepath/custom render, and owned tmux/editor/server cleanup passed. Verification: `cargo build --release && python3 scripts/test_plain_neovim.py && cargo test --test plain_actions && git diff --check` (21 action tests). **Limit:** the presentation fixture now has no final newline, matching server text; the earlier newline-terminated fixture exposed a render/echo repair loop. This slice does not fix or accept that separate sync issue, downstream adapters, or the full regression gate.

**Plain EOF diagnostic child complete (2026-10-30; repair and parent remain open):** `scripts/test_plain_neovim.py` now runs the standard plugin-free Neovim client against both initial `.magit` byte variants, saves `endofline`/`fixeol`/`fileformat`, and uses standard `vim.lsp.log` tracing (`tests/ui/plain_init.lua`, trace level `trace`). Each run's structured report and trace are under `build/ui/plain-render/{without-final-newline,with-final-newline}/`; trace captures are diagnostic artifacts, not tracked logs. In the LF variant, the trace orders `didOpen` text `stale presentation sentinel\n` before server `workspace/applyEdit` outbound/1, range `(0,0)..(1,0)`, replacing it with Tree text `Head: master\n\nStaged changes (0)\n\nUnstaged changes (0)\n\nUntracked files (0)\n\nWorking tree clean` (no final LF). Neovim returns `applied=true`; its following `didChange` replaces only sentinel characters, leaving the trailing LF. The next server deletion `(8,18)..(9,0)` with empty `newText` echoes as zero-width empty insertion `(8,18)..(8,18)`, then repeats through version 213 instead of converging. Actual native run fails `native picker timeout (file-discard-unavailable)` with `No code actions available`; do not treat it as action pass. Final state reports `endofline=true`, `fixeol=true`, `fileformat=unix`; cleanup reports tmux killed/none remaining, socket absent, and no server process remaining. In the no-LF variant, `didOpen` sends sentinel without LF, bootstrap reaches rendered text/version 4 with `endofline=false`; native Stage/Unstage/selected-lines operations complete through version 12 and preserve disk bytes (`presentation_disk_unchanged=true`), with the same successful cleanup. Evidence including native failures and cleanup: `build/ui/plain-render/{with-final-newline,without-final-newline}/failure.json` / `native.json` and `trace-excerpts.log` (first 100 lines of the former trace show the ordering). This establishes the client echo/EOF divergence, not a repair; no Neovim option or relaxed text equality was used. Production serialization repair, parent acceptance and Plain milestone remain pending.

**Fresh dual-EOF action evidence (2026-10-30; serialization/drift parent remains open):** Fresh `native.json` reports for both variants have no error and confirm the five native interactions: unavailable discard, file stage, file unstage, selected-lines stage, and subsequent request. HEAD stays unchanged; partial index is exactly `ONE\ntwo\nthree\n` while worktree remains `ONE\ntwo\nTHREE\n`. Presentation disk bytes are unchanged and owned cleanup is verified. Each trace counted 12 matching `workspace/applyEdit` log occurrences, 7 `didChange` occurrences, and zero empty-insert changes; these are log-line occurrence counts, not a deduplicated request count. No stale refusal or retry occurred in these fresh runs. The previous selected-lines `STALE_VIEW` refusal remains a separate diagnostic; no bounded retry is claimed. Persistent format-drift repair remains open.

**Exact EOF serialization regressions (2026-10-30):** Binary tests exercise LF/CRLF with and without a final terminator, sending a successful applyEdit response before the faithful didChange echo; after convergence, real file-row actions retain a generation/target and the phantom row after a final terminator has no content target. Controller tests cover Unicode final-line growth/shrink under UTF-8/16/32 and both EOF conventions, asserting exact echoed text, Ready state, retained action hash and no retry. These tests validate immutable open-time serialization; persistent format drift is not accepted here.

**Bound Plain format-drift guard (2026-10-30):** Controller regressions cover format-only LF/CRLF/EOF echoes in both response orders, exact delayed echo after an intermediate decomposed-format change, one warning/suspension, blocked action snapshots, repeated render/change, and reopen recovery. Actual-binary stdio coverage verifies one standard `window/showMessage`, no follow-up edit, empty code-action result while suspended, and successful reopen synchronization. Focused gate: `cargo test --test plain_render --test plain_actions --test plain_sync && cargo test --lib presentation:: && git diff --check`. This does not claim the parent native dual-EOF acceptance; no new native evidence was collected in this slice.

**Plain EOF repair parent accepted (2026-10-30; bugfix only):** Rebuilt-release
native smoke passed both EOF variants with
`cargo build --release && python3 scripts/test_plain_neovim.py && git diff --check`.
Fresh `native.json` plus per-variant `.screen` and `trace-excerpts.log` evidence
shows bootstrap at version 4, final action render at version 12, no error, and
all five interactions: discard visibly unavailable, file stage, file unstage,
exact selected-lines stage, then a subsequent request. HEAD is unchanged; file
stage/unstage and partial-index/worktree bytes match the expected exact states.
Both variants preserve exact presentation-file bytes and report no project Lua
modules, no rich notifications/custom renders, and verified owned tmux/editor/
server cleanup. The structured trace count is 12 matching applyEdit log
occurrences (including request/handler log entries), not a deduplicated outbound
request count; it reports 7 didChange occurrences and zero empty-insert changes.
The newline case now converges without the former repeating terminal deletion/
empty insertion. The earlier failed native reproduction remains preserved above
as historical diagnostic evidence. This closes the EOF bugfix parent only; it is
not Plain milestone acceptance and does not complete persistent format-drift
acceptance or gather the deferred evidence report.

**Scoped coverage slice (2026-10-30; parent remains unchecked):** a six-entry retention unit test verifies oldest eviction and newest generation; a data-driven binary test locates two modified-file offering rows in actual text and verifies half-open end-at-column-zero exclusion, non-zero inclusion, and distinct node IDs. This is not completion of title styles, contextual `Unavailable` policy, the full IMM-6 matrix or the parent acceptance.

**Plain action title composition (2026-10-30; scoped slice, parent remains unchecked):** offers use the retained snapshot scope, repository-resolved `actions.title_style` and keymap, plus the structured node path; outer and nested titles are identical. Missing mnemonic/path parts are omitted, and command IDs/arguments are unchanged. Isolated real-binary tests cover all four styles, configured user binding `z`, a trusted repository override, and stable action identity; pure helper tests cover missing components. Verified with `cargo test --test plain_actions --test config && git diff --check` (7 Plain-action and 24 config tests passed). Contextual `Planned`/`Unavailable` reasons, the full IMM-6 applicability matrix and latency smoke test remain open; no broad gate is claimed green.

**Plain `executeCommand` implementation (2026-10-30; parent remains unchecked):** offers stay side-effect free and snapshot-matched, since only the retained snapshot whose exact text hash equals the synced document offers actions and an unmatched or closed document offers none. Executable offers carry typed `PlainActionParams` `{command, documentUri, target, viewGeneration}` on the ordered `workspace/executeCommand` lane and no longer set the previous unconditional `disabled`. Execution revalidates, in order, that the document is open and not repairing, that the exact current-text `action_snapshot` matches, that `view_id`/`generation` match the offer, and that `target_is_current` agrees with fresh Git; only then does the existing typed `Session::dispatch` run bound to the retained render version and the node's resolved line. Binary coverage executes an offered stage with exact HEAD/index/worktree assertions and refuses changed-target, wrong-generation and out-of-sync/closed requests with no Git mutation; rich `magit.command` `common.confirm` payloads are unchanged. Landed as `069c4c7` and `2ba52c8`; `make test-rust FILTER="--test plain_actions"` exited 0 (6 tests passed). The earlier retained-snapshot and half-open-range evidence above remains valid.

**Plain selection boundary/stale slice complete (2026-10-30; parent remains unchecked):** In addition to selected-line/context/tamper coverage, real-binary cases assert file-header whole-file and hunk-header whole-hunk precedence, half-open end-column-zero and UTF-16 supplementary-character endpoints with CRLF content, atomic-before-apply refusal for a multi-file intent with one stale member, and tolerance of a unique selected hunk shifted by an independent fresh change. The multi-file stale case deliberately executes against the retained synced/rendered text without dispatching watcher applyEdits. Shared offer/freshness validation derives the same section-filtered constituent targets. This closes only the selection boundary matrix; contextual availability and latency remain open, as does parent R-PLAIN-ACTIONS acceptance.

**Intentional Plain section bulk slice (2026-10-30; parent remains unchecked):** Section headers now offer Stage for unstaged/untracked and Unstage for staged, with targets bound to retained section node identity plus command/semantic section rather than a captured file set. Offers stay Git-free; exact current document/snapshot/view-generation checks remain, and execution verifies the target before using the existing dispatcher’s fresh bulk operation. Actual-binary exact-state coverage includes unstaged Stage, untracked Stage, staged Unstage, deliberately added changes after the offer, and tampered/wrong-section refusal without mutation. Verified with `cargo test --test plain_actions --test commands --test dispatch && git diff --check` (14 Plain-action, 40 command, 43 dispatch tests; 97 total). Section discard is intentionally not offered; contextual availability, confirmation support and latency remain open parent criteria.

**Unsupported Plain content-action refusal (scoped child complete; parent remains unchecked):** A single pure availability policy is applied at offer and execution time: stage/unstage require `workspace.applyEdit`, `textDocument.codeAction`, and `workspace.executeCommand`; discard is temporarily unavailable until document-backed Plain confirmation exists. Binary cases cover disabled-support discard with a readable reason and no runnable command, forged discard execution returning a readable refusal with exact HEAD/index/worktree preservation and no prompt request, omission for clients without disabled-support, and a missing execute-command capability yielding a disabled stage with a reason. Existing real-binary stage/unstage execution remains passing. Exact verification: `cargo test --test plain_actions --test plain_core --test server_async && git diff --check` (17 Plain-action, 9 Plain-core and 34 async tests). Rich `common.confirm` remains unchanged. The document-backed prompt dependency and broader Planned/common/menu applicability remain open.

**Planned and unsupported contextual commands (scoped child complete; R-PLAIN-ACTIONS remains unchecked):** Pure registry availability and scope/status checks are shared with rich dispatch. Snapshot-matched offers include in-scope Planned and implemented commands lacking a Plain adapter as disabled entries, with stable command/document/generation identity, roadmap or adapter reason and configured title; these entries have no runnable command or fabricated content target. Without `disabledSupport` they are omitted. Existing executable content cursor/selection/section applicability is preserved. Binary coverage checks Planned `status.reverse` identity/reason, no duplicate IDs, omission without disabled support, malformed/forged Plain refusal before execution with unchanged Git state, and a subsequent supported stage request for responsiveness. The Plain refusal is gated on the Plain document/generation envelope, preserving rich `common.confirm`; the exact previously failing commit-msg-hook disconnect test now passes. Pure helper test passed: `cargo test --lib availability_tests`. Required gate passed: `cargo test --test server_async disconnect_during_commit_msg_hook_cancels_owned_hook_without_committing && cargo test --test plain_actions --test commands --test menu --test plain_core --test server_async --test editor_bridge && git diff --check`. Earlier evidence exposed rich confirmation interception and was corrected before this rerun. No navigation, clipboard, prompt, menu, commit or visibility adapter is claimed, and the ACTIONS parent remains open for latency and downstream adapter completeness.

- **Target Files:** `src/server.rs`, `src/commands.rs`, `src/protocol.rs`, `src/presentation/plain.rs`, `tests/plain_actions.rs` (new).
- **Reference Files:** TASKS §3.18 (config layers).
- **Constraints:** `codeAction` is side-effect free and does no Git subprocess; actions are discoverable without relying on custom kinds.
- **Acceptance:**
  - Server advertises `codeActionProvider` (custom `magit.*` kinds as hints) and `executeCommandProvider` (`magit.command`).
  - `codeAction` resolves the range against the snapshot whose text hash equals the synced document; if none matches it returns nothing. It returns one action per applicable registered command, with `Planned` and `Unavailable` entries disabled with a reason.
  - Titles follow `actions.title_style` (`plain`, `mnemonic`, `path`, `mnemonicPath`), set per user and per repository. Stable command IDs are unaffected by titles.
  - Arguments are `{command, documentUri, target: ActionTarget, viewGeneration}`. `executeCommand` re-validates through the dispatcher.
  - Selection conversion from half-open LSP ranges to inclusive selections (end at column 0 means the previous line).
  - Tests: every IMM-6 row through code actions, stale or out-of-sync returns none, a latency smoke test on a large view. 2026-10-30 smoke evidence: actual-binary 48-file/100-line modified fixture; 13,370 rendered bytes/919 lines; repeated cursor/file/hunk requests plus selection; repeated file-row command/target payloads match, offers are bounded and duplicate-free, independent HEAD/index/worktree invariants unchanged. Refined command `cargo test --test plain_actions large_retained_view_repeated_code_actions_report_latency_without_mutation -- --nocapture && git diff --check` passed. Observed cursor latency 2.80–5.24 ms and selection latency 3.29 ms, diagnostics only, not an SLA. Parent remains open for downstream adapter completeness/native action evidence.
- **Verify:** `make test-rust FILTER="--test plain_actions"`.

### R-PLAIN-STACK: view stack and menus in one document (M)

- **Target Files:** `src/presentation/plain.rs`, `src/session.rs`, `src/menu.rs`.
- **Constraints:** no assumption that the cursor moves after a transition; every action in a menu or prompt view is offered from any line.
- **Acceptance:**
  - `openView`, `pushView`, `popView` and `focusView` re-render the active logical view in the same document while the ViewStack keeps view identities. A Back action is present on every non-root view.
  - Menus render switches, options and actions as rows with actions, plus the argument preview line. A switch toggle re-renders; an option opens the prompt renderer.
  - Scenario: status, commit menu, toggle `-a`, back; log, then commit; undo after a transition is repaired.
- **Verify:** `make test-rust FILTER="--test plain_stack"`.

The scoped child **Serve Plain status-to-menu and Back transitions in the same document** is complete: binary coverage verifies status-to-commit-menu and Back via semantic view targets, retaining the same document URI and caller identity. **Native menu-switch/caller-restoration child complete (2026-10-30):** both EOF variants in `build/ui/plain-render/{without-final-newline,with-final-newline}/` retain freshly inspected `native.json`, `menu-on.screen` and `menu-off.screen`. Stock native code-action picker interactions open the commit menu, toggle `-a` on/off from distinct menu rows, show `[x]`/`git commit --all` then `[ ]`/`git commit`, and restore byte-for-byte caller text with unchanged independent Git state. Existing native content actions still pass afterward; presentation disk, plugin-free state and owned-process cleanup are verified. Exact gate passed: `cargo build --release && python3 scripts/test_plain_neovim.py && cargo test --test plain_stack && git diff --check`. Menu-option prompts, log-menu acceptance and remaining STACK scenarios are still pending; this does not complete the broader STACK parent.

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
- **Depends on:** R-EDIT-REAL, R-PLAIN-SYNC; consumes the implemented R-PLAIN-ACTIONS foundation. Final R-PLAIN-ACTIONS umbrella acceptance follows DRAFT.
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
  - Native plain-Neovim scenarios for status/stage and for commit editing; `.screen`/structured text state inspected; `scripts/ui stop` run; no orphan processes. No image capture or inspection.
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
