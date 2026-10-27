# Technical debt review

## Review

### Baseline metrics

Source: `build/review/metrics.txt`, collected before the review. These are baseline measurements, not measurements of subsequent concurrent changes.

| Metric | Baseline |
|---|---|
| Revision | `34389a24493f1eda34e93c7d913d05a8c730ec28` |
| Initial dirty files | `flake.lock`, `flake.nix` |
| tokei | SKIPPED: tokei not installed |
| `cargo build` | Passed |
| `cargo clippy --all-targets` | Failed, exit 101: `src/config.rs:589`, loop never actually loops |
| Clippy warnings | 18 library warnings; 19 in library-test build, including 18 duplicates |
| Unused dependencies | SKIPPED: cargo-machete not installed |
| Duplicate-block detection | SKIPPED: jscpd not installed |
| Rust tests | 335 passed, excluding separately launched child-test invocations |
| Neovim cases | 65: 6 keymap, 59 integration; one failed |
| Lua failure | `pressing s stages the change under the cursor`, `tests/test_magit_lsp.lua:1083` |
| `time make test` | Failed, exit 2; wall 36.686 s, user 21.273 s, sys 9.027 s |
| Separate Node tests | Passed: 23 tests plus asynchronous status-open lifecycle suite |
| Separate generated-keybinding check | Passed: 164 keybindings |
| Snapshot files | 21: 6 JSON, 3 Markdown, 12 text |
| `keep-for:` search | No matches; exit 1 |
| Physical-line fallback | 42,966 lines in 124 files, including blanks/comments; NOT tokei SLOC |

| Directory | Language | Files | Physical lines |
|---|---|---:|---:|
| clients | JavaScript | 13 | 4,196 |
| lua | Lua | 11 | 3,175 |
| scripts | Lua | 2 | 450 |
| scripts | Python | 7 | 2,220 |
| scripts | Shell | 1 | 42 |
| src | Rust | 52 | 16,655 |
| tests | Lua | 2 | 2,618 |
| tests | Python | 5 | 1,131 |
| tests | Rust | 31 | 12,479 |
| **Total** | | **124** | **42,966** |

### Ten-line summary

1. The baseline is not green; deletion success cannot assume a passing suite.
2. The editor bridge needs an explicitly private socket directory.
3. Both clients send close notifications that the server does not handle as notifications.
4. Positional request versions are transmitted but not consumed by key dispatch.
5. Visual actions reconstruct old document coordinates using fresh diffs.
6. Prompt waits hold the global session mutex, creating a reader-loop deadlock path.
7. VSCode has quit, edit-focus, selection-boundary and menu-activation seam gaps.
8. Keep below-soft-floor warnings on Git errors; delete multi-version test scaffolding while keeping floor/latest CI.
9. Dead APIs, repeated manifest metadata and duplicate render paths offer independent reductions.
10. Propose **32 tasks, estimated net −1,508 LOC**; no implementation is authorized by this report.

### Do not touch

- Existing or concurrent working-tree changes; do not reset, clean, overwrite or kill processes belonging to another session.
- `build/review/metrics.txt`, the collection-time evidence.
- Patch proptests and their regression corpus.
- Git-quirk fixtures: unborn HEAD, renames, CRLF, missing final newlines, binary files, type changes and unusual paths.
- Named UI regressions merely because another layer exercises the underlying Git operation.
- Deliberate Planned menu entries.
- Independent upstream Magit key audit: an external oracle, not redundant runtime data.
- Floor/latest Git CI coverage.
- User-visible warnings when a Git command fails and the installed Git is below the 2.30.8 soft floor; this is advisory, never a version gate.
- Already-open review tasks: reuse their ownership rather than creating parallel implementations.

### Proposed order

1. Resolve correctness/security seams A01–A12, A18, A19, A29, A33 and A34.
2. Establish a passing baseline, preserving investigation of the existing Lua and Clippy failures.
3. Perform independent deletions A13–A16 and A27.
4. Merge duplicate paths A17–A19, coordinating overlaps with the first group.
5. Add `rstest` once in A20; then A21, A30 and A31. Cases and runtime are retained.
6. Complete client reductions A24–A26 and A32; annotate retained scaffolding A28.
7. Run full tests and inspect the UI tour; measure actual LOC separately from these estimates.

### Assumptions and limitations

- This is a whole-repository review, not a diff review. Vendored dependencies, generated build output and bytecode are excluded from first-party inventory.
- The omitted Step 2 is interpreted as the supplied metrics, not an invented review phase.
- Editor-native window placement/sizing, fold translation and structured-menu-to-QuickPick formatting are permitted glue. Canonical grammar, navigation targets, command-key interpretation and menu transitions belong to the server.
- Existing tasks already cover Lua async requests/debug isolation, visit handling, highlight metadata, fake-Git extraction, test overlap, snapshots, source-text cursor lookup, mutation ordering, environment isolation and UI latency. This report does not replace them.
- Shell restrictions prevented the reviewer's initial bulk scan; its follow-up used read/grep tools to complete the command-ID and typed-field inventories. Python LSP was unavailable, so Python inventory used source declarations.
- Named functions are inventoried; anonymous callbacks inherit their enclosing operation's classification. Arbitrary JSON `args` has no finite leaf schema: declared fields and interpreted argument keys are inventoried instead.
- LOC estimates are not measured savings and include generated-data changes for A15/A29. Already-open backlog savings are excluded.
- The repository changed concurrently during review, including source, tests, roadmap and snapshots. Paths/line numbers and deletion preconditions must be revalidated before any future implementation. Baseline snapshot counts are deliberately historical; some snapshots were removed concurrently.
- Collection found no orphan server processes. A later process check found a live server during concurrent work; it was not killed or attributed to this review.

### Decisions needed

1. **VSCode text mode:** retain the read-only virtual document and provide native browsing/text mode (A29); do not build a writable adapter. Neovim's existing editable text mode remains unchanged.
2. **Completed R-2.3 versus still-Planned common commands:** retain the §5.1 bindings, but decide whether the four navigation/cycle operations need a new open implementation ticket or a corrected historical milestone. Do not silently delete them.

### Task ledger

| Task | Priority | Category | Estimated net LOC |
|---|---|---|---:|
| A01 | P0 | simplify | +30 |
| A02 | P1 | simplify | +25 |
| A03 | P1 | simplify | +60 |
| A04 | P1 | simplify | +15 |
| A05 | P1 | merge | +80 |
| A06 | P1 | simplify | +25 |
| A07 | P1 | delete | −55 |
| A08 | P1 | simplify | −10 |
| A09 | P1 | simplify | +15 |
| A10 | P1 | merge | +35 |
| A11 | P1 | simplify | +20 |
| A12 | P1 | merge | −15 |
| A13 | P2 | delete | −200 |
| A14 | P2 | delete | −135 |
| A15 | P2 | delete | −700 |
| A16 | P2 | delete | −160 |
| A17 | P2 | merge | −75 |
| A18 | P1 | merge | −10 |
| A19 | P1 | merge | −120 |
| A20 | P2 | simplify | −18 |
| A21 | P2 | simplify | −20 |
| A24 | P2 | simplify | −120 |
| A25 | P2 | move-to-server | −25 |
| A26 | P2 | delete | −60 |
| A27 | P2 | delete | −100 |
| A28 | P2 | keep-with-reason | +20 |
| A29 | P1 | simplify | +40 |
| A30 | P2 | simplify | −20 |
| A31 | P2 | simplify | −20 |
| A32 | P2 | simplify | −60 |
| A33 | P1 | move-to-server | +35 |
| A34 | P1 | simplify | +15 |
| **Total** | | | **−1,508** |

A22/A23 were withdrawn after deeper inspection; IDs are not renumbered.

| Priority | Count |
|---|---:|
| P0 | 1 |
| P1 | 16 |
| P2 | 15 |
| **Total** | **32** |

| Category | Count | Estimated net LOC |
|---|---:|---:|
| delete | 7 | −1,410 |
| merge | 6 | −105 |
| simplify | 16 | −23 |
| move-to-server | 2 | +10 |
| keep-with-reason | 1 | +20 |
| **Total** | **32** | **−1,508** |

### Proposed tasks — not dispatched

### Reconciliation — 2026-10-27

This ledger records the review's state against the concurrent history and current
source, not task completion inferred from commit subjects. Historical baseline
metrics above remain collection-time evidence, not current verification. Recheck
each task's complete acceptance gate before implementation; the priority work is
A01–A12, A18, A19, A29, A33 and A34, before deletions.

**Already addressed in part / assumptions stale:** A19's shared named key
vectors already exist (`tests/data/keys.json`); this does not establish shared
prompt/render vectors or false-default behavior. A29's log view and reusable
commit selection consumers now exist; this does not establish native read-only
VSCode browsing and queued-refresh behavior. A01–A34 preconditions and line
references must be revalidated. Discard is implemented (`status.discard` and
Git dispatch), contrary to older roadmap assumptions; A31's test-parameterization
work may still remain. The defensive-boundary principle is already stated in
AGENTS.md, but that does not settle each concrete boundary finding.

**Still open pending full acceptance evidence:** A02–A12, A18, A19, A29, A33
and A34 remain prioritized and unchecked. In particular, partial consumers,
shared key fixtures, or an implementation commit do not satisfy their gates.
Other tasks remain unchecked until their source preconditions are revalidated.
The history includes log-view/select work (d5da635 and related commits),
confirmed discard (3b175f2), and key-vector work; these are evidence pointers,
not completion claims. A22/A23 remain withdrawn. Do not touch the explicit
exclusions above, and preserve existing ownership.

**Verification distinction:** baseline test metrics are historical. A later
successful command is evidence only for the exact code and cases it ran; UI,
client, and real-Git acceptance gates still require their stated verification.

- [x] **[P0] A01 — Establish a private editor-bridge socket directory** (`src/editor.rs:359–375`)
  - **Problem:** Predictable `/tmp/magit-lsp-<pid>` is adopted with `create_dir_all`, without establishing ownership/privacy; an existing socket is removed. Under permissive umask another local user can reach askpass.
  - **Why it matters:** Security; askpass answers are returned to the connecting peer.
  - **Change:** Exclusively create a mode-0700 directory; reject pre-existing/symlinked directories. Bind inside it and clean up only owned resources.
  - **Done when:** Permissive-umask and pre-created-directory/symlink tests protect the actual bridge boundary.
  - **Acceptance evidence (2026-10-27):** `cargo test --test editor_bridge && make test && git diff --check` exited 0 on the single verification attempt: all 9 bridge tests passed; full gate reported `ok: build`, `rust`, `lua`, `vscode`, and `keybindings`. A child-only umask of 000 still produces a 0700 directory containing the logged, live socket; an askpass round trip succeeds without logging its answer. A child-shell stdin barrier before exec lets tests install a foreign directory, actual Unix socket, or directory symlink before normal startup. Initialize and shutdown succeed while the bridge logs exclusive-create refusal and never logs a listening socket. Directory-entry type/inode, symlink target, socket type/inode, and sentinel contents survive shutdown. Ordinary commit, cancel, askpass, credential-log, and client-disconnect tests remain green. All launch variants share one pipe/log transport constructor; no production test hooks.
  - **Cleanup review:** Failure before exclusive directory creation touches no foreign resources. Bind failure removes only the newly owned empty directory; thread-spawn failure removes its owned socket and directory. Listener-thread return also removes owned resources, but the process-lifetime accept loop does not normally return at shutdown, so this is not a claim of normal-exit filesystem cleanup. The new tests reclaim their isolated temporary roots after bounded server exit and closing the foreign listener. Post-gate process inspection found no remaining `magit-lsp` server/helper processes; the disconnect test also verifies its owned Git/editor children exit naturally.
  - **Constraints:** No credentials in logs; no network service.
  - **Category:** simplify
  - **Net LOC:** +30
  - **Risk:** medium — helper startup/security boundary.
  - **Verify:** `cargo test --test editor_bridge`; `make test`
  - **Parallel:** serial
  - **Behaviour lost:** none.

- [x] **[P1] A02 — Enforce the positional request version** (`src/session.rs:545–601`; `src/protocol.rs:224–233`)
  - **Problem:** `ViewKeyParams.version` has no production reader; existing stale-key tests do not cover an old request arriving after a newer acknowledgement.
  - **Why it matters:** An old cursor row can target a different change.
  - **Change:** Resolve against the request-version tree, or refuse with a readable stale-view message. Apply a consistent positional-invoke policy rather than substituting the newest ack.
  - **Done when:** A v1 action submitted after v2 acknowledgement cannot stage the new occupant of its old row.
  - **Evidence:** Positional key/invoke dispatch binds the exact current/acked tree or warns; commit confirmation/marking requires the exact ack. `acked_tree()` remains the actual snapshot. Both clients transmit displayed invoke versions. Regressions prove replacement-row index safety, current/unacked versus acked identity, missing/future/discarded versions, acknowledged-only selection, and ungated menu/non-positional actions. `cargo test --test views --test keys_protocol --test navigation --test log_selection && make test && git diff --check` passed (40 targeted tests; Rust, Neovim, VSCode, keybindings). Real UI: `visit-file.scn` and `commit-menu-shows-actions.scn` both passed; inspected `build/ui/visit-file/visit-file.screen` and `build/ui/commit-menu-shows-actions/commit-menu.screen`.
  - **Constraints:** Stable node identities; no unbounded tree history.
  - **Category:** simplify
  - **Net LOC:** +25
  - **Risk:** high — action identity boundary.
  - **Verify:** `cargo test --test views --test keys_protocol --test navigation`; `make test`
  - **Parallel:** serial
  - **Behaviour lost:** none.

