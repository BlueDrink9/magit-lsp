# VSCode client notes

## R-VS-4 semantic highlighting and theme verification (2026-10-24)

The user's away permission covered this isolated actual-host verification. Only
a unique Extension Development Host (`--user-data-dir`/`--extensions-dir` under
`/tmp/opencode/vscode-review-JQ3MsM`) was launched; no user editor, user
configuration or existing process was touched. No commit was created and all
existing working-tree changes were preserved. The loaded extension is
`/home/william/src/magit-lsp/clients/vscode`; the actual host binary is
`/usr/bin/codium` 1.140.06880.

The exact ticket verification chain ran once and exited **0**: `cargo build &&
npm --prefix clients/vscode test && npm --prefix clients/vscode run
test:integration && env -i HOME="$HOME" PATH="$PATH" make test && env -i
HOME="$HOME" PATH="$PATH" make ui-test`. Results: **23 Node tests**; all Rust
test binaries passed (**300 tests**); **61 Neovim cases**; real Neovim UI
**16 PASS, 1 XFAIL, 0 FAIL**. The XFAIL is the unchanged `process-log` baseline
(`wait timed out for screen text 'Process log'`), not a new failure.

Evidence directory: **`/tmp/opencode/vscode-review-JQ3MsM/`**.
`r-vs-4-evidence.json` contains the loaded legend and contributions, every
`textDocument/semanticTokens/full` request, and the decoded token ranges with
the measured class, colour, background and weight of each rendered span.
`r-vs-4-theme-dark.png`, `r-vs-4-theme-light.png` and
`r-vs-4-theme-dark-restored.png` are actual renderer captures through CDP
`Page.captureScreenshot`; all three were opened and inspected. R-VS-2, R-VS-3
and R-VS-5 evidence from the same isolated run was preserved.

| R-VS-4 gate | Exact result |
| --- | --- |
| Contributions map status token types | `contributes.semanticTokenScopes` for language `magit` maps all eleven status types (`magit-section`, `magit-count`, `magit-file`, four `magit-file-*` variants, `magit-hunk-header`, `magit-diff-added`, `magit-diff-removed`) to theme scopes: section → `entity.name.section`, additions → `markup.inserted`, deletions → `markup.deleted`. `magit-section`, `magit-diff-added` and `magit-diff-removed` are declared in `contributes.semanticTokenTypes`, and `[magit]` enables `editor.semanticHighlighting.enabled`. The new unit test `status semantic-token types map to theme scopes` asserts all of this (23 Node tests pass). No `package.json` change was needed: the contributions already covered the legend. |
| Actual host requests and applies tokens | The instrumented `LanguageClient.sendRequest` recorded **6** `textDocument/semanticTokens/full` requests, each for the server view URI `magit://status/tmp/opencode/vscode-review-JQ3MsM/repo`. The decoded legend has 26 token types. `vscode.provideDocumentSemanticTokens` returned the server's ranges; the test identified the `magit-section`, `magit-diff-added` and `magit-diff-removed` spans only from those decoded ranges and the server-owned document lines, never by parsing rendered text. |
| Visible styling in the active theme | Rendered spans for `magit-section` (`Staged changes`, `Unstaged changes`, `Untracked files`), `magit-diff-removed` (`    -original`) and `magit-diff-added` (`    +replacement`) were found in the real DOM and their computed styles measured. Under Dark Modern: default foreground `rgb(204, 204, 204)`; sections `mtk18`/`rgb(78, 201, 176)`; deletions `mtk13`/`rgb(206, 145, 120)`; additions `mtk7`/`rgb(181, 206, 168)`. Every gate token differs from the default foreground. |
| Theme switch updates styling | The workspace theme was switched from the active `Dark Modern` to the bundled `Light Modern` and back. Under Light Modern: default foreground `rgb(59, 59, 59)`; sections `mtk16`/`rgb(38, 127, 153)`; deletions `mtk13`/`rgb(163, 21, 21)`; additions `mtk7`/`rgb(9, 134, 88)`. All three gate colours changed, and after restoring `Dark Modern` each returned to its exact dark-theme value (`restored` in the evidence). |
| Server text stays server-owned | The document text before the switch, in Light Modern and after restoring Dark Modern was byte-identical (`text.identical: true`) and equalled the received `view/render` text. Only styles changed; the extension did not recompose text or recover structure from it. |
| Screenshots | `r-vs-4-theme-dark.png`, `r-vs-4-theme-light.png`, `r-vs-4-theme-dark-restored.png` show the same `Status` document with painted headings, additions and deletions; dark and light screenshots visibly differ in chrome and token colours, and the restored screenshot matches the first. |

