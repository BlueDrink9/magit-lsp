# VSCode client notes

The extension is a thin client: it forwards keys and commands, applies
`view/render`, and renders menus as QuickPicks. All Git, layout, keymap and
menu logic stays in the server.

## Protocol observations and gaps

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
`editor.toggleFold` to `magit.invoke` entries with `when: "magit.active"`. The
`Magit: Mirror my fold shortcuts` command shows those entries in a scratch
document and can copy them to the clipboard; it never modifies the user's file
silently.

## Regenerating the static bindings

`scripts/gen-keybindings.mjs` runs
`magit-lsp keymap --format vscode --profile standard` and replaces
`contributes.keybindings`. The output matches
`tests/snapshots/keybindings-standard.json`. Use `--check` to verify without
writing.