- [x] **[P1] A03 — Normalize selections against displayed identities** (`src/commands.rs:1013–1073,1086–1133`; `src/patch.rs:109–123`)
  - **Problem:** Selection handling assigns fresh diffs consecutive rows starting at an old file header; external edits/collapsed hunks invalidate those coordinates.
  - **Why it matters:** Visual actions can stage lines other than those selected.
  - **Change:** Resolve displayed tree selections into file/hunk identities and hunk-line offsets. Refresh Git only to verify/re-resolve identities; reject changed content rather than reconstructing old rows from fresh diffs.
  - **Done when:** External edits before a selection and selection below a collapsed hunk preserve the selected identities.
  - **Evidence (2026-10-27):** Request-tree section/path/content-hunk identities and `line_index` offsets now determine selection operations before fresh Git verification; all selected identities are checked before mutation. Real-Git view tests cover stage/unstage, external preceding edits, collapsed preceding hunks, retained acknowledged sources and changed-content refusal, with exact index/worktree assertions. The required `cargo test --test views --test git_backend --test visibility --test patch_cases && make test && git diff --check` passed. `scripts/ui run tests/ui/scenarios/visual-selection-identities.scn` passed for Vim at 100x30 and 80x24; inspected `build/ui/visual-selection-identities/vim-{100x30,80x24}/{exact-line-staged,exact-line-unstaged}.screen`. Owned manual and scripted UI sessions were stopped. Whole-file selections without displayed hunk identities conservatively refuse rather than guessing hidden content.
  - **Constraints:** Keep patch proptests; no text parsing or positional hunk IDs.
  - **Category:** simplify
  - **Net LOC:** +60
  - **Risk:** high — index mutation.
  - **Verify:** `cargo test --test views --test git_backend --test visibility --test patch_cases`; `make test`
  - **Parallel:** serial
  - **Behaviour lost:** none.

- [x] **[P1] A04 — Make client close messages reach the server** (`lua/magit-lsp/view.lua:537–558`; `clients/vscode/extension.js:1009–1030`; `src/server.rs:661–697`)
  - **Problem:** Clients send `view/close` notifications; the server handles requests only.
  - **Why it matters:** Closed views remain in server stacks/routing state.
  - **Change:** Use the existing request contract asynchronously in both clients; add client-seam coverage.
  - **Done when:** Repeated open/user-close cycles leave no stale view or parent-stack entry.
  - **Constraints:** No dual wire forms as compatibility shims.
  - **Implementation evidence (2026-10-27):** Both adapters now send asynchronous `view/close` requests using the existing server handler. Neovim uses its tracked per-client FIFO; VSCode awaits readiness, guards a disconnected/replaced client, and reports rejection without restoring closed state. Client-seam regressions cover repeated user closes, server-close suppression, failure/disconnect and independent client identity; existing edit cancellation is unchanged. `tests/server_async.rs::nested_user_close_and_reopen_does_not_retain_views_or_parents` drives nested open/close requests over real stdio, rejects resync of closed ids, checks id reuse and parent focus, and preserves a second repository. `tests/ui/scenarios/close-reopen-views.scn` uses text mode only to access native `:bwipeout!`, then reopens nested views and checks server-driven pops return to status. A user wipe of a main view also restores its hidden parent on the tracked event loop, after Neovim finishes replacing the wiped buffer.
  - **Verification evidence:** Final exact gate `make test && make ui-test && make vscode-test && git diff --check` exited **0** (Rust, all 54 Neovim cases, VSCode and keybinding checks, all 33 UI scenarios). Standalone `scripts/ui run tests/ui/scenarios/close-reopen-views.scn` exited **0**, covering standard/vim at 100x30 and 80x24. Inspected the real UI before/after a user wipe, and inspected saved nested, closed and reopened screens; all four final status screens show `Unstaged changes (2)`. Artifacts: `build/ui/close-reopen-views/{standard,vim}-{100x30,80x24}/{nested-before-close,user-closed-menu,user-closed-process,reopened-nested,returned-to-status}.screen`. All owned UI sessions stopped; no running `magit-lsp` processes remained. Earlier gate attempts exited **2**: an overstrict new stub rejected `view/resize`; the new UI scenario exposed missing main-parent restoration; an additional split-window unit assertion did not match native command behavior and was removed in favor of the passing real-UI regression. The stub and restoration were corrected before the final green gate.
  - **Category:** simplify
  - **Net LOC:** +15
  - **Risk:** low — existing handler reused.
  - **Verify:** `make test`; `make ui-test`; `make vscode-test`
  - **Parallel:** serial
  - **Behaviour lost:** none.

- [x] **[P1] A05 — Release session ownership before waiting for prompts** (`src/server.rs:344–365,669–673,1202–1236`)
  - **Problem:** Prompting commands hold the global sessions mutex. An ack/resize can block the reader on that mutex before it reads the prompt response.
  - **Why it matters:** A prompt can deadlock the transport.
  - **Change:** Split prompting flows into preparation/continuation: record the prompt and stable target, release session ownership, await, then reacquire to apply. Prompt waits hold no session-map lock; Git execution under that lock remains deferred as BUG-READ-1.
  - **Done when:** Prompt responses complete after intervening ack/resize; another repository remains serviceable.
  - **Constraints:** No timeout inflation or global queue workaround.
  - **Category:** merge
  - **Net LOC:** +80
  - **Risk:** high — command continuation ownership.
  - **Verify:** `cargo test --test server_async --test prompts --test commit_preflight`; `make test`; `make ui-test`
  - **Implementation scope:** `Session::start_invoke` / `start_key` return typed `Execution::Done` or `Prompt`, with exactly one session-owned continuation. The server waits outside ownership, then resumes once; synchronous/ScriptedUi callers drive those same methods. Menu options (including log `=f` paths) bind the menu/options snapshot and close invalidation, not the render version. Commit preflight binds HEAD, index bytes, patches and changed-file contents; discard additionally retains its resolved operation and displayed tree. Free-text commit selection is staged as well. No command replay, closure continuation, async View rewrite or new prompt wire version.
  - **Safety/limits:** New competing commands are refused readably; ack, resize, close and refresh remain available. Closed/cancelled/disconnected prompts cannot apply. An invalidated closed-view slot remains bounded to one until its answer/disconnect. Repository snapshots are deliberately conservative: unrelated repository changes or an index-cache rewrite can require a retry. Validation is not an atomic transaction against an external Git writer. Global-lock-over-Git/read responsiveness is **not solved** (the BUG-READ-1 test remains ignored); no registry/published-state redesign is claimed.
  - **Completed evidence (2026-10-27):** Exact gate `cargo test --test server_async --test prompts --test commit_preflight --test views && make test && make ui-test && git diff --check` exited 0: focused suites 47 passed / 1 intentionally ignored; full Rust/Lua/VSCode/keybindings and UI gates passed. Real stdio tests interpose ack/resize/resync before answering, service/mutate a second repository, refuse competing mutations, and prove cancellation/content-change/closed-view/disconnect/shutdown safety. `scripts/ui run tests/ui/scenarios/prompt-transport-responsive.scn` passed both profiles at 100x30 and 80x24; inspected waiting and answered screens (native author prompt, then `git commit --author=Alice`) at `build/ui/prompt-transport-responsive/{standard,vim}-{100x30,80x24}/{waiting,answered}.screen`. All owned UI runs were stopped; the timed-out initial full-gate attempt's remaining run was explicitly stopped before the successful rerun.
  - **Parallel:** serial
  - **Behaviour lost:** none.

- [ ] **[P1] A06 — Handle the quit effect in VSCode** (`clients/vscode/extension.js:749–778`)
  - **Problem:** Server `quit` falls into the JS unknown-effect branch, leaving client views alive after server views clear.
  - **Why it matters:** Broken root-quit lifecycle.
  - **Change:** Close/remove the affected session's views, dispose QuickPicks and reset contexts without recursive close requests.
  - **Done when:** Root quit leaves no visible/stored Magit view and reopening works.
  - **Constraints:** Preserve nested pop and unrelated repositories.
  - **Category:** simplify
  - **Net LOC:** +25
  - **Risk:** medium — document lifecycle.
  - **Verify:** `make vscode-test`; `npm --prefix clients/vscode run test:integration` with quit/reopen coverage.
  - **Parallel:** serial
  - **Behaviour lost:** none.

- [ ] **[P1] A07 — Remove JS edit-key policy and stale edit focus** (`clients/vscode/lib/edit.js:19–21,52–88`; `clients/vscode/extension.js:833–844,1116–1167`)
  - **Problem:** JS repeats profile keys/scope. `currentEdit()` returns the last edit even when another document has focus, swallowing view commands.
  - **Why it matters:** Incorrect dispatch and duplicate key policy.
  - **Change:** Use `effect.scope`, focused-document edit identity and generated invoke command IDs. Delete `CONFIRM_KEYS`, `CANCEL_KEYS` and `commandForKey`.
  - **Done when:** Profile changes need no JS key edits; status commands work with an edit in another tab.
  - **Constraints:** Keep save-before-confirm and cancel-on-close.
  - **Category:** delete
  - **Net LOC:** −55
  - **Risk:** medium — edit dispatch.
  - **Verify:** `make vscode-test`; `npm --prefix clients/vscode run test:integration` with bridge confirm/cancel coverage.
  - **Parallel:** serial
  - **Behaviour lost:** Hard-coded raw-key special case; generated bindings remain.

- [ ] **[P1] A08 — Bind each Lua prompt to its originating client** (`lua/magit-lsp/prompt.lua:13–44`; `lua/magit-lsp/init.lua:306–309`)
  - **Problem:** Async picker completion reads a mutable singleton client ID; overlapping repository prompts can redirect answers.
  - **Why it matters:** Answers can reach the wrong server.
  - **Change:** Capture/pass client ID through each prompt invocation and completion; remove the singleton.
  - **Done when:** Two fake clients answer in reverse order and receive only their own response.
  - **Constraints:** No secret answers in diagnostics.
  - **Category:** simplify
  - **Net LOC:** −10
  - **Risk:** low — explicit transport identity.
  - **Verify:** `make test-lua`; `make test`
  - **Parallel:** serial
  - **Behaviour lost:** none.

- [ ] **[P1] A09 — Include emitted process-log tokens in the legend** (`src/process_log.rs:131–142`; `src/semantic_tokens.rs:11–38`)
  - **Problem:** Emitted `magit.message.error`/`.warn` are absent from the legend and silently dropped by encoding.
  - **Why it matters:** Failure/warning styling never reaches clients.
  - **Change:** Add token types and VSCode contributions; test failed process-log output through encoding.
  - **Done when:** Decoded tokens carry emitted warning/error ranges in client theme vocabulary.
  - **Constraints:** Reuse Lua links; no hard-coded colors.
  - **Category:** simplify
  - **Net LOC:** +15
  - **Risk:** low — legend extension.
  - **Verify:** `cargo test --test process_log --test semantic_tokens`; `make test`
  - **Parallel:** serial
  - **Behaviour lost:** none.

- [ ] **[P1] A10 — Load repository configuration for actual sessions** (`src/server.rs:512–515,1131–1146`; `src/config.rs:288–300`)
  - **Problem:** Runtime `LoadOptions` omits `repo_path`; loader tests cover trusted repository settings the running server never loads.
  - **Why it matters:** Documented per-repository keys/settings are ignored.
  - **Change:** Resolve canonical root at open; merge/cache configuration and keymaps per root through the existing loader.
  - **Done when:** Real stdio sessions observe trusted config, ignore untrusted config, and isolate two roots.
  - **Constraints:** One layering implementation; preserve trust restrictions.
  - **Category:** merge
  - **Net LOC:** +35
  - **Risk:** medium — configuration ownership.
  - **Verify:** `cargo test --test config --test server_async`; `make test`
  - **Parallel:** serial
  - **Behaviour lost:** none.

- [ ] **[P1] A11 — Install the detached commit cancellation handle** (`src/server.rs:1385–1403`)
  - **Problem:** `spawn_commit` creates `_cancel` but never installs it on the commit thread.
  - **Why it matters:** Advertised progress cancellation cannot stop the Git child.
  - **Change:** Carry the handle into the worker thread and run under `runner::with_cancel`; release pending edit and end progress on cancellation.
  - **Done when:** Canceling a slow hook terminates the owned child, leaves HEAD unchanged and ends progress.
  - **Constraints:** Never kill unrelated processes or register probes as the mutation child.
  - **Category:** simplify
  - **Net LOC:** +20
  - **Risk:** medium — child lifecycle.
  - **Verify:** `cargo test --test editor_bridge --test server_async`; `make test`
  - **Parallel:** serial
  - **Behaviour lost:** none.

- [ ] **[P1] A12 — Test Git failures through the real dispatch path** (`src/server.rs:1202–1265,1461–1489`; `src/dispatch.rs:204–216`)
  - **Problem:** Test-only callers exercise production `server::mutation`, while real selection dispatch bypasses its readable-effect behavior with `?`.
  - **Why it matters:** Tests validate a second implementation instead of the runtime contract.
  - **Change:** Route real selection/command Git failures through readable effects; migrate five helper callers to Session/stdio seams, then delete the helper.
  - **Done when:** Actual requests return readable selection/index-lock errors; no `server::mutation` remains.
  - **Constraints:** Preserve stderr and real fixtures; protocol faults stay JSON-RPC errors.
  - **Category:** merge
  - **Net LOC:** −15
  - **Risk:** medium — error contract.
  - **Verify:** `cargo test --test errors --test dispatch --test views --test server_async`; `make test`
  - **Parallel:** serial
  - **Behaviour lost:** none.

- [ ] **[P2] A13 — Delete multi-version test scaffolding; retain soft-floor warnings** (`src/git.rs`; `src/errors.rs`; `tests/git_version.rs`; `tests/errors.rs`; existing Git-error effect boundary)
  - **Problem:** A dedicated fake-Git version harness and version matrices add test volume, but the below-floor warning is deliberate user-facing behavior, not dead runtime code.
  - **Why it matters:** Reduce test scaffolding without losing useful diagnosis when Git fails on an older installation.
  - **Change:** Retain the best-effort version probe, parser/comparison and failure-path callers needed to warn when installed Git is below 2.30.8. Ensure the warning reaches the user through a readable message effect, not only tracing/server stderr, while preserving the original Git failure and stderr. Delete the dedicated multi-version simulation harness and version-scenario matrices, including redundant parser/version tables. Move the genuine normal/piped stderr regression into general error coverage. Any retained warning-contract test must use a lightweight fake version response (stubbed process output or shared fake-Git support), never download, build or require an actual old Git binary. Do not add an elaborate warning suite or replacement multi-version runner.
  - **Done when:** A Git failure below the soft floor shows the original error plus an advisory warning naming the installed version and 2.30.8 recommendation. Successful commands do not trigger diagnosis; at/above-floor or unavailable/unparseable probe results add no below-floor warning. No dedicated multi-version test harness remains.
  - **Constraints:** Never block commands based on version. Keep diagnosis bounded, non-recursive and unable to mask the original failure. Keep documented 2.30.8 soft floor and integrity-pinned floor/latest CI; removing simulated version tests does not remove that CI coverage.
  - **Category:** delete
  - **Net LOC:** −200, revised estimate retaining runtime diagnosis and focused contract coverage.
  - **Risk:** low — preserve advisory behavior and original failure details.
  - **Verify:** `cargo test --test errors --test git_backend --test patch_cases`; `scripts/test_recommended_git.sh`; `make test`
  - **Parallel:** A24, A21
  - **Behaviour lost:** none; only dedicated multi-version test infrastructure and redundant scenario coverage are removed.

