# R-EDIT-REAL step-0 native client spike — 2026-10-29

## Verdict: OBSERVED FAIL — stop before production migration

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

Final runs above contain no harness exception. Their actual Codium result is
`gate:"observed-fail"`, `closeError:"Deadline: native tab close wire didClose"`;
Neovim is `gate:"pass"`. The hard gate remains failed. Do not check off
R-EDIT-REAL or begin production deletion/cutover on these results.
