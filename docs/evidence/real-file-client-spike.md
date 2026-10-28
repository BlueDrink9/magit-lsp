# R-EDIT-REAL step-0 native client spike — 2026-10-29

## Current verdict: scoped lifecycle observations PASS; parent/step 0 incomplete

### Native lifecycle follow-up (observer only)

Executed successfully (exit 0):

```sh
MAGIT_VSCODE_BIN=/home/william/.nix-profile/bin/codium node clients/vscode/test/e2e/real-file-spike.mjs --lifecycle && git diff --check
```

Codium `1.140.06880`, commit `807ba2d6075c29587f884698cd7370c81ab0724c`,
x64, Linux native display, Node `v24.21.0`, languageclient `9.0.1`.
The evidence clock reports `2026-10-28T22:43:03Z`–`22:43:18Z` (task calendar
date 2026-10-29). Each case uses a separate normal extension-development host,
without `--extensionTestsPath`, unique profile/extensions, real `git init`,
clean permanent `.git/COMMIT_EDITMSG`, and actual server `window/showDocument`.

| Native action | Retained directory under `/tmp/opencode/` | Observation |
| --- | --- | --- |
| `workbench.action.closeWindow` | `real-file-lifecycle-LKfk4O` | Action 22:43:07.190Z; actual deactivate .214Z; launcher exits 0 at .346Z; all profile processes absent before result. One activation, one deactivation. |
| `workbench.action.reloadWindow` | `real-file-lifecycle-0pw2kj` | Action 22:43:11.391Z; deactivate .404Z; second activation 22:43:12.688Z; ready .766Z in original repo with one matching draft tab. Issuing RPC rejects `Canceled` during host termination; restart independently observed. |
| `vscode.openFolder(Uri.file(<dir>/other-workspace), {forceReuseWindow:true})` | `real-file-lifecycle-DFAgvp` | Action 22:43:16.671Z; deactivate .684Z; second activation 22:43:17.743Z; ready .870Z in new folder, zero matching draft tabs. |

**None delivered an observed `event.closed` containing the draft; qualifying
draft-closed events: zero for all three.** Each trace has two initial tab events
with `closed:[]`, all-group matching count 1, `qualified:false`; no later closed
event. Each action records/asserts one permanent matching tab and `dirty:false`.
All three wire traces contain initial draft `didOpen` and successful show
response, but no `didClose`, `shutdown` or `exit`. Reload appends initialize and
draft didOpen; workspace-change appends initialize with the new root and no
draft didOpen. EOF/deactivation never confirms. No lifecycle false-confirmation
blocker was observed in this bounded configuration.

Inspected all three `pre-action.png` images: native Explorer, permanent draft
tab, visible `spike message`, no dirty marker/dialog. Inspected each
`command.json`, `api.jsonl`, `wire.jsonl`, `result.json`, `supervisor.jsonl`:
commands, timestamps, roots, exits and cleanup. `command.json` retains exact
expanded arguments/ports/version: binary above, `--wait --no-sandbox
--disable-gpu --skip-welcome --skip-release-notes --disable-workspace-trust`,
unique `--remote-debugging-port`, `--user-data-dir`, `--extensions-dir`,
`--extensionDevelopmentPath`, real repo path. `lifecycle-action.json` retains
action arguments; stdout/stderr files retain process output when nonempty.
API/wire logs append across host restart. External Node supervisor timestamps
wire observations (not send times), survives launcher exit/restart, and requires
actual reactivation/new root, not launcher exit alone, for restart completion.

Cleanup follows observation, kills only unique owned profiles and bounded-poll
checks absence. Separate inspection found no surviving owned profiles/Python
observers. No sleeps, fake events, synthetic didClose, production extension,
commit/helper release or production changes. Superseded attempt
`real-file-lifecycle-GTFsVY` misclassified reload RPC `Canceled` as harness error;
final harness records RPC rejection and requires external completion instead.
Earlier exploratory `yPVK5e`, `h85lCx`, `HW32TO` traces used subscription-disposal
markers; final paths above use the actual exported `deactivate` callback.

**Limits:** one clean draft/group/run per action on this Codium/Linux build;
workspace-change is another folder in the same window, not all multi-root
operations. Only events delivered while the extension/listener is alive can be
observed, not internal workbench events after disposal. No general lifecycle
guarantee, dirty-shutdown/OS-crash coverage or production arbitration acceptance.
Core/dirty probes and Neovim were not rerun. R-EDIT-REAL, step-0 overall
acceptance, pure-LSP limitations, run-correlated idempotency/helper arbitration
and migration remain pending. No production design/cutover or parent checkoff.