- [ ] **[P2] A14 — Delete unconsumed production facades** (`src/runner.rs:358–386`; `src/commands.rs:128–130,160–164`; `src/tree.rs:195–211`; `src/patch.rs:6–12,60–75,133–170`; `src/menu.rs:116–118`)
  - **Problem:** No production consumers found for `Runner`, `CommandCtx.cancel`, `Document`, `LineKind`/`Hunk::kind`/`is_change`, three `FileLayout` patch convenience methods, or `Group.columns`.
  - **Why it matters:** Dead surfaces and construction-only plumbing inflate code/tests.
  - **Change:** Remove these APIs/plumbing and trivial tests; retain mode/header Git quirks through real backend `patch_cases` assertions.
  - **Done when:** No alternate production API exists only for these implementation tests.
  - **Constraints:** Keep `CancelHandle`, `with_cancel`, normalization and proptests; recheck concurrent removals first.
  - **Category:** delete
  - **Net LOC:** −135
  - **Risk:** low — no production consumers found.
  - **Verify:** `cargo test --lib --test patch_cases --test status_render --test runner`; `make test`
  - **Parallel:** serial
  - **Behaviour lost:** Unused convenience APIs; none in current clients.

- [ ] **[P2] A15 — Remove repeated binding metadata from wire manifests** (`src/keymap.rs:178–190`; `src/manifest.rs:49–61,95–107`)
  - **Problem:** `doc` repeats `label`; unused `group` repeats command prefix; manifest bindings repeat their containing scope and wire structure.
  - **Why it matters:** Large generated-data duplication.
  - **Change:** Keep one description; delete unused group; share binding type inside `ScopeManifest`; derive scope from container. Remove `VscodeEntry.arg_key` if still unreferenced.
  - **Done when:** Description/scope occur once; required snapshots and bindings regenerate.
  - **Constraints:** Preserve command/key/mode/status semantics; no compatibility fields; account for concurrent snapshot cleanup.
  - **Category:** delete
  - **Net LOC:** −700, mostly generated data.
  - **Risk:** medium — intentional metadata-shape change.
  - **Verify:** `cargo test --test manifest --test keymap`; `make test`; `node clients/vscode/scripts/gen-keybindings.mjs --check`
  - **Parallel:** serial
  - **Behaviour lost:** Redundant manifest fields only.

- [ ] **[P2] A16 — Remove the hypothetical unavailable-backend state** (`src/commands.rs:77–80`; `src/menu.rs:36–37`; `src/session.rs:1347–1441`)
  - **Problem:** `Unavailable` is constructed only by a large injected-command test, not a current backend.
  - **Why it matters:** Speculative post-Phase-5 machinery.
  - **Change:** Delete both variants, branches/conversions and injected test/seam. Keep Implemented/Planned.
  - **Done when:** No unreachable backend-unavailable dispatch branch remains.
  - **Constraints:** Preserve every Planned entry and `magit.menu.disabled`, which pending-prefix dimming uses independently.
  - **Category:** delete
  - **Net LOC:** −160
  - **Risk:** low — no production producer.
  - **Verify:** `cargo test --test commands --test menus --test keymap`; `make test`
  - **Parallel:** A24, A21
  - **Behaviour lost:** Hypothetical backend-unavailable responses.

- [ ] **[P2] A17 — Construct each render and open view once** (`src/session.rs:157–216,407–450,1012–1053`)
  - **Problem:** Full/diff rendering duplicates version/tree/cursor/payload work; initial open/push duplicates view initialization.
  - **Why it matters:** DRY and lifecycle consistency.
  - **Change:** One render function with explicit delivery choice; one initial-view constructor given selected ID, view, placement and viewport.
  - **Done when:** Each version/tree/descriptor initialization has one implementation.
  - **Constraints:** Preserve ack fallback, text-mode replacement and pre-open notification ordering.
  - **Category:** merge
  - **Net LOC:** −75
  - **Risk:** medium — render lifecycle.
  - **Verify:** `cargo test --test views --test text_mode --test menu --test navigation`; `make test`
  - **Parallel:** serial
  - **Behaviour lost:** none.

- [ ] **[P1] A18 — Load status bodies once and only when visible** (`src/status_view.rs:50–99`)
  - **Problem:** Prepass checks file visibility but not parent section visibility; fallback duplicates diff/cache/error loading.
  - **Why it matters:** Collapsed sections can trigger unnecessary Git diffs on refresh.
  - **Change:** Check section visibility at the loading boundary; builder consumes prepared cache instead of a second Git-loading path.
  - **Done when:** Collapsed refresh loads no child diffs; expansion loads each needed body once.
  - **Constraints:** Preserve independent file/hunk visibility and lazy-file regression.
  - **Category:** merge
  - **Net LOC:** −10
  - **Risk:** medium — lazy-body behavior.
  - **Verify:** `cargo test --test visibility --test status_render`; `make test`; `make ui-test`
  - **Parallel:** serial
  - **Behaviour lost:** none.

- [ ] **[P1] A19 — Share client contract vectors and honor prompt defaults** (`tests/test_magit_lsp.lua:1549–1892`; `clients/vscode/test/test.js:76–126,398–468`; both prompt adapters)
  - **Problem:** Separate prompt/edit examples duplicate semantics; both adapters ignore a normal confirm's false default.
  - **Why it matters:** Unsafe default and duplicated contract tests.
  - **Change:** Shared named JSON prompt/render vectors run through both adapters: defaults, destructive focus, cancellation, empty text, custom/multi choice and resync. Keep minimal editor-specific masking/undo/widget checks.
  - **Done when:** Both clients consume shared semantics; false default does not select Yes.
  - **Constraints:** Preserve secrets; native checklist/JS multiselect remain distinct glue. Reuse already-open shared-vector work, not a competing format.
  - **Category:** merge
  - **Net LOC:** −120
  - **Risk:** medium — contract correction.
  - **Verify:** `make test-lua`; `make vscode-test`; `make test`; `make ui-test` with confirm coverage.
  - **Parallel:** serial
  - **Behaviour lost:** none; unsafe default corrected.

- [ ] **[P2] A20 — Parameterize the three stage-hunk resolver cases** (`tests/dispatch.rs:74–105`; `Cargo.toml`)
  - **Problem:** Three equality bodies differ only in target input.
  - **Why it matters:** Reduce repetition without a general assertion framework.
  - **Change:** Add `rstest` as a new shared dev-dependency; one test taking `NodeKind`, with named cases `stage_hunk_header_stages_hunk`, `stage_changed_line_stages_whole_hunk`, `stage_context_line_stages_whole_hunk`.
  - **Done when:** Three independently reported cases preserve the equality assertion.
  - **Constraints:** Leave section/file/selection/message cases separate. All three are synchronous L1; runtime unchanged.
  - **Category:** simplify
  - **Net LOC:** −18, including dependency/import overhead.
  - **Risk:** low — input parameter only.
  - **Verify:** `cargo test --test dispatch`
  - **Parallel:** serial
  - **Behaviour lost:** none.

- [ ] **[P2] A21 — Parameterize unusual-path status cases** (`tests/status.rs:22–58`)
  - **Problem:** Three real-Git bodies differ only in path input.
  - **Why it matters:** DRY without losing Git quirks.
  - **Change:** Do after: **Parameterize the three stage-hunk resolver cases**. One `rstest` body; retain `status_reports_path_with_spaces`, `status_reports_path_with_quote_and_backslash`, `status_reports_unicode_path` as named cases.
  - **Done when:** Three individually reported fixtures keep both original assertions.
  - **Constraints:** Reuse `rstest`; same layer; no runtime reduction.
  - **Category:** simplify
  - **Net LOC:** −20
  - **Risk:** low — fixture parameter only.
  - **Verify:** `cargo test --test status`
  - **Parallel:** A13, A24, after A20.
  - **Behaviour lost:** none.

- [ ] **[P2] A24 — Replace handwritten JSONC scanners** (`clients/vscode/lib/foldkeys.js:3–142`)
  - **Problem:** Two state machines implement standard comment/trailing-comma handling.
  - **Why it matters:** Excess parser code at a user-input boundary.
  - **Change:** Use an explicit maintained JSONC dependency with allowed comments/trailing commas and error reporting; remove scanners/exports.
  - **Done when:** Existing tricky strings/comments pass; malformed JSONC reports errors.
  - **Constraints:** Preserve fold mappings; no undocumented VSCode internals.
  - **Category:** simplify
  - **Net LOC:** −120
  - **Risk:** medium — new parser dependency.
  - **Verify:** `npm --prefix clients/vscode ci`; `make vscode-test`
  - **Parallel:** A13, A16, A21
  - **Behaviour lost:** none.

- [ ] **[P2] A25 — Move navigation policy out of Lua** (`lua/magit-lsp/view.lua:1005–1039`; `lua/magit-lsp/init.lua:220–229`; `src/commands.rs:972–1000`)
  - **Problem:** Lua sorts/classifies symbols and chooses targets; server separately implements section navigation.
  - **Why it matters:** Duplicate domain policy across boundary.
  - **Change:** Server tree navigation over node kinds returns cursor through normal outcome; `M.jump` becomes a thin invocation; delete local traversal/sorting.
  - **Done when:** Section/file/hunk jumps work without Lua choosing targets.
  - **Constraints:** Keep native outline support; structured nodes only.
  - **Category:** move-to-server
  - **Net LOC:** −25
  - **Risk:** medium — cursor semantics.
  - **Verify:** `cargo test --test visibility --test navigation`; `make test-lua`; `make ui-test` with navigation coverage.
  - **Parallel:** serial
  - **Behaviour lost:** none.

- [ ] **[P2] A26 — Delete local menu transition policy and unused normalization** (`clients/vscode/lib/menu.js:12–24,127–156`; `clients/vscode/extension.js:400–410`)
  - **Problem:** `isPopKey` duplicates server policy; `typedDelta` has only test consumers; normalization accepts unproduced status shapes.
  - **Why it matters:** Domain duplication and dead helpers.
  - **Change:** Stop queued work based on effects/view existence; remove pop list, delta helper/tests and unsupported status fallbacks; consume tagged status directly.
  - **Done when:** None of these local policies/helpers remain.
  - **Constraints:** Keep native QuickPick formatting and current structured Menu model. No additional presentation DTO.
  - **Category:** delete
  - **Net LOC:** −60
  - **Risk:** low — server remains transition authority.
  - **Verify:** `make vscode-test`
  - **Parallel:** serial
  - **Behaviour lost:** Unused APIs/unproduced status-shape acceptance; none supported.

- [ ] **[P2] A27 — Delete tests of trivial support/serialization plumbing** (`tests/prompts.rs:133–183,205–229`; `src/editor.rs:507–542`)
  - **Problem:** Queued-answer bookkeeping, direct serde shapes and a simple metadata match duplicate public integration protection.
  - **Why it matters:** Implementation-only test volume.
  - **Change:** Delete these blocks, retaining actual prompt round trips, cancellation, masking and bridge metadata assertions.
  - **Done when:** Public coverage remains without these trivial checks.
  - **Constraints:** Keep bridge failure/disconnect tests; do not replace deleted checks with snapshots.
  - **Category:** delete
  - **Net LOC:** −100
  - **Risk:** low — implementation-only coverage.
  - **Verify:** `cargo test --test prompts --test editor_bridge`; `make test`
  - **Parallel:** A24
  - **Behaviour lost:** none.

- [ ] **[P2] A28 — Mark retained near-term scaffolding with consumers** (`src/view.rs`; `src/config.rs`; `src/protocol.rs`; `src/semantic_tokens.rs`; `src/keymap.rs`; `lua/magit-lsp/highlights.lua`)
  - **Problem:** Small reserved seams lack local provenance, indistinguishable from abandoned scaffolding.
  - **Why it matters:** Controlled scope and understandable retention.
  - **Change:** Add `keep-for:` comments to retained surfaces listed in Review notes, citing actual open items/recorded decisions. Mark unapplied settings and unenforced prompt min/max inactive/reserved.
  - **Done when:** Each retained seam has a truthful local consumer/decision citation.
  - **Constraints:** No new implementation or hypothetical tests; exclude A14/A16 deletions.
  - **Category:** keep-with-reason
  - **Net LOC:** +20
  - **Risk:** low — provenance comments only.
  - **Verify:** `rg -n 'keep-for:' src lua`; `git diff --check`; compare cited roadmap items/decisions.
  - **Parallel:** A24
  - **Behaviour lost:** none.
  - **Keep-for:** R-3.1/§3.6; R-5.1; R-5.3; R-4.3; R-5.5; R-3.6; R-3.7; recorded §§3.2, 3.8–3.11, 3.15, 3.18, 4.1, 5.1. The notes explain which are decisions rather than open items.

- [ ] **[P1] A29 — Make VSCode text mode native read-only browsing** (`src/session.rs:1163–1174,1246–1249`; `clients/vscode/extension.js:74–82`; `clients/vscode/lib/context.js`; `clients/vscode/lib/views.js`; `clients/vscode/lib/requests.js`; `src/manifest.rs:198–208`; initialization capability types)
  - **Problem:** The existing VSCode document is a read-only virtual document; static clients may miss the entry mode update, while Magit capture and mirrored fold overrides prevent normal editor browsing.
  - **Why it matters:** Users need native navigation, search, selection and copy in the rendered view without a false editable-mode promise or losing stable selection during queued refreshes.
  - **Change:** Keep the server-owned read-only VSCode virtual document. Deliver reliable mode/context notifications for static clients across invoke/key/menu/setMode. While in text mode disable all ordinary Magit capture and shortcut overrides, including mirrored fold overrides, but retain an explicit return toggle/command. Native editor navigation/search/selection/copy operate on unchanged rendered text; queue refreshes while browsing and apply them normally on return. Neovim behavior is unchanged.
  - **Done when:** No entry path leaves server/client mode divergent; real VSCode UI verifies native navigation, search, select/copy, stable selection while refreshes queue, and toggle-back applies queued refreshes normally.
  - **Constraints:** No editable mode, scratch-copy feature, new file type or writable filesystem adapter. Do not change Neovim behavior.
  - **Category:** simplify
  - **Net LOC:** +40 (provisional; notification/context and key suppression may outweigh removed overrides).
  - **Risk:** medium — mode and static keybinding boundary.
  - **Verify:** `cargo test --test manifest --test text_mode`; `make vscode-test`; actual VSCode UI navigation/search/select/copy and toggle-back with queued refresh; `make test`
  - **Parallel:** serial
  - **Behaviour lost:** None of the working functionality.