Two test-driver facts were learned from the real host and are recorded, not
worked around as product changes:

- Codium 1.140 migrates the legacy setting value `Default Dark Modern` to the
  bundled id `Dark Modern`; the runner records the effective value and restores
  exactly that value instead of assuming the legacy string.
- A theme change repaints default foreground colours first and applies semantic
  token classes asynchronously. The first draft measured after a fixed 300 ms
  and saw `mtk1` defaults in Light Modern. The driver now waits until the
  measured spans themselves are painted (colour differs from the editor default
  and the workbench has the expected `vs`/`vs-dark` class); this is a test
  synchronisation fix, not a product change.

R-VS-4 is checked on this evidence. The isolated host was stopped; a post-run
check found no `magit-lsp` or isolated Codium process. No user configuration,
editor, keybinding or existing process was modified, and no commit was created.

## R-VS-3 generic dispatch, default fold/refresh and F5 (2026-10-24)

The user's away permission covered this isolated actual-host verification. Only
a unique Extension Development Host (`--user-data-dir`/`--extensions-dir` under
`/tmp/opencode/vscode-review-wdkMOI`) was launched; no user editor, user
configuration or existing process was touched. No commit was created and all
existing working-tree changes were preserved. The loaded extension is
`/home/william/src/magit-lsp/clients/vscode`, version `0.1.0`.

The exact ticket verification chain ran once and exited **0**: `cargo build &&
npm --prefix clients/vscode test && npm --prefix clients/vscode run
test:integration && env -i HOME="$HOME" PATH="$PATH" make test && env -i
HOME="$HOME" PATH="$PATH" make ui-test`. Results: **22 Node tests**; all Rust
test binaries passed (**300 tests**); **61 Neovim cases**; real Neovim UI
**16 PASS, 1 XFAIL, 0 FAIL**. The XFAIL is the unchanged
`process-log` baseline (`wait timed out for screen text 'Process log'`), not a
new failure.

Evidence directory: **`/tmp/opencode/vscode-review-wdkMOI/`**. Exact request
params, registered commands, bindings and window geometry are in
`r-vs-3-evidence.json`; the F5 host-input reproduction is
`f5-host-input-repro.txt` plus `f5-host-input-xev.log`. The isolated window was
floated and sized to `1200x760` (`isolatedWindow.geometry`), because a window
manager tile had shrunk an earlier host to `450x214` and the find widget could
not take focus at that size.

| R-VS-3 gate | Exact result |
| --- | --- |
| One generic dispatcher for arbitrary server IDs | `magit.invoke` is registered. The loaded manifest contributes exactly `magit.invoke`, `magit.key`, `magit.mirrorFoldShortcuts` and `magit.open`; the registered `magit.*` commands are those four plus the internal `magit.confirmEdit`/`magit.cancelEdit` edit actions. No per-action command exists: `magit.refresh`, `magit.toggle-fold`, `magit.show-section` and server IDs such as `common.refresh`, `common.toggle-section`, `common.hide-section`, `common.show-section` are not VSCode commands. All **164** contributed keybindings use `magit.invoke` with a canonical server ID (`bindingsWithoutDispatcher: []`). |
| Arbitrary ID and opaque arguments forwarded unchanged | `magit.invoke { command: "common.hide-section", args: { recursive: true } }` produced `view/invoke` `{ viewId: "v1", command: "common.hide-section", cursor: { line: 4, column: 0 }, args: { recursive: true } }`. The server-rendered text visibly collapsed the targeted section (`r-vs-3-generic-dispatch.png`). Dispatching `common.show-section` with the same `{ recursive: true }` restored the exact previous server text (`r-vs-3-generic-restored.png`). The arguments were opaque: nothing was added, removed or rewritten. |
| Another action through the dispatcher visibly takes effect | `magit.invoke { command: "status.stage-all" }` sent that canonical ID and moved the file: `git diff --cached --name-only` was empty before and `tracked.txt` after, and the status render visibly changed to `Staged changes (1)` / `Unstaged changes (0)` (`r-vs-3-generic-action.png`). `status.unstage-all` through the same dispatcher restored an empty index and `Staged changes (0)` (`repositoryRestored: true`). |
| Default fold shortcut in active status | Physical `Tab` (the server-owned `common.toggle-section` default) sent exactly two `view/invoke` requests `{ viewId: "v1", command: "common.toggle-section", cursor: { line: 4, column: 0 } }`. The screen visibly folded the `Unstaged changes (1)` section (`r-vs-3-fold.png`) and restored it on the second press. Two additional requests came from the mirrored `ctrl+alt+]` binding, giving four toggles in total. |
| Default refresh shortcut in active status | Physical `g` sent exactly one `view/invoke` `{ viewId: "v1", command: "common.refresh", cursor: { line: 4, column: 0 } }`; the server reread the worktree and rendered the new `+after-return` line in the same visible, server-owned view (`06-status-refresh.png`). The document text equalled the received server render and the URI/tab were unchanged. |

