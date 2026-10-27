# magit-lsp — Roadmap

Canonical roadmap, design, protocol and backlog. Terms are in §1, protocol
shapes in §§3–4, and live tasks state their acceptance gates. Read selectively
from [AGENTS.md](./AGENTS.md); `README.md` owns user-facing documentation.
Do not duplicate detailed design/history in `AGENTS.md`.

**Status note:** design sections describe intended behaviour; §10 records
implementation status and deviations at its stated point in time. Verify
current behaviour in source/tests rather than treating historical notes as
current facts.

**Project status:** early stage. **Backward compatibility is not a goal.**
Rename, delete, and reshape protocol methods, config keys and Lua APIs freely.
Delete the old thing; do not leave shims.

- [x] **Review A03 — Normalize selections against displayed identities:** implemented and verified; acceptance evidence is recorded in `REVIEW_REPORT.md` under A03.
- [x] **Review A25 — Move navigation policy out of Lua:** full verification gate passed; acceptance evidence is recorded in `REVIEW_REPORT.md` under A25.

## Contents

0. Decisions already made
1. Glossary
2. Immediate tasks (do these first)
3. Design decisions (expanded)
4. Protocol reference
5. Default keymaps (profiles)
6. View catalog
7. Phased roadmap
8. Testing strategy
9. Open questions
10. Implementation status and deviations
11. Client reviews
12. Review backlog (priority index in §7)

---

## 0. Decisions already made

These decisions are canonical unless superseded by an explicit later decision
here. General agent working conventions and verification guidance live in
`AGENTS.md`.

| Topic | Decision |
| --- | --- |
| Async runtime | `tokio` |
| Config format | TOML, with `include` of other TOML files (absolute or relative), default location from the OS config dir (XDG on Linux) |
| Neovim default profile | `vim` (evil-style) |
| Non-modal default profile | `standard` (Magit mnemonics, direct keys, no Emacs idioms) |
| Emacs idioms (`C-c C-c`, `M-n`, …) | Not kept. No `emacs` profile is shipped |
| Key notation | W3C `KeyboardEvent.key` names + VSCode-style `ctrl+shift+x` modifiers (see §3.8) |
| Section folding | Server-owned, but triggered by the client's own fold keys wherever possible (§3.9) |
| Unimplemented Magit operations | Shown in menus, greyed out, with their default key and description (Neogit style) (§3.12) |
| jj | May be a semi-hard fork. Do not complicate the Git design or drop Git features for jj's sake |
| Diff/patch | Git computes diffs; we parse its output and build patches for `git apply` (this is what Magit does, §3.13) |
| Rendering | Server re-renders fully (as Magit does) and sends line-range edits, so clients stay stable (§3.3) |
| Tests | Mostly Rust, in-process, asserting buffer text and repo state. Neovim tests are kept but few (§8) |
| Detail visibility | Four levels: section (staged/unstaged) → files → hunks → individually visible hunks; profile bindings and tests must match (§5.1) |
| Menu saved arguments | Defer `menus.save_scope` persistence; remove/defer the setting until semantics are understood |
| Magit parity keys | Verify keys against the Magit manual, not memory |
| Recommended Git version | Git 2.30.8 is the tested recommendation, not a hard minimum; older versions are not blocked but may have feature failures |
| Commit selection | Selecting a commit normally pushes/focuses a server-rendered log in select mode; clients forward selection/confirm/cancel and display its text verbatim. The server resolves structured commit nodes, restores the caller on confirm/cancel, and validates optional free-form revisions. This is not a client picker or choice prompt (§3.7) |
| Managed commit drafts (2026-10-27) | At coordinated commit-message cutover, use a server-managed per-run draft with exact owner/version write-back; save checkpoints never commit, normal user close confirms the saved checkpoint, explicit abort/disconnect cancel, and a bounded server-owned per-repository ring backs history-prev/next (§3.10, Phase 3E) |

---

## 1. Glossary

Use these words exactly. Do not invent synonyms.

| Term | Meaning |
| --- | --- |
| **Client** | An editor-side program (Neovim plugin, VSCode extension, TUI). It forwards input to the server and displays what the server sends. It contains no Git logic and no layout logic |
| **Server** | The Rust `magit-lsp` process. Owns all state, rendering, key handling and Git calls |
| **View** | One screen the server can show: status, log, a commit, a menu, a rebase todo, … Each view is a virtual document with a URI like `magit://status/<repo>` |
| **View kind** | The type of a view (`status`, `log`, `revision`, `menu.commit`, …) |
| **Document** | The text of a view as the client displays it, plus structure metadata |
| **Node** | One element in a view's tree: a section, file, hunk, diff line, commit, ref, menu item, … A node knows which text lines it covers |
| **Node ID** | A stable string identifying a node by *what it is*, not where it is, e.g. `unstaged/src/a.rs/hunk:9f3c1a`. Survives refresh. Never a line number or index |
| **Version** | An integer on every render of a view. Incremented on each change |
| **Ack** | The client's `view/rendered` notification saying "I applied version N" |
| **Cursor context** | `{line, column}` plus, for selections, `{startLine, endLine}` and the editor mode (`normal`/`visual`) sent with every key |
| **Command** | A named server operation with an ID such as `status.stage`. Everything a key can do is a command |
| **Command status** | `Implemented`, `Planned` (shown greyed, does nothing yet) or `Unavailable` (backend cannot do it) |
| **Scope** | A named group of key bindings, e.g. `common`, `status`, `log`, `menu.commit`. Scopes inherit from `common` |
| **Binding** | `key sequence → command ID`, within a scope, for some editor modes |
| **Profile** | A named, shipped bundle of default bindings: `standard`, `vim` |
| **Keymap** | The resolved set of bindings active right now for the focused view |
| **Effect** | An instruction from the server to the client: show a message, open a view, ask a question, copy text, … (§3.5) |
| **PassThrough** | An effect meaning "this key is not ours; do what the editor would normally do for it" |
| **Capture mode** | A view flag. While set, the client must send *every* key pressed in that view to the server and must not run native behaviour for it unless the server answers `PassThrough` |
| **Placement** | A hint for where a view should appear: `main`, `side`, `bottom`, `float` |
| **Prompt** | The server asking the user one question (yes/no, text, pick one, pick many) and waiting for the answer (§3.6) |
| **Menu** | A view of kind `menu.*` that lists switches, options and actions (Magit's "transient") (§3.11) |
| **Select mode** | A view (usually a log) opened to let the user pick commits; confirm returns them to a waiting command (§3.7) |
| **Editor bridge** | The mechanism that lets Git's "open an editor" calls open inside the user's editor (§3.10); current commit edits are real files, managed drafts are planned |
| **Process log** | A view listing every Git invocation with arguments, exit code, duration and output |
| **Text mode** | A view mode with no ordinary Magit key capture: Neovim retains its existing editable buffer behavior; VSCode browses its read-only virtual document with native editor keys and queued refreshes (§3.14) |
| **Semantic token** | LSP's standard way for a server to label ranges of text with a *type* (and modifiers) so the client colours them using the user's theme |
| **Parity** | Having the same operation available as Magit |

---

## 2. Immediate tasks (do these first)

Completed hands-on fixes, retained here for their behavioural contracts.
Remaining evidence gaps and follow-ups are tracked in §§7 and 12; a historical
completion mark is not proof that every original acceptance test exists.

### IMM-1 — Unstaging a file header fails in a repo with no commits

- [x] Unified path unstaging: `git reset -q -- <paths>`, falling back to
  `git rm --cached -q -f --ignore-unmatch -- <paths>`; renames include both paths.
  Unlike `restore --staged`, this works without HEAD, including index ≠ worktree.
  Preserve unborn-repo coverage for file/hunk/line/all stage and unstage and
  discard mutations. Oldest-supported-Git validation and fallback coverage
  remain the P2 minimum-Git task (§12), not an established version guarantee.

### IMM-2 — Visual stage/unstage with non-change lines corrupts the patch

- [x] Pure selection normalisation in `patch.rs`: use tree nodes and inclusive
  `[start,end]`; covering a file header selects the file, otherwise a hunk
  header selects its hunk, otherwise collect only selected `+`/`-` lines.
  Drop empty operations; an empty selection emits info "No changed lines in
  the selection". Cross-file/hunk operations apply in order (one apply per
  file); unstage/discard use reverse apply.
  Preserve original file headers and selected changes, convert unselected `-`
  to context, drop unselected `+`, and let `--recount` fix counts (§3.13).
  Tests cover context-only, mixed/header selections, two hunks/files and staged
  selections; random patches must pass `git apply --cached --check`, apply to
  the expected index and reverse to identity (§8.3).

### IMM-3 — Error messages must be readable

- [x] Git failures produce successful responses with `Effect::Message{level:"error"}`;
  JSON-RPC errors are only for protocol faults (bad params, unknown method,
  internal panic). `errors.rs` maps exit code + stderr to
  `UserError {summary, hint?, detail?, command?, processLogId?}`: a plain-language
  sentence, optional next step, raw stderr, command and log reference.
  Canonical mappings:

  | stderr contains | summary |
  | --- | --- |
  | `could not resolve HEAD` / `unborn` | "This repository has no commits yet." |
  | `corrupt patch` | "Git could not apply that selection. Try selecting whole lines that are changes." |
  | `index.lock` | "Another Git process is running in this repository." + hint to remove stale `.git/index.lock` only if no Git process is running |
  | `not a git repository` | "This folder is not inside a Git repository." |
  | `would be overwritten` | "Your local changes would be overwritten. Commit, stash or discard them first." |
  | `non-fast-forward` / `rejected` | "The remote has commits you do not have. Fetch or pull first." |
  | `Authentication failed` / `Permission denied` | "Git could not authenticate with the remote." |
  | `CONFLICT` | "The operation stopped because of conflicts." + hint to resolve then continue |
  | anything else | "`git <subcommand>` failed." with `detail` = stderr |
  Clients notify summary/hint at the right level and offer detail/process log;
  never show raw tables/JSON. Test each mapped failure's exact effect and one
  Neovim `vim.notify` with no table dump; remaining coverage is in §12.

### IMM-4 — Neovim status buffer must be read-only

- [x] `nofile`, `bufhidden=hide`, no swap/undo, `modifiable=false`,
  `modified=false`; text mode toggles editing (§3.14). The sole render writer
  temporarily enables modification, applies full/range edits, clears modified
  and locks again. Block `:w`/`:write` (`BufWriteCmd` plus the E382
  `CmdlineLeave` guard); unbound edits fail natively with E21.
  R-UI-3 removed `readonly=true` to avoid W10; the one-time read-only hint is
  on attempted write rather than E21. Tests must preserve text and refresh
  after `dd`, `x`, `ciw`, `u`, substitution; bound-key deviations and stronger
  read-only assertions remain in §12.

### IMM-5 — Two separate line stagings appear as one staged hunk; audit the diff/patch layer

- [x] No hunk-merging bug: Git merges gaps ≤ `2 × context` (six lines at U3).
  Regression fixtures change lines 5/35 in a 40-line file (two hunks) and
  changes four lines apart (one). Status must match `git diff --cached -U3`
  exactly. No diff computation in `git.rs`; `patch.rs` only parses/builds
  region patches for `git apply`, using `--unidiff-zero --recount` (§3.13).

### IMM-6 — Node-kind dispatch for stage/unstage/discard

Rule: **stage, unstage, discard, reverse and apply act on the smallest node under
the cursor, or on the selection if one exists.** Implement as one function that
maps `(command, node kind, selection)` to an operation:

| Cursor on | `status.stage` | `status.unstage` | `status.discard` |
| --- | --- | --- | --- |
| Section header "Unstaged changes" | stage all unstaged | n/a (message) | discard all unstaged (confirm) |
| Section header "Staged changes" | n/a | unstage all staged | n/a |
| Untracked section header | stage all untracked | n/a | delete all untracked (confirm) |
| File header | stage whole file | **unstage whole file** | discard file (confirm) |
| Hunk header | stage hunk | unstage hunk | discard hunk (confirm) |
| `+`/`-` line, no selection | stage the **hunk** | unstage the hunk | discard the hunk (confirm) |
| Selection (visual) | per IMM-2 | per IMM-2 | per IMM-2 (confirm) |
| Context line, no selection | stage the hunk | unstage the hunk | discard the hunk (confirm) |
| Anything else | `Message` "Nothing to stage here" | likewise | likewise |

- [x] Stage/unstage resolver and executor in `src/dispatch.rs`; every row
  requires an in-process test. Discard execution/wiring remains R-3.6.

### IMM-7 — Extract the test harness

- [x] Shared repo helpers and `RepoFixture` in `tests/common/mod.rs`, with
  deterministic environment (§8.2). `TestSession` migration is still in §12.

### IMM-8 — Spike: colour verification with `:TOhtml`

- [x] `scripts/color_spike.lua` / `make color-spike`: output, findings and
  colour-test rules are canonical in §8.4.

### IMM-9 — Update `AGENTS.md`

- [x] `AGENTS.md` links here; decisions in §0 and working conventions there.

---

## 3. Design decisions (expanded)

### 3.1 The server owns the document

- One server-owned node tree produces text, tokens, symbols, link ranges and
  `line → node`; never recover structure by parsing rendered text.
- Node IDs derive from identity (path/hunk content, commit hash, ref name,
  menu key), never position; preserve cursor, visibility and selection.
- Every render has a version; clients ack with `view/rendered`. Positional
  requests (symbols, definition, cursor keys) use the acked tree. Actions carry
  their version/Node ID; vanished nodes re-resolve by content hash or return a
  Message, never target the new occupant of an index.