- [ ] **[P2] A30 — Parameterize the three unstage-hunk resolver cases** (`tests/dispatch.rs:161–192`)
  - **Problem:** Three equality bodies differ only in target.
  - **Why it matters:** DRY.
  - **Change:** Do after: **Parameterize the three stage-hunk resolver cases**. One `rstest` body; named cases `unstage_hunk_header_unstages_hunk`, `unstage_changed_line_unstages_whole_hunk`, `unstage_context_line_unstages_whole_hunk`.
  - **Done when:** Three cases preserve the `UnstageHunk` assertion.
  - **Constraints:** Reuse dependency; synchronous L1 only; no stage/executor/async mixing; runtime unchanged.
  - **Category:** simplify
  - **Net LOC:** −20
  - **Risk:** low — input parameter only.
  - **Verify:** `cargo test --test dispatch`
  - **Parallel:** serial
  - **Behaviour lost:** none.

- [ ] **[P2] A31 — Parameterize the three discard-hunk resolver cases** (`tests/dispatch.rs:255–289`)
  - **Problem:** Three equality bodies differ only in target.
  - **Why it matters:** DRY.
  - **Change:** Do after: **Parameterize the three stage-hunk resolver cases**. Named cases `discard_hunk_header_needs_confirm`, `discard_changed_line_needs_confirm`, `discard_context_line_needs_confirm` share one `rstest` body.
  - **Done when:** Three cases preserve current `DiscardHunk` expectations.
  - **Constraints:** Preserve Planned behavior; coordinate existing interim-state cleanup; same synchronous L1 layer; runtime unchanged.
  - **Category:** simplify
  - **Net LOC:** −20
  - **Risk:** low — input parameter only.
  - **Verify:** `cargo test --test dispatch`
  - **Parallel:** serial
  - **Behaviour lost:** none.

- [ ] **[P2] A32 — Remove repeated canonical-key validation from Lua** (`lua/magit-lsp/keys.lua:70–127,171–207`; `lua/magit-lsp/keymap.lua:122–125`)
  - **Problem:** Lua duplicates canonical grammar validation and repeatedly parses already-validated server keys.
  - **Why it matters:** Boundary duplication.
  - **Change:** Keep canonical-to-native translation, split validated chords without another validator pass, avoid duplicate parsing and delete Rust-owned rejection-rule tests.
  - **Done when:** Valid keys translate losslessly without duplicate modifier/key-validity policy.
  - **Constraints:** Preserve native `from_lhs`, terminal notation and valid vectors; reuse existing shared-vector work.
  - **Category:** simplify
  - **Net LOC:** −60
  - **Risk:** medium — translation must remain lossless.
  - **Verify:** `make test-lua`; `cargo test --test keys_protocol --test keymap`; `make test`
  - **Parallel:** serial
  - **Behaviour lost:** Redundant Lua rejection API for invalid server keys; server validation remains.

- [ ] **[P1] A33 — Separate typed menu text from item activation keys** (`clients/vscode/extension.js:309–323`; `clients/vscode/lib/menu.js:120–124`)
  - **Problem:** Acceptance character-splits canonical item keys: `ctrl+t` begins with `c`; `g l` inserts Space.
  - **Why it matters:** Selecting an action can trigger a different menu/action.
  - **Change:** Server supplies activation steps from its existing interpretation: `-a` → two steps, `g l` → two steps, `ctrl+t` → one step. Forward verbatim on acceptance; `routeTyped` remains for actual typed text.
  - **Done when:** Requests for flags, spaced chords and modifier/named keys exactly match intended steps.
  - **Constraints:** No JS grammar/special command IDs; preserve Planned actions and A29's read-only native browsing mode.
  - **Category:** move-to-server
  - **Net LOC:** +35
  - **Risk:** medium — menu wire metadata/activation.
  - **Verify:** `cargo test --test menu --test menus`; `make vscode-test`; `npm --prefix clients/vscode run test:integration` with QuickPick activation coverage.
  - **Parallel:** serial
  - **Behaviour lost:** none.

- [ ] **[P1] A34 — Convert VSCode selections to inclusive rows correctly** (`clients/vscode/extension.js:984–1000`)
  - **Problem:** Exclusive editor selection end rows are sent directly as inclusive protocol rows, including an unselected row when ending at column zero.
  - **Why it matters:** Stage/unstage can include an extra changed line.
  - **Change:** Use ordered native start/end; convert the exclusive end to an inclusive protocol row, subtracting one for nonempty multiline selections ending at column zero; preserve the active cursor separately.
  - **Done when:** Thorough boundary regressions exercise forward and reverse equivalent selections; column-zero exclusive ends versus nonzero ends; single-line and empty/caret selections under existing intended semantics; first/last document rows and EOF with and without a final newline. Cover representative addition, deletion, context, and hunk-edge selections with adjacent unselected changed lines. Assert exact outgoing inclusive rows and preserved active cursor through registered command/dispatcher paths. Distinguish fast client request-shape checks from real-Git end-to-end stage and unstage checks: compare exact staged/unstaged diffs or index/worktree contents and prove neighboring boundary lines remain untouched, not merely that requests or operations succeed.
  - **Constraints:** Native boundary fix only; keep server inclusive contract; test registered dispatcher rather than exporting a test helper.
  - **Category:** simplify
  - **Net LOC:** +15 (provisional implementation estimate; excludes expanded regression coverage).
  - **Risk:** moderate — exclusive/inclusive boundaries interact with selection direction, cursor position, EOF, and patch application; request-shape checks alone cannot establish staging correctness.
  - **Verify:** `make vscode-test`; actual VSCode integration coverage through registered dispatch; `npm --prefix clients/vscode run test:integration`; `cargo test --test views --test git_backend --test patch_cases`; full `make test`.
  - **Parallel:** serial
  - **Behaviour lost:** none.

### Review notes

#### Inventory: suspicious names and parallel helpers

This initial inventory draws no conclusions by itself.

| Directory | Names/signatures to compare |
|---|---|
| src | `OpenView::render`/`render_full`; `Session::open_with_args`/`push_view`; `GitRepo::apply_index`/`command_in_with_env`; `CommandStatus`/`MenuStatus`; `Binding`/`ManifestBinding`; `Node`/`LineNode`/`Document`; `Runner`/`with_cancel`; `FileLayout` convenience patch builders/backend patch assembly |
| lua | `init.error_message`/`view.error_message`; `press`/`invoke` cursor extraction; canonical parsing/Rust grammar; `M.jump`/server `section_cursor`; `prompt.client_id`; parallel keymap deletion loops |
| clients | `statusInfo`; `describeItem`; `typedDelta`; `isPopKey`; `commandForKey`; `currentEdit`; `activateView`/`setContext`; two JSONC scanners; `press`/`invoke` |
| tests | Dispatch resolver assertions; three unusual-path status tests; three line-staging variants; three located-config errors; test-only `server::mutation` path |
| scripts | `_wait_for_socket`, `_wait_for_view`, `wait_prompt`, `wait_idle`, `wait_screen`; `FIXTURES`/`_BUILDERS`; completed color spike; future sequencer fixtures |

LSP document symbols were inspected for Rust, Lua, JS and test-harness representatives. Python LSP was unavailable; source declarations were used. The detailed inventories and mechanically seeded seam matrix follow.

#### 2. Completed command-ID seam matrix

##### Method

Identifiers were extracted from `src/commands.rs` and every `src/menus/*.rs` definition. Escaped literal alternatives were batch-searched across first-party Rust, Lua, JS, MJS and TOML. None of these result batches was truncated.

The deduplicated inventory contains **255 command IDs**:

- 65 common/profile/opener IDs;
- 190 menu-action IDs, including `commit.create`;
- 192 static menu-action occurrences contain a repeated `patch.apply-plain` and `open.patch-apply`, which is already counted among openers.

The independent TSV was used to cross-check occurrences, not to replace registry seeding.

##### Legend

- `C:n`: `src/commands.rs:n`.
- `M/file:n`: `src/menus/file.rs:n`; registry generation is `commands.rs:596–612`.
- `B(scope)`: shipped profile/config binding producer.
- `D`: generated dispatch-menu producer.
- `G`: generic client forwarding/effect handling, not an ID-specific handler:
  - Lua: `keymap.apply` → `view.press`/`view.invoke`;
  - JS: generated bindings → `pressCommand`/`invokeCommand`, or QuickPick forwarding.
- `F`: editor-native fold translation.
- `R`: registry/profile/manifest coverage.
- `P`: deliberate Planned coverage: registry invocation loop, menu-key loop, parity and independent audit.
- Additional test names denote focused behavioral coverage.
- `I`: implemented; `Planned`: intentionally displayed/routed without Git execution.

An absence of an ID literal in a client is **not** an unused consumer when `G` applies.

##### Common/profile/opener IDs — 65 rows

| ID | Server definition | Producer | Lua handler | JS handler | Tests | Verdict |
|---|---|---|---|---|---|---|
| common.refresh | C:260 | B(common), D | G; `init.refresh` | G | R, views, keys, client lifecycle | I |
| common.toggle-section | C:267 | B(common), D, F | G+F | G+F | R, visibility, client folding | I |
| common.show-section | C:274 | B(common), F | G+F | G+F | R, visibility, client folding | I |
| common.hide-section | C:281 | B(common), F | G+F | G+F | R, visibility, client folding | I |
| common.show-all | C:288 | B(common), F | G+F | G+F | R, visibility, JS integration | I |
| common.hide-all | C:295 | B(common), F | G+F | G+F | R, visibility | I |
| common.more-detail | C:302 | B(common), F | G+F | G | R, visibility | I |
| common.less-detail | C:309 | B(common), F | G+F | G+F | R, visibility, foldkeys | I |
| common.detail-level-1 | C:316 | B(common; standard) | G | G | R, keymap, visibility | I |
| common.detail-level-2 | C:323 | B(common; standard) | G | G | R, keymap, visibility | I |
| common.detail-level-3 | C:330 | B(common; standard) | G | G | R, keymap, visibility | I |
| common.detail-level-4 | C:337 | B(common; standard) | G | G | R, keymap, visibility | I |
| common.next-section | C:344 | B(common), F | G+F | G | R, visibility | I; parallel Lua navigation A25 |
| common.prev-section | C:351 | B(common), F | G+F | G | R, visibility | I; A25 |
| common.cancel | C:389 | B(common/edit/select) | G; edit adapter | G; edit adapter | R, keys, bridge | I; A07 |
| common.confirm | C:396 | B(common/edit/select) | G; edit adapter | G; edit adapter | R, bridge | I; A07 |
| common.quit | C:403 | B(common) | G; edit adapter | G | R, views, prompts | I; missing JS quit effect A06 |
| common.cycle-visibility | C:410 | B(common) | G | G | R, Planned registry loop | Planned; §5.1 decision, stale R-2.3 reference |
| common.next-sibling | C:416 | B(common) | G | G | R, Planned registry loop | Planned; §5.1 decision |
| common.prev-sibling | C:422 | B(common) | G | G | R, Planned registry loop | Planned; §5.1 decision |
| common.parent-section | C:428 | B(common) | G | G | R, Planned registry loop | Planned; §5.1 decision |
| common.toggle-text-mode | C:434 | B(common), D | G | G | R, text_mode, Lua | I; native VSCode browsing A29 |
| common.copy-value | C:441 | B(common), D | G | G | R, text_mode, process_log, Lua | I |
| common.visit | C:448 | B(status), D | Local visit exception | G | R, keymap, navigation/client tests | Planned server action; existing R-NV-4 |
| status.stage | C:358 | B(status), D | G | G | R, views, keys, dispatch | I |
| status.unstage | C:365 | B(status), D | G | G | R, views, keys, dispatch | I |
| status.stage-all | C:372 | B(status), D | G | G | R, views, stdio, clients | I |
| status.unstage-all | C:379 | B(status), D | G | G | R, views, stdio, clients | I |
| status.discard | C:454 | B(status), D | G | G | R, Planned loop, resolver rows | Planned; retain |
| status.reverse | C:460 | B(status), D | G | G | R, Planned loop | Planned; retain |
| status.apply | C:466 | B(status), D | G | G | R, Planned loop | Planned; retain |
| open.dispatch | C:472 | B(common/status), appended menu item | G | G | R, menu, menus, Lua/UI | I |
| open.commit | C:479 | B(status), D | G | G | R, menu, menus, bridge, JS | I |
| open.branch | C:492 | B(status), D | G | G | R, opener matrix | I |
| open.log | C:493 | B(status), D | G | G | R, opener matrix, keys_protocol | I: opens menu, not log view |
| open.diff | C:494 | B(status), D | G | G | R, opener matrix, prompts | I: opens menu |
| open.fetch | C:495 | B(status), D | G | G | R, opener matrix | I |
| open.pull | C:496 | B(status), D | G | G | R, opener matrix | I |
| open.push | C:497 | B(status), D | G | G | R, opener matrix | I |
| open.merge | C:498 | B(status), D | G | G | R, opener matrix | I |
| open.rebase | C:499 | B(status), D | G | G | R, opener matrix | I |
| open.stash | C:500 | B(status), D | G | G | R, opener matrix, dispatch profiles | I |
| open.tag | C:501 | B(status), D | G | G | R, opener matrix | I |
| open.reset | C:502 | B(status), D | G | G | R, opener matrix | I |
| open.revert | C:503 | B(status), D | G | G | R, opener matrix | I |
| open.cherry-pick | C:505 | B(status), D | G | G | R, opener matrix | I |
| open.bisect | C:509 | B(status), D | G | G | R, opener matrix | I |
| open.remote | C:510 | B(status), D | G | G | R, opener matrix | I |
| open.submodule | C:511 | B(status), D | G | G | R, opener matrix | I |
| open.worktree | C:512 | B(status), D | G | G | R, opener matrix | I |
| open.ignore | C:513 | B(status), D | G | G | R, opener matrix | I |
| open.patch-apply | C:515; M/patch:78 | B(status), D, patch-create item | G | G | R, opener matrix, audit | I |
| open.patch-create | C:520 | B(status), D | G | G | R, opener matrix | I |
| open.refs | C:524 | B(status), D | G | G | R, Planned loop | Planned R-5.3; retain |
| open.cherries | C:526 | B(status), D | G | G | R, Planned loop | Planned menu entry; retain |
| open.run | C:531 | B(status), D | G | G | R, Planned loop | Planned menu entry; retain |
| open.process-log | C:533 | B(status), D | G | G | R, process_log, views/UI | I |
| process-log.clear | C:540 | B(process-log) | G | G | R, process_log | I |
| select.toggle-mark | C:547 | B(select) | G | G | R, Planned loop | Planned R-5.1 |
| select.enter-rev | C:553 | B(select) | G | G | R, Planned loop | Planned R-5.1 |
| menu.save-args | C:559 | B(menu), menu control resolver | G | G | R, menu | I |
| menu.reset-args | C:566 | B(menu), menu control resolver | G | G | R, menu | I |
| commit-message.history-prev | C:573 | B(commit-message) | G; edit filter currently omits action | G; edit adapter currently omits action | R, Planned loop, keymap | Planned R-4.2; adapters need future implementation |
| commit-message.history-next | C:579 | B(commit-message) | G; edit filter omits action | G; edit adapter omits action | R, Planned loop | Planned R-4.2 |
| commit-message.add-signoff | C:585 | B(commit-message) | G; edit filter omits action | G; edit adapter omits action | R, Planned loop | Planned R-4.2 |