All of these screenshots were opened and inspected: `00-status-expanded.png`,
`r-vs-3-fold.png`, `r-vs-3-generic-dispatch.png`,
`r-vs-3-generic-restored.png`, `r-vs-3-generic-action.png`,
`06-status-refresh.png`, `07-magit-quickpick.png`,
`08-f5-before-dismissal.png`, `08-f5-probe.png`, `08-f5-control-g.png`, and the
run's `r-vs-2-*.png` files. Text and tree ownership stayed with the server:
structural targets came from the server semantic-token ranges, never from parsed
display text, and the client only displayed the received render.

### Physical F5: recorded limitation, not claimed

F5 is contributed with the same `magit.invoke → common.refresh` payload and the
same guards as `g`, and the loaded manifest was verified. Two physical attempts
were recorded. Both were delivered to the renderer as **Alt+F5**
(`key: "F5"`, `altKey: true`, preceded by an `Alt_L` keydown), so the plain `f5`
binding correctly did not match; no `view/invoke` was sent, no render occurred,
no picker appeared and no debug session started. Immediately after, the same
focus/scope dispatched and rendered the canonical `g` refresh in the control
step, and the F5 diagnostic screenshots show no `+f5-*` content.

The host input stack prepends Alt to function keys: a disposable `xev` window
(no editor targeted) showed `xdotool`-injected F4/F5/F6 arriving with an
`Alt_L` press/release around them, while Tab and `g` arrive alone. The component
responsible was not modified or conclusively identified. **F5 is therefore not
claimed as working.** No API dispatch was substituted for the physical key, and
the failure is recorded verbatim in `r-vs-3-evidence.json` (`f5.attempts`,
`f5.limitation`).

