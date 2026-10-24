# VSCode client notes

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
no API refresh substituted for F5. R-VS-3 therefore stays unchecked. R-VS-2
retains its first-open screen gate (the saved screen follows reopen/expansion),
and R-VS-4 retains explicit theme verification. R-VS-5's applicable-widget gate
is satisfied with the unsupported cases above explicitly bounded.

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

The standard defaults are Tab (toggle section) and F5 (refresh).

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