##### Menu-action IDs — 190 rows

For every row below, the producer is the named menu item, both clients use `G`, and Planned actions terminate in server Planned handling rather than Git. The repeated cells are explicit to distinguish generic routing from absent consumers.

| ID | Server definition | Producer | Lua | JS | Tests | Verdict |
|---|---|---|---|---|---|---|
| commit.create | C:486; M/commit:41 | commit item | G | G | preflight, bridge, menu | I |
| commit.extend | M/commit:46 | commit item | G | G | P; JS model fixture | Planned |
| commit.amend | M/commit:47 | commit item | G | G | P | Planned |
| commit.reword | M/commit:48 | commit item | G | G | P | Planned |
| commit.reshelve | M/commit:51 | commit item | G | G | P | Planned |
| commit.fixup | M/commit:61 | commit item | G | G | P | Planned |
| commit.squash | M/commit:62 | commit item | G | G | P | Planned |
| commit.alter | M/commit:63 | commit item | G | G | P | Planned |
| commit.augment | M/commit:66 | commit item | G | G | P | Planned |
| commit.revise | M/commit:71 | commit item | G | G | P | Planned |
| commit.instant-fixup | M/commit:79 | commit item | G | G | P | Planned |
| commit.instant-squash | M/commit:86 | commit item | G | G | P | Planned |
| commit.reword-past | M/commit:93 | commit item | G | G | P | Planned |
| commit.autofixup | M/commit:105 | commit item | G | G | P | Planned |
| commit.absorb-modules | M/commit:112 | commit item | G | G | P | Planned |
| branch.configure-description | M/branch:32 | branch item | G | G | P | Planned |
| branch.update-default | M/branch:64 | branch item | G | G | P | Planned |
| branch.checkout | M/branch:76 | branch item | G | G | P | Planned |
| branch.checkout-local | M/branch:83 | branch item | G | G | P | Planned |
| branch.orphan | M/branch:90 | branch item | G | G | P | Planned |
| branch.checkout-remote | M/branch:97 | branch item | G | G | P | Planned |
| branch.create-and-checkout | M/branch:109 | branch item | G | G | P | Planned |
| branch.spinoff | M/branch:116 | branch item | G | G | P | Planned |
| branch.worktree-checkout | M/branch:123 | branch item | G | G | P | Planned |
| branch.create | M/branch:135 | branch item | G | G | P | Planned |
| branch.spinout | M/branch:142 | branch item | G | G | P | Planned |
| branch.worktree-create | M/branch:149 | branch item | G | G | P | Planned |
| branch.configure | M/branch:161 | branch item | G | G | P | Planned |
| branch.rename | M/branch:166 | branch item | G | G | P | Planned |
| branch.reset | M/branch:167 | branch item | G | G | P | Planned |
| branch.delete | M/branch:168 | branch item | G | G | P | Planned |
| branch.shelve | M/branch:171 | branch item | G | G | P | Planned |
| branch.unshelve | M/branch:178 | branch item | G | G | P | Planned |
| fetch.from-pushremote | M/fetch:26 | fetch item | G | G | P | Planned |
| fetch.from-upstream | M/fetch:33 | fetch item | G | G | P | Planned |
| fetch.other | M/fetch:40 | fetch item | G | G | P | Planned |
| fetch.all | M/fetch:47 | fetch item | G | G | P | Planned |
| fetch.branch | M/fetch:59 | fetch item | G | G | P | Planned |
| fetch.refspec | M/fetch:66 | fetch item | G | G | P | Planned |
| fetch.modules | M/fetch:73 | fetch item | G | G | P | Planned |
| fetch.configure | M/fetch:84 | fetch item | G | G | P | Planned |
| pull.from-pushremote | M/pull:27 | pull item | G | G | P | Planned |
| pull.from-upstream | M/pull:34 | pull item | G | G | P | Planned |
| pull.other | M/pull:41 | pull item | G | G | P | Planned |
| pull.into-upstream | M/pull:52 | pull item | G | G | P | Planned |
| pull.fetch-remotes | M/pull:63 | pull item | G | G | P | Planned |
| pull.fetch-remotes-prune | M/pull:70 | pull item | G | G | P | Planned |
| push.to-pushremote | M/push:36 | push item | G | G | P | Planned |
| push.to-upstream | M/push:43 | push item | G | G | P | Planned |
| push.current | M/push:50 | push item | G | G | P | Planned |
| push.other | M/push:62 | push item | G | G | P | Planned |
| push.refspecs | M/push:69 | push item | G | G | P | Planned |
| push.matching | M/push:76 | push item | G | G | P | Planned |
| push.tag | M/push:81 | push item | G | G | P | Planned |
| push.tags | M/push:82 | push item | G | G | P | Planned |
| push.notes-ref | M/push:85 | push item | G | G | P | Planned |
| push.configure | M/push:96 | push item | G | G | P | Planned |
| merge.plain | M/merge:43 | merge item | G | G | P | Planned |
| merge.editmsg | M/merge:46 | merge item | G | G | P | Planned |
| merge.nocommit | M/merge:53 | merge item | G | G | P | Planned |
| merge.absorb | M/merge:60 | merge item | G | G | P | Planned |
| merge.preview | M/merge:67 | merge item | G | G | P | Planned |
| merge.squash | M/merge:74 | merge item | G | G | P | Planned |
| merge.dissolve | M/merge:81 | merge item | G | G | P | Planned |
| rebase.onto-pushremote | M/rebase:49 | rebase item | G | G | P | Planned |
| rebase.onto-upstream | M/rebase:56 | rebase item | G | G | P | Planned |
| rebase.elsewhere | M/rebase:63 | rebase item | G | G | P | Planned |
| rebase.interactive | M/rebase:75 | rebase item | G | G | P | Planned |
| rebase.subset | M/rebase:82 | rebase item | G | G | P | Planned |
| rebase.edit-commit | M/rebase:89 | rebase item | G | G | P | Planned |
| rebase.reword-commit | M/rebase:96 | rebase item | G | G | P | Planned |
| rebase.remove-commit | M/rebase:103 | rebase item | G | G | P | Planned |
| rebase.autosquash | M/rebase:110 | rebase item | G | G | P | Planned |
| rebase.change-dates | M/rebase:117 | rebase item | G | G | P | Planned |
| rebase.continue | M/rebase:129 | rebase item | G | G | P | Planned |
| rebase.abort | M/rebase:134 | rebase item | G | G | P | Planned |
| stash.both | M/stash:22 | stash item | G | G | P | Planned |
| stash.index | M/stash:23 | stash item | G | G | P | Planned |
| stash.worktree | M/stash:26 | stash item | G | G | P | Planned |
| stash.keep-index | M/stash:33 | stash item | G | G | P | Planned |
| stash.push | M/stash:38 | stash item | G | G | P | Planned |
| stash.snapshot-both | M/stash:46 | stash item | G | G | P | Planned |
| stash.snapshot-index | M/stash:53 | stash item | G | G | P | Planned |
| stash.snapshot-worktree | M/stash:60 | stash item | G | G | P | Planned |
| stash.wip-commit | M/stash:67 | stash item | G | G | P | Planned |
| stash.apply | M/stash:77 | stash item | G | G | P | Planned |
| stash.pop | M/stash:78 | stash item | G | G | P | Planned |
| stash.drop | M/stash:79 | stash item | G | G | P | Planned |
| stash.list | M/stash:85 | stash item | G | G | P | Planned |
| stash.show | M/stash:86 | stash item | G | G | P | Planned |
| stash.branch | M/stash:94 | stash item | G | G | P | Planned |
| stash.branch-here | M/stash:101 | stash item | G | G | P | Planned |
| stash.format-patch | M/stash:108 | stash item | G | G | P | Planned |
| tag.create | M/tag:26 | tag item | G | G | P | Planned |
| tag.release | M/tag:29 | tag item | G | G | P | Planned |
| tag.delete | M/tag:39 | tag item | G | G | P | Planned |
| tag.prune | M/tag:40 | tag item | G | G | P | Planned |
| reset.branch | M/reset:17 | reset item | G | G | P | Planned |
| reset.file | M/reset:24 | reset item | G | G | P | Planned |
| reset.mixed | M/reset:36 | reset item | G | G | P | Planned |
| reset.soft | M/reset:43 | reset item | G | G | P | Planned |
| reset.hard | M/reset:50 | reset item | G | G | P | Planned |
| reset.keep | M/reset:57 | reset item | G | G | P | Planned |
| reset.index | M/reset:64 | reset item | G | G | P | Planned |
| reset.worktree | M/reset:71 | reset item | G | G | P | Planned |
| cherry-pick.pick | M/cherry_pick:35 | cherry-pick item | G | G | P | Planned |
| cherry-pick.apply | M/cherry_pick:42 | cherry-pick item | G | G | P | Planned |
| cherry-pick.harvest | M/cherry_pick:49 | cherry-pick item | G | G | P | Planned |
| cherry-pick.squash | M/cherry_pick:56 | cherry-pick item | G | G | P | Planned |
| cherry-pick.donate | M/cherry_pick:68 | cherry-pick item | G | G | P | Planned |
| cherry-pick.spinout | M/cherry_pick:75 | cherry-pick item | G | G | P | Planned |
| cherry-pick.spinoff | M/cherry_pick:82 | cherry-pick item | G | G | P | Planned |
| revert.commit | M/revert:34 | revert item | G | G | P | Planned |
| revert.changes | M/revert:41 | revert item | G | G | P | Planned |
| bisect.start | M/bisect:29 | bisect item | G | G | P | Planned |
| bisect.run | M/bisect:32 | bisect item | G | G | P | Planned |
| bisect.good | M/bisect:42 | bisect item | G | G | P | Planned |
| bisect.mark | M/bisect:43 | bisect item | G | G | P | Planned |
| bisect.skip | M/bisect:44 | bisect item | G | G | P | Planned |
| bisect.reset | M/bisect:47 | bisect item | G | G | P | Planned |
| remote.add | M/remote:36 | remote item | G | G | P | Planned |
| remote.rename | M/remote:39 | remote item | G | G | P | Planned |
| remote.remove | M/remote:46 | remote item | G | G | P | Planned |
| remote.configure | M/remote:53 | remote item | G | G | P | Planned |
| remote.prune | M/remote:60 | remote item | G | G | P | Planned |
| remote.prune-refspecs | M/remote:67 | remote item | G | G | P | Planned |
| remote.unshallow | M/remote:74 | remote item | G | G | P | Planned |
| remote.update-default | M/remote:81 | remote item | G | G | P; audit-negative fixture | Planned |
| submodule.add | M/submodule:27 | submodule item | G | G | P | Planned |
| submodule.register | M/submodule:30 | submodule item | G | G | P | Planned |
| submodule.populate | M/submodule:37 | submodule item | G | G | P | Planned |
| submodule.update | M/submodule:44 | submodule item | G | G | P | Planned |
| submodule.synchronize | M/submodule:51 | submodule item | G | G | P | Planned |
| submodule.unpopulate | M/submodule:58 | submodule item | G | G | P | Planned |
| submodule.remove | M/submodule:65 | submodule item | G | G | P | Planned |
| submodule.list | M/submodule:77 | submodule item | G | G | P | Planned |
| submodule.fetch | M/submodule:84 | submodule item | G | G | P | Planned |
| worktree.checkout | M/worktree:17 | worktree item | G | G | P | Planned |
| worktree.create | M/worktree:24 | worktree item | G | G | P | Planned |
| worktree.move | M/worktree:36 | worktree item | G | G | P | Planned |
| worktree.delete | M/worktree:43 | worktree item | G | G | P | Planned |
| worktree.visit | M/worktree:50 | worktree item | G | G | P | Planned |
| ignore.topdir | M/ignore:17 | ignore item | G | G | P | Planned |
| ignore.subdir | M/ignore:24 | ignore item | G | G | P | Planned |
| ignore.gitdir | M/ignore:31 | ignore item | G | G | P | Planned |
| ignore.system | M/ignore:38 | ignore item | G | G | P | Planned |
| ignore.skip-worktree | M/ignore:50 | ignore item | G | G | P | Planned |
| ignore.no-skip-worktree | M/ignore:57 | ignore item | G | G | P | Planned |
| ignore.assume-unchanged | M/ignore:69 | ignore item | G | G | P | Planned |
| ignore.no-assume-unchanged | M/ignore:76 | ignore item | G | G | P | Planned |
| log.current | M/log:43 | log item | G | G | P | Planned |
| log.other | M/log:48 | log item | G | G | P | Planned |
| log.head | M/log:49 | log item | G | G | P | Planned |
| log.related | M/log:50 | log item | G | G | P | Planned |
| log.local-branches | M/log:58 | log item | G | G | P | Planned |
| log.all-branches | M/log:65 | log item | G | G | P | Planned |
| log.all-references | M/log:72 | log item | G | G | P | Planned |
| log.reflog-objects | M/log:79 | log item | G | G | P | Planned |
| log.matching-branches | M/log:86 | log item | G | G | P | Planned |
| log.matching-tags | M/log:93 | log item | G | G | P | Planned |
| log.merged | M/log:98 | log item | G | G | P | Planned |
| log.reflog-current | M/log:106 | log item | G | G | P | Planned |
| log.reflog-other | M/log:113 | log item | G | G | P | Planned |
| log.reflog-head | M/log:120 | log item | G | G | P | Planned |
| log.wiplog-index | M/log:132 | log item | G | G | P | Planned |
| log.wiplog-worktree | M/log:139 | log item | G | G | P | Planned |
| log.shortlog | M/log:148 | log item | G | G | P | Planned |
| diff.dwim | M/diff:42 | diff item | G | G | P | Planned |
| diff.range | M/diff:43 | diff item | G | G | P | Planned |
| diff.paths | M/diff:44 | diff item | G | G | P | Planned |
| diff.unstaged | M/diff:47 | diff item | G | G | P | Planned |
| diff.staged | M/diff:54 | diff item | G | G | P | Planned |
| diff.worktree | M/diff:61 | diff item | G | G | P | Planned |
| diff.show-commit | M/diff:68 | diff item | G | G | P | Planned |
| diff.show-stash | M/diff:75 | diff item | G | G | P | Planned |
| patch.apply-maildir | M/patch:37 | patch-apply item | G | G | P | Planned |
| patch.apply-patches | M/patch:44 | patch-apply item | G | G | P | Planned |
| patch.apply-plain | M/patch:51,81 | both patch menus | G | G | P; two audit occurrences | Planned |
| patch.create | M/patch:73 | patch-create item | G | G | P | Planned |
| patch.save | M/patch:88 | patch-create item | G | G | P | Planned |
| patch.request-pull | M/patch:95 | patch-create item | G | G | P | Planned |
| blame.start | M/blame:17 | blame item | G | G | P | Planned |
| blame.removal | M/blame:20 | blame item | G | G | P | Planned |
| blame.reverse | M/blame:27 | blame item | G | G | P | Planned |
| blame.echo | M/blame:32 | blame item | G | G | P | Planned |
| blame.next-chunk | M/blame:40 | blame item | G | G | P | Planned |
| blame.next-chunk-same-commit | M/blame:47 | blame item | G | G | P | Planned |
| blame.previous-chunk | M/blame:54 | blame item | G | G | P | Planned |
| blame.previous-chunk-same-commit | M/blame:61 | blame item | G | G | P | Planned |
| blame.cycle-style | M/blame:72 | blame item | G | G | P | Planned |

