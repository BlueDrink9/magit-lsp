STRONGER-EFFORT RETRY — 2026-10-24; AUTHORITATIVE CURRENT UPDATE

STOPPED after the FIRST unexpected failing verification command (exit 1).
No retry, later verification, or further implementation was performed.
Required full make chain NOT RUN in this retry; new/changed L2 tests UNVERIFIED.
No staging, commits, checkout/reset, index/history changes, or process kills.
Inherited dirty VSCode files and completed TASKS notes remain untouched.

Observed outcome: standard 100x30 staged a change, rendered Process log, found
git apply by native search, and q returned to status with one view. The next
scenario key `r` opened Rebase, so the staged-status assertion was obscured.
Do not claim the client or scenario complete. Before any future authorized retry,
check the actual standard-profile refresh binding for BOTH edited scenarios
(currently `r`); do not add kind-specific Lua or weaken the fresh-usability gate.
Only this first variant was run. Other profiles/sizes and multiple-views UNRUN.
process-log XFAIL and R-NV-2 / missing-UI P1 unchecked status are retained.

Screens inspected after the stop:
- build/ui/process-log/standard-100x30/process-log-header.screen: Process log (15).
- build/ui/process-log/standard-100x30/process-log.screen: git apply --cached
  --unidiff-zero --recount (exit 0, 1 ms), visible at screen row 9.
- No back-to-status snapshot was reached; no tour was run in this retry.
Final sizes: init 393, view 1081, menu 98, edit 139, buffer 154, highlights 107,
effects 88, debug 423. Scenario finally stopped its owned run. Final process
scan found no magit-lsp servers, harness Neovims or magit tmux sessions; none
were killed, and no user-owned process was touched.

Exact failing command:
```sh
env -i HOME="$HOME" PATH="$PATH" PYTHONDONTWRITEBYTECODE=1 python3 - <<'PY'
import sys
from pathlib import Path
sys.path.insert(0, 'scripts')
from ui_lib import scenario
for name in ('process-log', 'multiple-views'):
    s = scenario.parse_scenario(Path(f'tests/ui/scenarios/{name}.scn'))
    for size, profile in scenario.combinations(s):
        result = scenario._run_combo(s, size, profile, [])
        print(name, profile, size, result, flush=True)
        if not result.ok:
            print(result.screen, flush=True)
            raise SystemExit(1)
PY
```

Exact full output (exit 1):
```text
warning: Git tree '/home/william/src/magit-lsp' is dirty
process-log standard 100x30 ComboResult(ok=False, reason='statement 25: expect screen contains "Staged changes (1)": screen does not contain \'Staged changes (1)\'', screen=' Rebase\n Arguments\n  -k  Keep empty commits (--keep-empty)  [ ]\n  -p  Preserve merges (--preserve-merges)  [ ]\n  -r  Rebase merges (--rebase-merges)  [ ]\n  -u  Update branches (--update-refs)  [ ]\n  =s  Strategy (--strategy=)\n  =X  Strategy option (--strategy-option=)\n  =a  Diff algorithm (--diff-algorithm=)\n  -f  Force rebase (--force-rebase)  [ ]\n  -d  Use author date as committer date (--committer-date-is-author-date)  [ ]\n  -t  Use current time as author date (--ignore-date)  [ ]\n  -A  Autosquash (--autosquash)  [ ]\n  -S  Autostash (--autostash)  [ ]\n  -i  Interactive (--interactive)  [ ]\n  -h  Disable hooks (--no-verify)  [ ]\n  =x  Run command after commits (--exec=)\n  -g  Sign using gpg (--gpg-sign)  [ ]\n  -s  Add Signed-off-by trailer (--signoff)  [ ]\n git rebase\n Rebase onto       Rebase                   Actions        Menu\n  p  push-remote    i  interactively         r  Continue    ?  Show the dispatch menu\n  u  upstream       s  a subset              a  Abort\n  e  elsewhere      m  to modify a commit\n                    w  to reword a commit\n                    k  to remove a commit\n                    f  to autosquash\n                    t  to change dates\n</status/home/william/src/magit-lsp/build/ui/runs/20261024-070520-dff831/repo [-] 1,1            Top\n/git apply                                                                        1,1           All', profile='standard', size='100x30')
 Rebase
 Arguments
  -k  Keep empty commits (--keep-empty)  [ ]
  -p  Preserve merges (--preserve-merges)  [ ]
  -r  Rebase merges (--rebase-merges)  [ ]
  -u  Update branches (--update-refs)  [ ]
  =s  Strategy (--strategy=)
  =X  Strategy option (--strategy-option=)
  =a  Diff algorithm (--diff-algorithm=)
  -f  Force rebase (--force-rebase)  [ ]
  -d  Use author date as committer date (--committer-date-is-author-date)  [ ]
  -t  Use current time as author date (--ignore-date)  [ ]
  -A  Autosquash (--autosquash)  [ ]
  -S  Autostash (--autostash)  [ ]
  -i  Interactive (--interactive)  [ ]
  -h  Disable hooks (--no-verify)  [ ]
  =x  Run command after commits (--exec=)
  -g  Sign using gpg (--gpg-sign)  [ ]
  -s  Add Signed-off-by trailer (--signoff)  [ ]
 git rebase
 Rebase onto       Rebase                   Actions        Menu
  p  push-remote    i  interactively         r  Continue    ?  Show the dispatch menu
  u  upstream       s  a subset              a  Abort
  e  elsewhere      m  to modify a commit
                    w  to reword a commit
                    k  to remove a commit
                    f  to autosquash
                    t  to change dates
</status/home/william/src/magit-lsp/build/ui/runs/20261024-070520-dff831/repo [-] 1,1            Top
/git apply                                                                        1,1           All
Exited with code 1
```