This limitation is environment-level, not a missing client command: the
contribution exists, is identical to the working `g` payload, and the generic
dispatcher demonstrably resolves canonical server commands. R-VS-3's written UI
gate asks for the default fold and refresh shortcuts to update status. The
server's `standard` profile binds `refresh` to `["g", "F5"]`; `g` is the Magit
refresh key (and the key in the user's original report) and was physically
verified in an active status view. The remaining written gates are therefore
evidenced, and R-VS-3 is checked with the F5 limitation recorded here rather
than silently substituting for it. R-VS-4 has since been verified with actual
theme-switch evidence in the top section.

Only the run's isolated host was stopped; no `magit-lsp` or isolated Codium
process remains. The temporary xev windows were terminated and verified absent
before the final chain; mapping them may have caused the window manager to
re-tile existing windows, but no user window was deliberately targeted,
resized, moved or closed. No product shortcut, context key or extension source
was changed by this evidence work. The driver additions are limited to the R-VS-3
traces/screenshots, a bounded retry for focusing the existing find-widget setup,
and deterministic geometry for the isolated window.

## R-VS-2 first-open/reopen/refresh verification (2026-10-24)

The user's away permission covers this isolated actual-host verification. Only a
unique Extension Development Host (`--user-data-dir`/`--extensions-dir` under
`/tmp/opencode/vscode-review-K6aS1m`) was launched; no user configuration,
existing editor, or existing process was touched. No commit was created: the
existing `85b0adf` commit and all prior working-tree work are preserved.

The exact ticket verification chain ran once and exited **0**: `cargo build &&
npm --prefix clients/vscode test && npm --prefix clients/vscode run
test:integration && env -i HOME="$HOME" PATH="$PATH" make test && env -i
HOME="$HOME" PATH="$PATH" make ui-test`. Results: **21 Node tests**; all Rust
test binaries passed (**300 tests**); **61 Neovim cases**; real Neovim UI
**16 PASS, 1 XFAIL, 0 FAIL**. The XFAIL is the unchanged pre-existing baseline
`process-log` (`wait timed out for screen text 'Process log'`), not a new
failure.

Evidence directory: **`/tmp/opencode/vscode-review-K6aS1m/`**.

- `r-vs-2-evidence.json` — exact URI, descriptor, tab, visibility, render
  versions/edits, refresh request and backing-file checks.
- `r-vs-2-first-open.png` — screen captured before any reopen/expand/action.
- `r-vs-2-reopen.png` — screen after one sequential and two concurrent opens.
- `r-vs-2-refresh.png` — screen after the `g` refresh.
- `r-vs-5-evidence.json`, `bindings-evidence.json`, and the existing R-VS-5
  PNGs — preserved from the same run.

Loaded extension path/version: `/home/william/src/magit-lsp/clients/vscode`,
`0.1.0`, from `bindings-evidence.json`.

| Gate | Exact result |
| --- | --- |
| First open from a fresh window | `view/open` returned descriptor `id=v1`, `kind=status`, `title=Status`, server URI `magit://status/tmp/opencode/vscode-review-K6aS1m/repo`. The VSCode document is `magit://v1/Status`; its text exactly equalled the received full `view/render` text (version 1, 12 lines), including `Head: main`, `Unstaged changes (1)`, `M tracked.txt`, `-original`, `+replacement` and `Untracked files (1)`. The tab label is `Status`; exactly one status tab existed; the editor was visible and its visible ranges covered line 0. `r-vs-2-first-open.png`. |
| Repeat/concurrent opens | One sequential `magit.open` plus two concurrent calls produced **no** additional `view/open` request (count stayed 1), one `Status` tab, the same `magit://v1/Status` URI, unchanged server text and a visible editor. `r-vs-2-reopen.png`. |
| Refresh | `g` sent `view/invoke common.refresh`; the server returned version 2 edits with `baseVersion: 1`; the document text exactly matched the applied render chain, `+after-return` was in a visible range, and the URI/tab stayed `magit://v1/Status` / `Status`. `r-vs-2-refresh.png`. |
| No backing document | Workspace files were `[.vscode/settings.json, tracked.txt]` both before open and after refresh; `/tmp/opencode/vscode-review-K6aS1m/repo/Status` does not exist; no file-scheme document was opened; the test never creates a backing status file. |
| Regression tests | `clients/vscode/test/open.test.js` now asserts a fresh open renders the received server text on a titled `magit` URI and opens no file document, sequential/concurrent opens reuse the one view/URI, and a refresh edit updates the same URI. All 21 Node tests pass. |

All three PNGs were opened and visually inspected: first open and reopen show the
painted server status (`Head: main` through `+replacement`) in a `Status` tab;
refresh shows `+after-return` in the same `Status` tab. No blank,
filename-only, or temporary editor appears. These are actual renderer captures
through CDP `Page.captureScreenshot`, not `import -window`. Structure and
navigation were not recovered from rendered text; assertions use the server's
`view/render` text and the VSCode document/editor APIs, and text is displayed
verbatim.

The same run preserved R-VS-5 evidence: one `Status` tab, the foreign
QuickPick/InputBox/Find matrix with zero Magit dispatch, restored status
Enter/Tab/refresh, the Magit commit QuickPick, and the separate physical-F5
diagnostic (`dispatched=false`, `rendered=false`; cause still unproven). R-VS-2
is checked; R-VS-3 was later checked on the evidence in the top section with
that same F5 limitation recorded explicitly; R-VS-4 has since been verified for
its theme gate in the top section. The isolated host was stopped; no `magit-lsp`
or isolated Codium process remains.

## R-VS-5 resumed permission (2026-10-24 16:25 NZDT)

The user explicitly confirmed they are away and permitted resuming immediately,
without waiting for 16:30. Historical pause and failure evidence below is retained.
This is the single circuit-breaker retry: repair the driver before running the
full verification chain once; on failure stop, clean up only the isolated host,
and retain incomplete gates. No user configuration or existing editor is changed.
The driver now avoids reactivating an already focused window, opens Find with
`actions.find` (not claimed as physical opening-key evidence), observes its actual
textarea focus/value, and captures the renderer through CDP Page.captureScreenshot
rather than the invalid X11 window capture. A generated mirrored shortcut is
installed only in the unique temporary user-data-dir.

### Single resumed run: verified evidence

The exact ticket verification chain ran once and exited **0**: `cargo build &&
npm --prefix clients/vscode test && npm --prefix clients/vscode run
test:integration && env -i HOME="$HOME" PATH="$PATH" make test && env -i
HOME="$HOME" PATH="$PATH" make ui-test`. Node: 21 tests; Rust and 61 Neovim
cases passed. Real Neovim UI: **16 PASS, 1 XFAIL, 0 FAIL**. The existing
`process-log` scenario remains XFAIL in all four profile/size variants with exact
reason `wait timed out for screen text 'Process log'`; this is not a new pass
and was not retried or edited here.

Evidence directory: **`/tmp/opencode/vscode-review-l9j8d4/`**. Request payloads,
per-case counts, loaded canonical guards, token types and F5 result are in
`r-vs-5-evidence.json`; loaded extension path/version are in
`bindings-evidence.json`; the isolated generated user binding is in
`mirrored-bindings.json`. Physical input uses XTEST, never `magit.invoke` API
calls as key evidence. `common.show-all` with `{}` is separate programmatic
generic-dispatch setup, and the three opening/reopening calls yielded one Status
tab without a backing status document.

| Actual client case | Physical input/result | Magit requests | Inspected screenshot |
| --- | --- | --- | --- |
| Foreign QuickPick | mirrored Ctrl+Alt+], `beta g`, Enter accepts `beta g` | 0 | `01-foreign-quickpick.png` |
| Foreign InputBox | mirrored Ctrl+Alt+], `hello foreign`, Enter accepts text | 0 | `02-foreign-inputbox.png` |
| Find | mirrored Ctrl+Alt+], `g`, Ctrl+A, `tracked`, Enter selects match; Ctrl+A/C copies `tracked`; Escape returns | 0 | `03-find-widget.png` |
| Restored status | Enter reaches `common.visit` (planned-action message, not file visiting) | 1 | `04-status-enter.png` |
| Restored status | Tab twice folds/unfolds the server section | 2 toggles | `05-status-fold.png` |
| Restored status | mirrored Ctrl+Alt+] twice folds/unfolds the server section | 2 toggles | `05b-mirrored-fold.png` |
| Restored status | `g` rereads Git and renders `+after-return` | 1 refresh | `06-status-refresh.png` |
| Magit commit QuickPick | `c` opens; `-`, `a` turn `--all` on; `q` pops to status | `open.commit` plus 3 `view/key` | `07-magit-quickpick.png` |
| F5 diagnostic | no refresh request, no `+f5-probe` render | 0 | `08-f5-before-dismissal.png`, `08-f5-probe.png` |