**Conclusion:** no command ID in this inventory is demonstrably orphaned merely because it lacks a client literal. Genuine seams to change are the specialized client policies and transport mismatches already identified, plus A33 below.

---

#### 3. Per-field `view/*` seam matrix

Definitions are `src/protocol.rs` unless a nested type is named. Nested types are expanded once below; every use refers to that expansion.

Abbreviations:

- `S`: server/session producer or consumer named in the cell.
- `L`: Lua view/prompt/buffer adapter.
- `J`: JS extension or helper.
- `H`: Rust Session/stdio harness.
- `LC`: Lua integration tests.
- `JC`: JS unit/lifecycle tests.
- `reserved`: no current production value/functional consumer; disposition appears in §6.

##### Requests, notifications and responses

| Field | Definition | Producer → consumer | Lua | JS | Tests / verdict |
|---|---|---|---|---|---|
| view/open.kind | P160 | clients → `create_view` | sends status | sends status | H; consumed |
| view/open.args | P163 | clients → open/create view | sends repo args | sends repo args | H; opaque envelope |
| view/open.args.repo_path | P170–174 | clients → `open_view` | sends | sends | stdio; A10 |
| view/open.args.saved_args | session1456 | caller → menu initialization | no normal producer | no normal producer | menu test; consumed public argument |
| view/open.placement | P165 | caller → descriptor | omitted normally | omitted normally | H; advisory public override |
| view/open.viewport | P166 | clients → render context | sends screen size | sends fixed size | H; Viewport below |
| view/close.viewId | P188 | clients → close handler | wrong message form | wrong message form | A04 |
| view/invoke.viewId | P201 | clients → session lookup | sends | sends | H/LC/JC |
| view/invoke.command | P202 | clients → registry | generic | generic | command matrix |
| view/invoke.args | P204 | clients → command context | opaque | opaque | JC verifies opaque nested data |
| view/invoke.args.recursive | commands177–181 | fold adapters → visibility command | sends | sends | visibility/fold tests |
| view/invoke.args.editId | commands185–186 | edit adapters → edit registry | sends | sends | bridge tests |
| view/invoke.cursor | P206 | clients → action resolution | sends | sends | H/LC/JC; Cursor below |
| view/invoke.selection | P208 | clients → selection normalization | sends | sends | views; JS conversion A34 |
| view/invoke response.effects | P212 | server → effect application | applies | applies | H/LC/JC; Effect below |
| view/key.viewId | P225 | clients → session lookup | sends | sends | H/LC/JC |
| view/key.version | P227 | clients → **no production read** | sends | sends | tests miss newer-ack case; A02 |
| view/key.key | P229 | clients → key handler | canonical steps | canonical steps | keys tests; acceptance bug A33 |
| view/key.editorMode | P230 | clients → binding resolution | native mode | derived selection | keys_protocol |
| view/key.cursor | P231 | clients → action/pending resolution | sends | sends | H/LC; Cursor below |
| view/key.selection | P233 | clients → selection normalization | sends | sends | A03/A34 |
| view/rendered.viewId | P335 | clients → ack | sends | sends | navigation/views |
| view/rendered.version | P336 | clients → ack | sends applied version | sends version | navigation/views |
| view/resize.viewId | P343 | client → resize | sends | no producer | menu/stdio; native JS menu exception |
| view/resize.cols | P344 | client → menu layout | native width | absent | resize tests |
| view/resize.rows | P345 | client → stored viewport | native height | absent | resize tests; width-only current layout |
| view/resync.viewId | P323 | clients → full render lookup | sends | sends | views/LC/JC |
| view/setMode.viewId | P309 | caller → session | toggle used instead | toggle used instead | text_mode |
| view/setMode.mode | P310 | caller → mode transition | toggle used instead | toggle used instead | text_mode; A29 |
| view/setMode response.effects | alias P314 | server → clients | generic effects | generic effects | text_mode |
| view/promptRespond.promptId | P434 | client → pending prompt map | singleton routing bug | sends | A08 |
| view/promptRespond.cancelled | P435 | client → cancel/drop answer | sends | sends | prompt/bridge/LC/JC |
| view/promptRespond.value | P437 | client → typed answer conversion | sends | sends | PromptValue below |
| view/render.viewId | P118 | session → view lookup | applies | applies | H/LC/JC |
| view/render.version | P119 | session → applied state/ack | stores/acks | stores/acks | views/LC/JC |
| view/render.baseVersion | P121 | session → edit safety check | checks | checks | resync tests |
| view/render.text | P123 | session → full replacement | applies | applies | H/LC/JC |
| view/render.edits | P125 | session → incremental replacement | applies | applies | LineEdit below |
| view/render.cursor | P127 | session → editor cursor | applies | applies | views/visibility/client tests |
| view/render.mode | P129 | session → native mode/context | applies | applies if received | entry notification absent for static; A29 |
| view/render.pending | P132 | menu state → client | debug observation | ignored | menu/LC; real debug consumer |
| view/render.menu | P135 | menu view → native widget model | ignores; displays text | QuickPick model | menu/JC; nested Menu below |
| view/keymap.viewId | P289 | session/edit bridge → view | routes | intentionally no handler | keys/menu/LC |
| view/keymap.scope | P290 | session → mapping context | reads/stores | static descriptor instead | keys/menu |
| view/keymap.version | P291 | session → wire metadata | no functional read | no handler | metadata echo; reserved §3.9 |
| view/keymap.capture | P292 | session → capture mappings | applies | static descriptor instead | keys/LC |
| view/keymap.bindings | P293 | session → native maps | applies | generated manifest instead | Binding below |
| view/keymap.mode | P295 | session → text-mode maps | applies | no handler | text_mode/LC |
| view/keymap.pending | P298 | menu state → client | debug observation | no handler | menu/LC |
| view/keymap.liveKeys | P301 | menu state → client | debug key inventory | no handler | menu/LC; not a dead producer |
| view/effects.effects | P148 | detached work → client | applies | applies | bridge; Effect below |
| view/prompt.promptId | P380 | bridge-generated ID → response | returns | returns | bridge/stdio/LC/JC |
| view/prompt.kind | P381 | command → native picker | dispatches | dispatches | four kinds tested |
| view/prompt.title | P383 | command → native prompt | displays | displays | LC/JC/stdio |
| view/prompt.detail | P385 | optional server metadata | no stock display | displays | reserved/currently no command detail |
| view/prompt.yesLabel | P387 | optional server label | displays | displays | LC/JC |
| view/prompt.noLabel | P389 | optional server label | displays | displays | LC/JC |
| view/prompt.destructive | P391 | optional server policy | No-first | No-first | LC/JC |
| view/prompt.default | P394 | server default → picker | text only | text only | confirm ignored; A19 |
| view/prompt.placeholder | P396 | menu option → picker | ignored by current adapter | displays | prompt/JC |
| view/prompt.secret | P398 | askpass → masked input | inputsecret | password InputBox | bridge/LC |
| view/prompt.items | P400 | option/selector → picker | forwards items | maps items | PromptItem below |
| view/prompt.allowCustom | P402 | choice reader → custom option | implements | implements | prompts/LC/JC |
| view/prompt.preselected | P404 | multi-choice request → picker | implements | implements | LC; reserved production emitter |
| view/prompt.min | P406 | **no current producer** | not enforced | not enforced | reserved §3.6; not working validation |
| view/prompt.max | P408 | **no current producer** | not enforced | not enforced | reserved §3.6; not working validation |
| view/key.traceId | server1573–1575, not typed struct | Lua instrumentation → tracing | sends | normally absent | trace/UI |
| view/invoke.traceId | same | Lua instrumentation → tracing | sends | normally absent | trace/UI |

`view/prompt.error` is mentioned in design prose but is **not present in the implemented wire struct**. Likewise, select-mode options described in §3.7 have not become implemented typed `view/*` parameters.

##### Shared nested types

| Field | Definition | Producer → consumer | Lua | JS | Tests / verdict |
|---|---|---|---|---|---|
| ViewDescriptor.id | view145 | session → view identity | uses | uses | H/LC/JC |
| ViewDescriptor.kind | view146 | View → presentation | stores | chooses widget | H/LC/JC |
| ViewDescriptor.uri | view147 | session → document identity | uses verbatim | URI converter | navigation/clients |
| ViewDescriptor.title | view148 | View → native title | stores | title/URI label | bridge/client tests |
| ViewDescriptor.placement | view149 | server → advisory placement | applies | not honored by current document opener | §3.2/§3.11 glue |
| ViewDescriptor.mode | view150 | View → native mode | applies | context | text_mode/A29 |
| ViewDescriptor.scope | view151 | View → key context | stores; keymap also supplies | context | keymap/client tests |
| ViewDescriptor.capture | view152 | View → capture state | stores; keymap applies | context | key tests |
| ViewDescriptor.viewport | view153 | session metadata → clients | ignored as echo | ignored as echo | small §3.2 metadata seam |
| Viewport.cols | view131 | editor → server layout | supplies | fixed 100 | menu/resize |
| Viewport.rows | view132 | editor → server stored size | supplies | fixed 30 | menu/resize |
| Cursor.line | P93 | editor/server → resolution/cursor | native conversion | native conversion | views/navigation |
| Cursor.column | P94 | editor/server → cursor | native column | native character | cursor continuity |
| Selection.startLine | P101 | editor → normalization | inclusive normalized | min anchor/active | views; A34 |
| Selection.endLine | P102 | editor → normalization | inclusive normalized | incorrectly includes exclusive end row | A34 |
| LineEdit.startLine | P109 | text diff → native edit | uses | uses | LC/JC/views |
| LineEdit.endLine | P110 | text diff → native edit | exclusive | exclusive | LC/JC/views |
| LineEdit.lines | P111 | text diff → native edit | verbatim | verbatim | LC/JC/views |
| Binding.command | keymap48 | registry → native forwarder | uses | static generation | keymap/manifest/JC |
| Binding.keys | keymap50 | profiles → native forwarder | translates | static generation | keymap/manifest/LC |
| Binding.modes | keymap52 | config → native binding modes | applies | manifest carries metadata | keys_protocol |
| Binding.label | keymap54 | command doc → native description | uses | static manifest | A15 |
| Binding.doc | keymap56 | **same source as label** | no runtime use | no runtime use | Markdown/snapshots; A15 |
| Binding.group | keymap58 | command prefix → metadata | no runtime use | no runtime use | snapshots; A15 |
| Binding.status | keymap60 | command status → display | visit exception reads | static manifest | R-NV-4/A15 |
| PromptItem.id | P368 | server → selected answer | returns | returns | LC/JC |
| PromptItem.label | P369 | server → native picker | displays | displays | LC/JC |
| PromptItem.description | P371 | server → native picker | formats | displays | LC/JC |
| PromptItem.detail | P373 | server → native picker | forwarded; stock formatter omits | displays | fixtures; reserved actual producer |
| PromptChoiceAnswer.id | P415 | picker → `ServerUi::choice` | sends | sends | prompts/LC/JC |
| PromptChoiceAnswer.custom | P416 | input → `ServerUi::choice` | sends | sends | prompts/LC/JC |
| PromptValue boolean | P423 | confirm → typed answer | sends | sends | bridge/LC/JC |
| PromptValue string | P424 | text → typed answer | sends | sends | bridge/LC/JC |
| PromptValue choice | P425 | choice → typed answer | sends | sends | prompts/LC/JC |
| PromptValue.ids | P426 | multi-picker → typed answer | sends | sends | LC/JC; reserved command emitter |