- Server-owned collapsed bodies are neither rendered nor computed until
  expanded (as in Magit's lazy `magit-insert-section-body`).

### 3.2 Sessions, views and the view stack

- One `Session` per repository root per client. It holds a `ViewStack`.
- A `View` has: `id`, `kind`, `uri`, `title`, `mode` (`interactive`/`text`),
  `placement`, `scope`, `capture`, `viewport {cols, rows}` (sent by the client;
  needed to lay out multi-column menus), its node tree and visibility state.
- Trait: `render(&Ctx) -> Tree`, `scope() -> Scope`, `on_refresh()`, optional
  `load_more()` for lazy lists.
- The server gives a `placement` hint; the **client** decides actual windows.

### 3.3 Rendering and incremental updates (how Magit does it)

Magit rebuilds the buffer (potentially slow in large repos), restores section
identity plus line/column offset and scroll, and computes hidden bodies lazily.
We fully re-render server-side, diff old/new text into line-range edits (common
prefix/suffix or a line diff), and return the Node-ID successor cursor. Clients
retain scroll/marks without identity matching. Collapsed bodies stay lazy;
dirty-subtree rendering is deferred until profiling justifies it.
`view/render` carries `{version,baseVersion,edits}` or full `{version,text}`;
an applied-version/base mismatch requires `view/resync`.

### 3.4 Document delivery and standard positional requests

Standard LSP positional requests only make sense if the client buffer equals
the server's text. Hence the ack scheme above. While a view is in text mode,
position-based requests return `null`. Track LSP 3.18
`workspace/textDocumentContent`; adopt it as the transport when clients
support it.

### 3.5 Commands and effects

Every key action is a **command** in a registry:

```
Command { id: "status.stage", scope: "status", doc: "Stage the change at point",
          status: Implemented, run: async fn(&mut Ctx) -> Result<()> }
```

Commands are `async fn`s with a `Ui` handle (`ctx.confirm`, `ctx.prompt_text`,
`ctx.pick`, `ctx.pick_ref`, `ctx.pick_commit`, `ctx.edit_document`). A call
suspends the command until the client answers; this makes multi-step flows
ordinary linear code. Tests supply a `ScriptedUi`.

**Effects** (server → client). All have a `type` field:

| `type` | Fields | Client must |
| --- | --- | --- |
| `message` | `level: info/warn/error`, `text` (one line), `hint?`, `detail?`, `processLogId?` | Show a notification at that level. If `detail`/`processLogId` present, offer a way to open it |
| `openView` | `descriptor` (id, kind, uri, title, placement, mode, scope, capture) | Create a window/buffer/panel per `placement`, show it, attach the LSP client to it |
| `pushView` | same as `openView` | As `openView`, remembering the previous view so `popView` returns to it |
| `popView` | `viewId` | Close that view and focus the previous one |
| `focusView` | `viewId` | Focus it |
| `showDocument` | `uri`, `line?`, `column?`, `preview?` | Open a real file (standard `window/showDocument`) |
| `editDocument` | see §3.10 | Open it for editing in the client's own editor |
| `clipboard` | `text` | Put the text on the system clipboard |
| `passThrough` | `key` | Perform the key's normal editor behaviour as if unbound |
| `progress` | `token`, `title`, `message?`, `percentage?` | Display progress (standard `$/progress`) |
| `quit` | — | Close all magit-lsp views of this session |

Rendering updates are not effects; they arrive as `view/render`.

### 3.6 Prompts (what each kind means)

A **prompt** is one question the server asks the user. The server sends a
`view/prompt` *request* and waits for the response. The response always
contains `cancelled: bool`. If `cancelled` is true the command stops quietly
(no error message).

Common request fields: `promptId` (string, unique), `kind`, `title` (the
question, shown prominently, e.g. `"Delete branch feature/x?"`), `detail?`
(extra lines of explanation).

| `kind` | Question it asks | Extra request fields | Response `value` |
| --- | --- | --- | --- |
| `confirm` | Yes or no | `yesLabel?` (default "Yes"), `noLabel?` (default "No"), `destructive: bool` (client should style it as dangerous and make "No" the default focus), `default: bool` (default `false`) | `true` (yes) or `false` (no) |
| `text` | A free-form single line of text | `placeholder?`, `default?` (prefilled text), `secret: bool`, `error?` (shown when the server re-asks after rejecting the previous value) | the string the user typed (may be empty) |
| `choice` | Pick exactly one from a list | `items: [{id, label, description?, detail?}]` (`id` is what the server gets back; `label` is shown), `allowCustom: bool` (the user may type something not in the list), `default?` (an item `id`) | `{id: "<item id>"}` or, if custom, `{custom: "<typed text>"}` |
| `multiChoice` | Pick zero or more from a list | `items` as above, `preselected: [id]`, `min?`, `max?` | `{ids: ["…"]}` |

**`secret` flag.** When `true`, the typed characters must **not** be echoed
(masked or hidden), the value must not be written to any history, log or
completion list, and the server must not log it. Used for Git credential and
SSH passphrase requests arriving through the askpass helper (§3.10).

**Fuzzy matching** is a client concern, not part of the protocol. Clients may
filter generic `choice` prompt `items` however their picker does. Commit
selection is not a generic choice prompt and does not use client fuzzy matching.

**Commit picking is not a prompt kind.** The server implements
`ctx.pick_commit` by sending an `openView`/`pushView` for a log view in *select
mode* (§3.7). The client sees an ordinary view. When the user confirms, the
server resolves the waiting command.

**Mapping to editors** (default implementations; clients may use richer UI):

| Kind | Neovim | VSCode |
| --- | --- | --- |
| `confirm` | `vim.ui.select({yesLabel, noLabel}, {prompt = title})`. Result maps to `true`/`false`; closing the picker = `cancelled` | `showWarningMessage(title, {modal: true, detail}, yesLabel)`; the returned label = yes, `undefined` = `cancelled` |
| `text`, not secret | `vim.ui.input({prompt = title, default = default})`; `nil` result = `cancelled` | `showInputBox({title, value: default, placeHolder})`; `undefined` = `cancelled` |
| `text`, `secret` | `vim.fn.inputsecret(title)` (`vim.ui.input` cannot mask) | `showInputBox({..., password: true})` |
| `choice` | `vim.ui.select(items, {prompt = title, format_item = label})`. If `allowCustom`, prepend an item "Enter custom value…" that then runs a `text` prompt | `showQuickPick(items, {title})`. If `allowCustom`, use `createQuickPick` and add an item for the typed text |
| `multiChoice` | Loop of `vim.ui.select` that toggles items until the user picks a "Done" item (or a scratch-buffer checklist) | `showQuickPick(items, {canPickMany: true})` |

These map onto the user's own picker plugins automatically in Neovim because
`vim.ui.select`/`vim.ui.input` are overridable.

### 3.7 Select mode (picking commits and other things)

Used when a command needs the user to pick a commit (rebase onto, cherry-pick,
fixup target, reset to, …). This server-owned log select mode is the normal
commit-selection flow; it is not a client fuzzy picker, QuickPick, or `choice`
prompt. The client displays server-rendered text verbatim and forwards input.

- Server pushes and focuses a log view with
  `select: {purpose: "rebase onto", multi: false, prompt: "Rebase onto which commit?", allowFreeText: true}`;
  the visible purpose/prompt is rendered by the server. Native editor navigation
  and search remain available (search keys must not be swallowed by key capture).
- Extra bindings active (scope `log.select`): `common.confirm` returns the
  commit under the cursor (or all marked commits if `multi`),
  `common.cancel` returns without mutation, and `select.toggle-mark` is
  multi-only. Selection identity comes from structured commit nodes, never
  rendered-text parsing or row indices. Confirm returns the selected identity
  to the waiting operation and restores the caller view; cancel restores the
  caller without mutation.
- `allowFreeText` is an optional escape hatch, not the default flow: a key
  (`select.enter-rev`) opens a `text` prompt so the user can type `HEAD~3`, a
  branch, or a tag; the server validates it with `git rev-parse --verify`.
  No client Git, filtering, or commit-selection implementation is required.
- The same pattern selects a branch (refs view), a stash, or files.

**Proposed Enter/o scheme (not approved or implemented):** In a plain log,
`common.confirm`/Enter would open the structured commit under the cursor. In a
commit picker, Enter would remain the primary action and select/confirm the
commit. A proposed `o` action in commit-bearing log, select, and rebase views
would open a read-only commit inspection view without selecting a commit or
mutating the pending operation. `q` would return to the originating picker or
rebase view with its cursor, marks, and pending operation intact. This proposal
does not apply to status, where `o` opens submodules. Use terminal-portable
keys; do not depend on `ctrl+Enter`.

### 3.8 Key notation

No formal standard exists for *textual* shortcut strings. The W3C UI Events
spec standardises the key *values* (`KeyboardEvent.key`: `"Enter"`, `"Escape"`,
`"ArrowUp"`, …) and VSCode/Electron have de-facto readable string forms. We
combine them:

- **Format:** `[modifier+]*key`. Modifiers lowercase, in this order when
  several: `ctrl`, `alt`, `shift`, `meta` (`meta` = Cmd on macOS, Windows/Super
  elsewhere). There is no `mod`/`CommandOrControl` alias: platform differences
  are expressed explicitly in config.
- **Key names:** W3C values: `Enter`, `Escape`, `Tab`, `Backspace`, `Delete`,
  `Insert`, `Home`, `End`, `PageUp`, `PageDown`, `ArrowUp`, `ArrowDown`,
  `ArrowLeft`, `ArrowRight`, `F1`…`F12`. Two spelled-out names for characters
  that are awkward in text: `Space` and `Plus` (the `+` key).
- **Printable characters** are written as the character produced: `s`, `S`,
  `?`, `%`, `$`, `!`, `-`, `=`. **`shift` is not used with printable
  characters** (`S` means shift+s). `shift` is used only with named keys
  (`shift+Tab`).
- **Chords** (several keys in sequence) are separated by a space:
  `g g`, `ctrl+k ctrl+s`, `z a`.
- **Parsing rule:** split on whitespace into steps; split each step on `+`
  (a step that is exactly `Plus` is the plus character).
- **Examples:** `s`, `S`, `Enter`, `ctrl+Enter`, `Escape`, `Delete`, `shift+Tab`,
  `alt+ArrowDown`, `ctrl+c`, `] ]`, `g r`.

**Translators** (one small table per client; unit-tested both directions):

| Canonical | Neovim | VSCode `keybindings.json` |
| --- | --- | --- |
| `s`, `S`, `?` | `s`, `S`, `?` | `s`, `shift+s`, `shift+/` (layout dependent, see §3.9) |
| `Enter` | `<CR>` | `enter` |
| `Escape` | `<Esc>` | `escape` |
| `Tab` / `shift+Tab` | `<Tab>` / `<S-Tab>` | `tab` / `shift+tab` |
| `Delete` / `Backspace` | `<Del>` / `<BS>` | `delete` / `backspace` |
| `ArrowUp` | `<Up>` | `up` |
| `ctrl+c` | `<C-c>` | `ctrl+c` |
| `alt+ArrowDown` | `<A-Down>` | `alt+down` |
| `ctrl+Enter` | `<C-CR>` | `ctrl+enter` |
| `g g` | `gg` | `g g` |

**Terminal limits:** many terminals cannot distinguish `ctrl+Enter`,
`ctrl+shift+<letter>`, or `shift+Enter` from their plain forms. Clients report
`keyModifiers: "full" | "basic"` in `initialize`. Profiles (§5) avoid these
combinations in the `vim` profile; `standard` assumes a GUI editor.

### 3.9 Keybinding architecture

Goal: the server knows every key; clients only forward; users can customise
per view/menu while sharing analogous actions across views.

**Semantic commands and scopes**
- Bindings target command IDs. IDs are namespaced: `common.*` (shared by all
  views: `common.confirm`, `common.cancel`, `common.quit`, `common.refresh`,
  `common.toggle-section`, `common.toggle-text-mode`, …), view-specific
  (`status.stage`, `log.show-commit`, `rebase.move-up`), and menu openers
  (`open.commit`, `open.branch`, …).
- Scopes inherit: view-kind scope → `common`. A view-kind scope may override a
  `common.*` binding for itself. Resolution order: user config for the scope →
  profile for the scope → user config for `common` → profile for `common`.
- Rebinding `common.confirm` changes confirm in the log selector, the rebase
  todo and the commit message at once (unless a scope overrides it).
- **Editor-mode conditions:** a binding may be restricted to `normal`,
  `visual`, or both (default both). Visual-mode bindings receive the selection.
- The set of keys per scope never depends on repo state (only *enabled/greyed*
  state and labels do). This is what makes the full key space enumerable
  offline.

**Wire shapes** (see §4): `view/keymap` (server → client) carries
`{viewId, scope, version, capture, bindings: [{command, keys, modes, label, doc, group, status}]}`
where `keys` is a list of alternatives, each a canonical key string.

**Client delivery modes** (declared by the client in `initialize`):

1. **`dynamic`** (Neovim, TUI): the client applies each `view/keymap` as the
   complete current keymap for that buffer, atomically.
   Neovim tracks `applied[bufnr] = {lhs…}`: delete exactly those maps, install
   new buffer-local `vim.keymap.set` maps (`nowait=true`), store the new set in
   one synchronous function (no interleaved keys). Maps send canonical
   `view/key`; capture mode additionally forwards every other printable key.
2. **`static`** (VSCode): generate `contributes.keybindings` via
   `magit-lsp keymap --format vscode`; runtime `when` contexts are
   `magit.active/scope/textMode/capture`, refreshed with view state and editor
   focus (static clients use descriptors/renders, not dynamic keymap pushes).
   Native `keybindings.json` customises `magit.invoke` with
   `{"command":"status.stage"}` → `view/invoke`, or `magit.key` with
   `{"key":"s"}` → `view/key`. Editor bindings require the focus/widget guards
   below; do not infer safe capture from the document alone.
3. **`forwardAll`** (fallback): the client sends every key and the server
   answers `passThrough` for anything unbound.

**VSCode capture limits:**
- Gate editor bindings on `magit.active`, editor text focus and exclusion of
  foreign widgets (§12 R-VS-5), not merely the active document.
- Static bindings enumerate specific keys; layout affects printable keys
  (`keyboard.dispatch: keyCode` uses physical codes). IME/dead keys bypass them.
- `type` is window-global, single-owner and incompatible with VSCodeVim;
  routing all typing through `default:type` can be slow. Never own it
  permanently: register only for a focused capture view, dispose on blur/close,
  forward non-Magit typing, and fall back to static/QuickPick if already owned.
- Never shadow workbench shortcuts (`ctrl+p`, `ctrl+shift+p`, `ctrl+w`,
  `ctrl+tab`, F1, `alt+tab`, …). Generate only server-bound keys; deny-list
  tests require explicit reasoned exceptions.
- Standard `ctrl+c` cancel returns `passThrough` for a non-empty text selection
  when there is no menu, prompt or select mode to cancel, preserving copying.

**Fold keys: reuse the client's own fold keys** (so users who customised fold
keys keep them). Implementation per client:
- *Neovim:*
  1. Set `foldmethod=manual`, `nofoldenable` in magit-lsp buffers so native
     folding never interferes.
  2. Buffer-locally map the built-in fold commands to server commands:

     | Native | Command |
     | --- | --- |
     | `za`, `zA` | `common.toggle-section` (`zA`: recursive) |
     | `zo`, `zO` | `common.show-section` (`zO`: recursive) |
     | `zc`, `zC` | `common.hide-section` (`zC`: recursive) |
     | `zR` | `common.show-all` |
     | `zM` | `common.hide-all` |
     | `zr` / `zm` | `common.more-detail` / `common.less-detail` (visibility level ±1) |
     | `zj` / `zk` | `common.next-section` / `common.prev-section` |
  3. At view creation and on `BufEnter`, scan the user's mappings
     (`vim.api.nvim_get_keymap(mode)` and the buffer's) for any whose `rhs` is
     one of the fold commands above (also `:normal! za`, `<Cmd>normal! za<CR>`).
     Re-map each such `lhs` buffer-locally to the matching server command.
     This catches `nnoremap <Tab> za`, which a plain remap of `za` would miss
     (non-recursive mappings ignore other mappings).
  4. Known limit: Lua-function mappings cannot be inspected. Provide
     `fold_keys = { ["<Tab>"] = "common.toggle-section" }` in the Neovim client
     config for those.
- *VSCode:* an extension cannot intercept `editor.toggleFold` (user keybindings
  outrank extension keybindings, and the native command acts on VSCode's own
  folding model, which these views do not use). So:
  1. Ship default bindings for our commands on VSCode's default fold keys
     (`ctrl+shift+[`, `ctrl+shift+]`, `ctrl+k ctrl+l`, `ctrl+k ctrl+0`,
     `ctrl+k ctrl+j`, and macOS variants) with `when: magit.active`.
  2. At activation, best-effort read the user's `keybindings.json` (JSONC) and
     list entries whose command is `editor.fold*`, `editor.unfold*` or
     `editor.toggleFold`. If any, offer a command **"Magit: Mirror my fold
     shortcuts"** that shows (or, with consent, appends) equivalent entries
     targeting `magit.invoke` with `when: magit.active`.

**Server-side hard rules**
- The server validates the merged keymap and reports conflicts when distinct
  commands bind the same key at the same precedence in one scope chain. A more
  specific scope declaration intentionally shadows a broader declaration for
  that scope (including when the commands differ); equal-precedence ambiguity
  remains an error. Conflicts are reported as a `Message` and in
  `magit-lsp keymap --check`.
- `magit-lsp keymap --format json|vscode|neovim|markdown [--profile P]`
  emits the full static manifest (all scopes × bindings). CI snapshots it.

### 3.10 Editor bridge (open Git's editor inside the user's editor)

Git opens an editor for: commit messages (incl. `--amend`, reword), merge
messages, annotated tag messages, interactive rebase todo lists, `git commit
--fixup=reword:`, etc.

**Mechanism**
1. When the server spawns Git it sets: `GIT_EDITOR`, `GIT_SEQUENCE_EDITOR` (and
   `EDITOR`, `VISUAL` if unset) to `"<path to magit-lsp binary> --editor-helper"`,
   plus `MAGIT_LSP_SOCKET=<unix socket or named pipe path>` and
   `MAGIT_LSP_EDIT_ID=<unique id>`.
2. Git runs the helper with the file path as argument. The helper connects to
   the socket, sends `{editId, path, kind}` and **blocks** waiting for a
   reply (`ok` → exit 0, `abort` → exit 1; Git aborts the operation on 1).
3. The server maps the edit to the session that spawned Git and sends an
   `editDocument` effect to that session's client. `title`, `mode` and `scope`
   are server-owned metadata; `scope` names the command/keymap scope used by
   the edit:

   ```
   { type: "editDocument", editId, path, kind: "commit-message"|"rebase-todo"|"merge-message"|"tag-message"|"other",
     title: string, mode: "realFile", scope: string }
   ```
   The current helper emits `commit-message` only (`title: "Commit message"`,
   `scope: "commit-message"`). The bridge opens the Git-owned path as a real
   file. `merge-message` and `tag-message`, when emitted, use the same
   `commit-message` scope; other kinds use the shared `common` scope.
4. **The client opens it in its own editor, in the same window/instance**:
   - Neovim: open the file in a window per client config (split, tab, or float;
     default: bottom split), set `filetype` (`gitcommit`, `gitrebase`), attach
     the LSP client to the buffer, show the `commit-message` keymap.
   - VSCode: `showTextDocument(Uri.file(path))` in the active window; set
     context key `magit.editing`; restrict the LSP documentSelector to these
     filenames so diagnostics/completion apply.
   - TUI: may spawn `$EDITOR` itself (detail deferred).
5. **Current realFile implementation:** `common.confirm` saves the file and
   invokes confirm; the server replies `ok`. Cancel/close aborts the helper.
   This is current behavior, not the chosen commit-message target; historical
   realFile close acceptance claims remain historical.
   Cancel currently aborts immediately; a discard-confirm prompt for a non-empty
   message is not implemented.
6. The implemented bridge edits real Git-owned files (`mode: "realFile"`). It
   currently emits only commit messages. Merge/tag messages and `rebase-todo`
   are not emitted yet; in particular, virtual documents and a structured
   rebase-todo editor are not implemented. Do not send `mode: "virtual"` until
   that editor and its write-back path exist.
7. **Current behavior/limit:** client disconnect or helper socket closure aborts
   the pending edit. This does not establish cancellation of a blocked hook or
   inherited pipe; current progress can hang and this is not verified fixed.

**Chosen planned contract — managed commit drafts (coordinated cutover, no
compatibility mode).** Replace only real-file commit-message editing with a
mutable server-managed per-run draft. Git still launches helper, supplies the
template/path, runs hooks, performs cleanup/comment stripping, and owns commit;
never substitute `git commit -F`. This is user-editable document content, not a
writable server-rendered Tree/status/log or text-mode exception. Use an opaque
URI such as `magit-commit://<opaque-run>/message` and exactly one sync path:
standard LSP `didOpen`/`didChange`/`didClose`; no competing custom fallback.
R-EDIT-1 specifies sync ordering/versioning and bounds the current live draft
plus last saved checkpoint. Ordinary didChange syncs live text; didSave (or a
versioned save-checkpoint acknowledgement) marks saved without another text
channel. Neovim uses native acwrite/scratch; VSCode a writable provider, both
thin. Save/autosave only checkpoints, never confirms/commits. Explicit
`common.confirm` is a barrier: await applied sync, then send exact draft version
bound to run/owner and completion checkpoint; never infer it from first status.
Validate owner, run, version, helper target and canonical expected Git path/kind;
write exact draft to that validated path before helper `ok`. Failures retain
draft/helper pending. Git owns cleanup and hook ordering.

**R-EDIT-1 interface decisions (design only; no wire change until cutover).**
Use one standard LSP full-sync document: `didOpen` version 1 carries the initial
Git template; each `didChange` carries complete text and a strictly greater
integer version. Accept only the authenticated connection/session owning the
run, matching opaque `runId` and URI; reject stale, duplicate, wrong-owner and
terminal-run updates without changing state. Versions may skip. Ordered LSP
delivery plus the monotone check arbitrates arrival. Save/confirm names its exact
version and waits for that version to apply; checkpoint acknowledgement follows
copying that live version as the saved snapshot. `didClose` only detaches and is
never close intent. Bound live UTF-8 text and saved checkpoint independently to
1 MiB; reject oversized updates atomically, retain last accepted state, and
surface recoverable error. Never truncate. Confirm/normal close writes exact
checkpoint, not later unacknowledged changes.

Proposed request/ack order: client sends `draft/save {runId, uri, version,
requestId}` after native save flush; server replies `draft/checkpointed {runId,
uri, version, requestId}` only after matching full sync is applied and snapshot
copied. Finish is `draft/finish {runId, uri, version, requestId,
reason:"confirm"|"normal-close"}`; normal-close carries the checkpoint selected
by Save/Discard. Server validates owner/run/version, writes checkpoint, then
sends `draft/finished {runId, uri, version, requestId}` after helper success.
Errors correlate by requestId and leave run pending. These are proposed project
methods, alongside standard LSP notifications, not a second text channel.
Neovim BufUnload/BufDelete and VSCode document-close alone cannot distinguish
user close from detach/window shutdown. Adapters must track explicit close intent
before teardown; ambiguity cancels/recovery and never confirms. Validate this
native API feasibility in R-EDIT-4/5; LSP didClose is never intent.

Validate helper kind `commit-message`, one-shot token, run, repository, owner,
and canonical expected Git `COMMIT_EDITMSG` target (actual gitdir for
linked-worktrees); reject symlink/non-regular target and any other path. These
checks do not prevent same-user inherited-environment spoofing by nested hooks.
Lifecycle arbitration is serialized by the sole Child owner: active editor,
finalizing, terminal. Abort/progress cancel/EOF/disconnect/shutdown cancel every
nonterminal phase. Native owner/run-scoped user-close intent is distinct from
`didClose`, focus loss, reattach or editor shutdown. Dirty native close Save
confirms the acknowledged checkpoint; Discard confirms prior checkpoint
(initially template); Cancel leaves buffer/run open. Explicit confirm flushes,
awaits applied version, checkpoints, validates/writes exact snapshot, then helper
success. Child owner arbitrates finish versus cancel: pre-completion cancellation
aborts; observed completed Git exit reports actual result. Terminal cleanup is
idempotent and late responses rejected. Failed write/finish keeps helper/draft
pending and reopens/recovery-copies if buffer closed. Drafts are in-memory and
do not survive server crash; native recovery is best effort.

R-EDIT-8 ring limits: default 50 entries and 1 MiB aggregate UTF-8 payload per
canonical repository, in memory for server lifetime. Reject retention of an
entry larger than the cap with a visible warning; never truncate or silently
drop it. Otherwise evict oldest until both limits hold; exact consecutive dedup.
Existing eligibility/recall rules above remain canonical.

**Process feasibility gate (proposed R-EDIT-2 mechanism).** One async owner task
owns `Child`, cancel receiver, exit observation, final wait/reap, and reader
completion channels; handlers only enqueue idempotent cancellation. Start Git in
a new session/process group before exec (child-side spawn setup, not racy
post-spawn `setpgid`). No controlling tty: piped stdout/stderr, null stdin unless
the invocation requires otherwise. Concurrent readers retain at most 4 MiB per
stream, continue draining/discarding after cap, and flag truncation. Reader drain
deadline is 2 seconds after leader exit/terminal cancellation. When the owner
dequeues cancel, it first observes leader status with
`waitid(P_PID, pid, WEXITED|WNOHANG|WNOWAIT)`. If already exited, exit wins and
no signal is sent. Otherwise cancellation linearizes: abort a connected helper,
send TERM to the group, and send CONT to the group so stopped members can
process termination. Retain the unreaped leader for the full 2-second TERM
grace, then send exactly one group KILL even if the leader exited during that
grace. Reap only after KILL, using bounded observation; if KILL is not reaped within a further 2 seconds,
transfer the still-owned Child handle to a process-lifetime reaper task that only
waits/reaps; report cancellation incomplete and retain ownership until reap.
The retained unreaped leader pins PID/PGID identity: no other waiter, `try_wait`,
`waitpid(-1)`, or SIGCHLD auto-reap disposition is permitted. Never signal after
final reap. Exit observation is distinct from pipe EOF. On normal leader exit,
never signal to force inherited-pipe EOF; allow the 2-second drain deadline,
report incomplete drain, finalize.
Readers must support cancellation/parent-pipe close, bounded data and deadline,
not detached indefinitely-blocked threads. Crash cannot run this reaper or
guarantee child cleanup. Nested hooks share group; `setsid` escapees cannot be
contained. Never broad-kill, delete locks, or kill normal-exit daemons.

Portability gate: `setsid` and `waitid(...WNOWAIT)` are Unix interfaces with
platform-specific availability/semantics; Linux implementation and real-Git
tests are required before Linux R-EDIT-2 acceptance and R-EDIT-6 client cutover.
macOS group/WNOWAIT/reap behavior remains unverified and belongs to the separate,
later R-EDIT-9 CI/platform acceptance ticket, not a Linux acceptance dependency.
Retain the full-grace fallback on macOS; it is not validation. Investigate kqueue
exit observation only if required, not as an implemented alternative. Rust
`Child::try_wait` reaps on observed exit, so cannot precede a possible group
signal; use non-reaping waitid observation plus sole final reap or gate platform.
Windows lacks this Unix group/waitid scheme; named-pipe ACL and suspended
creation/atomic Job assignment require independent R-EDIT-7 feasibility and
validation. Numeric limits are proposed defaults, not current behavior.
Minimum adversarial acceptance for R-EDIT-2/A11: leader exits on TERM while a
TERM-resistant descendant remains, and the group still receives KILL after the
full grace; stopped member receives CONT then terminates; exit-before-cancel
produces no signal; normal-exit daemon survives pipe-drain timeout; trace proves
initial WNOWAIT observation → cancel linearization → TERM/CONT → full grace →
KILL → final reap, with no signal after reap. These are required tests, not
claims of platform execution.

Explicit abort, progress cancel, client disconnect/EOF/shutdown always cancel
the run at any phase; they are not normal buffer close. Normal user close of the
commit buffer completes the helper as a successful native COMMIT_EDITMSG editor
exit. Explicit `common.confirm` saves/syncs as a barrier then finishes;
save/autosave alone only checkpoints. For unsaved native close, honor Save /
Discard / Cancel: Save closes/confirms the acknowledged saved checkpoint;
Discard closes/confirms the last saved checkpoint (initially Git's template),
not latest unsaved didChange; Cancel keeps buffer/run open. Empty/comment-only
saved text is Git-owned failure, not explicit abort. Terminal server cleanup is
guarded against recursive confirm/cancel; focus change, language reattach,
transport didClose alone, or editor shutdown is not user-close intent. Bind
normal-close intent to owner/run. If finish/version/writeback fails, retain the
pending helper and recover/reopen the draft if its native buffer closed.
Terminal edit-close invalidates UI idempotently. Reject late responses; state draft
durability/crash limits: in-memory drafts do not survive server crash, and native
recovery copies are best-effort, not guaranteed durable recovery. Opaque run ID/one-shot helper token scoped to
owner/repository; no secret logging. A token alone cannot prevent inherited-env
hook spoofing (same-user/nested hook limitation). Preserve askpass protection,
A29 readonly VSCode status and Neovim text-mode semantics; do not implement other
edit kinds incidentally. Phase 3E R-EDIT-8 owns the bounded server-side message
ring; R-4.2 remains staged diff/trailers/lint owner and depends on this phase.
The ring is in-memory per canonical repository for server lifetime (default
limit 50), retaining successful nonempty submitted messages and confirmed
messages even when Git fails, using submitted text without stripping template
comments. Exclude template-only entries, per-keystroke/autosave drafts,
askpass/secrets, and message-body logs. Exact consecutive deduplication and a
UTF-8 byte bound prevent unbounded growth. Existing history-prev/history-next
remain server commands; clients display editable replacements only after
owner/run/version validation. First previous recall preserves the current
unsent draft for return at newest; navigation clamps at ends (no wrap), changes
draft only, and never saves, finishes, commits or implicitly confirms. Ring
writeback failures and canceled-draft recovery are separate. Do not populate
from Git log in this ticket.

Run lifecycle is hooks → editor waiting → finalizing → terminal; exactly one
completion/progress end/edit/config release. Cancellation differs from Git
failure and empty-message. Handler nonblocking-sends intent to sole Child owner;
graceful helper abort first when connected, then bounded TERM/KILL escalation;
late helper aborts immediately. One owner
arbitrates cancel/reap, never signals after reap/identity release; exit status is
separate from pipe EOF. Avoid try_wait/reap then group-kill races. Unix owned
group/session, documented controlling-tty choice, bounded TERM then KILL (INT
optional only), bounded bytes/output truncation error and finite drains on
normal exit/cancel, with pipe deadlines and no indefinitely blocked reader threads.
Do not kill normal-exit daemons to drain; escaped setsid
children are not contained, bound escaped-pipe cleanup; unreapable child goes to
owned reaper honestly. Never delete locks or broad-kill process names. Bounded
readonly lock observation uses the actual linked-worktree gitdir and common-dir
where applicable and is advisory only; warn forced cancellation may leave
locks. Exclude probes from mutation cancellation/optional writes. BUG-READ-1
global Git locking stays deferred. Current bridge is Unix-only: Linux real
validation is required for Linux acceptance; macOS group/reaping validation is
deferred to R-EDIT-9 without blocking Linux cutover or claiming macOS support
validated. Windows named pipe,
restrictive ACL and atomic suspended Job assignment is a separate feasibility
gate, not a claimed guarantee.

**Askpass:** `GIT_ASKPASS` and `SSH_ASKPASS` point at the same helper (with
`--askpass`). The server turns the request into a `text` prompt with
`secret: true` (or `confirm` for host-key questions). Never log the answer.

### 3.11 Menus (transients) and how they appear in each editor

A **menu** is a view of kind `menu.<name>`. The server renders it as text; the
client shows it. No client contains menu logic.

**Content model (server-side)**
```
Menu   { id, title, groups: [Group], savedArgs }
Group  { title, columns?: number, items: [Item] }
Item   = Switch { key: "-a", flag: "--all", label, on: bool, status }
       | Option { key: "=A", flag: "--author=", label, value?: string, reader, status }
       | Action { key: "c",  command: "commit.create", label, status }
status = Implemented | Planned | Unavailable{reason}
reader = text | choice | path | ref | commit | date | number
```
- **Switch** = on/off flag. Key is `-` + letter (two key presses).
- **Option** = flag with a value. Key is `=` + letter. Pressing it triggers a
  prompt (kind chosen by `reader`: `text`→`text`, `choice`/`ref`→`choice`,
  `commit`→select mode, `path`→`text` with completion, etc.). Pressing it again
  while set clears it.
- **Action** = runs a command using the current switch/option values. Single key.
- An **argument preview line** shows the resulting command, e.g.
  `git commit --amend --no-verify`.
 - Saved arguments: `menu.save-args` stores the current switches/options as the
   default for that menu; persistence scope is deferred until semantics are
   understood. Remove/defer `menus.save_scope` meanwhile. `menu.reset-args`
   restores defaults.

**Text rendering (what the server sends)**
```
 Commit                                         <- token menu.title
 Arguments                                      <- menu.group
  -a  Stage all modified and deleted files (--all)   [ ]   <- menu.key, menu.label, menu.flag, menu.switch.off
  -e  Allow empty commit (--allow-empty)             [x]   <- menu.switch.on
  =A  Override the author (--author=)                       <- menu.option, value in menu.value
 Create             Edit HEAD            Edit                       <- groups laid out in columns using viewport.cols
  c  Commit          e  Extend            f  Fixup          <- Planned items use menu.planned (greyed)
```
Layout columns are decided by the server from `viewport.cols`. The client tells
the server its size on open and on resize (`view/resize`).

**Highlighting:** semantic tokens (§3.15). Token types used: `menu.title`,
`menu.group`, `menu.key`, `menu.label`, `menu.flag`, `menu.switch.on`,
`menu.switch.off`, `menu.option`, `menu.value`, `menu.planned`,
`menu.disabled`, `menu.pending`, `menu.preview`. Each maps to a standard theme
group (§3.15).

**Keys inside a menu:** the view has `capture: true`. The client forwards every
key; the server keeps the typed steps as `pending` (`-`, `=`, or a chord prefix),
dims items that cannot complete them, and answers `passThrough`-free (unbound
keys are swallowed with a one-line hint while `pending` is kept). `Backspace`
pops one step. `Escape`, `ctrl+c` and `q` close the menu. `view/render` and
`view/keymap` carry `pending`; `view/keymap` also carries `liveKeys`, so the
client needs no chord logic.

Menus are not LSP hovers (position-anchored Markdown without window placement).
`showDocument` cannot position windows either. The server hints `bottom` or
optional `float`; clients use native windowing:

| Client | `placement: bottom` implementation |
| --- | --- |
| Neovim, default | Floating window: `nvim_open_win(buf, true, {relative="editor", row = vim.o.lines - height - vim.o.cmdheight - 1, col = 0, width = vim.o.columns, height = height, style = "minimal", border = "none"})`. `height` = number of rendered lines (max e.g. 40% of screen) |
| Neovim, alternative (config `menu.style = "split"`) | `botright {height}split` with a scratch buffer |
| VSCode, default | **QuickPick** (like the VSpaceCode which-key extension does for key menus; verify when building): `createQuickPick()` shows items with keys as labels; typed characters arrive in `onDidChangeValue` and are forwarded to the server as keys; the input box is cleared after each key. Uses the structured menu model (below), not the text |
| VSCode, text mode (config) | A read-only virtual document opened in a new editor group below, highlighted with semantic tokens, keys captured as in §3.9 |
| TUI | Bottom pane drawn by the client from the text and tokens |

**Both representations are sent.** `view/render` for a menu view includes the
text *and* the structured `menu` model, so a client can choose: text + tokens
(consistent look everywhere) or native widgets (QuickPick). Text is the
default and the reference; native widgets are an optimisation.

**Menu stack:** opening a menu from a menu (e.g. a "Configure…" action) pushes
a new view; `popView` returns.

### 3.12 Planned (greyed-out) features in menus

Following Neogit: every Magit operation that we have not built yet is still
listed in its menu with its default key and description, rendered in the
`menu.planned` style (greyed). This is how the roadmap is visible in the UI.

- The command registry has `status: Implemented | Planned{roadmapId} | Unavailable{reason}`.
- **Pressing a planned item** does nothing except show
  `Message{info, "Not implemented yet: Rebase onto upstream (roadmap R-7.3)"}`.
- **Unavailable** is different: the item is hidden or greyed with a reason
  because the backend cannot do it (not for Git today).
- Every Magit dispatch menu is created with its **complete** action list from
  the Magit manual (sections Committing, Branching, Fetching, Pulling, Pushing,
  Merging, Rebasing, Stashing, Tagging, Resetting, Cherry Picking, Reverting,
  Bisecting, Remotes, Submodules, Worktrees, Ignoring, Logging, Diffing,
  Applying Patches, Blaming).
- `cargo xtask parity` prints a table of Magit command → our command ID →
  status; `README.md`'s parity table is generated from it.
- A test asserts: every `Planned` command shows the message and changes no
  repo state; every parity-list key exists in some menu.
- Order of work: menu infrastructure first (Phase 2), then populate all menus
  with `Planned` entries, then implement them phase by phase.

### 3.13 Diff and patch layer (what Magit does; what we keep)

Findings (Magit `magit-apply.el`): Magit runs `git diff` and parses its text.
To stage/unstage/discard/reverse it builds a patch consisting of the **file
header** plus the hunk (or a region of the hunk), adjusts the hunk's new-start,
and pipes it to `git apply` (`--cached`, `--reverse`). Whole files use
`git add`, `git reset`, `git rm --cached`. It has no diff algorithm of its own.

What we keep (and nothing more):
1. **Parse** `git diff` / `git diff --cached` output into `File → Hunk → Line`,
   keeping the raw header text and raw hunk text.
2. **Build patches** for whole file, whole hunk, and selected lines, per IMM-2.
3. **Apply** via `git apply --cached [--reverse]` (index) or
   `git apply [--reverse]` (worktree discard).
4. **Whole-file operations** via `git add`/`git reset`/`git update-index`.

Never compute diffs ourselves. Never merge/split hunks ourselves.

`git apply --recount` tells Git to ignore the counts in hunk headers and infer
them. Try it to replace the custom renumbering in `patch.rs`; keep the
proptests as the safety net; keep the old algorithm only if the proptests show
`--recount` is insufficient, with a comment explaining why.

Diff context size is a setting (`diff.context`, default 3) passed as `-U<n>` to
*both* the display diff and any patch-building diff so line numbers agree.

### 3.14 Text mode

`common.toggle-text-mode` (bound in every scope, including text mode itself).
- **Enter:** server sets `mode = text`; sends a `view/keymap` containing only
  the toggle binding to dynamic clients. The server stops answering
  position-based requests for the view and **queues** refreshes (does not
  render them). Static clients receive a reliable mode/context notification
  for entry through invoke, key, menu or setMode.
- **Neovim:** preserve its existing editable text behavior: make the buffer
  modifiable, restore undo (`undolevels` back to the user's value), and stop
  forwarding keys. **Edits** are never parsed or synced to the server model.
- **VSCode:** retain the read-only virtual document and use native browsing
  (navigation, search, selection and copy). Disable all ordinary Magit
  capture/shortcut overrides, including mirrored fold-key overrides, while in
  text mode; retain the explicit return toggle/command. The document remains
  read-only.
- **Exit:** the server re-renders the current tree (with queued refreshes
  applied) as a full replace. Neovim replaces the buffer, resets
  `modified=false` and sets `undolevels=-1` before replacing so the user
  cannot undo into stale content; VSCode refreshes its read-only virtual
  document and restores ordinary Magit bindings.
- Complementary first-class commands avoid needing text mode for common
  copies: `common.copy-value` (hash for a commit, path for a file, diff text
  for a hunk/selection) returning a `clipboard` effect. Yank/copy values are
  server-owned structured-tree/content operations, never client parsing of
  rendered text. Whole-hunk, single-line and visual-selection yanks return raw
  code source: remove exactly the structural leading diff marker (`+`, `-`, or
  context space), preserve actual code/indentation and literal `+`/`-`
  characters and line order, and exclude hunk/file headers and no-newline
  annotations. They do not mutate Git or reconstruct old/new file contents.

### 3.15 Highlighting and theme integration

Goal: elements are visually distinct, readable, and **follow the user's colour
scheme** — so we use *links to standard groups*, not hard-coded colours.

- The server labels text with **semantic tokens**: a custom token *type* plus
  *modifiers*. Clients map each type to the standard theme group(s).
- **Neovim:** LSP semantic tokens create highlight groups named
  `@lsp.type.<type>.<filetype>` / `@lsp.mod.<modifier>.<filetype>`. The client
  defines each with `vim.api.nvim_set_hl(0, group, {link = "<standard group>", default = true})`
  so users can override and colour schemes apply. Never set `fg`/`bg` directly.
- **VSCode:** contribute `semanticTokenTypes` (with `superType` where one
  fits) and `semanticTokenScopes` mapping each type to TextMate scopes
  (`markup.inserted`, `markup.deleted`, …) so any theme colours it (verify exact
  contribution points when implementing).
- Prefer **foreground** colours (works with all themes). Full-line background
  tinting for added/removed lines is an optional later extra (client option),
  because semantic tokens cannot colour whole lines.

Initial mapping (finalise during IMM-8 and Phase 1; "verify" = check the group
exists in `:highlight` for the built-in schemes):

| Token type | Meaning | Neovim link | VSCode scope |
| --- | --- | --- | --- |
| `magit.section` | Section titles ("Unstaged changes") | `Title` | `markup.heading` |
| `magit.count` | `(3)` after a title | `Comment` | `comment` |
| `magit.file` | File path in a diff header | `Directory` | `entity.name.tag` |
| `magit.file.modified` / `.added` / `.deleted` / `.renamed` / `.untracked` | status word | `Changed` / `Added` / `Removed` / `Changed` / `Comment` | `markup.changed` / `markup.inserted` / `markup.deleted` / `markup.changed` / `comment` |
| `magit.hunk.header` | `@@ -1,3 +1,4 @@` | `Special` | `meta.diff.range` |
| `magit.diff.added` / `.removed` | `+` / `-` lines | `@diff.plus`→`Added`; `@diff.minus`→`Removed` | `markup.inserted` / `markup.deleted` |
| `magit.diff.context` | context lines | `Normal` (no token) | — |
| `magit.hash` | commit hash | `Constant` | `constant.numeric` |
| `magit.ref.local` / `.remote` / `.tag` / `.head` | refs | `Type` / `Special` / `Label` / `Type` (+ bold) | `entity.name.type` … |
| `magit.author` / `magit.date` | log columns | `Identifier` / `Comment` | `variable` / `comment` |
| `magit.menu.*` | see §3.11 | `Title`, `Statement`, `Keyword`, `Normal`, `Special`, `String`, `Comment`, `String`, `Comment`, `Comment` | as appropriate |
| `magit.message.warn` / `.error` | inline notices | `WarningMsg` / `ErrorMsg` | `markup.deleted` |

Modifiers: `staged`, `selected`, `planned`, `disabled`, `bold`. Neovim applies
`@lsp.mod.<modifier>.<filetype>` above the type group, so modifier links only
add attributes: `staged` / `bold` → `Bold`, `selected` → `Visual`,
`planned` / `disabled` → `Comment`.

### 3.16 Async runner, refresh, parsing

- `tokio`. Replace the synchronous loop. Git invocations go through **one
  runner**: serialises mutations per repository (a queue); lets reads and key
  handling continue during long operations; streams `$/progress`; supports
  cancellation; records command, exit code, duration, stdout, stderr in a ring
  buffer backing the **process log**; never lets child stdout reach the LSP
  pipe; logs via `tracing` to stderr/file only.
- **Refresh:** `common.refresh` plus automatic refresh from a debounced file
  watcher (`notify`) on the worktree and on `.git` (`HEAD`, `index`, `refs`,
  `MERGE_HEAD`, `rebase-*`, `CHERRY_PICK_HEAD`, …). Background reads use
  `--no-optional-locks` (`GIT_OPTIONAL_LOCKS=0`). Handle `index.lock` with a
  short retry then the IMM-3 message. Paused in text mode.
- **Parsing:** `git status --porcelain=v2 -z --branch` and `-z` everywhere
  (spaces, quotes, unicode, renames with both paths). In patches, reuse Git's
  own header lines verbatim.
- Hooks: capture output into the process log. Interactive hooks unsupported.

### 3.17 LSP-native features to use

| LSP feature | Used for |
| --- | --- |
| `textDocument/semanticTokens` | Highlighting (§3.15) |
| `textDocument/documentSymbol` (hierarchical) | Section/file/hunk navigation, outline, breadcrumbs. Replaces `vcs/targets` |
| `textDocument/definition`, `window/showDocument` | "Visit": hunk line → file+line in worktree; hash → revision view; staged-side line → blob view |
| `textDocument/documentLink` | Hashes, paths and refs clickable |
| `textDocument/hover` | Commit summary on a hash, ahead/behind on a ref |
| `textDocument/codeAction` | Context actions at the cursor (stage, unstage, discard, …) |
| `textDocument/publishDiagnostics` | Commit-message lint (summary > 50 chars, non-blank line 2, body > 72), unresolved conflict markers |
| `textDocument/completion` | In commit messages: refs, trailers |
| `$/progress`, `window/showMessage` | Progress, messages |

### 3.18 Configuration

- **Format:** TOML.
- **Default location:** the OS config directory, `<config>/magit-lsp/config.toml`:
  Linux and other Unix `$XDG_CONFIG_HOME` or `~/.config`; Windows `%APPDATA%`;
  macOS: use `~/.config` (XDG-style) unless `$XDG_CONFIG_HOME` is set, because
  that is what terminal-tool users expect (the `etcetera` crate's
  `choose_base_strategy` does this; the `dirs` crate would instead pick
  `~/Library/Application Support` — decide at implementation time, §9).
- **Overrides:** `--config <path>` flag and `MAGIT_LSP_CONFIG` env var.
- **Includes:**
  ```toml
  include = ["keys-work.toml", "/etc/magit-lsp/team.toml", "~/dotfiles/magit/extra.toml"]
  include_optional = ["local.toml"]   # missing file is not an error
  ```
  - Relative paths resolve against the **directory of the file containing the
    `include`**. Absolute paths and `~` are allowed. Environment variables
    (`$VAR`, `${VAR}`) are expanded.
  - Includes load first, in order; the including file's own values override
    them. Later includes override earlier ones.
  - Merge rule: tables merge key by key; **arrays and scalars replace**.
  - Include cycles are detected and reported as an error naming the chain.
- **Layers (lowest → highest):** built-in defaults → profile → user config
  (with includes) → client `initializationOptions` → per-repo
  `.magit-lsp.toml` (only non-executable settings: keys and display; opt-in
  setting `trust_repo_config`).
- **Keys:** `profile = "vim"` or `"standard"`; tables `[keys.common]`,
  `[keys.status]`, `[keys."menu.commit"]`, etc. use short command names and
  arrays of alternatives (`confirm=["Enter"]`, `cancel=["Escape","ctrl+c"]`,
  `stage=["s"]`, `discard=["x","Delete"]`, `amend=["a"]`). Arrays replace
  profile lists; `unbind=["K"]` removes a binding. A scope's `confirm=["Space"]`
  overrides common confirm only there (e.g. `[keys.log]`).
- Other settings (non-exhaustive): `[diff] context, whitespace`, `[sections]
  order, default_visibility`, `[log] default_args, margin`, `[ui]
  confirm_discard = true`. Menu argument persistence semantics must be reviewed
  before adding any persistence setting.
- Hot reload on file change or `workspace/didChangeConfiguration`; invalid
  config keeps the previous one and shows a `Message` with file and line.

### 3.19 Backend boundary and jj

Stance: **do not weaken the Git design for jj.** jj support may be a
semi-hard fork.
- Keep all `git` CLI calls under `backend::git` and domain types (`Revision`,
  `RefName`, `Diff`, `Commit`, …) free of Git-only strings where it costs
  nothing. No trait or capability layer is required now.
- Document the seams in `docs/FORKING.md` (where views, commands, menus and
  the runner are registered) so a fork can swap them.

---

## 4. Protocol reference

No backward compatibility: delete the old `vcs/*` methods as their
replacements land.

### 4.1 Client → server

| Method | Params | Returns |
| --- | --- | --- |
| `initialize` (options) | `clientInfo`, `capabilities: {keymapDelivery: "dynamic"/"static"/"forwardAll", keyModifiers: "full"/"basic", prompts: ["confirm","text","choice","multiChoice"], placements: ["main","side","bottom","float"], editorBridge: bool}`, `profile?`, `viewport` | server capabilities |
| `view/open` | `{kind, args?, placement?, viewport}` (`status` carries `args.repo_path`) | `ViewDescriptor` |
| `view/close` | `{viewId}` | — |
| `view/key` | `{viewId, version, key, editorMode, cursor: {line, column}, selection?: {startLine, endLine}}` | `{effects: [...]}` |
| `view/invoke` | `{viewId, command, args?, cursor?, selection?}` | `{effects: [...]}` |
| `view/rendered` (notification) | `{viewId, version}` | — |
| `view/resync` | `{viewId}` | full render |
| `view/resize` (notification) | `{viewId, cols, rows}` | — |
| `view/setMode` | `{viewId, mode: "interactive"/"text"}` | `{effects}` |
| `view/promptRespond` | `{promptId, cancelled, value?}` | — |

### 4.2 Server → client

| Method | Params |
| --- | --- |
| `view/render` (notification) | `{viewId, version, baseVersion?, text?, edits?: [{startLine, endLine, lines}], cursor?: {line, column}, mode, pending?, menu?: Menu}` |
| `view/keymap` (notification) | `{viewId, scope, version, capture, bindings: [{command, keys: [string], modes: ["normal","visual"], label, doc, group, status}], mode, pending?, liveKeys?: [string]}` |
| `view/effects` (notification) | `{effects: [...]}` for asynchronous effects |
| `view/prompt` (request) | See §3.6; returns `{cancelled, value?}` |
| standard | `window/showDocument`, `window/showMessage`, `$/progress`, semantic tokens, symbols, definition, links, hover, code actions, diagnostics, completion |

**PLANNED — Phase 3E draft wire contract (not implemented).** The current
`editDocument` effect still carries `editId`, Git-owned `path`, `kind`, `title`,
`mode: "realFile"` and `scope` (§3.10; `src/protocol.rs`). At coordinated
commit-message cutover only, replace that path-based edit with open-draft carrying
`runId`, opaque draft URI (`magit-commit://<opaque-run>/message`), initial version
and initial text. Standard LSP `textDocument/didOpen`, `didChange`, `didClose`
are the sole synchronization path; R-EDIT-1 resolves sync ordering/version and
size bounds, retaining live draft plus last-saved checkpoint. Save/autosave
checkpoints never commit. Explicit confirm carries
`runId` and exact `draftVersion`, bound to the authenticated owning client/session;
the barrier waits for applied sync, validates owner/run/version/target, and writes
that exact text to the validated helper target before helper `ok`. Write/version
failure leaves draft/helper pending. Normal user close confirms the last saved
checkpoint; explicit abort/progress cancel/disconnect/EOF/shutdown cancel the
run. Native Save/Discard/Cancel and didClose-not-intent semantics are specified
in §3.10. Exactly-once terminal edit-close identifies run/URI and outcome,
invalidates the editor idempotently without recursive cancel, and preserves
modified text for native recovery subject to §3.10's crash limits. Final names
and payload types land together in `src/protocol.rs`; no dual compatibility or
custom competing sync path. Status views remain readonly and server-rendered.

### 4.3 Example

Client presses `s` with the cursor on a `+` line in an unstaged hunk:
```json
{"method":"view/key","params":{"viewId":"v1","version":12,"key":"s","editorMode":"normal","cursor":{"line":18,"column":4}}}
```
Server stages the hunk and answers:
```json
{"result":{"effects":[]}}
```
followed by a notification:
```json
{"method":"view/render","params":{"viewId":"v1","version":13,"baseVersion":12,
 "edits":[{"startLine":9,"endLine":22,"lines":["  Staged changes (1)", "…"]}],
 "cursor":{"line":10,"column":4}}}
```

---

## 5. Default keymaps (profiles)

Profiles are data files shipped in the binary (`profiles/standard.toml`,
`profiles/vim.toml`) and validated by tests. Values below are **proposals**
meant to be committed as the first version; edit freely.

**Rules for every binding in every profile**
- R1. No Emacs prefix keys (`ctrl+c ctrl+*`, `ctrl+x …`) and no `alt+<letter>`
  meant to mimic `M-<letter>`.
- R2. Prefer Magit's mnemonic letters where the letter matches the Git
  operation name (`c` commit, `b` branch, `l` log, `d` diff, `f` fetch,
  `F` pull, `P` push, `m` merge, `r` rebase, `t` tag, `s`/`u` stage/unstage).
- R3. `standard` uses direct, widely-known keys: `Enter` confirms, `Escape`
  cancels, `Delete` discards, arrows navigate.
- R4. `vim` never rebinds visual-mode keys (`v`, `V`, `ctrl+v`), counts
  (`0`–`9` as counts), `h j k l`, `gg`, `G`, `ctrl+d/u/f/b`, `/`, `H`, `L`.
- R5. Every command has a binding (or an explicit "unbound" entry) in every
  profile; a test enforces this.
- R6. `ctrl+c` always means cancel in non-editable views (with the
  copy-passthrough rule from §3.9). Editable scopes use the keys below.

### 5.1 `common` scope

| Command | `standard` | `vim` |
| --- | --- | --- |
| `common.confirm` | `Enter` | `Enter` |
| `common.cancel` | `Escape`, `ctrl+c` | `Escape`, `ctrl+c` |
| `common.quit` | `q` | `q` |
| `common.refresh` | `g`, `F5` | `g r` |
| `common.toggle-section` | `Tab` | `Tab`, `z a` |
| `common.show-section` / `hide-section` | — | `z o` / `z c` |
| `common.cycle-visibility` (all sections) | `shift+Tab` | `shift+Tab` |
| `common.show-all` / `hide-all` | unbound | `z R` / `z M` |
| `common.more-detail` / `less-detail` | unbound | `z r` / `z m` |
| `common.detail-level-1` … `detail-level-4` | `alt+1` … `alt+4` | unbound |
| `common.next-section` / `prev-section` | `n` / `p` | `] ]` / `[ [` |
| `common.next-sibling` / `prev-sibling` | `alt+ArrowDown` / `alt+ArrowUp` | `g j` / `g k` |
| `common.parent-section` | `alt+ArrowLeft` | `g u` |
| `common.toggle-text-mode` | `ctrl+t` | `ctrl+t` |
| `common.copy-value` | `ctrl+shift+c` | `y y` |
| `common.visit` | `Enter` | `Enter` |
| `open.dispatch` (help / menu of menus) | `?` | `g ?` |

Visibility has four levels: section headers → file headers → hunk headers
(without body lines) → individually expanded hunk bodies. The four
`common.detail-level-N` actions set an absolute level; `standard` binds
`alt+1` … `alt+4` to these actions. `vim` uses `z m`/`z r` to step down/up
and `z M`/`z R` to hide/show extremes. These are server-owned visibility
actions; clients do not infer structure or key-specific state.

### 5.2 `status` scope (also used by diff-like views for staging)

| Command | `standard` | `vim` |
| --- | --- | --- |
| `status.stage` | `s` | `s` |
| `status.unstage` | `u` | `u` |
| `status.stage-all` | `S` | `S` |
| `status.unstage-all` | `U` | `U` |
| `status.discard` | `Delete` | `x` |
| `status.reverse` | `v` | `R` |
| `status.apply` | `a` | `a` |
| `open.commit` | `c` | `c` |
| `open.branch` | `b` | `b` |
| `open.log` | `l` | `l` |
| `open.diff` | `d` | `d` |
| `open.fetch` | `f` | `f` |
| `open.pull` | `F` | `F` |
| `open.push` | `P` | `P` |
| `open.merge` | `m` | `m` |
| `open.rebase` | `r` | `r` |
| `open.stash` | `z` | `Z` |
| `open.tag` | `t` | `t` |
| `open.reset` | `x` | `X` |
| `open.revert` | `V` | `-` |
| `open.cherry-pick` | `A` | `A` |
| `open.bisect` | `B` | `B` |
| `open.remote` | `M` | `M` |
| `open.submodule` | `o` | `o` |
| `open.worktree` | `%` | `g %` |
| `open.ignore` | `i` | `i` |
| `open.refs` | `y` | `g y` |
| `open.cherries` | `Y` | `g Y` |
| `open.patch-apply` | `w` | `w` |
| `open.patch-create` | `W` | `W` |
| `open.run` | `!` | `!` |
| `open.process-log` | `$` | `g $` |

`vim` keeps the single letters `f F t T m r M` (shadowing the Vim find/till/
mark/replace motions) because they match Magit and are useless in a read-only
buffer. `v V ctrl+v` remain Vim's visual modes, which is why reverse is `R`,
revert is `-`, stash is `Z` (since `z` is the fold prefix), reset is `X`
(since `x` is discard) and a few menu keys gain a `g` prefix.

### 5.3 Select mode (`log.select`, `refs.select`)

| Command | `standard` | `vim` |
| --- | --- | --- |
| `common.confirm` | `Enter` | `Enter` |
| `select.toggle-mark` (multi) | `Space` | `Space` |
| `select.enter-rev` | `i` | `i` |
| `common.cancel` | `Escape`, `ctrl+c` | `Escape`, `ctrl+c` |

### 5.4 Menu scopes (`menu.*`)

| Command | `standard` | `vim` |
| --- | --- | --- |
| Toggle a switch | `-` then its letter | same |
| Set an option | `=` then its letter | same |
| Run an action | its letter | same |
| `common.cancel` / close | `Escape`, `ctrl+c`, `q` | same |
| `menu.save-args` | `ctrl+s` | `ctrl+s` |
| `menu.reset-args` | `ctrl+r` | `ctrl+r` |
| `open.dispatch` (toggle help) | `?` | `?` |

### 5.5 Commit-message scope

| Command | `standard` | `vim` |
| --- | --- | --- |
| `common.confirm` | `ctrl+Enter` | `Z Z` (and `ctrl+Enter` where the terminal supports it) |
| `common.cancel` | `ctrl+shift+x` | `Z Q` |
| `commit-message.history-prev` / `next` | `alt+ArrowUp` / `alt+ArrowDown` | `[ m` / `] m` (normal mode) |
| `commit-message.add-signoff` | `ctrl+alt+s` | `g s` |

Closing the buffer is always cancel (§3.10). `ctrl+c` is **not** cancel here
because it is copy in non-modal editors.
The current bridge emits this scope for commit messages; merge/tag message
support is not emitted yet, and `rebase-todo` has no implemented edit scope.

### 5.6 Rebase todo (structured view)

| Command | `standard` | `vim` |
| --- | --- | --- |
| `rebase.pick/reword/edit/squash/fixup/drop` | `p`/`r`/`e`/`s`/`f`/`Delete` | `p`/`r`/`e`/`s`/`f`/`x` |
| `rebase.move-up` / `move-down` | `alt+ArrowUp` / `alt+ArrowDown` | `ctrl+k` / `ctrl+j` |
| `common.confirm` / `common.cancel` | `ctrl+Enter` / `Escape` (prompts if changed) | `Z Z` / `Z Q` |

(Other views define their bindings when implemented; they must follow R1–R6.)

---

## 6. View catalog

`Phase` refers to §7. Planned entries appear greyed in menus from Phase 2.

| View | Magit analog | Contents | Key commands | Phase |
| --- | --- | --- | --- | --- |
| **Status** | `magit-status` | Header lines (Head, Merge, Push, Tag); sections: in-progress operation, Untracked, Unstaged, Staged, Unmerged, Stashes, Unpulled, Unpushed, Recent commits, Submodules | stage/unstage/discard/apply/reverse, visit, toggle, refresh, all menus | 1–2, grows to 6 |
| **Log** | `magit-log` | Commit list with graph, decorations, author, date; options (`-n`, `--graph`, `--decorate`, `--author`, `--since`, `--grep`, `-S`, `-G`, path limits); lazy "load more"; **select mode** | show commit, checkout/reset/branch/rebase/cherry-pick/revert from the commit at point, confirm/cancel | 4 |
| **Log variants** | `l` menu | current, other, all, local branches, reflog, file log, line-range log (`-L`), `HEAD..upstream`, `upstream..HEAD` | same | 4 |
| **Revision** | `magit-revision` | Header (hash, parents, author, committer, dates, refs, message), diffstat, full diff with hunks; parent/child navigation; combined diff for merges | visit file, apply/reverse hunk, copy hash | 4 |
| **Diff** | `magit-diff` | Any range `a..b`, `a...b`, worktree vs index vs revision, path-limited, stash vs parent, stat-only; context/whitespace/rename/word-diff options | visit, stage/unstage when applicable | 4 |
| **Refs** | `magit-show-refs` | Local branches, remotes, tags with ahead/behind vs a base; also select mode | checkout, create, delete, rename, reset, merge, rebase onto, log | 4 |
| **Cherry** | `magit-cherry` | `git cherry` between two refs | cherry-pick, show | 7 |
| **Stash list / show** | `magit-stash-list` | Stashes; show = diff view of a stash | apply, pop, drop, branch | 6 |
| **Rebase todo** | `git-rebase-mode` | Planned structured interactive todo: pick/reword/edit/squash/fixup/drop/exec/label/merge lines with commit summaries; reorder | move up/down, set action, show commit, confirm, abort | 7 |
| **Rebase in progress** | status section | Todo position, stopped commit, conflicts | continue, skip, abort, edit todo | 7 |
| **Merge / conflicts** | `magit-merge` + unmerged | Unmerged section; per-conflict actions | take ours/theirs, mark resolved, open merge tool (client effect `openMergeTool`), continue, abort | 7 |
| **Sequencer** | `A`, `V` | In-progress cherry-pick/revert | continue, skip, abort | 7 |
| **Commit message** | with-editor buffer | Message editor (currently real file; planned managed draft) + companion read-only staged diff, server-owned history ring (R-EDIT-8), trailers, lint diagnostics | confirm, cancel, history prev/next, add trailer | 5 |
| **Process log** | `magit-process-buffer` | Every Git invocation: args, exit code, duration, output, hook output | visit, copy command, clear | 3 |
| **Blame** | `magit-blame` | Annotated file (hash, author, date, summary); also inlay hints/code lens in real file buffers | show commit, blame at parent, copy hash | 8 |
| **File at revision** | `magit-find-file` | Virtual doc `magit://blob/<rev>/<path>` | diff against worktree | 4 |
| **Bisect** | `magit-bisect` | In-progress section: good/bad/skip log, candidate | good, bad, skip, run, reset | 8 |
| **Worktrees** | `magit-worktree` | List with branch/path/state | add, checkout, move, delete, visit | 8 |
| **Submodules** | `magit-submodule` | Modules section and list | add, register, update, sync, fetch, populate, unpopulate | 8 |
| **Remotes / config** | `magit-remote`, `magit-gitconfig` | Remote list; per-branch/remote config | add, rename, remove, prune, set upstream/pushRemote | 6 |
| **Patches** | `w`, `W` | `git am`/`apply`, `format-patch`; in-progress am | apply, continue, skip, abort | 8 |
| **Notes / ignore / run** | `T`, `i`, `!` | Notes; gitignore targets; run arbitrary git command | per command | 8 |
| **Repository list** | `magit-list-repositories` | Multi-repo overview | open status | 9 |
| **Dispatch / help** | `magit-dispatch` | Generated from the command registry and keymap manifest | invoke by key | 2 |
| **Menus** | transients | commit, branch, fetch, pull, push, merge, rebase, stash, tag, reset, cherry-pick, revert, bisect, remote, submodule, worktree, diff, log, ignore, patch | §3.11 | 2 (framework + all Planned) → 4–8 (implemented) |

---

## 7. Phased roadmap

Checkboxes are for tracking. IDs (`R-n.m`) are used in `Planned` messages.
Each task ends with **Done when**. Use relevant, risk-scoped checks per task;
run full `make test` about every three completed implementation tasks and at
phase/handoff boundaries. Full UI suites are reserved for broad UI/shared
lifecycle changes, phase completion, or suspected regressions. Preserve explicit
platform/security acceptance gates and report skipped or deferred checks
truthfully. Historical task Verify command lists and recorded evidence describe
what was required or run at that time; do not rewrite them to imply current
policy or erase that evidence.

### Global unchecked backlog — priority order

The task specifications and checkboxes remain in §§7, 11.3 and 12; this index makes
the categories and ordering global. Complete all Review items before Feature
work. Bugfix work is a distinct category, not a scenario-behavior task.

#### Review (highest priority; grouped by stated priority)

**Active review sprint:** Work only from [`REVIEW_REPORT.md`](./REVIEW_REPORT.md)
for the current technical-debt review. Its dated reconciliation below supersedes
this older roadmap backlog for sprint scheduling; do not start unrelated unchecked
items here. Preserve that report's acceptance gates and do-not-touch constraints.
Current priority work is A12 (next), A18, A19, A29, A33 and A34 before deletion
work; A01–A10 are complete with recorded evidence. A11 is incomplete and has
sole acceptance ownership in Phase 3E (A11 / R-EDIT-6), deferred from this sprint;
neither Phase 3E implementation nor A12 is launched by this plan. A22/A23
remain withdrawn. Do not duplicate the report's 32-task ledger in this index.

- [x] A29 focused native verification subtask: isolated physical navigation,
  search/editor copy, shortcut/capture suppression and queued-refresh return;
  real dispatch-menu wire coverage. Evidence: `/tmp/opencode/vscode-review-04pmBr/a29-evidence.json`.
  Overall A29 acceptance: `make test && make vscode-test && scripts/ui run tests/ui/scenarios/text-mode-roundtrip.scn && git diff --check` passed. Wire evidence records same-version entry render and invoke/key/menu/setMode coverage; native screenshots/evidence confirm unchanged selection while queued refresh applies on exit, native copy/replacement and cursor navigation, and capture-mode descriptor with no suppressed requests. Existing Neovim semantics remain covered by the full gate.

- **P0:** none remaining for the VSCode review: R-VS-2, R-VS-3, R-VS-4 and
  R-VS-5 (§12) all have recorded regression/real-client evidence; R-VS-3's F5
  limitation and R-VS-4's actual theme measurements are explicit.
- **P1:** Build `TestSession`; verify askpass secrets are
  not logged; add missing real-UI scenarios; test resize reflow.
- **P2:** Fix keymap rule checks; resolve detail-level mismatch; authoritative
  parity keys; test unavailable/scope mismatch; acked versions; keymap delivery;
  cross-scope conflicts; config gaps; minimum Git version; editor disconnect;
  Neovim prompts; option clearing/saved args; VSCode keybinding drift; remove
  Lua sleeps; strengthen read-only assertions; mutation ordering (ordering
  review done; its unsolved read-responsiveness defect is BUG-READ-1,
  §7 Bugfix); prune duplicate tests; reduce snapshots; remove magic counts;
  tighten assertions; remove global environment mutation.
- **P3:** UI latency assertions; rendered-text parsing in tests; housekeeping;
  remaining small coverage gaps.
- Client review follow-ups: R-NV-1…R-NV-4 and R-VS-1.
- R-UI-7 status placement (from the Neovim client review).

#### Bugfix

- [ ] **[P1] BUG-READ-1 — Reads must stay responsive while a mutation is
  blocked inside real Git** (unsolved; moved out of the completed
  mutation-ordering review item, §12; **not scheduled for implementation
  until the user asks**)
  - **Requirement (preserved from the review item's read half):** a read
    (`view/resync`, and by the same root cause `view/rendered`, `view/resize`
    and `view/open` in any repository) must not be delayed by a `view/invoke`
    that is blocked inside a real Git command. The probe asserts this
    contract with the real gate; it currently fails and is `#[ignore]`d as
    BUG-READ-1 (run `cargo test --test server_async -- --ignored`), so the
    green debug+release suites say nothing about read responsiveness.
  - **Evidence (known-failing probe, retained intact in
    `tests/server_async.rs`):** bounded evidence from the failing run: the
    server spawned `git add -u` at 05:58:35.173604Z (child blocked in the
    gate, `entered` observed), the `view/resync` request was received at
    05:58:35.181196Z, and the read produced no response within the 3 s window
    while the server stayed alive (`server exit: Ok(None)`). Full log:
    `/tmp/opencode/read_probe1.log`. Cause: `invoke_view` locks
    `Shared.sessions` before `session.invoke_ui` and the whole Git command
    runs inside that call, while `view/resync` needs the same mutex; the lock
    is global across repositories, so a slow Git in one view also blocks
    `view/rendered`/`view/resize`/`view/open` everywhere.
  - **Proposed design-review gate (PROPOSED only — not implemented and not
    approved; flagged for design/user review, not a decided shortcut; the bug
    is not scheduled until the user asks):**
    - global registry lookup only → per-repo session handle;
    - per-view published latest-sent and last-acked immutable state, outside
      the Git and prompt locks;
    - a render publishes before it notifies and derives its edit base from
      the same ack slot;
    - `Session::ack` and the transport share one authoritative equal-version
      promotion implementation;
    - positional/resync reads never need the session lock;
    - acks are handled directly and resize is queued off the reader thread;
    - view lifecycle remains server-owned;
    - the current initial-unacked fallback is preserved unless explicitly
      reviewed, and a stale ack must never erase a valid ack;
    - a resync during a mutation returns the latest PUBLISHED pre-mutation
      state, with the post-mutation render arriving as a later notification.
  - **Required tests:** blocked-Git resync, positional and cross-repo reads;
    ack during a mutation and the edit-base derivation; stale-ack
    preservation; resize does not block intake.
  - **Escalation:** escalate before implementing if this needs a
    View/`CommandCtx` rewrite or multiple unacked versions per view; an
    async/backend redesign is out of scope.
  - **Minimal first step (unchanged from the review item's separate-ticket
    scope):** stop holding `Shared.sessions` across the Git run in
    `invoke_view` (and the identical `key_view`/`set_mode_view` shape), then
    un-ignore the probe; the now-unused debug-only `maybe_test_delay`/
    `MAGIT_LSP_TEST_DELAY_MS` hook in `src/server.rs` can be removed with it.
- **P1:** Diagnose Neovim harness startup/socket creation and
  connection independently of scenario behavior (§11.3).

#### Feature

- First scheduled feature: **R-5.1**, log plus reusable server-owned select-mode
  infrastructure. Phase order changed: Read views are now Phase 4 and Commit
  flow Phase 5; retain R-5.* and R-4.* IDs because Planned messages and other
  stable references already use them. Then R-5.2…R-5.5; R-4.1…R-4.3;
  R-6.1…R-6.5; R-7.1…R-7.4; R-8.1…R-8.3;
  R-9.1…R-9.5. Their full acceptance criteria remain in the phase entries.

### Task ticket template and UI rule

Every implementation TASK TICKET specifies **Target Files** (exclusive write
boundary), **Reference Files** (read first), **Architectural Constraints**
(invariants), **Acceptance Criteria** (observable outcomes), and **Verification
Command** (exact command that must exit 0).

For any task that changes what the user sees (`ui-test-plan.md` U2.1):

- **Acceptance Criteria** must contain a visible outcome: "after pressing X
  the screen shows …".
- **Verification Command** must include focused real native-UI evidence for
  the affected client, using relevant scenario(s), not automatically the full
  `make ui-test` suite. VSCode-only changes require VSCode native evidence;
  unrelated Neovim runs do not satisfy that gate. Use the full UI suite only
  for broad UI/shared lifecycle changes, phase completion, or suspected
  regressions.
- The report lists the scenario files, their results and saved evidence paths,
  and explicitly identifies checks skipped or deferred.
- A task is not done until a `.scn` scenario has driven the real UI
  (`scripts/ui`, §8.8).

Every task that opens a new view (log, revision, refs, commit message) also
adds a scenario for it.

### Phase 0 — Groundwork

- [x] R-0.1…R-0.7: immediate fixes (§2), porcelain-v2 parsing (including
  rename/unusual paths), separate Untracked section, patch edge-case fixtures,
  content-based hunk identity with stale-action rejection, tracing off the LSP
  pipe and Tokio stdio. Patch/backend boundaries remain §§3.13/3.19.

### Phase 1 — Server-owned document and sessions

- [x] R-1.1…R-1.6: server tree/rendering, sessions/view stack, full/edit
  delivery with ack/resync, Node-ID cursor continuity, tokens, symbols/visit
  and lazy server-owned visibility (§§3.1–3.4, 3.15; §4). Removed old `vcs/*`
  methods and text parsing. Visit uses `definition`, not `showDocument`.

### Phase 2 — Commands, keymaps, config, text mode, menu framework, second client

- [x] R-2.1…R-2.9: command registry, TOML keymaps/config/reload, dynamic and
  static input delivery, Neovim atomic mappings/visual capture/fold mirroring,
  manifest CLI/checks, text mode/copy, viewport-driven menus and Planned
  dispatch/parity inventory. VSCode is the thin second client (static bindings,
  context keys, QuickPick menus, fold mirroring). Requirements remain
  §§3.5, 3.8–3.14, 3.18 and §5; remaining client work is below and in §12.

### Phase 3 — Effects, prompts, runner, process log, editor bridge

- [x] R-3.1…R-3.4: prompts/`ScriptedUi`, runner queue/progress/cancel/ring log,
  process-log view and editor-bridge core with a fake-client commit test.
  This historical core milestone does not close R-3.4b or Phase 5 (commit flow). Protocol
  and runner decisions are §§3.6/3.10/3.16 and §10; UI coverage gaps are §12.

### Phase 3U — Real-UI harness and UI bug fixes (do before R-3.5)

The step lists (U0.x, U1.x, U2.x), the bug table and the scenario DSL live in
`ui-test-plan.md`.

- [x] R-UI-1…R-UI-6: tmux/ANSI/fixture/scenario harness and debug API; queued
  pre-open renders; help/menu-stack fixes; read-only W10 fix; action/flag/chord
  rendering; commit feedback/progress cleanup; tour and CI. Detailed U0–U2
  contracts and bug/scenario mapping remain in `ui-test-plan.md`; operating
  rules and artefact paths are §8.8. Server `pending`/`liveKeys`, not client
  chord logic, drive dimming and Backspace (§3.11).
  Historical final result: `make test`, `make ui-unit`, `make ui-test` green,
  13 PASS / 0 XFAIL / 0 FAIL across both profiles/sizes. `make ui-tour` wrote
  `build/ui/tour.md` (status, dispatch, 21 menus, switch/chord, unavailable
  action, clean-tree commit and hook failure). Bug 5 did not reproduce
  (11–16 ms); latency remains a regression guard, not a diagnosed slowdown.
- [x] R-UI-7 **Status fills the window by default**: `setup()` must no longer
      `vsplit`. The status view replaces the current window's buffer (the
      full screen); the client option `status = { placement = "replace" |
      "vsplit" | "split" | "tab" }` (default `"replace"`) restores a split or
      a tab. The server's `ViewDescriptor` placement stays advisory; the
      option lives in the client only. `visit` (file under cursor) must keep
      working when there is no other window (open the file in the same
      window; `q` or the buffer-list returns to status, or open a split when
      `placement` is a split). Document the option in the README.
      *UI:* scenarios `status-fills-screen` (`windows` shows exactly one
      non-floating window whose width equals the terminal columns; screen
      contains `Unstaged changes`) and `status-placement-vsplit` (set via an
      env/`MAGIT_LSP_PLACEMENT` hook in `scripts/minimal_init.lua`; two
      windows). Every existing scenario must still pass at 100x30 and 80x24.
      *Done when:* both scenarios PASS under both profiles and sizes;
      `make test` and `make ui-test` green.
- [x] R-VS-1 **VSCode extension: build, test and document** (see §11.2): a
      `clients/vscode/README.md` that explains how to run it (Extension
      Development Host, F5 launch config in `.vscode/launch.json`), how to
      point it at the server binary, how to package (`vsce package` or
      `npx @vscode/vsce`), how to run the tests, and the keybinding manifest
      workflow (`magit-lsp keymap --format vscode`). Add a test layer that
      works without VSCode: a fake `vscode` module so `extension.js` can be
      activated in node against the real `magit-lsp` binary over stdio
      (open status, render text, key press → render, menu QuickPick via a
      scripted fake, prompt answer). Add `make vscode-test` (node unit tests
      plus this integration test) and a CI step. Add an optional
      `@vscode/test-electron` suite under `clients/vscode/test/e2e/` that is
      skipped when `code`/Electron is unavailable. Split `extension.js` (about
      1200 lines) into modules under `clients/vscode/lib/` as part of this.
      *Done when:* `make vscode-test` passes locally, README steps are
      verified by running the commands listed in it, and CI runs
      `make vscode-test`.
- [x] R-3.4b **Editor bridge clients and askpass** (U2.4): handle
      `editDocument` in Neovim and VSCode, implement `--askpass`, and add the
      `commit-message-editor` scenario (`c c` with a staged change → bottom
      split with a `gitcommit` buffer → type and confirm → clean tree and the
      commit in the log). Runs after R-UI-5, which provides the
      `commit.create` pre-flight.
      Scenarios: `commit-message-editor`.
      *Done when:* remove the `xfail` tag from the listed scenario(s); `make
      test` and `make ui-test` green.
      *Done:* the Neovim client opens the real file in a bottom split with the
      kind's filetype (`lua/magit-lsp/edit.lua`), VSCode handles `editDocument`
      in its edit flow, and `--askpass` forwards Git's prompt to the running
      server without ever logging the secret (`src/editor.rs`, `src/cli.rs`;
      `tests/editor_bridge.rs` covers the typed commit, cancel, client
      disconnect and secret-never-logged cases). The driver now sends
      modifier+special keys (`C-Enter`, `shift+Tab`, …) as raw CSI-u bytes
      because tmux named keys drop the modifier.

### Phase 3N — Neovim client debt (findings in §11.1; do before R-3.5)

- [x] R-NV-1 **Async request layer** (§11.1 finding 1): replace
      `buf_request_sync`/`vim.wait` on every key, invoke and open with
      asynchronous `client:request` plus a small per-client request queue, so
      a slow Git command, a prompt or the editor bridge never freezes Neovim.
      Keep ordering: keys are sent in order; a key pressed while one is in
      flight is queued. `idle()` (debug API) must stay correct.
      *UI:* scenario `ui-stays-responsive`: a fixture with a slow `pre-commit`
      hook (or a sleeping fake git wrapper); press `c c`, then while it runs
      `?`/`j` still move the cursor (assert `cursor` changes before idle).
      *Done when:* no `vim.wait` remains outside tests and `debug.lua`;
      `make test` and `make ui-test` green.
- [x] R-NV-2 **One view manager** (§11.1 finding 2): replace the three parallel
      implementations (`init.lua` status, `menu.lua`, `edit.lua`) of
      render/keymap/press/invoke/resync/pending-queue with one `view.lua`
      keyed by `viewId`, driven only by the `ViewDescriptor` (placement
      `main`/`side`/`bottom`/`float`, `capture`, `mode`) and the server's
      `view/render`/`view/keymap`. Kind-specific code shrinks to a placement
      function and an effect handler. A new view kind (log, revision, refs,
      process log) must need no new Lua beyond a highlight link table entry.
      Remove the module-level singleton state (`M.buf`, `M.view_id`) so two
      repos or tabs work.
      *Done when:* `init.lua` is under ~400 lines, `menu.lua` and `edit.lua`
      contain only placement and effects; all L2 cases and `make ui-test`
      green; line counts recorded in §11.1.
- [x] R-NV-3 **Isolate debug instrumentation** (§11.1 finding 3): product code
      calls `debug_api.*` about 35 times. Route them through one thin hook
      table (`require('magit-lsp.hooks')` with no-op defaults) that
      `debug.lua` fills only when `MAGIT_LSP_TRACE` or the test harness is
      active; `debug.lua` must not be required on a normal start.
      *Done when:* a normal `setup()` never loads `debug.lua`
      (L2 assert on `package.loaded`); `make ui-test` green.
- [x] R-NV-4 **Structure and defaults** (§11.1 findings 4–7): split `init.lua`
      (highlights, buffer/read-only, setup/config); find the server binary in
      this order: `setup{cmd}`, `$MAGIT_LSP_BIN`, `target/release`,
      `target/debug`, `magit-lsp` on `$PATH`, and report a clear error instead
      of falling back to `cargo run`; drop the visit-key exception by
      implementing `common.visit` on the server (the Lua `visit` and
      `visit_key` option are then deleted); let the server ship the
      suggested highlight group per token type in the legend so new token
      types need no client edit (the Lua table keeps only an override).
      *Done when:* each item is done or recorded as rejected with a reason in
      §11.1; `make test` and `make ui-test` green.

### Phase 3 (continued) — file watcher, discard, error mapping

- [x] R-3.5 File watcher auto-refresh; `index.lock` handling.
      *UI:* add scenario `auto-refresh`: modify a tracked file outside Neovim,
      wait, and the status screen shows the new change without
      `common.refresh`. Verified PASS in `make ui-test`; saved screens:
      `build/ui/auto-refresh/standard-100x30/after-refresh.screen`,
      `build/ui/auto-refresh/standard-80x24/after-refresh.screen`,
      `build/ui/auto-refresh/vim-100x30/after-refresh.screen`, and
      `build/ui/auto-refresh/vim-80x24/after-refresh.screen`.
- [x] R-3.6 Discard (`status.discard`) for files, hunks, selections, with a
      destructive confirmation before mutation. Discarded content is not saved
      to a private ref: callers should commit or stash content they need to
      recover.
       *UI:* `discard-confirm` PASS in `make ui-test`: the hunk prompt is
       destructive, No preserves the change and Yes removes it. Screens:
       `build/ui/discard-confirm/standard-100x30/{confirm,declined,accepted}.screen`,
       `build/ui/discard-confirm/standard-80x24/{confirm,declined,accepted}.screen`,
       `build/ui/discard-confirm/vim-100x30/{confirm,declined,accepted}.screen`,
       `build/ui/discard-confirm/vim-80x24/{confirm,declined,accepted}.screen`.
       Verified with `make test && make ui-test && make ui-tour`.
- [x] R-3.7 `UserError` mapping table for all remaining Git failures as they
      are met. The pathspec failure emitted by the implemented `git add`
      operation now has a refresh-and-retry summary; other stderr still uses
      the readable `git <subcommand> failed.` fallback with raw output only in
      the detail field. Existing process-log reservation and secret redaction
      are unchanged. Regression coverage includes a real missing-path `git add`
      and the error UI scenario.
      *UI:* `git-error-readable` PASS in `make ui-test` under both profiles at
      100x30 and 80x24. Screens:
      `build/ui/git-error-readable/standard-100x30/readable-git-error.screen`,
      `build/ui/git-error-readable/standard-80x24/readable-git-error.screen`,
      `build/ui/git-error-readable/vim-100x30/readable-git-error.screen`,
      `build/ui/git-error-readable/vim-80x24/readable-git-error.screen`.
      Verified with `make test && make ui-test && make ui-tour`.

### Phase 3E — Managed commit drafts and cancellable Git runs (before Phase 5)

Decision dated 2026-10-27. Planning does not start implementation. Current
realFile behavior remains until coordinated cutover. User-reported Linux Git
2.43 reproduction is distinct from proven trace `build/review/a11-final-verification.log`:
10 pass/1 fail, cancellation observed during hook, no progress end/error before
timeout. No implementation was verified at that decision; the scoped Linux
refinement evidence below does not complete the phase, and macOS remains
unverified. Linux acceptance/cutover does not depend on macOS execution; later
macOS CI/platform acceptance is owned by R-EDIT-9, with the full-grace fallback
retained and no validation claimed. Windows remains separately deferred in
R-EDIT-7. R-4.2 remains
staged diff/trailers/lint owner, integrates the R-EDIT-8 ring (not a second
history implementation), and depends on this phase.

- [x] **R-EDIT-1 — Specify lifecycle and process feasibility**
  - **Paths:** `src/editor.rs`, `src/runner.rs`, `src/git.rs`, `src/server.rs`, `src/protocol.rs`, `lua/magit-lsp/edit.lua`, `clients/vscode/lib/edits.js`, `clients/vscode/lib/edit.js`, `tests/editor_bridge.rs`, `tests/ui/scenarios/commit-cancel-progress.scn`, `ui-test-plan.md`.
  - **Depends on:** none.
  - **Done when:** lifecycle/races, auth/path, draft sync/version/size bounds, native adapters, process group/tty/reap/output and crash/nested-hook/escapee limitations specified; Linux/macOS validation and Windows feasibility explicit. Specify native close-intent classification separately from LSP didClose (which is not confirmation), and synchronization ordering versus save-checkpoint acknowledgement. Contract is in §3.10; native acceptance maps to R-EDIT-4/5/A11 and process adversarial tests to R-EDIT-2/A11. Design review only; no implementation or platform execution claimed.
  - **Verify:** documentation/interface review and targeted platform-design feasibility review (not a claimed platform test); choose full/incremental sync, bounds, and history count/UTF-8 byte-cap overflow policy here (never silently truncate a message).
- [ ] **R-EDIT-2 — Own runs and safely cancel Git children**
  - **Paths:** `src/runner.rs`, `src/git.rs`, `src/server.rs`, `src/editor.rs`, `src/protocol.rs`, `tests/editor_bridge.rs`.
  - **Depends on:** R-EDIT-1; Linux acceptance does not depend on R-EDIT-9/macOS execution.
  - **Done when:** sole Child owner arbitrates nonblocking cancellation/reap; pre-hook, editor-active, commit-msg, finalizing and completed outcomes truthful (cancel-before-commit aborts; completed commit reports success); bounded drain, no detached indefinite readers, no normal-exit daemon killed to drain, one progress end/release.
  - **Verify:** `cargo test --test editor_bridge --test server_async` on Linux; real-Git foreground/background hooks, each cancellation phase, inherited pipes after normal exit, TERM-resistant escalation, exact HEAD/index/worktree, subsequent commit without manual lock cleanup, lock warning, no foreign kill. macOS runner/group/WNOWAIT/reap evidence is deferred to R-EDIT-9, not required for this Linux acceptance.
    4. Supervised native modal-handling refinement: PASS, `scripts/ui run tests/ui/scenarios/commit-owned-cancel.scn`, all four profile/size combinations; focused Rust checks also passed. Removed `:messages` and blind Enter/q; retained final-feedback snapshots before terminal-only conditional hit-enter dismissal, then checked bounded notification history and one begin/report/end for the actual `Git`/`commit` token. Fresh artifacts (local filesystem clock 2026-10-28 08:12:38–46) were inspected under `build/ui/commit-owned-cancel/{standard,vim}-{100x30,80x24}/`: `committing.screen` shows `Committing…`, `cancellation-feedback.screen` shows the readable Git failure, `cancelled.screen` retains staged changes, and `next-commit.screen` shows `Committed: After progress cancellation` with a clean tree. Each `cancel.trace.jsonl` records the unique token `magit-lsp/mutation/3`, one cancellation lifecycle, the notification check and scenario completion. Scenario assertions also passed unchanged HEAD, hook TERM and the subsequent commit without lock cleanup. Per-run intent→report / intent→end: standard-100x30 **0.677 / 40.852 ms**, vim-100x30 **0.891 / 36.696 ms**, standard-80x24 **0.644 / 28.587 ms**, vim-80x24 **0.711 / 29.166 ms**. These are client-trace progress timings, not key-to-idle or universal deadlines. The following full `make test` FAILED only at `tests/test_magit_lsp.lua:518` (visual-line staging, cached diff empty); exact Lua output is preserved in `build/test-logs/lua.VDSuAF` (55 cases, 1 failure). Subsequent `git diff --check` passed and the orchestrator found no owned process survivors. No production fast-path, grace or deadline changes were made in this retry.
    5. Final adviser-guided synchronization correction: the old completion predicate matched `+NEW line` already present in the initial **unstaged** hunk, so it could read Git before native `V`, `s` finished. Wait instead for `HELPERS.find_line('+', 'NEW line', 'staged') ~= nil`, whose section range comes from server `documentSymbol`, then assert the existing settled `HELPERS.wait_idle()`. Native input and the exact cached `+NEW line`, absent cached `line3 CHANGED`, and remaining-worktree assertions are unchanged. No sleeps, timeout inflation, version-only predicate or product changes.
       - [x] Single authorized final gate: `make test-file FILE=tests/test_magit_lsp.lua && make test && git diff --check` exited **0 on its only attempt**: focused Lua, full Rust/Lua/VSCode/keybindings and whitespace checks all passed. Output is retained in `/tmp/opencode/r-edit-2-final-4qi85mqn/verification.log`. The fresh four-variant screens/traces from attempt 4 were re-inspected without rerunning unrelated UI. Post-gate process audit found no owned server/editor/hook/UI survivors; pre-existing unrelated hooks, editor and tmux processes were untouched. All pre-existing work outside the three ticket files was byte-for-byte preserved.
  - **Remaining R-EDIT-2 gaps (Linux acceptance):** canonical pre-hook/editor-active/commit-msg/finalizing/completed outcomes and finish-v-cancel races; complete adversarial foreground/background/inherited-pipe/TERM-resistant coverage with exactly-once release and exact HEAD/index/worktree checks; lock-warning and no-foreign-kill obligations. macOS group/WNOWAIT/reap validation is separately deferred to R-EDIT-9; Windows feasibility is separate R-EDIT-7 work. This cooperative Linux refinement and a passing regression gate do not complete R-EDIT-2, the managed-draft adapters or Phase 3E.
- [ ] **R-EDIT-3 — Versioned draft and validated writeback**
  - **Candidate paths:** `src/editor.rs`, `src/protocol.rs`, `src/server.rs`, `src/session.rs` (edit dispatch), `src/git.rs`, `tests/editor_bridge.rs`; not an exhaustive rewrite list.
  - **Depends on:** R-EDIT-2.
  - **Done when:** opaque URI and sole standard LSP didOpen/didChange/didClose sync, exact owner/version/target/path checks and exact write-before-helper-ok; save never confirms; normal user close confirms the saved checkpoint, while explicit abort/disconnect cancel (canonical native close/dirty-save/shutdown guards in §3.10); errors retain draft/helper.
  - **Verify:** `cargo test --test editor_bridge`; stale/late/version/write failures, multi-root, disconnect/races; exact repo and neighboring content.
- [ ] **R-EDIT-8 — Server-owned commit-message history ring**
  - **Candidate paths:** `src/editor.rs`, `src/protocol.rs`, `src/server.rs`, `src/session.rs`, `src/commands.rs`, `tests/editor_bridge.rs`, `tests/commands.rs`; profile binding files only if existing history-prev/history-next bindings need wiring. Do not add client history policy.
  - **Depends on:** R-EDIT-3 for validated run/owner/version draft replacement; integrate before A11/R-EDIT-6 cutover. R-EDIT-4/5 clients only display editable server replacements.
  - **Done when:** bounded (default 50 entries; R-EDIT-1 chooses UTF-8 byte cap and overflow policy without silent truncation) in-memory rings are isolated by canonical repo; successful nonempty submitted and confirmed messages are retained, including Git failures, with exact consecutive dedup. Template-only means unchanged exact initial-template snapshot; do not strip comments or infer message meaning. Exclude per-keystroke/autosave drafts, secrets/askpass, and body logs. Previous/next preserve unsent draft on first previous, restore it at newest, clamp without wrapping, and replace editable draft only after owner/run/version validation—never save/finish/commit. No Git-log population, persistence/config subsystem, or client-owned history.
  - **Verify:** `cargo test --test editor_bridge --test commands`; cover two-root isolation, ordering/dedup/eviction, UTF-8 bound/overflow, unsent-draft restoration, stale owner/run/version rejection, and no implicit save/finish; native R-EDIT-4/5/A11 scenarios prove recall and return-to-current-draft. No Git-log/persistence implementation.
- [ ] **R-EDIT-4 — Neovim draft adapter**
  - **Candidate paths:** `lua/magit-lsp/edit.lua`, `lua/magit-lsp/init.lua`, `lua/magit-lsp/view.lua`, `lua/magit-lsp/buffer.lua` (only where sync/attachment/capabilities require), `tests/test_magit_lsp.lua`, `tests/ui/scenarios/commit-cancel-progress.scn`, `ui-test-plan.md`; not an exhaustive rewrite list.
  - **Depends on:** R-EDIT-3.
  - **Done when:** native acwrite/scratch syncs versions; save checkpoints only, explicit confirm flushes, normal user close confirms the saved checkpoint, and explicit abort/disconnect cancel (§3.10 defines dirty Save/Discard/Cancel and non-user-close/shutdown guards); status/text-mode unchanged. Native history-prev/next recall and return-to-current-draft behavior is covered.
  - **Verify:** `make test-lua`; `scripts/ui run tests/ui/scenarios/commit-cancel-progress.scn`; inspect screens/state and stop owned UI, including native history recall/return-to-current-draft behavior.
- [ ] **R-EDIT-5 — VSCode draft adapter**
  - **Candidate paths:** `clients/vscode/lib/edits.js`, `clients/vscode/lib/edit.js`, proposed `clients/vscode/lib/drafts.js` (writable provider if needed), `clients/vscode/package.json`, `clients/vscode/test/test.js`, `clients/vscode/test/integration.js`, `clients/vscode/test/e2e/suite.js`; not an exhaustive rewrite list.
  - **Depends on:** R-EDIT-3.
  - **Done when:** thin writable provider syncs exact versions; save never confirms; confirm barrier flushes; normal user close confirms the saved checkpoint, explicit abort/disconnect cancel (§3.10 defines dirty Save/Discard/Cancel and non-user-close/shutdown guards); close/recovery safe and status readonly. Native history-prev/next recall and return-to-current-draft behavior is covered.
  - **Verify:** `make vscode-test`; extend `clients/vscode/test/integration.js` for real-server/scripted-widget integration (`npm --prefix clients/vscode run test:integration`), and `clients/vscode/test/e2e/suite.js` for actual native VSCode confirm/cancel/progress/close/version-error and history recall/return-to-current-draft scenarios (`npm --prefix clients/vscode run test:e2e`). The former is not native-client evidence.
- [ ] **A11 / R-EDIT-6 — Migrate A11 and adversarial cutover**
  - **Candidate paths:** `src/editor.rs`, `src/runner.rs`, `src/git.rs`, `src/server.rs`, `src/session.rs` (dispatch), `src/protocol.rs`, `lua/magit-lsp/edit.lua` and relevant init/view/buffer attachment/capabilities from R-EDIT-4, `clients/vscode/lib/edits.js`, `clients/vscode/lib/edit.js`, proposed `clients/vscode/lib/drafts.js` if needed, `tests/editor_bridge.rs`, `tests/test_magit_lsp.lua`, `clients/vscode/test/test.js`, `clients/vscode/test/integration.js`, `clients/vscode/test/e2e/suite.js`, `tests/ui/scenarios/commit-cancel-progress.scn`, `ui-test-plan.md`; not an exhaustive rewrite list.
- [ ] **R-EDIT-9 — macOS CI/platform acceptance (deferred, not Linux release blocker)**
  - [x] Documentation-only acceptance split (2026-10-28): Linux gates and native guidance defer macOS execution here; R-EDIT-2 remains unchecked and no macOS validation is claimed.
  - **Candidate paths:** CI workflow configuration, `src/runner.rs`, `src/runner/owned.rs`, `tests/runner.rs`, `tests/runner_hooks.rs`, `tests/editor_bridge.rs`, `tests/server_async.rs`, `TASKS.md`, `ui-test-plan.md`; choose concrete CI paths when this later task starts.
  - **Depends on:** R-EDIT-1 design and Linux R-EDIT-2 acceptance; R-EDIT-6 for managed-draft native cutover acceptance on macOS. No reverse dependency from Linux R-EDIT-2/R-EDIT-6. Intended for a later macOS CI runner; unavailable local macOS execution is not a Linux blocker. Windows remains R-EDIT-7.
  - **Done when:** retained macOS CI artifacts identify runner image, macOS/architecture, Rust/Git versions and exact commands/results; real-Git tests prove pre-exec session/group ownership, non-reaping WNOWAIT exit observation with PID/PGID pinned through full TERM/CONT grace and final group KILL, sole final reap and no signals after reap. Cover leader-exits/TERM-resistant descendant, stopped member, exit-before-cancel/no signal, normal-exit daemon survival, inherited-pipe bounded drain, exactly-once release, unchanged HEAD/index/worktree on abort, subsequent commit without lock cleanup, lock warning and no foreign kills. Trace observation → cancellation → TERM/CONT → full grace → KILL → reap; retain native evidence for both clients before claiming macOS cutover acceptance. Full-grace fallback stays; Linux `/proc` latency evidence is not macOS evidence. Until these gates pass, macOS remains explicitly unverified.
  - **Verify:** on the macOS CI runner, `cargo test --test runner --test runner_hooks --test editor_bridge --test server_async`, plus the R-EDIT-6 native acceptance commands for macOS client cutover; retain process traces, runner metadata and native artifacts. This is a future verification requirement, not an execution claim.

  - **Depends on:** Linux R-EDIT-2 acceptance, R-EDIT-3…R-EDIT-5 and R-EDIT-8; sole A11 implementation owner. R-EDIT-9/macOS execution is not a Linux client-cutover dependency.
  - **Done when:** remove realFile commit-message compatibility at cutover; Git keeps hooks/cleanup. Native acceptance covers save-stays-editing, normal close commits exactly saved message, dirty Save/Discard/Cancel, no hidden changes submitted, explicit abort versus close, disconnect across phases, stale/terminal cleanup without commits, and failed close recovery. Also cover pre-hook/editor-active/commit-msg/finalizing and finish-v-cancel (cancel before commit aborts; completed commit reports reality), stale/late/version/multi-root, exact HEAD/index/worktree and neighboring content, inherited pipes/bounded drain, TERM resistance, subsequent commit/no lock cleanup, lock warning/no foreign kills; preserve askpass/A29/text mode and document limits. Extend integration.js and native e2e/suite.js at the paths above; scripted widgets alone do not meet native acceptance. R-EDIT-8 verifies two-root ring ordering/dedup/eviction and current-draft restoration.
  - **Verify:** `cargo test --test editor_bridge --test server_async && make test-lua && make vscode-test && npm --prefix clients/vscode run test:integration && npm --prefix clients/vscode run test:e2e && make test && scripts/ui run tests/ui/scenarios/commit-cancel-progress.scn && make ui-tour` on Linux; inspect relevant tour excerpts. Linux real-Git and both clients' native evidence are required; macOS remains unverified under later R-EDIT-9 and does not block Linux acceptance/cutover. No timeout inflation.
- [ ] **R-EDIT-7 — Windows feasibility (deferred, not Unix release blocker)**
  - **Paths:** `src/editor.rs`, `src/runner.rs`, `src/protocol.rs`, `TASKS.md`.
  - **Depends on:** R-EDIT-1 design; separate from Unix cutover.
  - **Done when:** named pipe, restrictive ACL and atomic suspended Job assignment feasibility/limitations documented; no guarantee claimed without evidence.
  - **Verify:** explicit platform design review/go-no-go; no test command claimed.
Shared design is canonical in §3.10. Unix owned group/session and tty decision,
bounded TERM/KILL/output, finite drains, no indefinitely blocked detached reader,
no signals after reap, exactly-once terminal/progress/edit/config release,
no broad kills or lock deletion; linked-worktree gitdir/common-dir lock probes
advisory only. Graceful helper abort precedes TERM/KILL; pipe deadlines are
independent of child exit. Crash/native recovery limits are explicit in §3.10.
BUG-READ-1 remains
deferred.

### Phase 4 — Read views

- [x] R-5.1 **First feature to implement: log (graph, decorations, filters,
      lazy loading) and reusable select mode** (§3.7). Implement the
      server-rendered log with visible selection purpose, cursor
      and structured commit identity; confirm/cancel; native editor navigation
      and search (search keys must not be swallowed by capture); and push/focus,
      return-to-caller lifecycle. Confirm returns the selected commit to the
      pending operation; cancel returns without mutation. This infrastructure
      is a dependency of target-dependent actions in R-4.1 and later reset,
      rebase and cherry-pick workflows. Selection must not depend on future
      commit/rebase mutation implementations.
      *Tests:* Rust/protocol acceptance tests for purpose rendering, structured
      identity (no text parsing or row indices), native search/input forwarding,
      confirm result, cancel without mutation, and restoration of the caller.
      *UI:* add `log-select`: open a selection log, search/navigate, confirm
      and assert the expected commit identity reaches a test operation and the
      caller is restored; repeat with cancel and assert no mutation and correct
      caller restoration. The scenario demonstrates selection independently of
      commit/rebase mutation support.
      *UI:* add scenario `log-view`: `l l` opens the log, the screen shows the
      commit subject, `q` returns to status.
       R-5.1 correctness findings are tracked in the [R-5.1 review](#r-51-logselect-review).
       Verification: `make test`, `make ui-unit`, `make ui-test`, `make ui-tour`,
       and `git diff --check` passed. UI evidence inspected for `log-view` and
       `log-select` across standard/vim at 100x30 and 80x24: decorated subjects,
       graph/ref labels and selection purpose render; generated `build/ui/tour.md`
       reviewed. Test sessions stopped; no orphan `magit-lsp` process found.
- [ ] R-5.2 Revision view; Diff view (ranges, args). In a plain log,
       `common.confirm`/Enter opens the structured commit under the cursor in
       the revision view. In select mode Enter selects as usual; the proposed
       `o` inspection action opens a read-only commit view without selecting or
       mutating, and `q` restores the originating picker/rebase view including
       cursor, marks, and pending operation. These are proposed keys/behaviour,
       not approved or implemented; keep keys terminal-portable and independent
       of `ctrl+Enter`.
       *Tests:* cover the structured confirmed commit, plus session and UI
       restoration of cursor, marks, and pending operation after inspection.
       *UI:* add scenario `revision-view`: from the log, `d` on a commit shows
       its diff and `q` returns to the log.
- [ ] R-5.3 Refs view; file-at-revision virtual docs; log variants.
      *UI:* add scenario `refs-view`: `y` opens refs, the screen lists branch
      and tag names, `q` returns to status.
- [ ] R-5.4 `documentLink` and `hover` for hashes/refs.
      *UI:* extend `log-view`: hashes and refs are styled, and the link/hover
      targets resolve (assert the style in the `.screen` dump).
- [ ] R-5.5 Diff-argument menus applied across views.
      *UI:* add scenario `diff-args`: toggling `-w` in the diff menu re-renders
       the status diff without whitespace-only hunks.
- [ ] R-5.6 Log colourisation. Add server-owned semantic styling metadata for
       commit hashes, branch/tag decorations, headings, author/date, and graph
       elements where appropriate. Keep styling independent from links/hover
       (R-5.4), whose existing scope remains hashes/refs and their targets; do
       not recover style by parsing rendered client text.
       *Tests:* verify semantic spans in both UTF-8 byte and UTF-16 code-unit
       coordinates, including non-ASCII text. *UI:* assert the rendered log's
       colours for these elements in a real-UI scenario.

### Phase 5 — Commit flow

- [ ] R-4.1 Commit menu: commit, extend, amend, reword, fixup, squash, augment,
      instant fixup/squash, alter; switches `--all`, `--allow-empty`,
      `--verbose`, `--no-verify`, `--signoff`, `--gpg-sign`, `--reset-author`,
      option `--author=`. Items flip from `Planned` to `Implemented`. Depends
      on R-5.1 for target-dependent commit actions; normal commit selection is
      the server-owned log select-mode flow, not a client picker.
      *UI:* extend `commit-menu-shows-actions` with switch toggling (`-a` flips
      the `--all` flag) and the `c c` flow; after pressing `c` the screen lists
      `Commit`, `Extend`, `Amend`, `Reword`, `Fixup`, `Squash`.
- [ ] R-4.2 Commit-message view over the editor bridge: staged-diff companion
      view, trailers, lint diagnostics, confirm/cancel/close. Server message-ring
      ownership is R-EDIT-8, not this task; this task integrates the history view.
      **Depends on:** Phase 3E Linux managed-draft cutover and acceptance
      (R-EDIT-1…R-EDIT-6 and R-EDIT-8); macOS CI acceptance R-EDIT-9 and Windows
      feasibility R-EDIT-7 are separately deferred, not Linux blockers.
      *UI:* add scenario `commit-message-view`: staged change, `c c`, a bottom
      split shows the `gitcommit` buffer and the staged diff; confirm closes
      it and the status screen shows a clean tree.
- [ ] R-4.3 Status history sections: **Unpushed** and **Pushed**.
      Keep history as server-owned structured commit nodes with full OIDs;
      reuse section folding/navigation and integrate Enter-to-view with R-5.2's
      revision view. Clients display the rendered history verbatim and never
      reconstruct or parse Git history. This task owns both history sections;
      R-6.4 owns its independent Unpulled section and status header lines, not
      Unpushed.

      Unpushed is initially expanded, showing the newest 20 commits, with any
      older entries in a folded `More` group loaded lazily when expanded.
      Pushed is initially folded whenever a status view is newly opened; when
      expanded it is bounded and lazy-loaded. Preserve each section's user fold
      choice across refreshes (do not forcibly refold it). Classify against a
      resolved remote-tracking baseline: honor the configured push target when
      present, otherwise the upstream, and use that baseline consistently for
      both sections. Do not fetch implicitly or claim pushed/unpushed state
      without a resolvable baseline. With no upstream/baseline, detached HEAD,
      or an unborn repository, show a clearly labelled recent-history fallback
      as appropriate rather than asserting push state.

      *Tests:* cover baseline classification (including configured push target
      versus upstream), more than 20 commits and lazy `More`, default folding
      for newly opened views, preservation of both fold choices over refresh,
      and missing-baseline/detached/unborn fallback behavior. Verify structured
      full-OID commit identity and Enter integration with R-5.2.
      *UI:* add real-UI evidence for both sections, expanding `More` and Pushed,
      and opening a revision with Enter then returning to status; assert the
      rendered screen at each step.

### Phase 6 — Branches, remotes, stash

- [ ] R-6.1 Branch menu: checkout, create, create-and-checkout, spin-off,
      spin-out, rename, reset, delete, configure upstream/pushRemote.
- [ ] R-6.2 Fetch/pull/push menus (`--force-with-lease`, `--set-upstream`,
      `--dry-run`, `--prune`, pull `--rebase`/`--ff-only`) with progress and askpass.
- [ ] R-6.3 Remote and tag menus; reset menu; checkout file from revision.
- [ ] R-6.4 Status: Unpulled section and header lines (Head, Merge, Push,
      Tag). Unpushed history section ownership and its baseline classification
      belong to R-4.3; coordinate the shared status-screen integration without
      duplicating that work.
- [ ] R-6.5 Stash menu (stash, snapshot, apply, pop, drop, branch), list and
      show views, Stashes section.

### Phase 7 — History editing

- [ ] R-7.1 Merge menu; Unmerged section; conflict actions; merge-tool effect;
      continue/abort.
- [ ] R-7.2 Cherry-pick and revert with sequencer state; Cherry view.
- [ ] R-7.3 Rebase menu (onto upstream/push-remote/elsewhere, interactive,
      autosquash, `--update-refs`, `--rebase-merges`), rebase-todo view via the
      editor bridge (`mode: virtual`), in-progress section. Non-interactive
      edits (reword, drop, fixup, modify) via a scripted `GIT_SEQUENCE_EDITOR`.
- [ ] R-7.4 Integrate remaining commit-picker workflows via the R-5.1 select
      mode infrastructure (including commit selection across rebase,
      cherry-pick and reset); do not defer the core picker/log infrastructure
      to this integration task.

### Phase 8 — Extras

- [ ] R-8.1 Blame (view + inlay hints/code lens in real files).
- [ ] R-8.2 Bisect, worktrees, submodules, patches, notes, ignore, run command.
- [ ] R-8.3 File dispatch (log/blame/diff/stage for the current file from any
      editor buffer).

### Phase 9 — Polish and forks

- [ ] R-9.1 Theming docs; section layout customisation; optional full-line
      background tinting.
- [ ] R-9.2 Performance: large-repo budgets, diff size limits, node-level
      incremental rendering if profiling demands.
- [ ] R-9.3 Multi-repo, repository list, worktrees as separate sessions.
- [ ] R-9.4 Windows paths/CRLF, packaging, published VSCode extension and
      Neovim plugin.
- [ ] R-9.5 `docs/FORKING.md`; optional jj fork (semi-hard).

### Known limitations → where fixed

| Limitation | Task |
| --- | --- |
| Renames/copies show only new path; no hunk staging | R-0.2, R-0.4 |
| Binary files expose no hunks | R-0.4 (whole-file only stays) |
| No-trailing-newline lightly tested | R-0.4 |
| Paths with spaces/special characters | R-0.2 |
| `stage_all` skips untracked | R-0.3 |
| Hard-coded keys/groups/layout | R-2.2 (keys), R-9.1 (layout) |
| Client/server rendered-format coupling | R-1.1, R-1.6 |

---

## 8. Testing strategy

**Target distribution:** ~80% Rust in-process, ~15% protocol-level (spawned
binary over stdio), ~5% Neovim. The Neovim tests that exist stay as
integration tests; their *intent* for logic moves to Rust.

### 8.1 In-process `TestSession` harness (main tool)

Canonical migration API/acceptance is the P1 `TestSession` task (§12), with
visibility/keymap/effects inspection as needed. Drive the real key/invoke path,
locate cursor targets through tree metadata, snapshot buffers (`insta` or
`expect-test`) and assert repo state with Git. `ScriptedUi` queues text,
confirm, choice and `edit_document(|text| …)` replies.

### 8.2 Determinism

Fixtures set `GIT_AUTHOR_DATE`, `GIT_COMMITTER_DATE`, `GIT_CONFIG_NOSYSTEM=1`,
a temp `HOME`, `XDG_CONFIG_HOME` to a temp dir, `LC_ALL=C`, fixed author and
committer, so hashes are stable and appear verbatim in snapshots. Normalise
only temp paths and timings.

### 8.3 Other Rust layers

- **Patch engine:** `proptest` — random file pairs and random selections;
  `git apply --cached --check` passes; staging everything selected gives the
  expected index; stage-then-unstage is identity; selections containing context
  or header lines never produce a corrupt patch (IMM-2).
- **Fixture matrix:** renames, copies, mode changes, symlinks, binary, CRLF,
  no trailing newline, spaces/unicode/quote paths, unborn HEAD, detached HEAD,
  merge conflicts, in-progress rebase/merge/cherry-pick, worktrees, submodules,
  empty repo, large diffs, changes 4 lines apart and 20 lines apart (hunk
  merging, IMM-5).
- **Keymap:** no duplicate keys within a scope chain; every command has a doc
  and a binding in each profile (R5); manifest snapshot per profile; text-mode
  keymap has only the toggle; workbench deny-list; key-notation round trips
  for every translator.
- **Config:** include order, relative/absolute/`~`/env expansion, optional
  includes, cycle detection, array-replace/table-merge, error messages with
  file and line.
- **Document model:** stable Node IDs across refresh; cursor successor;
  stale-action rejection; lazy bodies not computed while collapsed.
- **Menus:** render snapshots at several widths; switch/option/action flows
  through `ScriptedUi`; `Planned` entries show the message and change nothing.
- **Editor bridge/askpass:** fake `GIT_EDITOR` flow end to end through the
  socket helper; client disconnect aborts Git.
- **Error mapping:** one test per row of the IMM-3 table.

### 8.4 Colour and presentation tests with `:TOhtml` (Neovim)

**IMM-8 findings (Neovim 0.12.5, `make color-spike`):** ordinary extmarks
appear as resolved-group HTML classes (e.g. MagitHeader → Title), but `tohtml`
skips namespaces containing `nvim.lsp.semantic_tokens` (including per-client
namespaces) and warns that LSP tokens are unsupported. Thus HTML is a visual
reference, not semantic-class evidence; use `get_at_pos()` plus `nvim_get_hl`.
`hl_eol` is ignored outside the explicit range.

API: `:packadd nvim.tohtml` in 0.12.5 (built in on 0.10/0.11), then
`require('tohtml').tohtml(winid?, opt?) -> string[]`, with options
`title/number_lines/font/width/range`; `:TOhtml [file]` writes/splits the HTML.
Set `termguicolors` first, disable folds (or `zR`), reapply highlight links
after `:colorscheme` clears defaults. Query `nvim_get_hl` with `link=true` for
targets and `false` for attributes (numeric colours formatted `#%06x`).
Warm terminal-colour caches/drain scheduled warnings before asserting them;
headless uses Normal's GUI background. Width affects fillers, not wrapping.

The fixed added/removed mapping uses `@diff.plus` → Added and `@diff.minus`
→ Removed, not nonexistent built-in `diffAdded/diffRemoved`. Inspect all
attributes: Title in habamax/lunaperche is bold-only, not necessarily broken.
Spike outputs are `build/test-output/status-<scheme>-<background>.html` and
`build/test-output/semantic-tokens.md` (token → link → resolved fg), for
default dark/light, vim, habamax, delek and lunaperche.

Plan:
1. Helper `render_status_html(scheme, background)`: start headless Neovim with
   `scripts/minimal_init.lua`, `:set background=…`, `:colorscheme …`, open the
   status view on a fixture repo, **wait for semantic tokens** (poll
   `vim.lsp.semantic_tokens.get_at_pos()`; extmarks only exist after a draw),
   run `:TOhtml`, write `build/test-output/colors/<scheme>-<background>.html`.
2. HTML → cells: parse the `<style>` block (class → fg/bg/bold) and the spans
   into `{text, fg, bg, bold}` runs. Keep this parser small and tested.
3. Assertions per scheme (use built-in schemes only, no plugins: `default`
   dark and light, `vim`, plus a few shipped ones such as `habamax`, `delek`,
   `lunaperche`; list with `:colorscheme <Tab>`):
   - every semantic token type in the fixture appears in the output;
   - **distinct elements have distinct colours**: added ≠ removed ≠ context;
     section title ≠ file path ≠ hunk header ≠ hash; menu key ≠ menu label;
   - `menu.planned` is visibly dimmer than `menu.label` (lower contrast but
     still above a minimum contrast ratio against the background);
   - no foreground equals the background; contrast ratio ≥ 3:1 for all tokens
     except `planned`/`disabled`;
   - **theme integration:** each `@lsp.type.magit.*` group is a *link*
     (`nvim_get_hl(0, {name = g, link = true})` returns `link`), the link target
     is in the allowed list of §3.15, and the resolved colour **changes** when
     the colourscheme changes (i.e. nothing is hard-coded). The client
     re-applies the links from a `ColorScheme` autocmd.
4. Emit a legend `build/test-output/semantic-tokens.md`: token → link target →
   resolved fg per scheme, so a human can review at a glance. Optionally render
   the HTML to PNG with a headless browser if available (not required).
5. `:TOhtml` does not include semantic-token extmarks (finding 2), so the
   spike's token table comes from `vim.lsp.semantic_tokens.get_at_pos()` and
   `nvim_get_hl`; the HTML is only a visual reference.

### 8.5 Protocol-level tests

A few tests spawn the real binary over stdio: `initialize` capabilities,
`view/open` → `view/render`, `view/key` round trip, ack/resync, prompt request
and response, standard LSP requests (`documentSymbol`, `semanticTokens`).

### 8.6 Neovim integration (kept, few)

Attach and no orphan processes; atomic keymap replace (old keys gone, new
present, nothing between); visual-selection forwarding; fold-key mirroring
(including a user `nnoremap <Tab> za`); `passThrough` of an unbound key;
read-only enforcement (IMM-4); text-mode toggle; error notifications (IMM-3);
prompt effects via stubbed `vim.ui.input`/`vim.ui.select`; the editor bridge
opens the commit-message buffer in the same instance; bottom menu placement.

### 8.7 CI

`cargo test` on Linux/macOS/Windows; Neovim tests on stable and nightly;
manifest-snapshot job; generated VSCode keybindings up to date; run IMM-1 tests
on the oldest supported git version; a `ui-test` job (tmux + Neovim) that runs
`make ui-test` and uploads `build/ui/` on failure (R-UI-6).

### 8.8 Real-UI tests (L3)

L3 drives a real Neovim inside tmux and asserts on the captured screen. It
catches what the other layers cannot: timing, chord timeouts, key encoding,
the command line and real colours.

| Layer | Tool | Use it for |
| --- | --- | --- |
| L1 | Rust `TestSession` | Menu content, pending-chord state, view-stack depth, messages, token attributes |
| L2 | mini.test child Neovim | Window counts, floats, keymaps applied, buffer options, `:messages` |
| L3 | Real Neovim in tmux | What the user sees: timing, chord timeouts, key encoding, the command line, real colours |

- `scripts/ui start <fixture>` builds a deterministic repo (§8.2 environment)
  and opens the client in a dedicated tmux server; `keys`, `wait`, `screen`,
  `state`, `trace` and `stop` form the "inspect, act, inspect" loop described
  in `AGENTS.md`.
- `lua/magit-lsp/debug.lua` exposes `snapshot()` (windows, view stack, pending
  chord, render versions, keymap, messages) and `idle()`; `MAGIT_LSP_TRACE=<path>`
  records per-hop timings for `ui trace`.
- Scenarios are `.scn` files in `tests/ui/` using the DSL in
  `ui-test-plan.md` §A.5. Artefacts land in `build/ui/<scenario>/` as
  `.screen` and `.ansi`.
- **Never sleep:** poll `idle()` with `ui wait`.
- A known bug is an `xfail: <bug id>` scenario. The runner reports XFAIL, and a
  passing `xfail` fails the run until the tag is removed.
- `make ui-test` runs every scenario; `make ui-tour` writes
  `build/ui/tour.md`; `make ui-unit` runs the harness's own Python tests.
- Every menu scenario runs under both profiles (`standard`, `vim`) at 100x30
  and 80x24; `size-only`/`profile-only` opt out for latency and the tour.

---

## 9. Open questions

1. **macOS config dir:** `~/.config` via `etcetera` (proposed) or
   `~/Library/Application Support` via `dirs`.
    -- Use dirs. We should have a root command for editing the config file in the client though.
2. **Editable-scope cancel key** in `standard` (`ctrl+shift+x` proposed).
   Closing the buffer already cancels, so this is a convenience.
   -- Escape should probably cancel most things. For closing the commit message or rebase buffers... unsure. Probably leave it to the client to exit without saving/with code fail?
3. **VSCode menu default:** QuickPick (proposed) vs. text document in a lower
   editor group. Revisit after the Phase 2 spike.
4. **Per-repo config trust:** is `.magit-lsp.toml` worth the risk surface?
   Currently limited to keys and display, behind `trust_repo_config`.
5. **Minimum supported Git version** (decide by running IMM-1 tests on old
   versions).
6. **Full-line background tinting** for diff lines (optional client feature).
  -- yes, good idea. Magit does this, looks great.
7. **LSP 3.18 `workspace/textDocumentContent`** adoption when clients support it.
8. **Fold keys in VSCode** for users with customised `editor.toggleFold`:
   the "mirror my shortcuts" command (proposed) vs. any better mechanism found
   during the spike.

---

## 10. Implementation status and deviations (written after R-2.8)

Historical milestone summary is in §7, immediate contracts in §2 and colour
findings in §8.4. These notes record decisions/deviations, not current-source
proof or a claim that unchecked client/Phase 4–9 work is complete.

| Area | Historical decision/deviation |
| --- | --- |
| Git/patch | Reverse new-file patches use `/dev/null` to avoid empty index entries. Unmerged `u` shows `U` without hunks; no copy detection (`-M`, not `-C`). Binary/mode-only/symlink-type/multi-section-type changes are whole-file only. Metadata: `orig_path/head/upstream/ahead/behind`. |
| Identity/render | Hunk IDs: 16 hex SHA-256 over markers/text, not header numbers; duplicate suffixes, stale-ID warning/render, never guessing. Status text stayed byte-identical; positions use acked trees. Default fully expanded, no fold indicator; `diff_invocation_count` tests laziness. |
| Tokens/visit | Dotted types work; modifiers add attributes above types. Added/removed use `@diff.plus`/`@diff.minus`. Visit is `definition` (deleted files empty). Inline diffs replaced `vcs/*`; historical Lua API: `setup/open/refresh/invoke/error_detail`. |
| Input/config | Visit was status-scoped (confirm's Enter); vim log `g l`, not native `l`. Modified shifted letters allowed. Reload: 150 ms hash poll, not notify. Full-chord lhs incurs `timeoutlen`; visit exception → R-NV-4. Reasoned deny-list exceptions: `ctrl+t`, `ctrl+c`, menu-only `ctrl+s`/`ctrl+r`. |
| Menus/parity | Saved args session-only; `cargo xtask parity` aliases `magit-lsp parity`; reset `X`. Clashing in-progress keys omitted; branch `u/r/p/R/P` are options. Historical inventory: 22 menus, 355 rows, ~191 Planned; manual-key validation → §12. |
| Runner | Deliberately retain synchronous `GitRepo`/`std::process` in `spawn_blocking` rather than API-wide Tokio rewrite without behavioural gain. `RepoLocks` serialises mutations; reads remain concurrent; cancellable work-done progress and bounded invocation log. Thread-local `CancelHandle` kills by PID (`kill -KILL`, no libc), consumes cancellation so failure can refresh. The former debug-delay test (`MAGIT_LSP_TEST_DELAY_MS`) slept *before* the sessions lock was taken, so its pass never proved read responsiveness during a mutation blocked inside real Git; it was removed and replaced by a gated real-Git probe that fails and is ignored as BUG-READ-1 (§7 Bugfix): `invoke_view`/`key_view`/`set_mode_view` still hold `Shared.sessions` across their Git runs, so runner-level read concurrency is not read responsiveness. |
| Process log | Read-only `runner::global_log()`, scope `process-log`, Main placement; status-scoped opener uses normal views. Sole command `process-log.clear`. Monotonic record IDs and `Node.copy = git <args>` support identity-based copy/refresh; no new tokens. |
| Bridge core | `--editor-helper <path>` sends `{editId,path,kind}` on per-process Unix socket, waits for `{ok}`. No session/repo lock during Git's editor wait; create returns immediately, confirm/cancel answers edit ID, worker later sends render/effects. Fake Rust client milestone; real clients/askpass → R-3.4b, full commit flow → R-4.1; other edit kinds not emitted then. |
| Prompt protocol | Notification `view/prompt`, request `view/promptRespond` returning null, avoiding nested client requests. Worker std mpsc wait, not Tokio; sync `CommandCtx` carries `&dyn PromptUi`, convenience session methods use `NullUi`, UI-aware mode variants exist. Reader text/path/date/number/ref/commit use text; Choice empty items + `allowCustom:true`. Cancel: no change/render. `common.quit` on a nested view is a server-side pop to the existing parent; at the root it retains the established close-all behavior. |
| Build footprint | Line-table debug info and disabled incremental builds followed disk exhaustion; keep `target/` small. |

---

## 11. Client reviews (orchestrator, written before R-3.4b landed)

### 11.1 Neovim client (`lua/magit-lsp/`)

Review-time sizes: init 996, menu 564, debug 423, keys 326, edit 224 (in
progress), prompt 193, keymap 194 lines. Findings map to canonical §7 tickets:
Accepted R-NV-2 sizes: init 385, view 1068, menu 98, edit 139, buffer 154,
effects 88, highlights 107; debug 423, keys 326, prompt 193, keymap 194.
The Lua suite, Rust tests, UI unit suite, all 18 UI scenarios, and all 3 tour
scenarios passed. `process-log` and `multiple-views` passed both profiles at
100x30 and 80x24. Inspected screens show the visible `git apply` entry and the
staged status restored after nested quit. The nested-quit regression confirms
the existing parent view and root close-all contract. Full evidence is recorded
in `tmp-task.md`.

1. Blocking key/invoke/open and re-entrant waits → R-NV-1.
2. Triplicated view plumbing and status singleton → R-NV-2.
3. Always-loaded instrumentation (~35 calls) → R-NV-3.
4–7. Mixed responsibilities, cargo-run discovery fallback, visit exception and
hand-maintained token links → R-NV-4. Startup splitting → R-UI-7.
Keep the intentional fold mirroring, pure key translation, atomic keymaps,
prompt adapter and no Git/tree parsing. Acceptance criteria live only in §7.

R-NV-4 resolution (findings 4–7):
- `config.lua` now owns placement validation, project root and the ordered
  server lookup (`setup{cmd}`, `$MAGIT_LSP_BIN`, `target/release`,
  `target/debug`, `magit-lsp` on `$PATH`), plus key-modifier detection and
  client capabilities. There is no `cargo run` fallback; a missing binary is a
  clear `vim.notify` error. `init.lua` only wires sessions and views.
- `common.visit` is implemented on the server: it resolves the acked tree's
  line to a file/new-side line and returns `Effect::VisitFile`. The Lua
  `M.visit`, `visit_key` option, keymap override and visit exception are
  deleted; the client only opens the server-provided URI/line.
- `highlights.lua` keeps only `OVERRIDES` and modifier links. The semantic-token
  legend carries `tokenTypeGroups` (suggested theme group per token type), so a
  new token type needs no Lua edit; the client applies it from `on_init`.
- VSCode's `package.json` theme contributions are deliberately retained:
  VSCode cannot consume arbitrary per-token legend suggestions, so its own
  theme mapping stays. It does handle the new `visitFile` effect.
- New UI scenarios `visit-file` (single window, replace) and
  `visit-file-split` (split keeps status visible) drive the real editor.
Sizes after R-NV-4: init 385, config 107, view 1111, menu 98, edit 139,
buffer 154, effects 133, highlights 100, keymap 178; debug 442, keys 326,
prompt 193.

### 11.2 VSCode extension (`clients/vscode/`)

At review time `extension.js` was 1197 lines, with only NOTES and pure-helper
tests. Build/launch/package documentation, integration testing and modularity
requirements live in R-VS-1 (§7); current resolution evidence is in §12.

### 11.3 UI harness review and bugfix

- [x] **[P1] Diagnose Neovim harness startup/socket failure** (`scripts/ui`, `scripts/ui_lib/`)
  - **Problem:** Neovim does not open the expected `build/ui/runs/.../nvim.sock` socket.
  - **Change:** diagnose process startup, socket path creation, permissions and connection ordering. Keep harness startup/socket diagnosis separate from scenario behavior: a scenario assertion failure is not evidence of socket failure, and a socket failure is not a scenario behavior regression.
  - **Done when:** the harness reports a clear startup/socket outcome and can connect to the expected socket; scenario behavior is independently tested by `make ui-test`.
  - **Done (2026-10-26):** root cause found: the socket path was 74 bytes (limit 107) and its permissions were fine; the failure was the launcher command. `_nvim_command` put the whole inherited environment (≈19 KB in the Nix development shell) on the `tmux respawn-pane` command line, so tmux failed before Neovim started with `command too long`. `start` now writes `launch-nvim.sh` (cleared env, stderr to `nvim.stderr`, status to `nvim.exit`) and tmux runs only the short script, preserving the environment exactly. Plain `scripts/ui start` now works without the `env -i HOME=… PATH=…` workaround.
  - **Diagnostics:** `scripts/ui_lib/startup.py` reports socket path and byte length against the 107-byte Unix limit, existence/type/mode, pane liveness, recorded exit status, one bounded connect probe and the stderr tail. `driver.StartupError` is raised for startup/socket failures; scenario rows set `ComboResult.startup` and print `STARTUP FAILED (harness/socket, not scenario behaviour)` instead of an `expect:` failure. An overlong socket path is rejected before tmux starts.
  - **Evidence:** an injected fake `nvim` (exit 3, stderr `fake nvim failed`) produced: `socket … (74 bytes, limit 107); missing`, `nvim pane: exited`, `nvim exit status: 3`, `connect probe: not attempted: no socket to contact`, `nvim stderr (tail): fake nvim failed`; no orphan tmux session remained. `make ui-unit` passed 164 tests (new `tests/ui/test_startup.py` plus launcher/report/startup-label cases in `test_driver.py`/`test_scenario.py`); `make ui-test` passed 25 PASS / 0 XFAIL / 0 FAIL without `env -i`; `make test` passed the full Rust suite, 53 Neovim cases, 23 VSCode tests and 164 generated keybindings. `ui-test-plan.md` §A.1.1 documents the launcher and report.
  - [x] **Scoped diagnostic follow-up:** PTY reproduction confirmed that an RPC helper launched with terminal stdin and without `--headless` enters the TUI, emits terminal control queries, and routes the remote-expression result to stderr. All RPC helper invocations now use `--headless` and stdin from `/dev/null`; the actual tmux Neovim remains interactive. Failure-only reports retain the last idle result and bounded client busy/view/message state. The readiness condition is unchanged.
  - [x] **Scenario shell follow-up:** Direct scenario RPC expressions use `nvim --headless`; `sh:` steps use `/dev/null` stdin and convert expected nonzero exits, timeouts and launch errors into per-combination failures while stopping the run and continuing the matrix.

---

## 12. Review backlog

### R-5.1 log/select review

- [ ] **[P1] Return raw source for structured diff yanks** (`src/commands.rs`; `src/tree.rs`; public copy/invoke seam)
  - **Problem:** Diff copy must yield code rather than rendered patch decoration, with exact structural identity even for literal marker characters.
  - **Why it matters:** Copying whole hunks, lines and visual selections should produce usable source without parsing display text or changing repository state.
  - **Change:** Implement whole-hunk, single-line and visual-selection raw-source yanks as server-owned structured-tree/content operations returning clipboard effects. Strip exactly one structural leading diff marker (`+`, `-`, or context space); preserve remaining code, indentation, literal `+`/`-` characters and line order; exclude file/hunk headers and no-newline annotations. Do not reconstruct old/new file semantics or add bindings beyond existing profile bindings.
  - **Done when:** Rust tests exercise the public command/request seam for added, removed and context lines, literal marker characters, indentation, multiple lines, and excluded metadata; none mutates Git. A scripted real-UI scenario verifies whole-hunk, single-line and visual-selection copy output.
  - **Constraints:** No client parsing of rendered text; use structured nodes/content. Never mutate Git. No new keybindings.

- [x] **[P1] Return readable log-open failures without desynchronizing the view stack** (`src/session.rs:934-958,1268-1273,1338-1340,1574-1599`)
  - **Problem:** Log creation and argument validation use `?` outside the command error-to-effect handling. Entering an invalid `=n` value or an invalid grep regex and choosing `l` therefore produces a request error, not a readable message effect. The menu has already been removed server-side, but its pop effects are lost with the error.
  - **Why it matters:** Correctness and the command-effects contract; the client can retain a menu that no longer exists on the server.
  - **Change:** Handle expected argument/Git failures for `log.current` and selection-log creation as successful `InvokeResult`s containing readable `Effect::Message`s (use the existing Git failure mapper). Make failed menu-triggered opens transactional: preserve the menu and its arguments for correction, or deliver all necessary pop/focus effects with the failure. Cover log refresh failures in the command path too; `log.load-more` already maps Git errors.
  - **Done when:** Real `view/key`/`view/invoke` tests exercise invalid menu page size and invalid grep regex, assert a successful message-effect response, no new log, and matching client/server view stacks; a valid retry works. Do not substitute a direct `open_with_args().unwrap_err()` test.
  - **Constraints:** Keep internal programming faults as request errors; do not alter unrelated commands or implement later log variants.

- [x] **[P1] Require acknowledged identity before positional selection actions** (`src/commands.rs:824-829,999-1004`)
  - **Problem:** Confirm and toggle-mark fall back to the latest server tree when no tree has been acknowledged. If the picker renders v1, refreshes to a differently ordered v2 before the initial acknowledgement, and receives a cursor still addressing v1, these handlers can copy or mark the new occupant of that row.
  - **Why it matters:** Correctness: commit selection must use the client's acknowledged structured identity, never guess from a newer row layout.
  - **Change:** For positional confirm/toggle-mark, require `acked_tree()` and return a readable retry/resync message when it is absent. Continue resolving a stale acknowledged tree by full OID and reject an OID no longer loaded, rather than using its former row in the current tree.
  - **Done when:** Selection tests cover both an unacknowledged initial picker followed by a reordered refresh (no clipboard/mark side effect) and an acknowledged old tree followed by an unacknowledged reordered refresh (select the old structured OID, or refuse if it disappeared). Exercise both key and invoke entry points.
  - **Constraints:** Keep this fix scoped to log/select handlers; do not redesign the general acknowledgement protocol.

- [x] **[P1] Make the implemented path filter reachable through real menu keys** (`src/menus/log.rs:23,28`, `src/keyhandler.rs:70-89`)
  - **Problem:** The new `=path` option cannot be typed one key at a time: `=` then `p` completes the existing planned `=p` first-parent item. The menu matcher also does not concatenate the remaining `a`, `t`, `h` characters, so no native client can reach the advertised path filter.
  - **Why it matters:** Correctness: a filter marked Implemented is inaccessible in the actual menu workflow.
  - **Change:** Assign the path filter a free, supported two-step option key, update its translation in `create_view`, implemented-option allowlist, tests and generated snapshots. Prefer this over adding a new ambiguous multi-character flag grammar.
  - **Done when:** A session/UI test opens the log menu, sends individual canonical key steps to the path option, answers its prompt, opens `log.current`, and verifies only commits touching the literal selected path appear.
  - **Constraints:** Preserve the planned first-parent key and literal-path semantics; do not implement R-5.3 options as part of this fix.

- [x] **[P1] Apply log-menu filters to the selection action** (`src/commands.rs:800-820`, `src/session.rs:1268-1273`)
  - **Problem:** The log menu passes its active options to `log.select-and-copy`, but that handler ignores them and selection creation always queries default `GitLogOptions`. An active author/message/path/page-size filter or disabled graph/decorations silently disappears when choosing the implemented selection action instead of `l`.
  - **Why it matters:** Correctness: the supported menu arguments and preview do not describe the selection log that opens.
  - **Change:** Carry typed log options in `SelectionRequest` and initialize `LogView` with those options. Share the existing menu-argument-to-log-options conversion with `log.current`, while retaining direct invocation's selection settings (`purpose`, `prompt`, `multi`, `allowFreeText`).
  - **Done when:** A real menu-key test sets a supported filter and toggles graph/decorations, chooses `c`, and verifies the picker honors those settings and still returns the selected full OID to its caller. Existing direct-invocation selection tests pass.
  - **Constraints:** No client-side filtering, new selection consumer, or later-phase mutation implementation.

- [x] **[P1] Emit log token ranges in bytes before LSP conversion** (`src/log_view.rs:268-298`, `src/semantic_tokens.rs:123-135`)
  - **Problem:** `commit_node` calculates author/date positions and lengths with `.chars().count()`, but the shared semantic-token encoder interprets those fields as UTF-8 byte offsets before converting them to UTF-16. Non-ASCII subjects, ref names or authors shift or truncate the emitted highlight ranges.
  - **Why it matters:** Correctness: the new log renderer violates the existing token-coordinate contract for ordinary Unicode commit metadata.
  - **Change:** Calculate all log `Token.start`/`length` values from UTF-8 byte lengths of the actual rendered segments; let the existing encoder perform the only UTF-16 conversion. Add deterministic renderer/encoder coverage with non-ASCII decorations, subject and author, including a supplementary-plane character.
  - **Done when:** Tests assert the decoded semantic ranges select the exact hash, decorations, full author and full date from the rendered row, in both graph and multi-selection rendering.
  - **Constraints:** Do not change the shared token representation/encoder or implement R-5.4 links/hover.

- [x] **[P1] Disable color in the machine-readable Git log query** (`src/git.rs:227-242,284-316`)
  - **Problem:** The new query does not override Git color configuration. With `color.ui=always` or `color.log=always`, graph output contains ANSI escape sequences; the parser's first-hex-digit heuristic then sees digits in those escapes instead of the full OID and discards commit records or renders escape bytes.
  - **Why it matters:** Correctness: supported user Git configuration can make a nonempty repository appear to have no selectable commits.
  - **Change:** Explicitly request no color for this structured `git log` invocation rather than stripping ANSI sequences or weakening OID validation. Add backend coverage with forced-color Git configuration and a branching/merge graph.
  - **Done when:** Forced-color and ordinary queries return the same full commit OIDs and ordered graph rows; rendered text contains no ANSI escapes.
  - **Constraints:** Override presentation only for this machine-readable query; do not modify user/repository configuration or unrelated Git commands.

<!-- Retained for existing links to the former unnumbered heading. -->
<a id="review"></a>

### VSCode extension review (immediate queue for this request)

- [x] **[P0] R-VS-5 — Keep Magit editor bindings out of foreign widgets**
  - **Resumed permission (2026-10-24 16:25 NZDT):** user confirmed they are
    away and explicitly permitted testing now, superseding the historical pause
    below; do not wait until 16:30. One circuit-breaker retry only, after repairing
    find focus and actual-renderer screenshot capture; stop on verification failure.
    Only a unique isolated development host/profile may be touched.
  - **Verified single resumed run:** exact ticket chain exited 0; 21 Node tests,
    Rust tests and 61 Neovim cases passed; real Neovim UI reported 16 PASS,
    1 XFAIL (`process-log`: `wait timed out for screen text 'Process log'`),
    0 FAIL. No failed-chain retry was needed. Evidence:
    `/tmp/opencode/vscode-review-l9j8d4/r-vs-5-evidence.json`,
    `bindings-evidence.json`, `mirrored-bindings.json` and actual-renderer PNGs.
    Screenshots and request trace were inspected: foreign QuickPick, InputBox
    and Find retain physical typed input/Enter plus mirrored Ctrl+Alt+] with
    zero Magit requests; return to status restores Enter, Tab, mirrored folding
    and `g` refresh; Magit commit QuickPick forwards `-`, `a`, `q` to the server.
    Find setup uses `actions.find`, not claimed physical Ctrl+F evidence.
    Rename/suggest are unsupported in this read-only no-provider fixture and
    have unit guard evidence only; foreign Tab/F5 are not claimed. Exact input,
    request counts and screenshot names are recorded in `clients/vscode/NOTES.md`.
    Physical F5 separately produced no request/render; its cause is unproven.
    R-VS-3 is since checked with its F5 limitation recorded; R-VS-4 has since
    been verified with actual theme measurements; R-VS-2 is verified separately.
    Only the unique isolated host was stopped; no orphan server remains.
  - **Original finding:** editor bindings swallowed Enter in foreign widgets.
  - **Scope:** require editor text focus in canonical generated bindings and
    mirrored fold bindings; preserve Magit-owned QuickPick handling and generic
    programmatic invocation. Verify foreign widget keyboard input and restored
    status Enter/fold/refresh in actual VSCode/VSCodium.
  - **Remaining gate:** recorded automated/real-client regression evidence for
    the full foreign-widget matrix and status Enter/fold/refresh. Ctrl+P Enter
    is user-confirmed working; do not keep treating that symptom as unresolved.
  - **Status (2026-10-24 14:43 NZDT): paused with R-VS-5 unchecked.** The
    automated gate landed: canonical status `enter`, `tab`, `g` and `f5` plus
    mirrored fold bindings require `editorTextFocus` and exclude Quick Input,
    suggest, find and rename widgets (21 Node tests pass). Two isolated client
    runs verified the foreign QuickPick and InputBox cases with zero Magit
    dispatch, then stopped at the find-widget case. A stopped diagnostic probe
    showed physical `ctrl+f` opens the widget but the input did not take focus
    under synthetic X11 input, while `actions.find` did focus it; this is a
    test-driver problem, not evidence of a binding regression. Live client
    testing is paused by the user: do not launch an editor or run
    `npm --prefix clients/vscode run test:integration` until the user confirms
    they are away (expected today 16:30 NZDT). Details and evidence paths are
    in `clients/vscode/NOTES.md`.

- [x] **[P0] R-VS-2 — Fix virtual status opening blank or as a temporary file**
  - **Finding:** opening status first displays a blank editor; opening it a
    second time shows only a temporary filename instead of the status view.
  - **Change:** trace the initial open/render/virtual-document lifecycle in the
    VSCode extension and fix it so a server-owned, read-only status document is
    rendered on the first open and remains correctly associated with its view.
    Do not require creating a corresponding file on disk; preserve the server's
    text and structure verbatim.
  - **Regression tests:** cover opening status from a fresh VSCode window and
    assert the first render is displayed, then reopening/refreshing status and
    asserting the rendered status remains visible rather than a temporary
    filename or blank document. Tests must not create a backing status file.
  - **VSCode UI verification:** launch the extension in an actual Extension
    Development Host, open status in a repository, and verify the first screen
    contains the server-rendered status; open it again and verify status text
    still appears with no temporary filename-only editor.
  - **Done when:** regression tests pass and the described behavior is verified
    in the actual VSCode UI without any status document on disk.
  - **Verified (2026-10-24):** exact ticket chain exited 0 (21 Node tests, 300
    Rust tests, 61 Neovim cases, real Neovim UI 16 PASS / 1 unchanged XFAIL /
    0 FAIL). Isolated Extension Development Host evidence:
    `/tmp/opencode/vscode-review-K6aS1m/` (`r-vs-2-evidence.json`,
    `r-vs-2-first-open.png`, `r-vs-2-reopen.png`, `r-vs-2-refresh.png`).
    First open matched the received server render on `magit://v1/Status` with a
    `Status` tab; sequential plus two concurrent opens sent no additional
    `view/open` and kept one tab/URI/text; `g` refreshed the same URI and made
    `+after-return` visible. Workspace files were unchanged before/after; no
    `repo/Status` file or file-scheme document existed. All three screenshots
    were inspected. No commit was created; R-VS-5 evidence was preserved.
  - **Status:** user confirms status appearance and duplicate-open are fixed;
    regression and actual-host evidence gates now pass. R-VS-3 has since been
    checked with its F5 host limitation recorded; R-VS-4 has since been checked
    with its theme/token gate.
- [x] **[P0] R-VS-3 — Expose generic server commands and restore defaults**
  - **Resolution:** repository bindings use `magit.invoke` with canonical server
    IDs; missing `magit.refresh` / `magit.toggle-fold` came from external
    edamagit/user binding conflicts. The user confirms "That was the issue."
    Do not add per-action aliases or reopen the resolved root-cause question.
  - **Change:** provide one generic, argument-taking VSCode dispatcher for
    arbitrary server command IDs (rather than proliferating one VSCode command
    per server action), and provide working default shortcuts for server fold
    toggling and refresh. Dispatch through the existing server command
    mechanism and retain server-owned folding/state.
  - **Regression tests:** test that the generic dispatcher forwards an
    arbitrary command ID and its arguments to the server; verify the default
    fold and refresh shortcuts are contributed and invoke the corresponding
    server commands in an active status view.
  - **VSCode UI verification:** in an actual Extension Development Host, use
    the default fold shortcut to toggle a status section and the refresh
    shortcut to update status; invoke a different server command through the
    generic dispatcher and verify it reaches the server. Confirm no
    per-action VSCode command registrations are required.
  - **Done when:** regression tests pass and all three interactions are
    verified in the actual VSCode UI.
  - [x] **Audit/regression hardening:** build, generated-binding check, npm
    tests, `make test` and `make ui-test` passed with a minimal inherited
    environment (164 bindings, 61 Neovim cases, 13 real-Neovim UI scenarios).
    Node checks cover the registered dispatcher/envelope, opaque args and
    omitted args; Rust validates contributed payloads against the registry.
    Generation now defaults to `cargo run` (explicit `MAGIT_LSP_BIN` override).
    This is not the remaining actual-VSCode UI acceptance evidence.
  - **Verified (2026-10-24):** exact ticket chain exited 0 (22 Node tests, 300
    Rust tests, 61 Neovim cases, real Neovim UI 16 PASS / 1 unchanged XFAIL /
    0 FAIL). Isolated Extension Development Host evidence:
    `/tmp/opencode/vscode-review-wdkMOI/` (`r-vs-3-evidence.json`, request
    traces, 20 PNGs, `f5-host-input-repro.txt`, `f5-host-input-xev.log`).
    Gates: `magit.invoke` is the only generic dispatcher and all 164 contributed
    bindings use it with canonical server IDs; the manifest contributes no
    per-action command (no `magit.refresh`, `magit.toggle-fold`,
    `magit.show-section`); `common.hide-section` / `common.show-section` with
    opaque `{recursive:true}` folded and restored the server text unchanged;
    `status.stage-all` through the dispatcher staged `tracked.txt` visibly and
    `status.unstage-all` restored the repository; physical `Tab` folded and
    restored the status section (plus mirrored `ctrl+alt+]`); physical `g`
    refreshed the same server-owned view and showed `+after-return`.
    Physical F5 is **not claimed**: this host's input stack delivers it as
    Alt+F5, so it matched no plain `f5` binding and sent no request, while the
    `g` control in the same focus rendered immediately. The identical F5
    contribution and the `standard` profile's `refresh = ["g", "F5"]` are
    recorded; `g` is Magit's canonical refresh key and satisfies the refresh
    gate. All inspected screenshots, counts and the verbatim F5 limitation are
    in `clients/vscode/NOTES.md`. No product shortcut, context key, extension
    source or user configuration was changed; no commit was created.
  - **Status:** checked on the recorded evidence above with the F5 host
    limitation explicit. R-VS-4 has since been checked for its theme/token gate.
- [x] **[P0] R-VS-4 — Restore semantic highlighting and theme integration**
  - **Finding:** VSCode status views have no colours or formatting.
  - **Change:** restore the LSP semantic-token path and VSCode theme
    integration for server-provided token types, using contributed semantic
    token types/scopes or the appropriate VSCode-supported mappings. Keep
    token ranges and meaning server-owned; do not parse or reformat rendered
    text in the extension.
  - **Regression tests:** assert semantic tokens are requested/applied for a
    rendered view and that the extension contributes mappings for the status
    token types, including section headings and added/removed diff lines.
  - **VSCode UI verification:** run the extension in an actual Extension
    Development Host with a theme, open a status view containing section
    headings and added/removed lines, and verify the corresponding text is
    visibly styled and follows the active theme.
  - **Done when:** regression tests pass and the actual VSCode UI confirms
    semantic highlighting and theme integration.
  - **Verified (2026-10-24):** exact ticket chain exited 0 (23 Node tests, 300
    Rust tests, 61 Neovim cases, real Neovim UI 16 PASS / 1 unchanged XFAIL /
    0 FAIL). Isolated Extension Development Host evidence:
    `/tmp/opencode/vscode-review-JQ3MsM/` (`r-vs-4-evidence.json`,
    `r-vs-4-theme-dark.png`, `r-vs-4-theme-light.png`,
    `r-vs-4-theme-dark-restored.png`). The host requested the server's
    `textDocument/semanticTokens/full` 6 times for the server view URI; the
    decoded server ranges identified section, added and removed spans, whose
    actual computed styles were measured. Dark Modern: default `rgb(204,204,204)`,
    sections `rgb(78,201,176)`, deletions `rgb(206,145,120)`, additions
    `rgb(181,206,168)`. Light Modern: default `rgb(59,59,59)`, sections
    `rgb(38,127,153)`, deletions `rgb(163,21,21)`, additions `rgb(9,134,88)`;
    restoring Dark Modern returned each exact dark value. The server text was
    identical before, during and after the switch and equalled the received
    `view/render`. The unit test asserts the status token mappings and declared
    types; no `package.json` change was needed. The theme repaint is
    asynchronous, so the driver waits for painted spans instead of a fixed
    delay. All screenshots were inspected. No commit was created.

#### VSCode resolution and evidence (2026-10-24)

Human evidence: Ctrl+P Enter works; status appearance and duplicate-open are
fixed; the commit menu works quickly. Refresh/fold command-not-found was the
identified external edamagit/user binding conflict, confirmed by the user.
These are resolved functional findings, not proof of automated acceptance.
R-VS-2, R-VS-3, R-VS-4 and R-VS-5 now have recorded
regression/actual-host evidence; R-VS-4's theme/token gate was measured from
rendered spans across a dark/light theme switch. R-VS-3's physical-F5 attempt is
a recorded host limitation, not a substituted or claimed success.
Keep the client thin: server text/structure verbatim, generic invocation.

Earlier runs hit tmux's inherited-environment command-length limit; the
sanitized chain later passed as recorded under R-VS-3. Earlier isolated VSCode
runs timed out on picker/render/token/F5 waits; the later R-VS-2/R-VS-3/R-VS-4/
R-VS-5 runs completed and their evidence is recorded in `NOTES.md`. The isolated
physical-F5 interception remains unproven, distinct from the user-confirmed
external conflict. The read-only audit changed no user config or editor
processes. `clients/vscode/NOTES.md` retains raw audit paths/run
details; its earlier uncertainty about the user's conflict is superseded by
this confirmation, not a new open root-cause investigation.

### Test-suite review (static read; tests were not run)

Scope: `tests/*.rs`, `tests/*.lua`, `tests/ui/**`, `tests/snapshots/*`,
`tests/common/mod.rs`, in-source `#[cfg(test)]` modules, `Makefile`, CI. Before
starting, run `make test && make ui-unit && make ui-test` and
`pgrep -af magit-lsp` to get a baseline. Order: do the "Do first" tasks before
the migration tasks that depend on them. File:line references are from the review
date; re-find by function name if lines moved.

Completed P1 review work (historical tracking, not new verification):
- [x] **Make the editor-bridge cancel test able to fail:** non-empty message
  cancellation preserves HEAD/index and reports abort; confirm checks server
  title/mode/scope. Real-file metadata correction was user-authorized (§3.10).
- [x] **Make IMM-2 proptests apply selections and check the index:** shared
  property uses `RepoFixture`, ordered application, an independent expected
  index and reverse identity; empty selections report no apply (≤64 cases).
- [x] **Extract one shared stdio test client:** `tests/common/stdio.rs` owns
  spawn/framing/initialize/notifications/stderr/bounded shutdown for async,
  tokens, text-mode, bridge and stdout-hygiene tests; per-file semantics stay.

- [x] **[P1] Build the §8.1 in-process `TestSession` and migrate the session tests** (`tests/views.rs:14`, `visibility.rs:14`, `keys_protocol.rs:33`, `text_mode.rs:28`, `menu.rs:34`, `menus.rs:33`, `commands.rs:117`, `prompts.rs:35`, `process_log.rs:38`, `commit_preflight.rs:29`)
  - **Problem:** `open_status`, `keymaps()`/`session_with()`, `line_with_marker`, `node_at`, and `press`/`invoke` wrappers (with a hard-coded `version: 1`) are copied in about ten files.
  - **Change:** add `tests/common/session.rs` with `TestSession::open_status(&RepoFixture, Profile)`, `press(key)`, `press_visual(start,end,key)`, `invoke(cmd)`, `invoke_args(cmd,args)`, `ack()`, `goto_marker(path, marker)`, `goto_text(path, text)`, `text()`, `tree()`, `focused_node()`, and a `version` field that tracks the last render so keys send the real version. Migrate the listed files one per commit; delete their local copies.
  - **Done when:** `grep -c "fn open_status" tests/*.rs` is 0 for in-process files; no test sends a literal `version: 1`.
  - **Constraints:** keep behaviour of each test; use tree nodes (`line_nodes`, `marker`), never rendered text, to find cursor lines.

- [x] **[P1] Test that the askpass secret is never logged** (`tests/editor_bridge.rs:484-525`)
  - **Problem:** §3.6/§3.10 say the secret must not be written to any log; the askpass test sends `s3cret` but never inspects the server's stderr or log file.
  - **Change:** with the shared `TestServer` (after the stdio-client task) spawn with trace logging to stderr and a second run with `MAGIT_LSP_LOG_FILE`; after the round trip assert neither contains `s3cret`.
  - **Done when:** the test fails if the answer is passed to `tracing::debug!`.
  - **Constraints:** do not print the secret in assertion messages.

- [x] **[P1] Add missing real-UI scenarios** (`tests/ui/scenarios/`)
  - **Problem:** §7's rule "every task that opens a new view adds a scenario" and "UI-affecting tasks are not done until a `.scn` drove the UI" is unmet for: R-3.3 process log view; R-3.1 menu option prompt; staging/unstage/fold/visual/refused-edit basics (R-1.x/R-2.4); the R-UI-5 unstaged-only confirm ("Nothing staged. Stage all changes and commit?"). `no-readonly-warning.scn` only checks no W10 and never proves an edit is refused.
  - **Change:** add `process-log.scn` (stage a hunk, open the process log with `$`/`g $`, expect `git apply` on screen, `q` returns to status), `option-prompt.scn` (`c`, `=A`, answer the prompt, expect `--author=` on screen), `status-basics.scn` (stage `s`, unstage `u`, `Tab` fold, visual `V s`, then a native edit like `J` leaves the screen unchanged), `commit-confirm.scn` (`c c` with unstaged only, answer `n` then `y`). Under both profiles and sizes unless an opt-out is justified in a header comment.
  - **Done when:** `make ui-test` passes with the four new scenarios and writes their `.screen` files.
  - **Constraints:** no `sleep`; use `wait`. Do not add wall-clock assertions.

- [x] **[P1] Test `view/resize` reflow** (no test exists; `src/session.rs`, `src/menu.rs`)
  - **Problem:** R-2.7 says menu layout is "re-flowed on `view/resize`"; `grep resize tests/` is empty.
  - **Change:** in `tests/menu.rs` (or the new `TestSession`) open the commit menu at 140 cols, send the resize through the real session/server entry point to 60, and assert the resulting render's text (after applying edits) equals `menus::named("commit").render(Viewport{cols:60,..})`. Add one stdio check that the `view/resize` notification yields a `view/render` with `baseVersion`.
  - **Done when:** removing the resize handler fails the test.
  - **Constraints:** reuse the existing snapshots, add none.

- [x] **[P2] Fix the vacuous R4 check and add rule tests** (`tests/keymap.rs:113-127`)
  - **Problem:** the forbidden set contains `"gg"` but canonical chords are space-separated (`"g g"`), so it never matches. Only whole chords are compared, so a `g g` or `ctrl+d <x>` chord is not caught. Rules R2, R3, R6 have no test.
  - **Change:** use `"g g"`; also compare `chord.steps()[0]` for single-key motions; extract the check into a function and add an injected-key negative test (like `tests/manifest.rs:216`). Add small tests: standard `common.confirm=Enter`, `common.cancel` contains `Escape`; `ctrl+c` is cancel in every non-editable scope (R6).
  - **Done when:** adding `g g` to the vim profile (temporarily) fails the suite.
  - **Constraints:** do not edit the shipped profiles.

- [x] **[P2] Resolve the spec/impl mismatch on detail levels** (`tests/visibility.rs:232-331`, §5.1)
  - **Problem:** §5.1 defines levels 1-4 and `alt+1..4`; implementation and test have three levels (AGENTS says 1-3).
   - **Decision:** retain four levels: section (staged/unstaged) → files → hunks → individually visible hunks. Align §5.1, shipped profile bindings and tests with this model.
  - **Done when:** spec text, profile bindings and test agree on all four visible levels.
  - **Constraints:** no change to unrelated visibility behaviour.
  - **Verified (2026-10-24):** focused visibility/keymap tests, clean-environment `make test` (64 Neovim cases), `make ui-test` (19 PASS, 0 FAIL), and VSCode generated bindings check (164 bindings) passed. The detail-level scenario sends real `M-3`/`M-4` keys for standard and `z m`/`z r` for Vim. Inspected `build/ui/detail-levels/{standard,vim}-{80x24,100x30}/hunk-{headers,bodies}.screen`: headers retain `@@` without bodies; expanded screens show `+line3 CHANGED`. Full verification output: `/tmp/opencode/detail-levels-verification.log`.

- [x] **[P2] Make `PARITY_KEYS` authoritative, not circular** (`tests/menus.rs`)
  - **Problem:** the in-test list was not a complete/independent audit. A prior draft checked this item prematurely while the tests still used `PARITY_KEYS`.
  - **Change:** TSV now records each static action's literal key, upstream Magit command and local command; tests validate every static action to data and data back to action, including blame mode-map evidence. Negative tests inject missing, orphan and duplicate rows. Narrow documented state aliases cover rebase `s`/`e` and bisect `B`; reset `X` is corrected as a top-level prefix, not a menu suffix.
  - **Done when:** all 192 occurrences in 22 menus are audited and the requested focused/full tests pass.
  - **Constraints:** no production menu changes; provenance is pinned to Magit v4.7.1 commit `659f89955cf60fe3d4326d881c412df06c69680d`.
  - **Verified (2026-10-24):** `cargo test --test menus` passed (8/8), including bidirectional validation over 192 rows and injected missing/orphan/duplicate cases. `env -i HOME="$HOME" PATH="$PATH" make test` passed: full Rust suite and Neovim integration suite (64 cases). The initial focused run found the draft's count of 194 was inaccurate; runtime definitions and TSV both contain 192, corrected here and in the audit. No production menu changes.

- [x] **[P2] Add tests for `Unavailable` and scope-mismatch replies** (`src/session.rs:1115-1131`)
  - **Problem:** neither the `Unavailable` branch nor `"Not available in this view"` has a test.
  - **Change:** in `tests/commands.rs` invoke `status.stage` against a menu view and assert the info message and no repo change. For `Unavailable`, add a test-only registry seam or a unit test of the branch with a fake command.
  - **Verified (2026-10-24):** the real status-stage invocation against the real commit menu asserts the exact Info effect and preserves menu/view state, HEAD, index tree, worktree status and file bytes. A private dispatcher test injects an Unavailable command with a panic handler and checks the exact reason/level, no render/open/commit, and unchanged view/repository state. Temporarily bypassing each guard made its focused test fail as expected; guards were restored. `cargo test --test commands`, `cargo test --lib` and `env -i HOME="$HOME" PATH="$PATH" make test` passed.
  - **Constraints:** no production-only test hooks beyond a `#[cfg(test)]` seam.

- [x] **[P2] Test the acked-version rule and send real key versions** (`tests/views.rs`, `tests/navigation.rs`)
  - **Problem:** §3.1 says positional requests resolve against the last acked tree; every test queries a fresh tree, and `view/key` is always sent with `version: 1`.
  - **Change:** a real stdio protocol test renders structurally distinct v1/v2 trees, checks actual `definition`/`documentSymbol` handlers still return v1 while v2 is unacked, then verifies both switch after the v2 ack. In-process stale-key coverage sends an observed old version at its old cursor row after that row's node disappears; it asserts the readable refusal and independently verifies the new occupant remains unstaged. Navigation helpers locate targets from tree metadata, and ordinary TestSession key helpers retain observed render versions.
  - **Done when:** making positional requests use the latest tree fails the test.
  - **Constraints:** use tree nodes; do not parse rendered text or hard-code request versions.
  - **Verified (2026-10-24):** `cargo test --test views --test navigation` and `env -i HOME="$HOME" PATH="$PATH" make test` passed. Coverage includes actual stdio positional handlers before/after ack and stale-key repository-state assertions.

- [x] **[P2] Test `keymapDelivery` `static`/`forwardAll` and `keyModifiers`** (`src/server.rs:316,461,1157,1267,1304`)
  - **Problem:** only `"dynamic"` is exercised (`tests/text_mode.rs:338`).
  - **Change:** stdio tests: with `static`, no `view/keymap` is pushed on `view/open` or config reload; with `forwardAll`, an unbound key replies `passThrough`. For `keyModifiers: "basic"`, test the consumer or delete the stored field.
  - **Done when:** changing the gating condition fails a test.
  - **Constraints:** use the shared `TestServer`.
  - **Verified (2026-10-25):** shared-TestServer stdio coverage proves static delivery suppresses open/reload keymaps behind an ordered reload barrier, dynamic delivery receives the changed keymap, and `forwardAll` passes through an unbound key while a bound key still stages. Removed the unused stored `ClientCaps.key_modifiers` field; the accepted capability parser and debug logging remain, but there was no consumer. `cargo test --test server_async && env -i HOME="$HOME" PATH="$PATH" make test` passed.

- [x] **[P2] Cover cross-scope key precedence** (`tests/keymap.rs`)
  - **Decision:** a specific view binding may shadow an inherited common binding,
    even when it names a different command. The former blanket conflict claim
    for `status.stage=Enter` versus inherited `common.confirm=Enter` was stale;
    conflicts are only for distinct commands at equal precedence. This preserves
    the shipped status `Enter` visit and selector `Enter` confirm behavior.
  - **Change:** assert actual lookup ownership for the different-command shadow,
    a view-scoped rebind of `common.confirm` with no leakage, the shipped Enter
    behavior in both profiles, and readable equal-precedence conflict diagnostics.
  - **Done when:** those resolution cases are asserted.
  - **Verified (2026-10-25):** `cargo test --test keymap && env -i HOME="$HOME" PATH="$PATH" make test` passed. Tests assert actual lookup ownership for scoped shadowing, non-leaking `common.confirm` rebinding, shipped visit/confirm Enter behavior in both profiles, and equal-precedence conflict diagnostics.

- [x] **[P2] Fill the config test gaps** (`tests/config.rs`)
  - **Problem:** untested: the `initializationOptions` layer order; `default_config_path()` under `XDG_CONFIG_HOME`; the `$VAR` form; nested includes resolving against the including file's directory; `.magit-lsp.toml` limited to keys/display; parse errors naming a line (`config.rs:143-149` checks only the file). `set_var("HOME", …)` at lines 91-92 races other tests.
  - **Change:** add one test per gap. Replace the `set_var` calls by running the case through `ConfigService`/CLI with an explicit env, or by a `serial_test` guard.
  - **Done when:** each listed behaviour has a test; no `std::env::set_var` remains in `tests/config.rs`.
  - **Constraints:** do not change config semantics.
  - **Verified (2026-10-25):** `cargo test --test config && env -i HOME="$HOME" PATH="$PATH" make test` passed (19 config tests, full Rust suite, 64 Neovim cases). Child-process env covers HOME/XDG and independent `$VAR`/`${VAR}` expansion without parent env mutation; nested relative includes and exact parse-error file/line are asserted. Initialization overrides user profile/trust/keys; trusted repo keys override initialization keys, repo diff/sections override user display settings, and repo profile/trust overrides are ignored. Removed the unsupported menu-persistence restriction assertion; production semantics are unchanged.

- [x] **[P2] Choose and test the recommended Git version** (`README.md`, `.github/workflows/ci.yml`)
  - **Policy:** Git 2.30.8 is recommended, not a runtime minimum. Older Git is not blocked, but compatibility is not guaranteed. The runtime only emits a tracing warning after a completed failure when its actual Git is below this recommendation; it never blocks startup or successful operations. The best-effort probe is capped at 500 ms, kills/reaps its own timed-out child, and does not register with operation cancellation.
  - **Validation:** `RECOMMENDED_GIT_VERSION` is consumed by the runtime and pinned-source test script. The CI container builds exact 2.30.8 from the kernel.org archive after checking SHA-256, asserts both executable and helper paths, then runs `git_backend`, `status`, and `patch_cases`, including the rename-with-modifications hunk regression. The current-Git full suite is a separate CI job.
  - **Historical limitation:** A prior exact Git 2.19.6 run passed all 38 `git_backend` tests but failed `patch_cases::rename_with_modifications_stages_hunks_and_unstages` with `g.txt: already exists in index`. This is retained as a known failure, not a compatibility claim.
  - **Local verification (2026-10-25):** `scripts/test_recommended_git.sh && cargo test --test git_version --test git_backend --test errors && env -i HOME="$HOME" PATH="$PATH" make test` exited 0. Unmodified Git 2.30.8 source archive SHA-256: `62e95f694f28ba4f35bae1d0d19328093ac4783dc5b9c84e2e3d5eca0ab064e9`. The selected executable was `/tmp/opencode/git230/install/bin/git`, helpers `/tmp/opencode/git230/install/libexec/git-core`; backend 38, status 15, patch cases 7 (including renamed-file hunks) passed. Current Git 2.56.0 passed the full Rust suite and all 64 Neovim cases. Hosted CI was not run here.
  - **Build environment/evidence:** Exported `RECOMMENDED_GIT_CFLAGS='-I/nix/store/a76rmqrp6l3w6h9pppfwzb3s6dzi0y4g-zlib-1.3.2-dev/include'` and `RECOMMENDED_GIT_LDFLAGS='-L/nix/store/fkcbg2c1w29jr5yp9awai9w3v1wvxdk9-zlib-1.3.2/lib -Wl,-rpath,/nix/store/fkcbg2c1w29jr5yp9awai9w3v1wvxdk9-zlib-1.3.2/lib'` before that command. Full successful output: `/tmp/opencode/git230/verification-final.log`. Initial attempts exposed test assertions expecting original stderr in the RPC summary instead of its readable error detail, then quoted display-field logging; their exact outputs remain in `verification.log` and `verification-retry.log`. These assertions were corrected before the passing gate.
  - **Verified locally (2026-10-25):** `cargo test --test editor_bridge && env -i HOME="$HOME" PATH="$PATH" make test` exited 0. The negative abort-mutation control is separate from the passing gate (deliberately restored; the passing gate excludes any modification).

- [x] **[P2] Test that a client disconnect aborts Git** (`tests/editor_bridge.rs`)
  - **Problem:** §3.10 step 7 and §8.3 name this case; no test.
  - **Change:** start a commit, receive `editDocument`, close the client's stdin/exit, and assert the Git child ends and no `git commit` is left.
  - **Done when:** the test passes and fails if the abort is removed.
  - **Constraints:** bounded wait, no fixed sleep.
  - **Verified (2026-10-25):** `cargo test --test editor_bridge && env -i HOME="$HOME" PATH="$PATH" make test` exited 0 (7 bridge tests, full Rust suite, 64 Neovim cases, no orphan processes). The test `a_client_disconnect_aborts_the_edit_so_git_never_hangs` starts a commit, waits until both the `git commit` process and its editor helper are alive, closes the client stdin, and asserts a bounded natural exit of the server and both Git-owned processes with HEAD and the staged worktree unchanged. Process identity uses the per-server unique bridge socket path plus edit id (not edit id alone), and kill-identity is re-verified before any cleanup guard fires. The server aborts every pending edit at `exit` and at stdin EOF via `EditRegistry::abort_all` (§3.10 step 7). Negative control: with both abort calls temporarily removed the test fails as expected (the server and Git commit hang until the 20 s deadline); the abort logic was restored and the positive gate re-run to confirm.

- [x] **[P2] Test the Neovim prompt mapping for every kind** (`lua/magit-lsp/prompt.lua`, `tests/test_magit_lsp.lua:1444`)
  - **Problem:** only `text` via `vim.ui.input` is covered; `confirm`, `choice` (±`allowCustom`), `multiChoice`, `secret` are not (§8.6).
  - **Change:** one table-driven L2 case that stubs `vim.ui.select`/`vim.ui.input`/`vim.fn.inputsecret`, sends one `view/prompt` per kind and checks the `view/promptRespond` payload shape and `cancelled`.
  - **Done when:** each kind's response shape is asserted in one case.
  - **Constraints:** do not add more Lua cases than this one.
  - **Verified (2026-10-25):** `env -i HOME="$HOME" PATH="$PATH" make test` exited 0 (full Rust suite, 65 Neovim cases, 0 failures). The one added case `every prompt kind answers view/promptRespond with its §3.6 shape` drives 17 scenarios through the real `view/prompt` notification handler with a per-run fake capturing client: `text` (typed value/empty value/cancel, with and without a `default`), `confirm` (yes/no/cancel; destructive focuses No first with custom labels), `choice` (fixed id/cancel), `choice` with `allowCustom` (typed custom value/cancelled custom input), `multiChoice` (preselect/toggle on/toggle off/Done, empty selection Done, cancel), `secret` (typed/empty/aborted passphrase via `inputsecret`, never through `vim.ui.input`). Each row asserts the wire payload shape and `cancelled`, the restored prompt counter, and the representative input options, select items/steps/labels or secret titles; the synthetic secret is redacted before any equality check so it never reaches failure diagnostics. No other Lua case was added.

- [x] **[P2] Cover option clearing and saved args** (`tests/menu.rs`, `tests/config.rs`, `tests/ui/scenarios/option-prompt.scn`)
  - **Problem:** "pressing a set option again clears it" (§3.11) and `view/open` `args.saved_args` have no test; `menus.save_scope` is parsed but untested.
   - **Change:** set `=A`, press `=A` again with a `ScriptedUi` that has no reply queued, assert value `None` and no prompt. Add a `saved_args` open test. Defer any VSCode scenario and treat it as drift until it matches the real server.
  - **Done when:** each case is asserted.
  - **Verified (2026-10-25):** `env -i HOME="$HOME" PATH="$PATH" make test` exited 0. `tests/menu.rs`: `pressing_a_set_option_again_clears_it_without_prompting` presets `--author=Alice`, presses `=A` again with no `ScriptedUi` reply queued, asserts the option value is cleared to `None` with no prompt sent, and `opening_a_menu_with_saved_args_initializes_and_restores_the_arguments` drives the `args.saved_args` round-trip. `tests/config.rs`: `obsolete_menus_save_scope_is_rejected_with_its_exact_name` asserts the setting is rejected with its exact name (`main.toml:4: unknown setting \`menus.save_scope\``). `tests/ui/scenarios/option-prompt.scn` exercises the real Neovim UI: `=A` answers the author prompt, expects `--author=Alice` on screen, presses `=A` again while set, expects `--author=Alice` gone and no prompt, with a `cleared` snapshot.

- [x] **[P2] Guard VSCode keybinding drift and run node tests in `make test`** (`clients/vscode/package.json`, `Makefile`, CI)
  - **Problem:** nothing compares `package.json#contributes.keybindings` to `magit-lsp keymap --format vscode`; node tests are outside `make test`/CI (§8.7).
  - **Change:** add a check (the existing `gen-keybindings` script with `--check`, or a Rust test reading `package.json`) and a `make vscode-test` target run in CI. Overlaps R-VS-1; coordinate.
  - **Done when:** editing a profile without regenerating fails CI.
  - **Constraints:** do not hand-edit generated keybindings.
  - **Verified (2026-10-25):** `make vscode-test` exited 0 (23 node tests; `gen-keybindings.mjs --check` reports `ok 164 keybindings`) and `env -i HOME="$HOME" PATH="$PATH" make test` exited 0 (full Rust suite, 65 Neovim cases with 0 failures, then the same 23 node tests and drift check). `Makefile` adds phony `vscode-test` running `npm --prefix clients/vscode test` and `node clients/vscode/scripts/gen-keybindings.mjs --check`, added to `make test` dependencies. The CI `test` job sets up Node 22 and `npm ci --prefix clients/vscode` before `make test`, so a stale manifest fails CI on its own. Editing a profile without regenerating flips the `--check` exit to 1 with the "contributes.keybindings is stale" message. README documents the local Node/npm prerequisites and the locked dependency setup (`npm ci` as the only supported dependency setup). No editor launches; no generated keybindings were hand-edited.

- [x] **[P2] Replace Lua sleeps with an idle wait** (`tests/test_magit_lsp.lua:329,419,695,706,794,966,989,1008,1046,1065,1085,1142,1157,1191`; poll loops at `413-420`, `960-967`, `984-990`, `1002-1009`, `1040-1047`, `1059-1066`, `1079-1086`)
  - **Problem:** `vim.loop.sleep` and copy-pasted `for _ = 1, 40 do … sleep(50)` loops contradict "never sleep, poll `idle()`"; `type_edit` reads `v:errmsg` after a fixed sleep.
  - **Change:** add `HELPERS.wait_idle()` (poll `require('magit-lsp.debug').idle()` with `vim.wait`) and `HELPERS.wait_for(fn)`; replace every sleep/loop.
  - **Done when:** `grep -n "loop.sleep" tests/test_magit_lsp.lua` is empty; `make test-lua` passes 5 consecutive runs.
  - **Constraints:** do not change assertions.
  - **Verified (2026-10-25):** `HELPERS.wait_for(fn)` (documented, bounded `vim.wait` with a 20 ms interval, default 5000 ms) and `HELPERS.wait_idle()` (polls `require('magit-lsp.debug').idle()`) were added in the child `HELPERS` table; all 14 `vim.loop.sleep` calls and all seven copied `for _ = 1, 40/100` sleep-poll loops were replaced — expected-state waits for renders (`has_line`), fold collapse (line count), file opening (buffer name) and text-mode edits; violation watches (bounded `wait_for` on the failure state) give the real input-processing barrier for the "unchanged" assertions that read `v:errmsg` (`type_edit`, `:write` twice, planned `<CR>` visit) before reading or asserting unchanged state, and `type_edit` drains `HELPERS.wait_idle()` after the refusal so a mapped command's round trip cannot leak. `grep -n "loop.sleep" tests/test_magit_lsp.lua` is empty, no fixed delays or busy-spin loops remain, all 65 cases and every existing assertion are unchanged (no production code touched), and five consecutive `env -i HOME="$HOME" PATH="$PATH" make test-lua` runs each reported 65 cases with `Fails (0) and Notes (0)`; `env -i HOME="$HOME" PATH="$PATH" make test` exited 0. Changes left uncommitted alongside the other in-flight work.

- [x] **[P2] Make `type_edit` assert what IMM-4 says** (`tests/test_magit_lsp.lua:324-341`, `621-657`)
  - **Problem:** it passes both when the key succeeds and when it fails with E21, so a key that silently edits nothing passes. `dd`/`x` from IMM-4 are skipped because they are bound.
  - **Change:** each attempt now asserts the full buffer text is unchanged and `modified=false`; keys unbound in the active profile must set E21 (cleared `v:errmsg` before input, bounded waits, no sleeps). Mapped no-ops (`u` on the header) assert no E21; `c` opens the commit view, so native `ciw` is exercised with mappings bypassed (`feedkeys 'n'`). The `dd`/`x` deviation is documented in the helper header. Non-vacuity: a temporary, fully restored modifiable mutation makes a bypassed native `x` succeed, so the guard itself demonstrably fails before the read-only suite continues. Case count unchanged (65).
  - **Done when:** making the buffer modifiable fails the test.
  - **Constraints:** no new Lua cases.
  - **Verified (2026-10-25):** corrected MiniTest `fail_reason` options; native keys expand termcodes and execute without mappings. The modifiable negative control edits, snapshots and restores buffer/options in one protected child call without yielding or retrying; its caught failure identifies the unchanged-buffer guard. `env -i HOME="$HOME" PATH="$PATH" make test-lua && env -i HOME="$HOME" PATH="$PATH" make test` exited 0 on the first verification attempt: both Lua runs reported 65 cases, Fails (0), Notes (0); Rust tests, 23 VSCode tests and 164 generated keybindings passed. Unexpected verification failures: 0. The unrelated Nix jscpd/pulp bootstrap failed before the commands, which still ran successfully.

- [x] **[P2] Force overlap in the mutation-ordering test** (`tests/server_async.rs`)
  - **Problem (bounded scope):** `mutations_for_one_repository_apply_in_request_order` passed even without serialisation because both mutations were fast, and the read-concurrency test existed only under `cfg(debug_assertions)` via `MAGIT_LSP_TEST_DELAY_MS`.
  - **Change (bounded scope):** reuse the fake-git `add` hook (shared fixture, see C9 task) so the first mutation blocks until released; send the second; assert the second's `$/progress` `begin` follows the first's `end`; run the coverage in both debug and release.
  - **Done when:** removing the repo lock fails the ordering test.
  - **Constraints:** bounded waits only.
  - **Completed (2026-10-25) — bounded scope only; the read-responsiveness half moved to BUG-READ-1 (§7 Bugfix, unchecked).**
    The shared gated fixture landed as `tests/common/fake_git.rs`: a tests-only fake `git`
    with an explicit entered/release handshake (the gated child writes its pid to `entered`
    and blocks; the test creates `release`; bounded polls, no fixed sleeps and no debug-only
    delay), scoped to the server child's `PATH` only, whose `Drop` always releases and then
    verifies (never kills a foreign, possibly recycled pid) its own gated child. The fixture
    now also backs `a_cancel_notification_aborts_a_slow_mutation` (the `sleep 30` fake and
    `fake_git_env`/`wait_for_file` are gone), and the debug-only
    `MAGIT_LSP_TEST_DELAY_MS` test was removed, so the coverage runs in `--release`.
    `mutations_for_one_repository_apply_in_request_order` arms the gate on `add`: it waits
    for the first mutation's `create`/`begin` plus the entered marker (first mutation
    verifiably inside real Git), submits `status.unstage-all` only then, releases, and
    records every progress token/phase; it asserts the exact stream
    `create/begin/end` ×2 with two distinct tokens (second `begin` strictly after first
    `end`) and the repository result (index empty again, worktree still changed).
    Negative controls (temporary `src/server.rs` edits, fully reverted — `git diff` empty):
    bypassing both `MutationTurn.wait` **and** the repo lock fails the test with
    `[create t1, begin t1, create t2, begin t2, end t1, end t2]`, i.e. the second begin
    precedes the first end; bypassing only the repo lock still passes, because the turn
    gate alone also serialises — so the original "removing the repo lock fails" gate was
    only reachable via the both-bypass control.
    Verified (2026-10-25): `cargo test --test server_async && cargo test --release
    --test server_async && env -i HOME="$HOME" PATH="$PATH" make test` exited 0
    (14+14 async tests including the new ordering test in both profiles, full Rust suite,
    65 Neovim cases, 23 VSCode tests, 164 keybindings; no orphan processes).
  - **Split (2026-10-25):** the unmet real-Git read-responsiveness requirement and its
    evidence moved to **[BUG-READ-1]** (§7 Bugfix, still unchecked, probe retained
    failing and `#[ignore]`d). This checked item covers only the forced mutation
    overlap/order, the negative controls and the debug+release coverage; the green
    debug+release suites do **not** cover read responsiveness.

- [x] **[P2] Prune cross-layer duplicate tests** (see list)
  - **Problem:** the same behaviour is asserted in several layers: ~12 Lua staging/untracked cases (`tests/test_magit_lsp.lua:395-500, 800-872`) duplicate `views.rs`/`git_backend.rs`/`status.rs`; the 23-line status table in Lua `800-829` duplicates `status_render.rs:102`; `tests/keys.rs` duplicates `src/keys.rs:392-420`; `tests/manifest.rs:86-127` duplicates `src/keys.rs:423-485`; `src/runner.rs:392-420` duplicates `tests/runner.rs:131-169`; `unbind_removes_a_profile_binding` exists in `config.rs:172` and `keymap.rs:219`; Planned-action checks exist in `commands.rs:203`, `menu.rs:296`, `menus.rs:110`.
  - **Change:** keep one Lua stage round trip (`pressing s…`, `visual line range`), the Lua notify case for planned actions, and delete the rest of the listed duplicates. Delete `tests/keys.rs`, the two `*_keys_follow_the_section_3_8_table` tests and one of each duplicated pair above. For Rust/Lua translator drift, share one `tests/data/keys.json` read by `src/keys.rs` tests and `test_keymap.lua`.
  - **Done when:** each behaviour has one primary owner; `make test` passes; Lua case count drops by at least 12.
  - **Constraints:** do not remove the only test of any behaviour; do after the `TestSession` task.
  - **Done (2026-10-25), tests-only.** Neovim case count 65 → 52 (`test_magit_lsp.lua` 59 → 46, `test_keymap.lua` unchanged at 6): −13 ≥ 12. Deleted Lua staging/untracked cases and the 23-line status table all had equivalent Rust coverage; two unique assertions were transferred instead of lost (marked below). `tests/data/keys.json` is now the single §3.8 fixture consumed by `src/keys.rs` cfg(test) (`include_str!`) and `test_keymap.lua` (`vim.json.decode`), holding the union of all former cases (modified punctuation `%`/`$`, `ctrl+Plus`/`meta+PageUp`, chords, invalid canonical + invalid Neovim lhs). Behaviour → retained primary owner:
    - status content render (staged/unstaged/untracked/rename) → `status_render.rs::snapshot_shows_staged_unstaged_untracked_and_a_rename`; clean-tree non-blank message (unique, transferred) → new `status_render.rs::a_clean_tree_renders_a_non_blank_working_tree_clean_message`
    - stage a hunk/diff line → `views.rs::staging_a_hunk_line_sends_edits_and_moves_the_cursor_to_the_staged_hunk`; visual line-range staging stays Lua (`staging a visual line range…`) with `views.rs::a_visual_selection_stages_only_the_lines_it_covers`
    - unstage a hunk → `dispatch.rs::execute_unstage_hunk_unstages_only_that_hunk` (+ `git_backend.rs::unstage_hunk_reverts_staged_hunk`); stage/unstage file and hunk headers → `dispatch.rs::stage_file_header_stages_file` / `stage_hunk_header_stages_hunk` / `unstage_*` + `execute_stage_file_stages_it` / `execute_unstage_file_unstages_it`
    - `S`/`U` stage-all/unstage-all → `views.rs::stage_all_and_unstage_all_run_through_the_view` (+ `dispatch.rs::execute_stage_all_unstaged_stages_tracked_changes` / `execute_unstage_all_staged_unstages_everything`); unborn-HEAD unstage-all → `git_backend.rs::unstage_all_works_without_commits`
    - untracked section render/stage → `status.rs::untracked_files_are_separate_from_unstaged` / `stage_untracked_stages_only_untracked_files` + `dispatch.rs::stage_untracked_section_stages_all_untracked` / `execute_stage_all_untracked_stages_only_untracked` + `git_backend.rs::stage_file_stages_untracked_file`; untracked-file unstage notify (unique, transferred) → new `commands.rs::unstaging_an_untracked_file_reports_nothing_to_unstage`
    - §3.8 key round trips → `src/keys.rs` unit tests + `tests/test_keymap.lua` from `tests/data/keys.json` (single owner pair, one fixture); deleted `tests/keys.rs` and both `manifest.rs::*_keys_follow_the_section_3_8_table` tests (all their cases are in the fixture)
    - process-log ring/truncation → `tests/runner.rs::capacity_evicts_the_oldest_and_ids_strictly_increase` / `oversized_output_is_truncated_per_field` (deleted both `src/runner.rs` cfg(test) duplicates); config `unbind` → `config.rs::unbind_removes_a_profile_binding` (covers TOML parse + `Keymap::resolve`; deleted the `keymap.rs` copy); Planned actions → `commands.rs::planned_commands_report_their_roadmap_and_change_nothing` (registry) + `menus.rs::every_planned_menu_action_reports_its_roadmap_and_changes_nothing` (every menu, Git state) + Lua planned notify (deleted the single-item `menu.rs` copy)
    - Kept unique adapter contracts untouched: read-only/E21, `:write` hint, tokens/theme, prompts, independent views/queues, input forwarding, failed-mutation notify, key-driven `pressing s…` round trip.

- [x] **[P2] Reduce snapshot sprawl and use one snapshot mechanism** (`tests/manifest.rs:22-43`, `tests/parity.rs:119-164`, `tests/menu.rs:146-158`, `tests/menus.rs:527-556`, `tests/snapshots/`)
  - **Problem:** three snapshot mechanisms (`expect_file!`, a custom `UPDATE_SNAPSHOTS` helper twice). `parity` is snapshotted three times (README block, `parity.md` with 355 rows, CLI output). 12 menu layout snapshots pin incidental spacing. Manifest snapshots in four formats pin docs/statuses that change every phase.
  - **Change:** use `expect_file!` everywhere and delete both custom helpers. Delete `tests/snapshots/parity.md` and its test; keep the README-block check. Keep layout snapshots for the commit menu at 60 and 140 only; for rebase/push/diff assert width invariants (no line wider than `cols`, fewer columns at smaller widths, every Planned action has a `magit.menu.planned` token). Keep `manifest-*.json` and `keybindings-*.json` (R-2.5 requires a CI snapshot); drop the markdown and `.neo.json` snapshots.
  - **Done when:** `tests/snapshots/` has ≤ 6 files and one update command (`UPDATE_EXPECT=1`); documented in `README.md`/`AGENTS.md`.
  - **Constraints:** keep the R-2.5 manifest snapshot.
  - **Done (2026-10-25), tests + one small layout fix.** `tests/snapshots/` now holds exactly six files (`manifest-standard.json`, `manifest-vim.json`, `keybindings-standard.json`, `keybindings-vim.json`, `menu-60.txt`, `menu-140.txt`); deleted `parity.md`, both `keymap-*.md`, both `keymap-*.neo.json`, `menu-100.txt` and the nine `menus-*` spacing snapshots. Both custom `UPDATE_SNAPSHOTS` helpers are gone (`tests/manifest.rs` snapshot fn deleted, `tests/parity.rs` update branches deleted); every snapshot comparison is `expect_file!`. The README-block check now runs `expect_file!` against the reconstructed README (prose preserved; `UPDATE_EXPECT=1` rewrites the block in place) and `the_parity_markdown_matches_its_snapshot` is deleted; the neovim/markdown serializers keep structural checks (`neovim_serialises_lhs_entries`, `markdown_renders_one_table_per_profile_and_scope`) plus the existing CLI tests. `menus.rs::the_largest_menus_reflow_and_grey_every_planned_action` asserts, for rebase/push/diff at 60 and 140: every line ≤ `cols` (chars), strictly fewer action columns per line at 60 than at 140, and the sorted planned-token texts equal the sorted planned action labels. `menu.rs::the_commit_menu_reflows_its_action_columns` keeps the 60/140 snapshots (100 dropped) and additionally checks the wrapped line's label/flag/marker tokens.
    - **Small `src/menu.rs` layout fix (authorized):** the known defect (rebase menu at 60 columns rendered its `-d` switch 78 columns wide; the commit menu's `-R` line was 66) is fixed by wrapping rendered argument lines: `wrap_line` breaks at space runs, indents continuations under the label column, hard-breaks words wider than the viewport, and re-bases (or splits) tokens so each still covers exactly its text; wrapped items keep one item node spanning both lines plus a per-line `line_node`. No text, keys, statuses or tree identities changed; `menu-140.txt` is byte-identical, `menu-60.txt` gains exactly one wrapped line.
    - **Real-UI evidence:** new `tests/ui/scenarios/menu-width.scn` (fixture `modified-two-files`, `size: 60x24` so the runner pairs it with 80x24 under both profiles; the 60-column size is justified in the header because the default pair 100x30/80x24 never reaches the wrap point). The menu float is `nowrap`, so the scenario's flag expectations fail unless the server wrapped. Inspected screens: 60x24 shows `  -R  Claim authorship and reset author date` with continuation `      (--reset-author)  [ ]`; 80x24 shows the unwrapped line; artefacts in `build/ui/menu-width/{vim,standard}-{60x24,80x24}/commit-menu-width.screen`. All four combos PASS.
    - **Verified (2026-10-25):** `cargo test --test manifest --test parity --test menu --test menus` exited 0 (15/19/8/5 tests), `env -i HOME="$HOME" PATH="$PATH" make test` exited 0, `env -i HOME="$HOME" PATH="$PATH" make ui-test` exited 0 with `menu-width` passing under both profiles and both sizes. Unexpected verification failures: 2 (`expect_file` mismatch after the layout change, then a `then_some` eager-evaluation overflow in `wrap_line`) — both fixed before the final green runs. The unrelated Nix bootstrap failure of `scripts/ui start` outside `env -i` (command too long) was worked around by running the driver with the same minimal env the Makefile targets use; BUG-READ-1's ignored probe remains explicitly unsolved.

- [x] **[P2] Replace magic counts and interim-state asserts** (`tests/commands.rs:199`, `tests/menus.rs:162`, `tests/parity.rs:82`, `tests/menu.rs:716-742`, `tests/dispatch.rs:583-664`)
  - **Problem:** `== 32`, `> 100`, `> 150` thresholds; asserts that commit keys `e`/`f`/`a` are `Planned R-4.1` (breaks when R-4.1 lands); five `execute_discard_*` tests pin "Not implemented yet" for R-3.6; `needs_confirm: true` appears in every discard row with no `false` case.
  - **Change:** use relational checks (`planned + implemented == total`, "at least one planned action exists"); keep a single discard-deferred test marked `// remove with R-3.6`; drop `needs_confirm` or add a row where it is false.
  - **Done when:** landing R-4.1 or R-3.6 breaks at most one clearly marked test.
  - **Done (2026-10-25):** all named magic counts and phase pins are gone.
    - `tests/commands.rs` replaced `status.count == 32` with the exact §5.2 partition (`status.*` actions plus every `open.*` opener except `open.dispatch`).
    - `tests/menus.rs` replaced `checked > 100` / `expected.len == 192` with a set-based comparison: the sorted, deduplicated command ids pressed across every menu (including the generated dispatch menu) must equal the sorted registry set of planned `Scope::MenuAction` plus planned `Scope::Status` commands, plus a non-vacuous `checked > 0` guard.
    - `tests/parity.rs` replaced `actions > 150` with a derived subset relation: every command the registry exposes as `Scope::MenuAction` must appear in the static menu-definition walk (a walk can also contain opener items registered as `Scope::Status`, so exact equality would be wrong).
    - `tests/menu.rs` replaced the `e`/`f`/`a` hardcoded `Planned R-4.1` assertions with a registry-driven comparison (menu status must equal the registered status for each command, whatever that status is).
    - `tests/test_magit_lsp.lua` dropped the last `R-4.1` reference: the `find_planned_action` helper comment no longer pins the roadmap id, and the planned-token/once-notify cases already pick a real planned action (label/roadmap/key) from server menu metadata rather than a fixed label.
    - The single clearly marked `// remove with R-3.6` discard guard remains in `tests/dispatch.rs:576`; `needs_confirm` cases were consolidated by the previous worker and are preserved.
  - **Evidence:** `cargo test --test commands --test menus --test parity --test menu --test dispatch` exited 0 (5/41/19/8/5 tests), `env -i HOME="$HOME" PATH="$PATH" make test` exited 0 with 52 Lua cases (46 `test_magit_lsp.lua`, 6 `test_keymap.lua`) and 23 VSCode tests.

- [x] **[P2] Tighten loose or tautological assertions** (`tests/git_backend.rs:55-73,162-212,633-716`, `src/patch.rs:714-721`, `tests/manifest.rs:198-213`, `tests/keys_protocol.rs:416`, `tests/text_mode.rs:226`, `tests/parity.rs:16-83`, `tests/commit_preflight.rs:74`)
  - **Problem:** `contains` checks where the exact diff is known (and `stage_hunk_stages_entire_file` is misnamed, the fixture has one hunk); `full_hunk_reconstruction_is_valid` checks three substrings; `allow_entries_are_scoped_with_reasons` asserts constants against themselves; stray final asserts that repeat earlier ones; `every_menu_item_has_a_parity_row` uses `ends_with(" {key}")` so `a` matches `g a`, and `every_action_row_matches_the_registry…` never reads rows; `_tmp` is used.
  - **Change:** `assert_eq!` against the full expected `git diff --cached -U3`; rename `stage_hunk_stages_entire_file` to `stage_hunk_stages_the_hunk`; check the reconstructed hunk via `apply_check`; replace the allow-list test with "every `WORKBENCH_ALLOW` key is in the deny list and has a non-empty reason"; delete the stray asserts; compare exact `(menu, key)` tuples in parity; rename the registry-row test.
  - **Done when:** each listed assert is exact or removed.
  - **Constraints:** do not delete the behaviour coverage.
  - **Done (2026-10-25):** every listed assertion is now exact or removed; behaviour coverage is unchanged.
    - `tests/git_backend.rs` replaces the `contains` chains with `assert_eq!` against named full-diff constants (`STANDARD_DIFF`, `MODIFICATION_DIFF`, `ADDITION_DIFF`, `DELETION_DIFF`, `INSERTION_AND_DELETION_DIFF`, `TWO_HUNKS_DIFF`, the two-file diffs) and renames `stage_hunk_stages_entire_file` to `stage_hunk_stages_the_hunk`.
    - `src/patch.rs::full_hunk_reconstruction_is_valid` now rebuilds the original side from the hunk and requires `git apply --check` to accept the reconstructed patch (new `apply_check` test helper); the substring checks are gone.
    - `tests/manifest.rs` replaces `allow_entries_are_scoped_with_reasons` with `every_allow_entry_overrides_a_denied_key_with_a_reason`: each `WORKBENCH_ALLOW` key must be in `WORKBENCH_DENY_LIST` and carry a non-empty reason.
    - Deleted the stray trailing `registry()` assert in `tests/keys_protocol.rs` (and its unused import); `tests/text_mode.rs` now asserts the interactive `documentSymbol` names (`Staged changes`, `Unstaged changes`, `Untracked files`) instead of "is an array". `tests/commit_preflight.rs` already held only exact assertions and no `_tmp`, so it needed no change.
    - `tests/parity.rs::every_menu_item_has_a_parity_row` compares the exact sorted set of `(menu, key)` tuples derived from the definitions (including the `?` dispatch row) against `parity::rows()`, and the registry check is renamed `every_action_matches_the_registry_and_the_menu_status`.
  - **Evidence:** the ticket's verification command (`cargo test --test git_backend --test manifest --test keys_protocol --test text_mode --test parity --test commit_preflight && cargo test --lib patch && make test`) exited 0: 38/15/10/4/5/5 integration tests, 16 `patch` unit tests, then `make test` with 52 Neovim cases and 23 VSCode tests plus the keybindings drift check.

- [x] **[P2] Remove process-global env mutation from tests** (`tests/config.rs:91-92`, `tests/runner.rs:37-82`, `tests/process_log.rs:22`)
  - **Problem:** `set_var("HOME"/…)` and `set_var("PATH", …)` in a multithreaded test binary race with other tests; the process log is global.
  - **Change:** run those cases through the CLI/stdio server with an explicit env, or guard with `serial_test`. For the process log, filter by a unique `cwd` (as `runner.rs` does) and drop the lock.
  - **Done when:** `grep -rn "set_var" tests/` only matches `tests/common/mod.rs`'s once-guarded init.
  - **Done (2026-10-25):** no test binary mutates its own environment any more.
    - `tests/config.rs` already ran its `HOME`/`XDG_CONFIG_HOME` cases in a re-executed child with `Command::env`, so it needed no change.
    - `tests/runner.rs` keeps the fake `git` script free of the process env: it reads its start marker from `MAGIT_LSP_FAKE_GIT_MARKER`, and both cancellation cases re-execute themselves alone in a child whose `PATH` (fake first) is set only on `Command::env`. `PATH_LOCK` and the `Drop` PATH restore are gone.
    - `tests/process_log.rs` drops the `LOCK`: the two read-only cases no longer clear the ring, and the stage case finds its record by unique `cwd` plus `add -u` args and then its `process-log:record:<id>` node instead of by rendered position. The destructive `process-log.clear` case runs alone in a re-executed child, which keeps "clear empties the view" exact while the other cases share the log.
  - **Evidence:** the ticket's `cargo test --test config --test runner --test process_log && ! grep -rn "set_var" tests/ | grep -v tests/common/mod.rs` exited 0, and `make test` exited 0 with 52 Neovim cases, 23 VSCode tests and 164 keybindings; the two fake-git suites were also run five times in a row without a failure.

- [x] **[P3] Make UI latency budgets informational and avoid blind key counts** (`tests/ui/scenarios/latency-menu-open.scn:12-32`)
  - **Problem:** hard wall-clock budgets (`<= 100ms/80ms/400ms`) fail on slow CI; `j ×10 Right ×4` finds a diff line by count.
  - **Change:** report latency without failing unless `UI_STRICT_LATENCY=1`; add a `goto "+line3 CHANGED"` DSL step (or use a search key) instead of counted motions.
  - **Done when:** `make ui-test` passes on a loaded machine; the scenario still reports numbers.
  - **Constraints:** keep `ui-unit` tests for the DSL green.
  - **Done (2026-10-25):** latency budgets are informational by default and `latency-menu-open` names its cursor target instead of counting motions.
    - New `goto "<text>"` step (`scripts/ui_lib/scenario.py`; `driver.goto_text`): it moves the cursor to the first character of the first buffer line containing the quoted literal and fails when the text is absent. The search text travels as base64 inside one `luaeval` RPC expression. It is not a key press, so it never enters the trace or a latency measurement.
    - `check_latency` still always reports the measured key-to-idle value; the runner prints every latency expectation (e.g. `latency('?') = 39.3ms (budget 100.0ms)`). An over-budget value only fails when `UI_STRICT_LATENCY=1` (missing trace still fails); `UI_BUDGET_SCALE` still scales budgets.
    - `latency-menu-open` replaces `j ×10 Right ×4` with `goto "+line3 CHANGED"` plus `expect cursor on "+line3 CHANGED"`; `s` still stages f.txt's hunk. `ui-test-plan.md` §A.5 documents the step, and `tests/ui/test_scenario.py` / `tests/ui/test_driver.py` cover parsing, the literal/base64 expression and strict-versus-informational latency.
  - **Evidence:** `make ui-unit` passed 137 tests; `env -i HOME="$HOME" PATH="$PATH" make ui-test` passed 20 PASS / 0 XFAIL / 0 FAIL and printed 16 measurements under both profiles and both sizes (`?` 36.8–40.8ms, `c` 42.2–44.5ms, `a` 3.7–6.0ms, `s` 37.7–80.8ms); `make test` exited 0 (Rust suites, 52 Neovim cases, 23 VSCode tests, 164 keybindings). Screens: `build/ui/latency-menu-open/{standard,vim}-{100x30,80x24}/{dispatch,commit-menu,toggled,staged}.screen`. Plain `make ui-test` in the ambient Nix shell still hits the pre-existing "command too long" tmux startup failure noted in §11.3, so the run used the minimal environment the target expects; no orphan `magit-lsp` processes or `magit-ui` tmux sessions remained.

- [x] **[P3] Stop deriving cursor lines from rendered text in protocol/Lua tests** (`tests/server_async.rs:495-501,602-610`, `tests/test_magit_lsp.lua:150-169`)
  - **Problem:** `trim_start().starts_with('+')` and `line:match('^  %S ')`/`startswith('    @@')` parse rendered text (AGENTS convention).
  - **Change:** request `textDocument/documentSymbol` and use the hunk/file symbol `selectionRange`, or `M.jump`.
  - **Done when:** no helper pattern-matches line shapes.
  - **Constraints:** keep helper names so call sites do not change.
  - **Done (2026-10-25):** both test suites resolve positions through the server's outline.
    - `tests/server_async.rs` adds `first_hunk_line`, which requests `textDocument/documentSymbol` for the view URI and returns `hunk_symbol_line`'s first `SymbolKind::METHOD` `selectionRange.start.line` (recursing through section/file children). The three former `trim_start().starts_with('+')` scans (`view_open_then_invoke_stage_round_trips_over_stdio`, `forward_all_passes_through_unbound_keys_and_dispatches_bound_keys`, `view_open_pushes_a_keymap_and_view_key_round_trips_over_stdio`) now use it; the full-render text assertion is preserved as `is_array()`.
    - `tests/test_magit_lsp.lua` keeps `section_range`, `find_line` and `find_first` but derives their lines from `M.symbols()`: new `symbols`/`section_symbol` helpers find the section by its server name, `section_range` returns the section symbol's `range`, and `find_first` returns the first file/hunk child's `selectionRange.start.line`. No rendered line shape is matched anywhere; `find_line` still compares the exact expected line text. No Lua cases were added.
  - **Evidence:** `cargo test --test server_async && make test` exited 0 — 14 passed / 1 ignored (`BUG-READ-1`) in `server_async`, 52 Neovim cases, 23 VSCode tests and 164 keybindings; no orphan `magit-lsp` processes remained.

- [x] **[P3] Housekeeping** (`tests/stdout_hygiene.rs:210-257`, `.github/workflows/ci.yml:26-27`, `tests/runner.rs:29-93` + `tests/server_async.rs:835-881`, `tests/common/mod.rs:252-295`)
  - **Problem:** (a) a hand-rolled source scanner for `println!`; (b) the manifest test runs twice in CI; (c) fake-git fixture duplicated; (d) five repo constructors (`init_repo`, `init_long_repo`, `init_two_file_repo`, `init_40_line_repo`, `init_repo_40`) and doubled helper names (`make_standard_changes`/`standard_changes`, `make_two_hunks`/`two_hunk_changes`) beside `RepoFixture`, plus hand-rolled `git init` in `errors.rs:133-139,270-282,297-310` and `git_backend.rs:324-329`; `(1..=40).map(|i| format!("line{i}"))` inlined about eight times.
  - **Change:** (a) add `#![deny(clippy::print_stdout)]` and delete the scanner test (keep the two stdio tests); (b) delete the extra CI step; (c) move one `FakeGit` to `tests/common/fake_git.rs`; (d) keep `RepoFixture` + `init_repo()`, add `lines(n).with(idx, text)`, convert the hand-rolled setups to the builder, delete the wrappers.
  - **Done when:** each sub-item is done; `make test` passes.
  - **Constraints:** do after the `TestSession` and stdio-client tasks.
  - **Done (2026-10-25):** each sub-item is complete.
    - (a) `src/lib.rs` and `src/main.rs` carry `#![deny(clippy::print_stdout)]`, plus `#![deny(clippy::dbg_macro)]` so deleting the scanner loses no coverage; `tests/stdout_hygiene.rs` keeps only the two stdio tests and drops `collect_rust_files`/`first_macro_hit`.
    - (b) `.github/workflows/ci.yml` no longer runs the extra `manifest snapshots` step; `make test` already runs that test.
    - (c) the runner's start-marker `FakeGit`, its `CHILD_ENV`/`MARKER_ENV` constants and `real_git_path` moved into `tests/common/fake_git.rs` beside the `GatedGit` that `tests/server_async.rs` already shares, so no fake-git fixture is duplicated.
    - (d) `tests/common/mod.rs` adds `lines(n)`, returning a `Lines` builder with `with(idx, text)` (1-based), `insert_after`/`remove` and a newline-terminated `Display`. `numbered_lines`, `make_standard_changes`, `make_two_hunks`, `init_long_repo`, `init_two_file_repo`, `init_40_line_repo` and `init_repo_40` are gone; call sites build through `RepoFixture` directly (`.commit("init", [("f.txt", lines(40).to_string())])`, `.modify_standard()`, `.modify_two_hunks()`, `.modify("f.txt", lines(40).with(35, "line35 CHANGED").to_string())`), including every inlined `(1..=40).map(|i| format!("line{i}"))` body. The hand-rolled `git init`/config setups in `tests/errors.rs` (`unborn_head…`, `would_be_overwritten…`, `conflicts…`) and `tests/git_backend.rs` (`unstage_all_works_without_commits`) now build through `RepoFixture`; branch/merge steps the builder does not model stay on `fixture.git(...)`.
    - Decision: `tests/git_version.rs`'s local `FakeGit` fakes `git --version`/`status` output rather than gating a real subprocess, so it is a distinct fixture; only the duplicated runner/server fake moved.
  - **Evidence:** `make test` exited 0 (30 lib unit tests, every integration suite green — e.g. 38 `git_backend`, 41 `dispatch`, 15 `status`, 10 `errors`, 9 `status_render`, 5 `runner` — then 52 Neovim cases, 23 VSCode tests, 164 keybindings). `tests/stdout_hygiene` is now 2 tests. `cargo clippy --all-targets` was already failing before this change on `clippy::never_loop` in the untouched `src/config.rs` (`git show HEAD:src/config.rs | diff - src/config.rs` is empty); with that pre-existing error allowed it reports only the pre-existing `tests/menu.rs:429` unused-variable warning and nothing from the new code. No orphan `magit-lsp` processes remained.

- [x] **[P3] Remaining small coverage gaps**
  - **Problem:** (a) IMM-3: exit code is 128 in every row of `errors.rs:52-119`; no real `non-fast-forward` via a local bare remote; no `processLogId`; (b) §3.13 `diff.context` parsed but no test that `-U<n>` reaches both diffs; (c) §8.4 colour checks beyond links (section ≠ file ≠ hunk header ≠ hash; `menu.planned` dimmer than `menu.label`) are absent; (d) R-UI-5 progress ending on a failed commit is only asserted at L3; (e) `.cargo/config.toml` alias `cargo xtask parity` is untested; (f) `make test` omits `ui-unit` and the node tests (document it).
  - **Change:** add one test per item (reuse existing fixtures; (c) goes into the existing colour-scheme Lua case, (d) into `server_async.rs` with a failing `pre-commit` hook).
  - **Done when:** each item has a test or a recorded decision to skip.
  - **Constraints:** do not add Lua cases beyond (c).
  - **Done (2026-10-25):** five gaps have tests and (b) is a recorded skip.
    - (a) `tests/errors.rs::a_real_non_fast_forward_push_is_readable_without_a_process_log` pushes a divergent local commit to a local bare remote (advanced from a clone), asserts the real rejected push does not use exit code 128, maps to the non-fast-forward summary at exit codes 1/128/255, and that `processLogId` is `None` and absent from the serialised effect.
    - (b) **Decision: skip.** `diff.context` is parsed into `Config::diff.context` (`src/config.rs:396`) but never reaches `GitRepo::diff_side`, which hardcodes `-U3` (`src/git.rs:300`). A passing test would need config→session→`StatusView`→`GitRepo` plumbing in `src/`, outside this ticket's target files, so no test can assert `-U<n>` today.
    - (c) The existing `theme links survive a colorscheme switch` Lua case now checks `magit.menu.planned` is dimmer than `magit.menu.label` and, after the switch, that section/file/hunk-header/hash foregrounds are pairwise distinct. No Lua case was added.
    - (d) `tests/server_async.rs::a_failed_commit_ends_its_progress_with_an_error` installs a failing `pre-commit` hook, runs `commit.create` on a staged change, and asserts the create/begin/end handshake shares one token and ends after a failed `git commit` whose error carries the hook output.
    - (e) `tests/parity.rs::the_cargo_xtask_alias_runs_the_parity_command` reads `.cargo/config.toml`, asserts the `xtask` alias forwards to `parity`, and runs the binary the alias expands to (spawning `cargo` from `cargo test` would wait on the target-directory lock).
    - (f) `README.md` ("Running the tests") and the `test` target comment in `Makefile` state that `make test` covers Rust, Neovim and VSCode only; `make ui-unit` and `make ui-test` are separate.
  - **Evidence:** `cargo test --test errors --test server_async --test parity` exited 0 (11 errors, 15 passed + 1 ignored `server_async`, 6 parity), `make test-file FILE=tests/test_magit_lsp.lua` passed 46 cases, and `make test` exited 0 with 52 Neovim cases, 23 VSCode tests and 164 keybindings. No orphan `magit-lsp` processes remained.

### Test-review summary (for the human)

Priority index: §7. Decisions: §0. All remaining acceptance gates are above;
review-time findings must be rechecked against source before implementation.

### Documentation maintenance (this ticket)

- [x] Reconcile the active technical-debt review in `REVIEW_REPORT.md` and
  direct the sprint there without duplicating its task ledger.
- [x] Consolidate completed history and VSCode resolution notes; preserve the
  unresolved task inventory, priorities, dependencies, acceptance gates and user
  annotations. Verify links/references and `git diff --check && make test`.