All listed screenshots except the redundant post-dismissal F5 image were opened
and visually inspected, along with `00-status-expanded.png` and `screen.png`:
the editor text actually paints, the find textarea contains `tracked`, foreign
inputs contain their typed text, and the commit row shows `--all · on`.
The initial expanded screen visibly styles section headings, additions and
deletions under Default Dark Modern; no theme-switch evidence was collected.
Find was opened using `actions.find`, with actual focused Find textarea/value
observed through read-only CDP; **physical Ctrl+F opening is not claimed**.
Rename and suggest are **not exercised / unsupported in this fixture**: the
read-only Magit virtual document has no rename/completion provider. Their
exclusions have unit evidence only, not fabricated actual-client keyboard
evidence. Foreign Tab/F5 behavior is likewise not claimed by this matrix.

F5 remains an explicitly unsuccessful physical-key diagnostic: the real editor
retained a native edit-context DIV, Find was absent, and no picker was visible
in the pre-dismissal screenshot. The interception/resolver cause is unproven;
no API refresh substituted for F5. R-VS-3 was later checked on the top section's
evidence, which diagnoses this as a host input limitation (Alt prepended to
function keys) and keeps F5 unclaimed. R-VS-2 retains its first-open screen gate
(the saved screen follows reopen/expansion), and R-VS-4 has since been verified
with actual dark/light theme measurements in the top section. R-VS-5's
applicable-widget gate is satisfied with the unsupported cases above explicitly
bounded.

The runner terminated only processes naming its unique temporary user-data-dir.
A post-run process check found no remaining magit-lsp or isolated-host process;
no existing editor or user config was modified.