##### Menu model nested inside `view/render`

`menu.rs` defines these fields; the server also uses this same model internally. A client’s lack of use of an internal-state echo does not by itself justify introducing a second DTO.

| Field | Server producer/consumer | Lua | JS | Tests / verdict |
|---|---|---|---|---|
| Menu.id | definitions/session | ignores raw model | stored | menus/parity |
| Menu.title | definitions | text instead | QuickPick title | menu/JC |
| Menu.groups | definitions | text instead | rows | menu/JC |
| Menu.savedArgs | save/reset model | ignores raw model | ignored | menu saved-args tests; active server state |
| Group.title | definitions/layout | text instead | separator | menu/JC |
| Group.columns | always None; **no reader** | none | none | A14 deletion |
| Group.items | definitions | text instead | rows | menus/JC |
| Item.kind | enum serializer | text instead | widget mapping | JC |
| Item.key | definitions/profile dispatch | forwards captured input | label and acceptance | A33 |
| Item.flag | switch/option definition | text instead | description | menu/JC |
| Item.label | definition | text instead | label | menu/JC |
| Item.on | switch state | text instead | description | switch tests |
| Item.value | option state | text instead | description | option tests |
| Item.reader | definition → server prompt choice | none | ignored | prompts; server consumer exists |
| Item.command | action → server registry | generic forwarding | model echo, not direct dispatch | command matrix |
| Item.status | definitions/registry conversion | text tokens instead | description/status | Planned tests |
| Item.status.status | tagged status enum | text instead | normalizes | JC; obsolete shape fallback A26 |
| Item.status.roadmapId | Planned definition | text instead | description/detail | P/JC |
| Item.status.reason | **no production Unavailable constructor** | none | fallback description | A16 |
| Args.switches | save/reset/open preset | none | ignored | menu preset/save/reset |
| Args.options | save/reset/open preset | none | ignored | menu preset/save/reset |

##### Effect fields nested inside `view/effects` or invoke/key responses

| Field | Server producer → client consumer | Lua | JS | Tests / verdict |
|---|---|---|---|---|
| Effect.type | enum tag → dispatcher | handles | handles except quit | A06 |
| message.level | errors/commands → notifier | native severity | native severity | errors/LC |
| message.text | errors/commands → notifier | displays | displays | errors/LC/JC |
| message.hint | error mapper → notifier | appends | appends | errors |
| message.detail | error mapper → detail presentation | stores for scratch view | output channel | errors/client tests |
| message.processLogId | always None | no functional reader | no functional reader | reserved R-3.7/§3.5 |
| passThrough.key | key handler → native replay | replays | limited replay | keys/LC |
| clipboard.text | copy command → clipboard | copies | copies | text_mode/LC |
| openView.descriptor | **no current emitter** | shared open branch | shared open branch | retained small §3.5/§3.7 seam |
| pushView.descriptor | session → open view | opens | opens | views/menu/clients |
| popView.viewId | session → close view | closes | closes | views/menu |
| focusView.viewId | session → focus view | focuses | focuses | views/menu |
| quit | root quit → close session | handles | missing | A06 |
| editDocument.editId | bridge → answer routing | uses | uses | bridge |
| editDocument.path | bridge → real file | opens | opens | bridge |
| editDocument.kind | helper → native filetype | maps | maps | bridge |
| editDocument.title | server metadata → native presentation | stores | not used as explicit title | bridge metadata |
| editDocument.mode | currently only realFile | assumes real file | assumes real file | explicit §3.10 contract |
| editDocument.scope | server metadata → edit bindings | separate keymap supplies actual scope | hard-codes commit-message | A07; Lua does not read this effect field either |

**Bounded coverage limit:** arbitrary JSON inside `args` cannot have a finite exhaustive leaf inventory. The matrix includes every declared wire field and every currently interpreted `view/*` argument key, plus the untyped tracing extension. No claim is made about arbitrary user-owned nested JSON.

---

#### 4. Named Lua/JS function classification

Codes:

- **T** transport, queues, identity routing, instrumentation.
- **A** applying server data.
- **E** editor-native glue, including native widget adaptation and key spelling.
- **D** domain policy/grammar; change or existing task named below.
- Test-only Git setup/assertion logic is marked **D-test**, not proposed for migration into runtime code.

Named declarations, methods and named function-valued bindings are listed. Anonymous event callbacks without their own binding inherit their enclosing operation’s classification.

##### Lua

```text
buffer.lua
  undo_levels E
  is_write_command E
  notify_read_only E
  configure_read_only E
  fresh E
  rename E
  set_lines A
  apply_edits A
  enter_text_mode E
  exit_text_mode E
  enable_folding E

debug.lua
  pack T
  trace T
  record_notification T
  configure T/E
  view_open T
  view_close T
  render_received T
  render_applied T
  render_acked T
  keymap_received A
  set_pending A
  key_pressed T
  trace_id T
  progress_started T
  progress_finished T
  key_sent T
  track_request T
  request_started T
  request_finished T
  prompt_started T
  prompt_finished T
  schedule T
  idle T
  active_view E
  window_state E
  recent_messages E
  snapshot E/T
  configure's vim.notify replacement E/T

edit.lua
  place E
  setup E
  confirm E/T
  cancel E/T
  on_closed E/T
  answer T
  keymap_options A/E
  keymap_options.handlers.press T
  keymap_options.handlers.command T
  keymap_options.commands.common.confirm E/T
  keymap_options.commands.common.cancel E/T
  keymap_options.commands.common.quit E/T
  open A/E

effects.lua
  notify E
  copy E
  replay E
  error_detail E

highlights.lua
  define A/E
  setup E

init.lua
  project_root E
  default_cmd E
  detect_key_modifiers E
  client_capabilities T
  error_message T
  session_for_client T
  session_for_view T
  status_view E
  open_status T
  open T/E
  press T
  invoke T
  refresh T
  resize T
  target_record E
  symbols T
  jump T [wrapper; delegated local policy is A25]
  visit D/E [existing R-NV-4]
  error_detail E
  configure T/E
  configure.host.visit D/E [existing R-NV-4]
  configure.host.edit_document E
  configure.host.set_error_detail A
  handler.view/render A
  handler.view/keymap A
  handler.view/prompt T/E
  handler.view/effects A
  setup T/E

keymap.lua
  fold_target E
  user_folds E
  clear E
  apply A/E
  apply.add E
  apply.drop_lhs E
  apply.command_fn T

keys.lua
  fail E
  one_char E
  split_ws A
  parse_step D/E [canonical validation duplicated; A32]
  step_to_lhs E
  steps D [revalidates server keys; A32]
  to_lhs E
  special_to_canonical E
  from_lhs E
  termcodes E

menu.lua
  cap E
  height_for E
  window_options E
  place E
  fit E
  setup E

prompt.lua
  configure T
  client T
  respond T
  finish T
  format_item E
  ask_text E
  ask_confirm E [protocol default handling A19]
  ask_choice E
  ask_multi E
  ask_multi.step E
  handle T/E

view.lua
  configure T/E
  register_placement E
  client T
  error_message T
  view_key T
  find T
  M.find T
  view_for_buf E
  remove_from_order E
  current E
  status E
  last_for_client E
  focus E
  focus_base E
  focus_previous E
  uri T
  sync T
  ack T
  refresh_tokens T
  set_cursor A/E
  request_resync T
  apply_render A
  flush_deferred A/T
  apply_keymap A
  apply_keymap.handlers.press T
  apply_keymap.handlers.command T
  apply_keymap.visit.fn D/E [existing R-NV-4]
  place_main E
  place_side E
  float_config E
  place_float E
  fit_float E
  attach E/T
  on_wiped E/T
  open A/E
  open_document A/E
  close E
  quit E
  close_all E
  press T/E
  invoke T/E
  apply_effects A
  resize E/T
  collect_symbol_lines D [A25]
  jump D [A25]
  setup_lifecycle E/T
  resize_all E/T
  aliases M.place_main/M.place_side/M.place_float E
```

Native window sizing is not server menu-column layout. Fold mirroring is expressly client-native adaptation under §3.9. Neither is proposed for movement.

##### JS runtime and libraries

```text
extension.js
  activate T/E
  provider.provideTextDocumentContent A
  deactivate T
  createClient T
  createClient.uriConverters.code2Protocol T
  createClient.uriConverters.protocol2Code T
  createClient.middleware.provideDocumentSemanticTokens A
  createClient.middleware.initialize T
  stateForUri T
  activeState E
  isMenuKind A/E
  ensureView A
  showView A/E
  showDocument E
  showMenu E/T
  updateQuickPick A/E
  menuItems A/E
  hideMenu E
  queueSteps T [local pop policy removed by A26]
  press T
  invoke T
  applyRender A
  handlePrompt T/E
  promptFor E
  resync T
  applyCursor A/E
  activateView A/E
  closeView E
  focusView E
  applyEffects A
  showMessage E
  openEdit E
  currentEdit E [focus bug A07]
  statusViewId T
  answerEdit T
  confirmEditCommand E/T
  cancelEditCommand E/T
  closeEdit E
  setContext E
  replay E
  editorInfo E [inclusive-selection conversion A34]
  onDocumentClosed E/T
  onActiveEditorChanged E
  openStatus T/E
  openStatus.opening T
  viewport E
  pressCommand T/E
  invokeCommand T/E
  mirrorFoldShortcuts E
  keybindingsPath E
  message T

lib/context.js
  staticScope A
  deriveContext A
  inactiveContext A

lib/edit.js
  isEditEffect A
  languageForKind E
  editingContext A [hard-coded scope A07]
  inactiveContext A
  commandForKey D [A07]
  answerArgs T

lib/foldkeys.js
  stripJsonComments E [user-input parser; A24]
  stripTrailingCommas E [user-input parser; A24]
  parseJsonc E
  mapFoldCommand E
  mapFoldKeys E

lib/menu.js
  statusInfo A [unsupported-shape fallbacks A26]
  describeItem A/E
  quickPickItem A/E
  quickPickItems A/E
  charToKey T/E
  routeTyped T/E [appropriate for typed text, not activation keys: A33]
  typedDelta T [no runtime consumer; A26]
  isPopKey D [A26]

lib/prompt.js
  promptItems E [defaults A19]
  answerFor A
  customChoice A

lib/render.js
  applyEdit A
  applyRender A

lib/semantic.js
  register A/E
  provideDocumentSemanticTokens T/A
```

**Correction to A26’s earlier premise:** QuickPick row labeling from the explicitly supplied structured menu is approved native-widget adaptation under §3.11. Moving all of it into a new DTO would introduce another model rather than clearly reduce complexity. Withdraw that part of A26; retain deletion of local transition policy, dead helpers and unsupported wire-shape normalization.

##### JS scripts and tests

```text
scripts/gen-keybindings.mjs
  serverCommand T

scripts/test-integration.mjs
  ownedPids E
  stopOwned E

test/test.js
  test T
  generated-binding test's gate A

test/open.test.js
  testStatusOpenLifecycle T/E
  Uri.constructor E
  Uri.from E
  Uri.parse E
  Uri.toString E
  EventEmitter.constructor E
  EventEmitter.event E
  EventEmitter.fire E
  EventEmitter.dispose E
  fake.workspace.getConfiguration E
  fake.configuration.get E
  fake.workspace.registerTextDocumentContentProvider E
  fake.workspace.onDidCloseTextDocument E
  fake.workspace.openTextDocument E
  fake.document.getText A
  fake.document.lineAt E
  fake.window.createOutputChannel E
  fake.output.appendLine E
  fake.output.dispose E
  fake.window.onDidChangeActiveTextEditor E
  fake.window.showTextDocument E
  fake.window.showErrorMessage E
  fake.window.showWarningMessage E
  fake.window.showInformationMessage E
  fake.languages.registerDocumentSemanticTokensProvider E
  fake.languages.setTextDocumentLanguage E
  fake.commands.registerCommand E
  fake.commands.executeCommand E
  returned disposable.dispose methods E
  LanguageClient.constructor T
  LanguageClient.onNotification T
  LanguageClient.start T
  LanguageClient.stop T
  LanguageClient.sendRequest T
  LanguageClient.sendNotification T
  Module._load's named load T

test/integration.js
  settle T
  waitFor T
  workspaceFiles E
  workspaceFiles.walk E
  run T/E/D-test
  recordRender A
  magitTraffic T
  invoked T
  keysSent T
  cdp T
  observe E
  shot E
  startKeyProbe E
  injected listener E
  stopKeyProbe E
  quickInputState E
  captureFailure E
  statusTabs E
  documentLines E
  styleEvidence E
  injected normalize E
  themeSetting E
  isDarkWorkbench E
  isPainted E
  waitForStyles E/T
  writeThemeEvidence E
  colorOf E
  selectLine E
  key E
  type E
  focusStatus E
  entriesAt E
  pressF5 E/T
  stagedNames D-test
```

No AST-backed inventory of anonymous callbacks is claimed. All named declarations, methods and function-valued local bindings found by the source scans are listed above; vendored packages are excluded.

---

#### 5. Parameterization corrections

##### Withdraw A22

Original functions:

- `stage_lines_stages_only_modification`
- `stage_lines_stages_only_addition`
- `stage_lines_stages_only_deletion`

They have similar setup/application, but the modification case makes four assertions and the other two make three, with different assertion sets. Parameterizing would first require introducing present/absent assertion loops. That exceeds the requested “identical except inputs and expected values” criterion.

Do not count its previous −25 LOC estimate.

##### Withdraw A23

Original functions:

- `required_include_missing_names_file_and_line`
- `unknown_settings_name_file_and_line`
- `unknown_command_names_name_file_and_line`

These do fit a common body after literal substitution. However, each current test is only nine lines. Named `#[case]` attributes with their long names and source strings consume most of the savings once formatted. Expected net savings are below approximately 15 lines.

Do not count its previous −20 LOC estimate.

##### Replace A20; retain A21 with corrected dependency