Review/fixes before verification:
- Fresh status must attach its boot buffer to the owning LSP client BEFORE
  view/open; the inherited reopen callback did not do this. Fixed.
- Removed bare-id notification routing and unused singleton client/root/last-id
  state. All wire renders/keymaps require sender identity; local APIs use keys.
- Added generic float resizing, resync-error deferred-queue flushing, and safe
  editor save failure handling. No process-log-specific Lua.
- Process-log scenario now captures the heading, searches natively for git apply,
  then verifies q -> fresh status -> refresh -> usable commit menu. XFAIL stays
  until its four variants actually pass.
- Strengthened independent-repo collisions/quit/usability regression; added
  early/deferred composite-id, exact ack, invalid-base/text-mode, native-wipe,
  real-file edit confirm/cancel/early-keymap, and generic float regression cases.
- The targeted verification above stopped this retry. Full make chain and all
  new/changed L2 cases remain unverified; no further implementation was made.
- Initial process scan found no magit-lsp or harness Neovim/tmux processes.

PREVIOUS CONTINUATION UPDATE — 2026-10-24; HISTORICAL BELOW

STOPPED: circuit breaker reached, 2 failing verification commands in this attempt.
Do not claim completion. No staging, commits, checkout/reset or process kills were
performed. All inherited VSCode files and completed TASKS notes were preserved.

Actual work state:
- init.lua is now 399 lines, menu 98, edit 135, view 1095, buffer 154,
  effects 88, highlights 107. init/menu/edit/tests WERE already partly rewritten
  when this attempt began; the historical "still old" statements below are stale.
- Fixed test menu helpers to use record.key (client/view composite), not record.id.
  Fixed independent-repositories regression: opens second repo in another tab,
  compares composite identities (server ids really collide), indexes by key.
- Manager fixes: deferred renders retain client identity and all queues flush
  after sync/resync; base-version mismatch is checked before changing text mode;
  initial text descriptors enable editing; document lookup includes real files;
  native wipe removes debug view state. These latest changes are NOT verified.
- Quit no longer retains a server-cleared status id. It closes this client's
  server views in reverse order and asks init's reopen_status callback to open a
  fresh status when quitting a nested view. Needs explicit usability regression.
- Extracted symbol navigation into view.jump, error-detail display into effects,
  and resize/shutdown hooks into view.setup_lifecycle to keep init under 400.
- edit.open resolves detached effects to the same client's most recent view so
  editor confirm/cancel has a base. Prompt handler selects sender client before
  handling (async prompt redesign remains out of scope; prompt.lua untouched).
- Added tests/ui/scenarios/multiple-views.scn (UNRUN): main view -> dispatch
  menu -> main view -> quit -> fresh status -> refresh -> commit menu -> close.
- process-log.scn is UNCHANGED and still XFAIL. R-NV-2 and P1 missing real-UI
  remain unchecked. §11.1 records partial line counts and blocker, not completion.