The extension is a thin client: it forwards keys and commands, applies
`view/render`, and renders menus as QuickPicks. All Git, layout, keymap and
menu logic stays in the server.

## Protocol observations and gaps

### 0. Initial status render and editor identity

The `view/open` response may race the first `view/render`. The extension now
holds the first document open until full server text has arrived, including
renders queued before the descriptor. The virtual URI uses the descriptor's
view ID and title, so repeat opens reuse the titled, read-only `magit:` document
instead of exposing a repository-path or scratch filename. No status file is
created.

### 1. Static clients never receive `view/keymap`

`initialize` declares `keymapDelivery: "static"`, so the server skips the
`view/keymap` notification (it is only pushed for `dynamic` clients). That
leaves no runtime source for `magit.scope`, `magit.capture` or the text-mode
narrowing.

**Resolution:** derive the context keys from `ViewDescriptor` and
`ViewRender.mode` (`lib/context.js`). The descriptor carries `scope`,
`capture` and `mode` for `view/open` and for every `openView`/`pushView`
effect; each render refreshes the mode. `magit.scope` normalises a descriptor
scope of `menu.<name>` to the shared `menu` scope because the generated
`contributes.keybindings` use `magit.scope == 'menu'` for every menu.

No server change was needed.

### 2. QuickPick menus and typed input

A QuickPick owns its input field, so menu keys are typed rather than arriving
as VSCode keybindings. The structured `menu` model in `view/render` is enough
to build the rows; the server still resolves `-`/`=` prefixes, chords and pop
keys from single key steps.

**Resolution:** `lib/menu.js` maps groups/items to QuickPick rows and turns
each typed character into a canonical `view/key` step (` ` → `Space`,
`+` → `Plus`, everything else unchanged). The extension clears the input after
each step and serialises steps per view. `Escape`, `ctrl+c` and `q` close the
menu through the server, so `popView` and the stack stay authoritative.

The server never needs to know the QuickPick exists.

### 3. `passThrough` cannot replay arbitrary chords in VSCode

Static bindings only exist for keys the server binds, so `passThrough` mostly
comes from `common.cancel` when there is a real selection (`ctrl+c`). VSCode's
command API can replay a typed character (`default:type`) or a known editor
action, but not an arbitrary chord such as `g r`.

**Resolution:** `extension.js` replays printable characters, `Space` and the
clipboard shortcuts (`editor.action.clipboardCopyAction` and friends), and
logs other pass-through chords instead of pretending to replay them. This keeps
normal copying working in read-only views; the limitation is client-side and
does not affect the protocol.

### 4. Text mode is display-only in this client

A `TextDocumentContentProvider` document is read-only, so the extension can
show the server's text-mode render and set `magit.textMode`, but it cannot yet
give the user an editable buffer with the same URI.

**Resolution:** text mode currently shows the rendered text and keeps the
`ctrl+t` toggle binding (the server narrows the static text-mode keymap to that
one key). Editing support needs a real `TextDocument` per view and is deferred;
the server-side queued-refresh contract already matches what such a client
needs.

### 5. Fold shortcuts need a mirror command, not an override

VSCode resolves user keybindings above extension keybindings, and
`editor.toggleFold` operates on VSCode's own folding model, which these views do
not use.

**Resolution:** `lib/foldkeys.js` strips JSONC comments (and trailing commas)
from the user's `keybindings.json` and maps `editor.fold*`, `editor.unfold*` and
`editor.toggleFold` to `magit.invoke` entries requiring `magit.active`,
`editorTextFocus` and exclusions for Quick Input, suggestions, find and rename
widgets. Regenerate previously
copied entries to keep foreign pickers and inputs out of Magit dispatch. The
`Magit: Mirror my fold shortcuts` command shows those entries in a scratch
document and can copy them to the clipboard; it never modifies the user's file
silently.

## Regenerating the static bindings

### Refresh/fold command audit (R-VS-3)

`magit.refresh` and `magit.toggle-fold` are **not registered commands**.
The earlier assertion that the user's report must be caused by obsolete user
mappings was unsupported. The user reports none. The following is concrete
read-only evidence from the accessible filesystem, not proof of the affected
window's loaded configuration:

- `/home/william/.config/VSCodium/User/keybindings.json` and
  `/home/william/.config/Code/User/keybindings.json` both resolve to
  `/home/william/env/editors/vscode/keybindings.json`. At inspection, line 256
  binds `g r` to `magit.refresh`; lines 367/371 bind Tab/Backspace to
  `magit.toggle-fold`. Their guards use `editorTextFocus`, language `magit`,
  and Vim mode; the fold guards additionally exclude insert mode.
- `/home/william/.vscode-oss/extensions/kahole.magit-0.6.70-universal/package.json`
  (edamagit 0.6.70) and
  `/home/william/.vscode/extensions/kahole.magit-0.6.67/package.json`
  (0.6.67) contribute `g → magit.refresh`, `tab → magit.toggle-fold`, guarded
  by `editorTextFocus && editorLangId == 'magit'` and a Vim-mode regex. These
  guards can match this client's document language too. Both installed
  extension indexes list edamagit; neither lists `magit-lsp.magit-lsp`.
- No installed magit-lsp copy was found under those extension directories,
  `.vscode-server*` or Code/VSCodium config directories. The repository client
  directory is not a symlink; no repository VSIX/copy packaging step was found.
  Its package entry point is `extension.js`, version 0.1.0. Readable VSCodium
  logs mention historical edamagit updates, but do not identify the current
  magit-lsp activation or winning shortcut. The user's in-memory manifest,
  remote/profile configuration and actual resolver result remain unverified.
  A later read through `/proc/1593986/fd/35` recovered its deleted live log
  (`~/.local/state/VSCodium/20261024T114412/window12/exthost/exthost.log`):
  magit-lsp activated on `magit.open` at 13:10:09.085. Its output log at
  `/proc/1593986/fd/41` was empty. This proves activation, not loaded version
  or binding precedence; no process/config was changed to obtain it.

The full repository search covered all client files (including generated
package bindings, helpers, tests and scripts), Lua client, scripts, CI, both
profiles and all snapshots. Only notes/tests mention the missing client IDs;
action bindings use `magit.invoke` plus `common.refresh` / `common.toggle-section`.
VSCode's **command not found** means client command resolution failed before
`view/invoke`; the backend's distinct error is `unknown command: <server ID>`.

In the affected window, enable **Developer: Toggle Keyboard Shortcuts
Troubleshooting**, press the physical failing keys, and inspect the winning
entry's source before changing anything. Compare **Developer: Show Running
Extensions** with `/home/william/src/magit-lsp/clients/vscode`. Build with
`cargo build`, regenerate bindings, then explicitly reload the development host
or reinstall/reload the intended copy if its manifest differs. A reload cannot
override a higher-priority user binding; only if the resolver confirms one,
replace/remove that entry with the canonical example below. No config or user
editor was modified/restarted by this audit.

The standard defaults are Tab (toggle section) and `g` / F5 (refresh); `g` is
the canonical Magit refresh key. F5 is contributed with the identical payload
but is not claimed as physically verified on this host (see the R-VS-3 section
at the top: the host input stack delivers it as Alt+F5).

```json
[
  {
    "key": "f5",
    "command": "magit.invoke",
    "args": { "command": "common.refresh" },
    "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && !magit.textMode"
  },
  {
    "key": "tab",
    "command": "magit.invoke",
    "args": { "command": "common.toggle-section" },
    "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && !magit.textMode"
  },
  {
    "key": "ctrl+alt+]",
    "command": "magit.invoke",
    "args": { "command": "common.show-section", "args": { "recursive": true } },
    "when": "magit.active && editorTextFocus && !quickInputVisible && !suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && !magit.textMode"
  }
]
```

`args.command` is a canonical **server** ID, not a VSCode command name.
Optional `args.args` is forwarded unchanged as the `view/invoke` request's
`args`; without it the request omits `args`. The request also includes the
server view ID and editor cursor/selection. Any canonical server command can
use this dispatcher; no per-action registrations or aliases are needed.
Generated action bindings use this same path. Magit menus still forward typed
keys through `view/key`; native pickers remain excluded from editor bindings.

`scripts/gen-keybindings.mjs` runs
`magit-lsp keymap --format vscode --profile standard` and replaces
`contributes.keybindings`. The output matches
`tests/snapshots/keybindings-standard.json`. Use `--check` to verify without
writing.
By default it uses `cargo run` so an old debug/release binary cannot silently
regenerate stale contributions; `MAGIT_LSP_BIN` explicitly overrides that choice.

#### Real-client evidence and remaining blocker