The original 18-row A20 mixed direct resolution equality with message-helper assertions and overstated savings. Replace it with the three genuinely equivalent stage-hunk bodies below.

A21 remains suitable: after replacing the literal path with a parameter, all three cases perform the same fixture setup and the same two assertions. Change its dependency to the replacement A20 title.

| Task | Original functions / named cases | Cases | Estimated net LOC | Dependency |
|---|---|---:|---:|---|
| A20 | `stage_hunk_header_stages_hunk`; `stage_changed_line_stages_whole_hunk`; `stage_context_line_stages_whole_hunk` | 3 | −18, including first shared dependency/import overhead | New `rstest` dev-dependency |
| A21 | `status_reports_path_with_spaces`; `status_reports_path_with_quote_and_backslash`; `status_reports_unicode_path` | 3 | −20 | Reuse `rstest` |
| A30 | `unstage_hunk_header_unstages_hunk`; `unstage_changed_line_unstages_whole_hunk`; `unstage_context_line_unstages_whole_hunk` | 3 | −20 | Reuse `rstest` |
| A31 | `discard_hunk_header_needs_confirm`; `discard_changed_line_needs_confirm`; `discard_context_line_needs_confirm` | 3 | −20 | Reuse `rstest` |

Each group stays within synchronous Rust L1 tests. Parameterization preserves all cases and does **not** reduce runtime.

Other skipped groups:

- Existing key-parser/error-mapping/menu matrices already use loops.
- Include-cycle, parse-span and environment tests have distinct bodies.
- Unborn-head mutation tests differ in setup, operation and assertions.
- Patch normalization variants need distinct setup/identity expectations; no additional qualifying ≥15-line group was established.
- No identical-profile snapshot group was established.

---

#### 6. Retained scaffolding: exact disposition

A recorded design decision is cited explicitly rather than inventing an open consumer.

| Surface | Current production use | Justification through Phase 5 | Disposition |
|---|---|---|---|
| `multiChoice` kind/value/UI method and adapters | No current command emitter | **Recorded §3.6 decision**, implemented in Phase 3/R-3.1: four generic prompt kinds. This is a cross-language protocol/native-widget seam, not speculative Git behavior | Keep; annotate R-3.1/§3.6; no extra bookkeeping tests |
| `preselected`, `min`, `max` | `preselected` adapters work; min/max unused and unenforced | Same recorded §3.6 decision; small fields within the retained generic seam | Keep as explicitly reserved/inactive constraints; do not claim validation works |
| `openView` effect | No current emitter; handlers share branches with `pushView` | **Recorded §3.5 decision**; §3.7 explicitly permits `openView`/`pushView` for **open R-5.1** selectors | Keep; tiny variant/shared branch |
| `selected` modifier | No current modifier producer | **Open R-5.1** marked/select-mode views; recorded §3.15 | Keep |
| `bold` modifier | No current modifier producer | **Open R-5.3** refs; §3.15 specifies HEAD ref plus bold | Keep |
| `planned`, `disabled` modifiers | No current modifier producer; menu token types are separate | **Recorded §3.15 decision**, associated with Phase-2/R-2.8 Planned menu rendering; only small legend/link entries remain | Keep with explicit provenance; do not pretend current menu tokens use these modifiers |
| Hash/author/date Lua links | No current emitted token types | Open R-4.3/R-5.1; recorded §3.15 | Keep small native mappings |
| Ref Lua links | No current emitted token types | Open R-5.3; recorded §3.15 | Keep |
| `magit.diff.context` Lua link | No emitted token; context deliberately uses Normal | Recorded §3.15 presentation decision | Keep small mapping; no synthetic emitter/test |
| `View::load_more` | No caller/override | Open **R-5.1** lazy log | Keep small trait seam |
| `LogSettings.default_args`, `.margin` | Parsed, not applied | Open **R-5.1**; recorded §3.18 | Keep, mark inactive |
| `DiffSettings.whitespace` | Parsed, not applied | Open **R-5.5** | Keep, mark inactive |
| `UiSettings.confirm_discard` | Parsed; discard execution absent | Open **R-3.6** | Keep |
| `SectionSettings.order`, `.default_visibility` | Parsed, not applied | **Recorded §3.18 decision** from Phase-2 configuration design | Keep small typed/layering surface, explicitly inactive—not a working feature |
| `message.processLogId`, `UserError.process_log_id` | Always None | **Recorded §3.5/IMM-3 decision** and open **R-3.7** error integration | Keep small optional link; no consumer claimed |
| Prompt optional detail/labels/default/item metadata | Mostly no current command values; adapter/test consumers exist | Recorded §3.6 generic prompt contract | Keep; A19 fixes actual confirm-default behavior |
| `Reader::{Path,Ref,Commit,Date,Number}` metadata | Several variants occur in real menu definitions; Path has no definition consumer | **Recorded §3.11 reader schema**, Phase-2/R-2.7; tiny enum metadata, Planned entries protected | Keep; do not add specialized executors now |
| `ViewDescriptor.viewport` | Server stores it; clients ignore echo | Recorded **§3.2** view descriptor decision | Keep small metadata field |
| `ViewKeymap.version` | No functional client reader | Recorded **§3.9** complete keymap payload decision | Keep small metadata field |
| `EditDocumentMode::RealFile` | Current bridge contract; clients assume it | Recorded **§3.10**, open R-3.4b/R-4.2 | Keep; no virtual-mode scaffolding added |
| Client capability declarations / `KeyModifiers` parser | Delivery is functional; modifier report is debug-only; other declarations unnegotiated | Recorded **§3.8/§4.1** client contract | Keep small declarations; document lack of negotiation; R-3.4b owns bridge capability completion |
| Four Planned common navigation/cycle IDs | Profiles route them; no handler implementation | Recorded **§5.1** bindings | Keep. Their R-2.3 completion/reference inconsistency needs roadmap disposition, not silent deletion |
| `Unavailable` machinery | Only injected test constructor | Alternate-backend consumer is beyond Phase 5; branch/conversion/test apparatus is not a small remaining field | Delete under A16 |
| `Group.columns` | Always None, no reader | No implemented override consumer | Delete under A14 |
| `Document`, `Runner`, `CommandCtx.cancel`, patch convenience APIs | No production consumers | No named near-term consumer established | Delete under A14 |

**Important:** retaining `multiChoice` does not mean R-5.1 commit picking consumes it. Commit picking is explicitly a select-mode view, not a prompt kind.

---

#### Token seam matrix

The reviewer checked these references against current source, including concurrent edits.

Legend: **D** = `src/semantic_tokens.rs`; **SV** = `src/status_view.rs`; **MV** = `src/menu.rs`; **PL** = `src/process_log.rs`. Lua links are in `lua/magit-lsp/highlights.lua:9–55`. JS scopes are in `clients/vscode/package.json:63–101`, transported by `lib/semantic.js:4–23`. **Q** means native QuickPick menus: the generic legend includes the type but no semantic-menu document/theme consumer is needed. **L-links** tests mappings, not emission; **J-scopes** tests configured scopes, with real paint checks for section/added/removed types.

##### All 26 registered token types

| Token | Server definition | Actual producer | Lua consumer | JS consumer | Tests | Verdict |
|---|---|---|---|---|---|---|
| `magit.section` | D12 | SV419; PL63 | Title | entity.name.section | status_render; stdio round-trip; Lua highlighting; J-scopes/paint | Emitted; keep |
| `magit.count` | D13 | SV426; PL70,107 | Comment | constant.numeric | status_render; stdio; L-links; J-scopes | Emitted; keep |
| `magit.file` | D14 | SV477; PL100 | Directory | string | status_render; UTF-16 round-trip; Lua; J-scopes | Emitted; keep |
| `magit.file.modified` | D15 | SV774, used at470 | Changed | markup.changed | status_render; stdio; Lua; J-scopes | Emitted; keep |
| `magit.file.added` | D16 | SV770 | Added | markup.inserted | status_render; staged round-trip; J-scopes | Emitted; keep |
| `magit.file.deleted` | D17 | SV771 | Removed | markup.deleted | L-links/J-scopes; no focused emitted-type assertion found | Emitted branch; keep |
| `magit.file.renamed` | D18 | SV772 | Changed | markup.changed | Rename fixtures; L-links/J-scopes; no focused type assertion found | Emitted branch; keep |
| `magit.file.untracked` | D19 | SV773 | Comment | comment | Untracked fixtures; L-links/J-scopes; no focused type assertion found | Emitted branch; keep |
| `magit.hunk.header` | D20 | SV537 | Special | meta.diff.range | status_render; stdio; Lua; J-scopes | Emitted; keep |
| `magit.diff.added` | D21 | SV585,595 | @diff.plus | markup.inserted | status_render; stdio; Lua/theme switch; JS paint | Emitted; keep |
| `magit.diff.removed` | D22 | SV586,595 | @diff.minus | markup.deleted | status_render; stdio; Lua/theme switch; JS paint | Emitted; keep |
| `magit.menu.title` | D23 | MV378 | Title | Q | Menu decode; L-links | Emitted; keep |
| `magit.menu.group` | D24 | MV398,602 | Statement | Q | Menu decode/reflow; L-links | Emitted; keep |
| `magit.menu.key` | D25 | MV815,503–525 | Keyword | Q | Menu decode; pending-prefix suffix; L-links | Emitted; keep |
| `magit.menu.label` | D26 | MV821 | Normal | Q | Decode; implemented-action/reflow; L-links | Emitted including argument labels; keep |
| `magit.menu.flag` | D27 | MV453 | Special | Q | Decode; L-links | Emitted option flag, distinct from switch flags |
| `magit.menu.flag.on` | D28 | MV429 | Special | Q | Toggle/decode, tests/menu.rs:616–628 | Emitted; keep |
| `magit.menu.flag.off` | D29 | MV431 | Theme-derived dim | Q | Decode; switch flag; wrapped continuation; L-links | Emitted; keep |
| `magit.menu.switch.on` | D30 | MV441 | String | Q | Toggle/decode | Emitted; keep |
| `magit.menu.switch.off` | D31 | MV443 | Comment | Q | Decode; wrapped continuation; L-links | Emitted; keep |
| `magit.menu.option` | D32 | MV813 | String | Q | Decode; pending/reset; L-links | Emitted; keep |
| `magit.menu.value` | D33 | MV466 | String | Q | Option flows; L-links; no explicit value-type assertion found | Emitted for populated option; keep |
| `magit.menu.planned` | D34 | MV822 | Comment | Q; structured Planned descriptions | Decode; all-menu Planned checks; Lua assertion | Emitted; keep Planned entries |
| `magit.menu.disabled` | D35 | MV427,439,455,468,510–511; also823 | Theme-derived dim | Q | Pending-prefix dim test; L-links | Actively emitted independently of Unavailable; keep after A16 |
| `magit.menu.pending` | D36 | MV516 | Theme-derived bold | Q | Pending-prefix assertion; L-links | Emitted; keep |
| `magit.menu.preview` | D37 | MV544 | Comment | Q | Decode; argument preview; L-links | Emitted; keep |

All 26 registered types have reachable production emitters. Lack of a focused assertion is not evidence that a type is dormant; no new test task is proposed merely for every row.

##### Produced but unregistered types

| Token | Definition | Actual producer | Lua consumer | JS consumer | Tests | Verdict |
|---|---|---|---|---|---|---|
| `magit.message.error` | Absent from legend | PL137: failed-command nonempty stderr | ErrorMsg exists, but no encoded token arrives | No advertised type/configured scope | process_log raw-tree assertion only | A09: encoder drops it at D76–78 |
| `magit.message.warn` | Absent from legend | PL139: successful-command nonempty stderr | WarningMsg exists, but no encoded token arrives | No advertised type/configured scope | No focused emission/encoding assertion found | A09: same encoding loss |

A09 must verify both failed-command and successful-command stderr through encoding; a raw-tree assertion does not protect delivery. `magit.unknown` occurs only in an encoder rejection test, not a production emitter.

##### All five modifiers

Definitions: D41; bitset encoding: D128–136. JS forwards the legend/bitsets generically; no modifier-specific theme rules are configured.

| Modifier | Actual producer | Lua consumer | JS consumer | Tests | Verdict |
|---|---|---|---|---|---|
| staged | SV459–478,527–595 | Bold | Generic legend/bitset | status_render modifiers; stdio staged assertion; encoder; L-links | Actively emitted; keep |
| selected | None found | Visual | Generic legend/bitset | Legend/L-links only | A28; open R-5.1 select/marked views |
| planned | None found | Comment | Generic legend/bitset | Legend/L-links only | A28; recorded §3.15/Phase-2 decision; not menu.planned type |
| disabled | None found | Comment | Generic legend/bitset | Legend/L-links only | A28; recorded §3.15 decision; not menu.disabled type |
| bold | None found | Bold | Generic legend/bitset | Legend/L-links only | A28; R-5.3 refs/HEAD and §3.15 |

Lua intermediate-group bold/dim attributes do not constitute server modifier emission. Client-only future links (`magit.diff.context`, `magit.hash`, `magit.ref.local`, `.remote`, `.tag`, `.head`, `magit.author`, `magit.date`) retain the precise A28 dispositions above; they are not claimed as current registered/emitted types.

#### Prompt-kind seams

| Kind | Server definition/producer | Lua handler | JS handler | Tests | Verdict |
|---|---|---|---|---|---|
| confirm | protocol/commit preflight | Native select | QuickPick | preflight/bridge/client prompt cases | A19 fixes default handling |
| text | protocol/menu options/askpass | Input or inputsecret | InputBox | prompts/bridge/client cases | Keep masking and transport identity |
| choice | protocol/menu option reader | Select/custom input | QuickPick/custom input | prompts/client cases | Shared semantic vectors A19 |
| multiChoice | protocol; no command emitter | Checklist/select loop | Native multiselect | Client prompt cases | A28: small recorded §3.6/R-3.1 seam, not a claim that select-mode log uses it |

### Final defensive-code principle — suitable for AGENTS.md

> Validate protocol input, user configuration and Git output at their boundaries. Once internal values are validated, trust their invariants: do not reparse canonical keys, invent fallback command statuses, reconstruct displayed selections from fresh diffs, or maintain alternate production paths solely for tests. Keep recovery where failure is genuinely external: lost render acknowledgements, editor buffer lifetime, Git failures and client disconnects.