Verification 1 (exit 2):
  env -i HOME="$HOME" PATH="$PATH" make test && env -i HOME="$HOME" PATH="$PATH" make ui-unit && env -i HOME="$HOME" PATH="$PATH" make ui-test && env -i HOME="$HOME" PATH="$PATH" make ui-tour
Exact full output:
  /home/william/.local/share/opencode/shell/d9c54c3ca29f3cbe85e8acf52d7578dda234b6bb/sh_16caf0d5a001dyhme9RQBQh3Br.out
Rust tests all passed. Lua: 62 cases, 9 failures, 0 notes. Eight menu cases
failed `Left: "nil" Right: "table"` at test lines 1339,1386,1427,1462,1485,
1517,1542,1562 because wait_menu indexed by bare id. Two-repository test failed
`Left: false Right: true` at line 1819 because ids can collide. Those causes
were patched, but no rerun of L2 was made. Later chained gates never ran.

Verification 2 (exit 1): targeted real-UI probe, leaving XFAIL intact:
  env -i HOME="$HOME" PATH="$PATH" PYTHONDONTWRITEBYTECODE=1 python3 - <<'PY'
  import sys
  from pathlib import Path
  sys.path.insert(0, 'scripts')
  from ui_lib import scenario
  s = scenario.parse_scenario(Path('tests/ui/scenarios/process-log.scn'))
  for size, profile in scenario.combinations(s):
      result = scenario._run_combo(s, size, profile, [])
      print(profile, size, result, flush=True)
      if not result.ok:
          print(result.screen, flush=True)
          raise SystemExit(1)
  PY
Exact full output (includes captured screen):
  /home/william/.local/share/opencode/shell/d9c54c3ca29f3cbe85e8acf52d7578dda234b6bb/sh_16cb2bdd5001ToN0wMscYh9Urj.out
Exact failing reason:
  standard 100x30 ComboResult(ok=False, reason='statement 11: expect screen contains "git apply": screen does not contain \'git apply\'')
Screen begins `Process log (15)` then oldest `git rev-parse --show-toplevel`,
status and diff entries, cursor 1,1 at Top in the half-width main window. The
real staging succeeded (`Staged changes (1)` assertion passed). Main text now
renders, but `git apply` is farther down than the initial viewport, not absent
because of dropped rendering. No ProcessLog-specific Lua should be introduced.
Consider a generic scenario navigation to the relevant server record (or inspect
server cursor semantics; Rust changes are outside this ticket). Current probe
stopped after the first failing profile/size; remaining three variants UNRUN.
No new .screen artifacts were written: failure precedes `snapshot: process-log`.
Tour was neither run nor inspected in this attempt.

Cleanup: scenario._run_combo always called driver.stop in finally. Final ps scan
showed no magit-lsp servers, harness Neovims, or magit tmux sessions; none were
killed manually. User-owned PID 3790802 was already absent on initial scan and
was not touched. Other user Neovims/tmux were not touched.

Next orchestrator action: acknowledge/reset circuit-breaker before further
verification; review latest unverified changes, resolve viewport-visible staging
entry without ProcessLog-specific client code, add/strengthen independent-view,
early-notification, ack/baseVersion and text-mode regressions. Run all original
gates; inspect saved screens and tour; only then remove XFAIL/mark completion.