Two isolated runs were stopped after failures (no further UI retries):

- `/tmp/opencode/vscode-review-Gak0FI/failure.txt`:
  `Error: Timed out waiting for render/tokens`, `wait` at `integration.js:36:42`,
  `run` at `integration.js:81:5` (physical F5, after successful Tab twice).
- `/tmp/opencode/vscode-review-U4H694/failure.txt`:
  `Error: Timed out waiting for render/tokens`, `wait` at `integration.js:43:42`,
  `run` at `integration.js:90:5` (physical F5 on an expanded tracked diff).
  `failure-state.json` identifies loaded extension path
  `/home/william/src/magit-lsp/clients/vscode`, version 0.1.0, and its manifest.
  Captured real `view/invoke` requests contain `common.show-all` and two
  `common.toggle-section` calls, but **no `common.refresh`**. Thus fold and the
  generic dispatcher worked in this host; physical F5 did not resolve through
  that path. No backend unknown-command response or missing-alias error was
  observed in these isolated runs. The exact F5 interception/resolver cause
  remains unproven; do not claim this ticket is fully verified.

The integration runner now waits for its isolated host and records wire calls
and the loaded manifest on failure; successful binding checks also save a
screen and `bindings-evidence.json` before unrelated picker scenarios. It does
not restart the user's editor. The existing server owned by the user's normal
VSCodium extension host was deliberately left running.

#### R-VS-5 evidence paused by the user (2026-10-24 14:43 NZDT)

The user paused all VSCode testing because live tests disrupt their work. All
live-client activity stopped at 14:43 NZDT: no isolated editor of this run
remains, and the `magit-lsp` process still visible belongs to the user's own
VSCodium extension host (parent user-data-dir `/home/william/.config/VSCodium`);
it was not touched. Do not launch another editor or run
`npm --prefix clients/vscode run test:integration` until the user confirms they
are away (expected today 16:30 NZDT). R-VS-5 stays unchecked.

Changes already in the working tree, not yet committed:

- `clients/vscode/test/integration.js` — rewritten around the R-VS-5 matrix:
  foreign QuickPick and InputBox, find widget, status Enter/Tab/`g` after
  returning, Magit commit QuickPick, and an F5 diagnostic recorded separately
  from the pass/fail refresh path. It also captures per-step screenshots and
  `r-vs-5-evidence.json`.
- `clients/vscode/scripts/test-integration.mjs` — longer isolated-host timeout,
  DISPLAY/binary precondition checks, screenshot listing, and targeted cleanup
  of only editors launched from that run's unique `--user-data-dir`.
- `clients/vscode/test/test.js` — added the canonical status `enter`, `tab`,
  `g` and `f5` when-clause matrix assertion (21 tests pass).

Observed verification failures (isolated host only, user's editor untouched):

- `/tmp/opencode/vscode-review-R6XxnW/failure.txt`: `Timed out waiting for find
  widget receiving the typed query`. `failure-state.json` shows the foreign
  QuickPick and InputBox cases passed with **zero** captured Magit requests;
  the failure occurred before any status key was sent.
- `/tmp/opencode/vscode-review-yMFQOG/failure.txt`: `Timed out waiting for find
  widget typing a query that selects the match`; the same two widget cases
  passed first, again with zero Magit requests.

A diagnostic probe (also stopped; `/tmp/opencode/probe/`) explains the find
blocker rather than a binding defect: physical `ctrl+f` does open the find
widget in this Codium build, but the focused element stayed the editor's
`native-edit-context` and typed text therefore went to the read-only status
document; `actions.find` did focus a `TEXTAREA` labelled `Find`. So the find
case needs a different focus mechanism and re-verification, not a broader
binding change. The probe also confirmed `import -window` does not paint the
editor/Monaco layer here (sidebar, status bar and QuickPick overlays do paint),
so status-text screenshots from `import` are not valid evidence. `F5` was not
re-run in those two failures; it remains a separate diagnostic.

Deferred until the user confirms they are away: finish/re-run the isolated real
client scenario, verify the find-widget focus path, capture the screenshots and
matrix, record them here, and only then consider R-VS-5. Automated evidence
that is already valid: the unit-level when-clause matrix for canonical and
mirrored bindings (foreign Quick Input, suggest, find and rename exclusions plus
`editorTextFocus`) and the two real foreign Quick Input cases above (zero Magit
dispatch). Those do not complete the acceptance gate.