### Previously verified core fallback and dirty choices

Observer-only follow-up on 2026-10-29; **R-EDIT-REAL is not done**. No production
code, helper release, commit arbitration or task checkoff is included. The
historical pure-`didClose` failure below remains valid but is not the fallback
gate: failing `didClose` alone does not block the optional VSCode tab observer.

Executed successfully (exit 0, including diff check):

```sh
MAGIT_VSCODE_BIN=/home/william/.nix-profile/bin/codium node clients/vscode/test/e2e/real-file-spike.mjs && git diff --check
```

Retained verified extension-test host: `/tmp/opencode/real-file-vscode-XSt31I/`.
Retained verified normal development host: `/tmp/opencode/real-file-vscode-jLMkbH/`.
Each has `command.json` (exact binary/expanded arguments/version), `result.json`,
`api.jsonl`, `wire.jsonl`, `output.txt` and PNGs. Both use owned disposable real
Git repositories and isolated profiles/extensions. Codium is `1.140.06880`,
commit `807ba2d6075c29587f884698cd7370c81ab0724c`, x64; languageclient `9.0.1`.
Neovim was **not rerun** in this follow-up; its evidence below is historical.

Both final hosts assert:

| Probe | Observed/asserted result |
| --- | --- |
| Actual server `window/showDocument` | Real language-client `middleware.window.showDocument` invoked; permanent tab (`isPreview:false`). The middleware calls `showTextDocument` with `preview:false`; languageclient 9's default handler ignores a `preview` property added to protocol params. |
| Focus/switch to another file and back | Draft remains in tabs; zero qualified closures. |
| Same URI split across two groups; close first group's draft | Actual `event.closed` contains draft URI, but one matching tab remains in the other group; zero qualified closures. |
| Last draft tab close | Actual `event.closed` contains draft URI, all groups have zero matches; exactly one qualified event, in 57 ms (test host) / 52 ms (normal host). |
| Close unrelated tab while draft is absent | Actual closed URI is unrelated; zero additional qualifications. |
| Change open draft language to plaintext | One wire `didClose` then `didOpen`, tab still present; zero additional qualified closures. |
| Entire observation sequence | Exactly one qualified draft closure in the test host; three in the normal host (clean close, Save-on-close, Don't Save). Not a commit/helper-release assertion. |

The document is still alive at last-tab close, with zero wire `didClose` delta
in both hosts. This is a separate immediate observation, **not** a new five-second
reliability test; the historical five-second failure remains below. The sole
wire close in each new trace belongs to language change, not native tab close.

Normal host additionally drives the actual dirty Save/Don't Save/Cancel dialog
through CDP keyboard input. Cancel retains the dirty open tab without qualifying;
save alone leaves one open tab and qualifies zero times. Save-on-close saves
exact edited disk text and qualifies once; Don't Save closes,
qualifies once, and leaves disk at the previous saved text despite unsaved editor
text. The unsaved `discarded text\n` is captured and asserted **before** close,
including its exact-version full-text wire `didChange`; sampling the document
after Don't Save instead sees the reverted saved text. Both Save-on-close and
Don't Save tab events log/assert `savedDiskText` as
`dirty native text\ndirty native text\nspike message\n`. The observer reopens and
uses the live editor for the following refresh/language probes.
Extension-test mode suppresses the native dialog and makes no such claim.
These are tab-observer observations, not production freshness or commit-barrier
tests.

Inspected both verified hosts' API/wire traces and the normal host screenshots:
`03c-dirty-save-dialog.png` (Save focused; two dirty native text lines),
`03e-dirty-discard-dialog.png` (actual Save/Don't Save/Cancel prompt over
`discarded text`), `03b-dirty-cancel.png` (dirty tab retained),
`03d-dirty-save-closed.png` and `03f-dirty-discard-closed.png` (no draft tab),
and `04-refresh-language.png` (reopened, refreshed dirty plaintext editor).
Both results have `gate:"pass"`. Owned profiles
are killed and bounded-poll checked absent even on failure; process inspection
also found no surviving owned profiles or Python observer. SIGTERM alone can
leave a dirty dialog open. This abrupt cleanup is **not** shutdown acceptance.
No sleeps, synthetic close notifications or production extension are used.

The earlier pending clean close-window/reload/folder-change observations are
supplied by the scoped follow-up above. Production run-correlated
idempotency/helper arbitration and migration remain pending. No transport
shutdown or extension deactivation is interpreted as confirmation.

## Historical verdict: pure didClose OBSERVED FAIL

Everything below records the earlier run, not the new fallback gate or a new
Neovim run. Its original command failure and retained paths are preserved.

**Codium's ordinary real-file tab close did not deliver `didClose` within
5 seconds.** The tab disappeared, but the document remained alive. This happened
both with `--extensionTestsPath` and in a separate normal development host
without the extension test runner. Neovim passed with the specified
`bufhidden=wipe` and explicit LSP attach. No close arbitration, production code,
helper/security/lifecycle changes or migration were made. R-EDIT-REAL is not done.

This replaces the earlier *incomplete scaffolding* report: the failure below is
now actual native runtime evidence, not a missing test driver or missing didOpen.

## Commands and environment

Linux x64, `DISPLAY=:0`; Codium `1.140.06880`, commit
`807ba2d6075c29587f884698cd7370c81ab0724c`; Neovim `0.12.6` Release,
LuaJIT `2.1.1785763465`; tmux `3.8`; Python `3.14.8`; Node `v24.21.0`;
Git `2.56.0`. Project `vscode-languageclient` `9.0.1` is loaded by absolute module path.

```sh
MAGIT_VSCODE_BIN=/home/william/.nix-profile/bin/codium node clients/vscode/test/e2e/real-file-spike.mjs && git diff --check
python3 tests/spikes/run_real_file_nvim.py
git diff --check
```

Results: first command **exit 1** (genuine Codium close gate failure; the `&&`
does not execute diff checking); Neovim **exit 0**; standalone diff check **exit
0**. Neither client was skipped. Both fixtures use real disposable `git init`
repositories and actual `.git/COMMIT_EDITMSG` files. This is an observer spike,
not an actual Git commit/helper operation or Plain implementation acceptance.

Final JS syntax checks and Python in-memory compilation also passed. Process
inspection found no surviving owned editor/observer processes after cleanup.

The Codium launcher retains the exact expanded binary/arguments in each
`command.json`. It uses unique `--user-data-dir`, `--extensions-dir` and
`--extensionDevelopmentPath`, plus a real `--extensionTestsPath` in its first
phase. The second phase intentionally omits only the test path to allow native
dialogs. Neovim is launched by an owned 100x30 tmux server with:

```sh
nvim -u /home/william/src/magit-lsp/tests/spikes/real_file_nvim.lua --listen <unique-evidence-dir>/nvim.sock
```

Environment supplies the fixture path, evidence directory and project root.
The init opens the target after startup, attaches on BufReadPost, and retains a
separate status window so `:q` tests buffer closure rather than editor shutdown.
No `--clean` override, arbitrary settling sleeps, fake Git directories, synthetic
didOpen/didClose notifications or production extension load are used. Readiness
and evidence waits poll real conditions with bounded deadlines. Screenshots use
the isolated renderer CDP target; tmux captures are plain text. Only owned unique
profiles/tmux servers are terminated; no user editor is reused or closed.

## Retained and inspected evidence

Extension-test host: `/tmp/opencode/real-file-vscode-WqxaLp/`.
Normal host: `/tmp/opencode/real-file-vscode-SwgLGH/`.
Neovim: `/tmp/opencode/real-file-nvim-6ic9e_t0/`.

Both Codium directories retain `wire.jsonl` (actual observer stdin messages),
`api.jsonl` (separate editor API observations), `result.json`, `command.json`,
`output.txt` and renderer PNGs. The Neovim directory retains `wire.jsonl`,
`result.json` (including exact command/version), and `.screen` captures.
These are retained local artifacts, not portable repository attachments.

Inspected the normal host's `01-open.png`, `02-native-close.png`,
`03-dirty-dialog.png` and `04-refresh-language.png`: real file/text visible;
then no tab; native Save/Don't Save/Cancel dialog; refreshed dirty plaintext
document. Inspected Neovim's `01-open.screen`, `02-dirty-refused.screen`,
`03-refresh-language.screen`, `03b-external-change-warning.screen`,
`04-native-close.screen`: target visible; E37 and retained dirty text; replacement
text; external-change write warning; then only the status buffer. Actual wire
open/save/close records and standard request responses were inspected separately.

## Findings

| Probe | Codium | Neovim |
| --- | --- | --- |
| Actual attach/open | Wire didOpen, file URI, `git-commit`, initial text | Wire didOpen, file URI, `gitcommit`, initial text |
| Ordinary clean close | **FAIL:** native `workbench.action.closeActiveEditor`, zero tabs, `documentClosed:false`, zero wire closes at 5027 ms (test host), 5018 ms (normal host) | **PASS:** native `:q`, buffer wiped, one wire didClose in 4 ms; editor/status window remains alive |
| Dirty native handling | Normal host displays Save/Don't Save/Cancel; close promise remains pending. Escape cancels; document stays open/dirty. Subsequent save emits wire didSave | Native `:q` refuses with E37, leaves buffer dirty; `:w` saves exact text and emits didSave without close |
| Disk rewrite + standard server `window/showDocument` on reused open URI | Immediately after successful show response, still displays old saved text, not replacement disk template | Also retains old saved text after successful show response |
| Standard versioned `workspace/applyEdit` refresh | `applied:true`, exact replacement visible, dirty | `applied:true`, exact replacement visible, modified |
| Language-mode change while open | `git-commit` → `plaintext` emits one wire didClose then didOpen for the **same URI while its tab is still open** | `:set filetype=text` leaves client attached; observed zero additional opens/closes; later real close still sends didClose |

The showDocument observation is immediate, not a claim that Codium's file watcher
never eventually reloads. A showDocument success is not a freshness barrier.
Neovim's subsequent `:w` after external rewrite produces the native “file has
been changed since reading” confirmation; the driver inspects it, answers `y`,
then dismisses the observed hit-enter acknowledgement before closing.

The later Codium didClose in each wire trace belongs to **language change**, not
the earlier native tab close. Treating that notification as unconditional commit
completion would finish while the user is still editing. No workaround is added.

## Existing rich abort ownership (source inspection, not new UI acceptance)

Neovim `lua/magit-lsp/edit.lua` `M.cancel` invokes `common.cancel`, notifies close,
calls `view.close`, then focuses the base view. `lua/magit-lsp/view.lua`'s
`M.close` force-deletes its owned buffer. VSCode `clients/vscode/lib/drafts.js`
`cancel` invokes owning-view `common.cancel`, then `forget`; that closes matching
tabs through `closeTab`/`vscode.window.tabGroups.close` and restores the owning
view. These current managed-draft paths own their UI cleanup, rather than relying
on a standard server close-buffer request. Existing native suite assertions at
`clients/vscode/test/e2e/suite.js:1728–1734` cover cancel returning focus and no
commit; they were **not rerun here**. This source check is not a claim that a
new real-file abort migration has been tested. That migration remains stopped.

## Harness corrections and non-accepted attempts

Earlier supplied scaffolding lacked an actual extension test path, used sleeps
and a fake `.git`, and attached Neovim before file read with conflicting
`--clean -u`. Those runs did not test the gate.

During correction, non-accepted artifacts/errors were retained:

- `real-file-vscode-pMyOzB`: `TypeError: Cannot read properties of undefined
  (reading 'activate')`; wildcard engine declaration prevented extension lookup;
  corrected manifest to `^1.85.0`.
- `real-file-vscode-4O75PV` and `real-file-vscode-VXwCua`: genuine close failure,
  then `Deadline: native dirty save dialog`. Investigation found Codium's
  FileDialogService suppresses save confirmation under extensionTestsPath.
  Added the separate normal host, **not** a production dialog override.
- `real-file-vscode-Ui19V0`: `Deadline: painted editor`; Monaco renders a
  nonbreaking space, corrected whitespace-normalized readiness condition.
- `real-file-nvim-wkyns3gi`: `Deadline: actual didOpen`; startup autocmd needed
  nested BufReadPost events, corrected `nested=true`.
- `real-file-nvim-nkzu3oo2`: `Deadline: save refreshed template`; the native
  external-change confirmation was pending. `real-file-nvim-2p3ivw2_`: RPC
  `TimeoutExpired(..., 5)` while the following hit-enter prompt was pending.
  Driver now inspects/answers both native prompts, rather than bypassing them.

Historical final runs above contain no harness exception. Their Codium result was
`gate:"observed-fail"`, `closeError:"Deadline: native tab close wire didClose"`;
Neovim was `gate:"pass"`. That pure-didClose gate failed. Do not check off
R-EDIT-REAL or begin production deletion/cutover on these results.