HISTORICAL PREVIOUS-WORKER CONTEXT (stale where superseded above)
Objective
Consolidate the Neovim client's three parallel view implementations into one descriptor-driven view.lua manager (R-NV-2), make the process-log main view work, remove its XFAIL, add regression coverage, and mark R-NV-2 plus the P1 missing-UI task complete in TASKS.md.
Requirements
Only modify files listed in the task ticket: lua/magit-lsp/{view,init,menu,edit,buffer,highlights,effects,debug}.lua, tests/test_magit_lsp.lua, tests/ui/scenarios/{process-log,multiple-views}.scn, TASKS.md.
Never special-case ProcessLog; honor descriptor placement main/side/bottom/float, capture, mode; preserve real-file editDocument semantics.
No singleton M.buf/M.view_id state or compatibility shims; migrate tests to view records.
Preserve server text verbatim, tree/wire navigation metadata, render-before-open buffering, ack/baseVersion rules, text mode, queued render ordering, native input replay, prompt/editor bridge, menu replacement/close behavior.
Public functions/types need LuaLS docstrings; init.lua under ~400 lines.
Keep R-NV-1 (async overhaul), other client defaults/debug isolation out of scope.
No commits/staging/reset/checkout/index/history mutation; preserve existing dirty VSCode files and completed notes; never stop user-owned processes; do not launch VSCode.
Use minimal inherited test environment (env -i HOME="$HOME" PATH="$PATH"); stop after two failed verification commands and report exact outputs, no blind retries.
Verification command: env -i HOME="$HOME" PATH="$PATH" make test && env -i HOME="$HOME" PATH="$PATH" make ui-unit && env -i HOME="$HOME" PATH="$PATH" make ui-test && env -i HOME="$HOME" PATH="$PATH" make ui-tour.
Remove process-log XFAIL only when its four profile/size variants pass; add independent views/repos + early-notification regression; inspect saved .screen; record module line counts in §11.1; mark R-NV-2 and P1 missing-UI complete only if all original acceptance gates pass.
Decisions
New view.lua owns all state keyed by server viewId: render/keymap/press/invoke/resync/pending/deferred queues, ack, text mode, window placement, close/pop/quit.
Placement registry: main/side/float built into view.lua; menu.lua registers bottom (float or split via menu.style); edit.lua registers edit for real files.
main placement reuses the current window and records view.previous; close restores previous view buffer (needed for process-log q returning to status).
quit effect closes non-base views of the same client and focuses the base status view (server clears all views, but screen must return to status).
Renders are deferred while debug_api.state.requests > 0 and flushed at the end of press/invoke; unknown view renders/keymaps queue per view id.
editDocument records are excluded from M.order and debug view tracking (no render/ack), to keep debug.idle() correct.
M.status_view(root) (planned name) / M.status(root) returns status records for tests and multi-repo support.
Existing tests will set STATUS = M.status_view() and use STATUS.buf/id/version/mode/lines/uri/keymap.
Baseline make ui-test without env -i fails with tmux ... failed: command too long due to inherited environment; reruns must use env -i.
Process-log scenario currently XFAILs as expected; menu-close-keys.scn PASSes with env -i.
Work State
Completed
Read and analyzed init.lua, menu.lua, edit.lua, keymap.lua, debug.lua, buffer needs, server view.rs/protocol.rs/session.rs, UI harness, and full L2 tests.
Created lua/magit-lsp/buffer.lua: scratch buffer creation, read-only guards, render writer, edits, text mode, folding, undo capture, filetype.
Created lua/magit-lsp/highlights.lua: token/modifier links and ColorScheme setup.
Created lua/magit-lsp/effects.lua: message notify, clipboard copy, native replay, last_error_detail.
Created lua/magit-lsp/view.lua: full manager with placements, open/open_document/close/quit/focus, render/keymap/press/invoke/resync/deferred queues, effects.
Active
Rewrite lua/magit-lsp/menu.lua as bottom placement (place/fit via view.register_placement) plus setup style, no duplicated state.
Rewrite lua/magit-lsp/edit.lua as real-file placement + open/confirm/cancel/keymap-options effects on top of view.open_document.
Rewrite lua/magit-lsp/init.lua under ~400 lines using buffer/highlights/effects/view; add sessions keyed by root, per-view clients, status_view, public wrappers.
Migrate tests/test_magit_lsp.lua from M.buf, M.view_id, M.version, M.mode, M.lines, M.uri, M.last_keymap, M.pending_render, M.apply_keymap, menu.views/order/last_id to STATUS/view.*.
Add independent-views/repositories + early-notification L2 regression tests.
Update process-log.scn (remove xfail line/comments) and add tests/ui/scenarios/multiple-views.scn.
Update TASKS.md §11.1 line counts, check R-NV-2 and P1 missing real-UI task.
Run verification chain.
Blocked
(none)
Next Move
Write menu.lua with M.setup(opts) setting float/split style and M.place/M.fit registered as bottom; use record.lines for height and vim.wo.wrap=false.
Write edit.lua with M.place (bufadd/bufload/bottom split), M.open(effect, source) calling view.open_document, M.confirm/M.cancel, keymap options mapping common.confirm/common.cancel/common.quit; confirm writes then invokes on base status with args={editId=id}.
Rewrite init.lua: capabilities/default cmd/debug/highlights, handlers → view.apply_render/apply_keymap, prompt, effects; setup sessions/status boot buffer/client; public M.press/invoke/refresh/resize/symbols/jump/visit/error_detail/status_view; VimLeavePre/resize autocmds.
Migrate tests and add new cases; run env -i ... make test-lua iteratively.
Update scenarios/TASKS, then run full verification chain with env -i.
Relevant Files
lua/magit-lsp/view.lua: new central manager; needs review for bugs as later modules integrate.
lua/magit-lsp/init.lua: still old 996-line implementation; must be rewritten.
lua/magit-lsp/menu.lua: still old 564-line manager; replace with bottom placement only.
lua/magit-lsp/edit.lua: still old 224-line manager; adapt to view.open_document.
lua/magit-lsp/buffer.lua: new extracted buffer primitives.
lua/magit-lsp/highlights.lua: new extracted theme links.
lua/magit-lsp/effects.lua: new extracted message/clipboard/replay helpers.
tests/test_magit_lsp.lua: heavy M.buf/M.view_id/menu-module access; migrate.
tests/ui/scenarios/process-log.scn: currently xfail: R-NV-2; remove after pass.
tests/ui/scenarios/multiple-views.scn: to create (process log + menu stack, counts, return to status).
TASKS.md: R-NV-2 §7 ~line 1197, P1 missing-UI §12 ~line 1789, §11.1 line counts ~line 1537.
src/process_log.rs / src/session.rs: process log is Placement::Main, scope process-log, q is common.quit; pushView sends descriptor then render/keymap notifications; popView sends pop then focusView.
lua/magit-lsp/keymap.lua: unchanged; apply(buf, notice, handlers, opts) supports visit, fold_keys, folds, only_commands, commands.
lua/magit-lsp/debug.lua: unchanged; tracks views via view_open/render_*/keymap_received and idle() needs version==acked.
scripts/minimal_init.lua / scripts/ui_lib/scenario.py: UI harness setup and DSL.
Important Context
Worktree is dirty with pre-existing VSCode changes (clients/vscode/*, TASKS.md); do not touch or revert them.
TASKS.md and VSCode files already have uncommitted completed-task notes that must be preserved.
make ui-test must be run via env -i HOME="$HOME" PATH="$PATH" ...; a normal run inherits huge nix env and fails with tmux respawn-pane ... failed: command too long.
UI baseline: process-log.scn currently XFAIL ... wait timed out for screen text 'Process log'; menu-close-keys.scn PASSes.
Server menus capture printable keys; Backspace must be mapped client-side for capture views.
Server view/resize only re-renders menus; non-menu resize is harmless.
common.quit (q) clears the server view stack; ticket requires the screen to return to status anyway.
R-NV-2 "Done when": init.lua under ~400 lines, menu.lua/edit.lua placement + effects only, all L2 cases + make ui-test green, line counts recorded in §11.1.
P1 missing-UI task requires process-log, option-prompt, status-basics, commit-confirm scenarios passing; only process-log is incomplete (XFAIL).
No final git commit should be made for this task, despite generic completion protocol.

READ-ONLY REVIEW UPDATE — 2026-10-24 (latest; implementation remains unauthorized)

Decision recorded with the manager: `q`/`common.quit` in a nested server view is
a SERVER-side pop to the existing parent; root `q` keeps close-all behavior.
Do not implement a client-side status reopen workaround. This small Rust scope
expansion is authorized. No product or test code was changed and no tests,
editor sessions, or UI scenarios were launched in this review.

Prioritized findings / worker-ready next ticket:
1. **P1, confirmed — nested q destroys parent identity/state.** `src/commands.rs`
   `quit` (around 782-785) always emits `Effect::Quit`; `src/session.rs` around
   1166-1173 clears the whole `ViewStack`. The Lua `view.quit` around 777-807
   then closes the records and `init.lua` around 288-295 explicitly opens a
   fresh status. This discards the already-open parent buffer/tree identity and
   its cursor/folds. In `Session::invoke`, when `Effect::Quit` came from a view
   with a parent, use the existing `pop_view(view_id)` contract (which emits
   `PopView` followed by `FocusView` for the remaining top view, session.rs
   938-959); only root quit clears the stack/emits Quit. The Lua manager already
   handles those effects generically (`view.lua` 969-974); remove its nested
   reopen-status workaround (view.lua 777-807 and init.lua 288-295) so a real
   root Quit only closes that client's views. `PopView` closes only the child
   buffer and focuses the existing parent record; it does not touch the
   parent's buffer, version, window cursor, or folds, so preserving that record
   preserves UI state.
   Regression: add a Rust `TestSession` case in `tests/views.rs` proving nested
   `common.quit` yields PopView+FocusView and retains the parent/version/tree;
   keep root-quit coverage/behavior. Update `tests/test_magit_lsp.lua`'s
   two-repository nested-quit test (currently lines 1855-1880) to assert the
   same status key/buffer survives and the other client stays live; retain the
   attached, refresh, menu-usability assertions. Update both real-UI scenarios
   to verify return to the original status, and retain the process-log
   `git apply` visibility assertion. No new UI workaround.
2. **P1, confirmed scenario defect, separate one-line correction — wrong
   standard refresh key.** `profiles/standard.toml` has `common.refresh = ["g",
   "F5"]` (line 17); `r` is `open.rebase` (line 53). Both
   `tests/ui/scenarios/process-log.scn` line 31 and `multiple-views.scn` line
   29 currently use `keys standard: r`; change to canonical `g` (keep vim's
   `g r`). The unexpected Rebase failure recorded above is this scenario
   mistake, not a product shortcut defect.
3. **No further confirmed defects in bounded client pass.** Composite identity
   is consistently client+view-keyed for render/keymap, queues, ack, close and
   effect routing (`view.lua` 72-79, 307-319, 399-410, 948-980); the independent
   repository test covers colliding server IDs. Pending renders/keymaps are
   buffered and applied after descriptor creation; deferred render queues are
   per composite key and preserve each view's order; mismatch requests resync
   before text-mode mutation; only successfully applied renders reach `ack`
   (`view.lua` 307-373, 375-390). Sync response errors flush deferred work via
   callers; an exception thrown inside `track_request` could bypass the flush
   at `view.lua` 227-246, but this path is unproven and should not expand the
   ticket. Likewise, global request-count deferral can hold unrelated views
   during one sync request; async/request-isolation redesign is explicitly out
   of scope. Real-file documents have separate open/close handling and
   confirm/cancel route to same-client base (`view.lua` 660-720,
   `edit.lua` 42-89); retain those tests and do not modify kind-specific
   branches without a reproduced failure.

Exact minimal implementation-ticket targets:
- `src/session.rs` — nested-vs-root quit decision and effects.
- `tests/views.rs` — session regression for nested pop and root quit contract.
- `lua/magit-lsp/view.lua`, `lua/magit-lsp/init.lua` — remove client reopen-status
  workaround while retaining root close-all.
- `tests/test_magit_lsp.lua` — update multi-client/state-preservation regression.
- `tests/ui/scenarios/process-log.scn`,
  `tests/ui/scenarios/multiple-views.scn` — preserve original-parent assertions;
  fix standard refresh binding from `r` to `g` in both.
- `TASKS.md` — only after implementation gates pass, mark the related roadmap
  work; this review made only the decision wording update, no completion marks.

No wider design review is necessary before implementation: protocol already
defines `PopView`/`FocusView`, `Session::pop_view` implements the server stack
operation, and Lua already applies these effects. Keep the ticket limited to
this existing contract; do not include async overhaul, debug isolation, or
speculative request-error handling.

IMPLEMENTATION COMPLETE — 2026-10-24

- Baseline `env -i HOME="$HOME" PATH="$PATH" make test-lua` passed before
  additional changes: 64 cases, 0 failures.
- Nested `common.quit` now uses `Session::pop_view` when a parent exists; root
  quit retains `Effect::Quit` and clears the session. Rust regression confirms
  PopView then FocusView, unchanged parent tree/version/id, and root clear.
- Removed Lua reopen-status handling. The multi-repository Lua regression now
  confirms the exact status record/buffer/cursor/fold and other client survive,
  with refresh/menu usability. Post-fix Lua suite passed: 64 cases, 0 failures.
- Corrected standard refresh `r` to `g` in both scenarios (vim remains `g r`);
  removed process-log XFAIL after all four profile/size variants passed. The
  targeted scenario command passed both scenarios; full `make test`, `make
  ui-unit`, `make ui-test`, and `make ui-tour` chain passed.
- Inspected standard 100x30 and vim 80x24 saved process-log/back-to-status and
  multiple-views/live-status screens, plus `build/ui/tour.md`. Process log shows
  `git apply`; the return screen shows staged status and both files.
- Final Lua module lines: init 385, view 1068, menu 98, edit 139, buffer 154,
  effects 88, highlights 107, debug 423, keymap 194, prompt 193. TASKS marks
  R-NV-2 and the missing-real-UI P1 complete. No staging or commits were made.
