# magit-lsp — Roadmap

Canonical roadmap, design, protocol and backlog. Terms are in §1, protocol
shapes in §§3–4, and live tasks state their acceptance gates. Read selectively
from [AGENTS.md](./AGENTS.md); `README.md` owns user-facing documentation.
Do not duplicate detailed design/history in `AGENTS.md`.

**Status note:** design sections describe intended behaviour. Verify current
behaviour in source/tests rather than treating historical notes as current facts.

**Project status:** early stage. **Backward compatibility is not a goal.**
Rename, delete, and reshape protocol methods, config keys and Lua APIs freely.
Delete the old thing; do not leave shims.

## Contents

0. Decisions already made
1. Glossary
2. Completed correctness contracts
3. Design decisions (expanded)
4. Protocol reference
5. Default keymaps (profiles)
6. View catalog
7. Dependency-ordered implementation queue
8. Testing strategy
9. Open questions
10. Implementation status and deviations
11. Client reviews (completed history archived)
12. Review backlog (compact summary)

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
| VCS boundary | Keep presentation and standard-LSP transport VCS-neutral so future jj support requires effectively zero Plain-client changes. Keep Git discovery, mutation validation, commit-file paths, helper lifecycle and completion lookup in backend/workflow providers. Do not add a jj backend, lowest-common-denominator VCS hierarchy, or speculative wire protocol |
| Diff/patch | Git computes diffs; we parse its output and build patches for `git apply` (this is what Magit does, §3.13) |
| Rendering | Server re-renders fully (as Magit does) and sends line-range edits, so clients stay stable (§3.3) |
| Tests | Mostly Rust, in-process, asserting buffer text and repo state. Neovim tests are kept but few (§8) |
| Detail visibility | Four levels: section (staged/unstaged) → files → hunks → individually visible hunks; profile bindings and tests must match (§5.1) |
| Menu saved arguments | Defer `menus.save_scope` persistence; remove/defer the setting until semantics are understood |
| Magit parity keys | Verify keys against the Magit manual, not memory |
| Recommended Git version | Git 2.30.8 is the tested recommendation, not a hard minimum; older versions are not blocked but may have feature failures |
| Commit selection (target) | Intended selection uses server-rendered log select mode with structured stable identities and caller restoration; no client picker or choice prompt (§3.7). Current implementation must be checked against source; Plain's parameterized single/multi flow is pending. |
| Plain presentation (target) | Standard-LSP presentation is VCS-neutral; concrete backend/workflow providers supply repository identity and mutation semantics. Reusable interactions target concrete views/data: serving/stacking; parameterized stable-ID single/multi pickers with caller restoration; provider-file message edits with finish/cancel callbacks; one semantic dispatcher; and a fake-provider independence test without jj implementation. Intended architecture, not current behavior. |
| Plain client naming and selection (target) | “Plain” means spec-only generic LSP clients, not an editor-specific adapter; one semantic dispatcher selects `customRender` only when explicitly requested, otherwise Plain. |
| Plain document sync (target) | Incremental sync uses one URI-keyed store; apply each notification's changes sequentially, accepting ranged and rangeless changes and negotiated encodings. |
| Plain repository resolution (target) | Resolve an unambiguous repository from explicit/workspace/document context; never guess for mutations, use no launcher (the editor helper is required), and exclude presentation files from Untracked. |
| Plain action/rendering (target) | Content-addressed targets are revalidated at execution; stale targets refresh without mutation. Render expanded by default, auto-cap large sections with expand actions, and retain folding support. |
| Plain prompts/capabilities (target) | Selector is capabilities.experimental.magit; standard capabilities govern Plain support, while declared optional effects/prompts/placements are bounded by the block. |
| Plain deferred/out-of-scope (target) | History is deferred to R-EDIT-HISTORY; character input, launcher, diagnostics, discard recovery refs, direct executeCommand bindings, custom URIs and LSP 3.18 virtual content are out of scope. |

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
| **Editor bridge** | Git's editor helper waits on a server-managed commit draft (§3.10); the Neovim managed-draft adapter is R-EDIT-4 and VSCode follows in R-EDIT-5 |
| **Process log** | A view listing every Git invocation with arguments, exit code, duration and output |
| **Text mode** | A view mode with no ordinary Magit key capture: Neovim retains its existing editable buffer behavior; VSCode browses its read-only virtual document with native editor keys and queued refreshes (§3.14) |
| **Semantic token** | LSP's standard way for a server to label ranges of text with a *type* (and modifiers) so the client colours them using the user's theme |
| **Parity** | Having the same operation available as Magit |

---

## 2. Completed correctness contracts

Completed hands-on fixes, retained here for their behavioural contracts.
These contracts remain canonical; open evidence gaps and follow-ups are tracked
in §7. Historical completion marks are not proof that every original test exists.

### IMM-1 — Unstaging a file header fails in a repo with no commits

- [x] Unstage paths with `git reset -q -- <paths>`, falling back to
  `git rm --cached -q -f --ignore-unmatch -- <paths>`; include both rename paths.
  This is safe in unborn repositories and when index differs from worktree.
  Preserve unborn-repo file/hunk/line/all mutation coverage; oldest-Git validation
  and fallback coverage remain a P2 task, not an established version guarantee.

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
  never show raw tables/JSON. Test each mapped failure and Neovim notification.

### IMM-4 — Neovim status buffer must be read-only

- [x] `nofile`, `bufhidden=hide`, no swap/undo, `modifiable=false`,
  `modified=false`; text mode toggles editing (§3.14). The sole render writer
  temporarily enables modification, applies full/range edits, clears modified
  and locks again. Block `:w`/`:write` (`BufWriteCmd` plus the E382
  `CmdlineLeave` guard); unbound edits fail natively with E21.
  R-UI-3 removed `readonly=true` to avoid W10; the one-time hint occurs on
  attempted write rather than E21. Text remains intact and refreshes after edits.

### IMM-5 — Two separate line stagings appear as one staged hunk; audit the diff/patch layer

- [x] No hunk-merging bug: Git merges gaps ≤ `2 × context` (six lines at U3).
  Git merges gaps ≤ twice context (six lines at U3). Fixtures verify lines 5/35
  in a 40-line file produce two hunks and four-line gaps produce one. Status
  matches `git diff --cached -U3`; `patch.rs` builds region patches for `git apply`
  with `--unidiff-zero --recount` (§3.13), not diff computation in `git.rs`.

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

- [x] Shared helpers, `RepoFixture` and `TestSession` live in
  `tests/common/session.rs` with deterministic setup (§8.2); see completed archive.

### IMM-8 — Spike: colour verification with `:TOhtml`

- [x] `scripts/color_spike.lua` / `make color-spike`; findings and colour-test
  rules are recorded in §8.4.

### IMM-9 — Update `AGENTS.md`

- [x] `AGENTS.md` links to this roadmap; decisions live in §0 and working
  conventions in `AGENTS.md`.

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
position-based requests return `null`. The custom `view/rendered` acknowledgement
identifies an applied custom render version/tree; it differs from Plain synced-text
snapshots, which associate document text with render state without that custom
ack protocol. Plain presentation uses standard LSP documents and providers. The adopted target
uses the declared capability block, logical stack and bounded prompts/placements;
this supersedes the obsolete no-prompts/no-placements/no-editor-bridge target claim. Virtual-content transports and custom URIs remain
out of Plain scope; see §9 for deferred LSP 3.18 work.

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
| `showDocument` | `uri`, `line?`, `column?` | Open a real file (standard `window/showDocument`) |
| `editDocument` | see §3.10 | Open it for editing in the client's own editor |
| `clipboard` | `text` | Put the text on the system clipboard |
| `passThrough` | `key` | Perform the key's normal editor behaviour as if unbound |
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
  foreign widgets (R-VS-5, completed; archive), not merely the active document.
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

**Production real-file implementation slice (2026-10-29).** The server and both
bundled adapters now open the canonical actual-gitdir `COMMIT_EDITMSG` file
(`editDocument.mode: "realFile"`, `file:` URI, separate opaque `runId`). The old
virtual provider and wire `draft/save`/`draft/finish` methods are removed. Native
save writes disk and keeps editing. Full-sync versions remain bounded to 1 MiB.
Explicit confirmation is `workspace/executeCommand` with command `magit.command`
and one argument `{command:"common.confirm",runId,uri,version,
source:"synchronized"}`: exact latest version validation and safe writeback precede
helper release. `source:"saved-file"` instead reads bounded saved disk bytes and
never writes unsaved synchronized text. Default `didClose` uses saved-file
confirmation; VSCode is the known exception (identified by clientInfo), ignoring
didClose as confirmation and sending run-aware saved-file intent only on an actual
last matching `tabGroups` closed event across all groups. Explicit abort remains
`view/invoke common.cancel` with `{runId}`. All terminal decisions use the registry
lock. Safe-save inode replacement is accepted only in the pinned Git directory,
with regular-file, no-symlink/no-hardlink and owner checks. History remains
in the bounded server ring; rich recall routes are removed and both history
commands are planned under R-EDIT-HISTORY. Confirmation returns
`{type:"confirmed",runId,uri,version}` after helper release, not Git success.
Rejected sync is reported as a readable `Message` through `view/effects` without
replacing local edits. Failed ordinary native close also re-presents the same
pending `EditDocument` run/snapshot; Neovim restores its wiped native buffer,
full-sync observer and keymaps with retained local text/version. No custom draft
error notification remains. Git owns
templates, cleanup, effective-empty behavior and hooks.

This is an implementation/integration slice, **not parent R-EDIT-REAL native or
adversarial acceptance**; that parent stays unchecked. The material below is the
**archived pre-cutover managed-draft design**, not today's wire contract.

- [x] **R-EDIT-REAL existing-effects recovery slice (2026-10-29).** Remove the
  last obsolete draft error notification and both client handlers. Rejected
  open/change reports a readable message without overwriting native edits;
  failed saved-file native close re-presents only the same pending run through
  existing effects. Recovery preserves local text/version, native full-sync and
  keymaps, with retry/explicit abort and stale-run resurrection guards. Parent
  R-EDIT-REAL remains unchecked.
  - **Verification:** exact ticket command passed: `cargo test --test editor_bridge
    --test editor_drafts --test server_async` (64 tests), `make test-lua`,
    `make vscode-test` (unit/integration and generated bindings),
    `scripts/ui run tests/ui/scenarios/commit-draft-close.scn` (1 PASS),
    `MAGIT_VSCODE_BIN=/home/william/.nix-profile/bin/codium MAGIT_VSCODE_EDIT_ONLY=1
    npm --prefix clients/vscode run test:e2e`, and `git diff --check`.
    Stdio tests cover unreadable-file failed close retaining the same run/version,
    then retry committing restored disk rather than unsaved synchronized text.
    Lua covers recovered-buffer explicit abort; both client suites cover retained
    edits/version and terminal-run re-presentation rejection.
  - **Inspected native evidence:** `build/ui/commit-draft-close/{failed-close-retained,
    recovery-text,recovered-close-committed}.screen` shows readable failure,
    retained text and successful same-run retry; scenario assertions also check
    restored keymaps/version. `/tmp/opencode/vscode-review-umgCTE/real-file-native-{evidence,
    trace}.json` and `real-file-native-3-discard-dialog.png` retain production
    Codium native Save/Don't Save/Cancel and all-group last-owned-tab evidence:
    exactly one saved-file confirmation per run and exact saved subject/body,
    never unsaved text. Additional inspected screenshots in the passing
    `/tmp/opencode/vscode-review-vAMNxo/` run cover two groups and native Cancel.
    No owned live UI/server remained after cleanup. No test deadline increased.
  - Full regression is **not clean**: the inherited release baseline at fc47bcf
    already failed `tests/errors.rs::normal_and_piped_apply_failures_keep_original_details`
    at line 477 (`build/test-logs/rust.EwFXOb`). This slice does not modify that
    unrelated failure or claim `make test` passed.

- [x] **R-EDIT-REAL implementation/integration slice.** Real file server/client
  cutover and focused integration verification; parent native/adversarial
  acceptance and Magit message-ring parity remain pending.
  - Focused production Neovim evidence: `commit-draft-history.scn` and
    `commit-draft-close.scn` passed with the debug server; saved/recalled/restored,
    discarded-to-saved and recovered-close screens inspected in `build/ui/`.
    Native VSCode production suite was updated but **not run** in this slice;
    native dirty choices/multi-group/lifecycle adversarial acceptance remains
    for the orchestrator. Spike results are not reused as production evidence.
  - Verification: exact ticket gate passed (68 Rust tests, Lua, VSCode unit and
    generated bindings, Node integration including 240 real-Git selection cases,
    `git diff --check`). Full native VSCode acceptance and full regression gate
    remain unrun; parent R-EDIT-REAL stays unchecked.

- [x] **R-EDIT-REAL rich-history removal slice (2026-10-29).** Remove the obsolete
  history route/types, per-draft navigation and rich-client recall commands;
  retain the bounded server ring and saved finish/abort retention from 79384c2.
  History commands are planned under R-EDIT-HISTORY. Semantic executeCommand
  confirmation returns `type:"confirmed"`; no new custom method or compatibility
  shim. Exact-version/full-sync/writeback, saved-file close, terminal arbitration
  and e141e89's all-group owned-last-tab behavior are unchanged. `draft/error`
  recovery is intentionally untouched for the next existing-effects slice.
  - **Verification:** exact ticket gate passed: regenerated manifest/menu/parity
    snapshots (keybindings and README parity unchanged), five private editor/ring
    tests, Lua, VSCode unit/generated bindings, native `commit-confirm.scn` and
    production Codium edit-only e2e. Supplemental unfiltered `editor_drafts`,
    `editor_bridge`, `server_async`, `commands` suites passed all 73 cases;
    Node integration passed, including 240 exact repository-state selection cases.
  - **Inspected evidence:** `build/ui/commit-confirm/{standard,vim}-{100x30,80x24}/{editor,staged}.screen`
    proves prompt decline/accept, native Git editor opening and explicit abort
    retaining staged changes. `/tmp/opencode/vscode-review-DfgHFR/real-file-native-evidence.json`
    and `real-file-native-trace.json`, plus `real-file-native-2-cancel-dialog.png`
    and `real-file-native-3-committed.png`, verify actual native Save/Don't Save/
    Cancel and all-group last-owned-tab semantics: exactly one saved-file
    confirmation per run, exact saved subject/body, no unsaved-text submission.
    Owned UI/server processes were absent after cleanup. No test deadline changed.
  - **Limits:** parent R-EDIT-REAL remains unchecked; no broader lifecycle,
    platform or deferred R-EDIT-HISTORY acceptance is claimed.

**Target-versus-current:** the Plain-client target proposes Git's real
`COMMIT_EDITMSG` provider-file session, pending R-EDIT-REAL acceptance. The
explicit decision is to assume that `didClose` is an acceptable confirmation
signal by default; clients may be modified or this revisited as issues arise.
VS Code is currently the only known exception. Standard `didClose` has no run
identity, and its reliability/late-event limits are consciously accepted as an
assumption, not proven by protocol. Run-aware explicit events still validate
run/session. One
session/owner/run-aware idempotent path distinguishes saved-file close from
exact-version synchronized-buffer confirmation (including validation/writeback);
terminal/abort arbitration is atomic, with no duplicate helper release/commit.
Saved-file close never substitutes unsaved synced text. Until acceptance, the
current recorded contract below remains managed drafts. See [Plain-client target and ticket bodies](./magit-lsp-plain-client-tasks.md)
for the presentation/provider reuse design; this note does not change current
helper or process-lifecycle guarantees.

Git opens an editor for: commit messages (incl. `--amend`, reword), merge
messages, annotated tag messages, interactive rebase todo lists, `git commit
--fixup=reword:`, etc.

**Mechanism**
1. When the server spawns Git it sets: `GIT_EDITOR`, `GIT_SEQUENCE_EDITOR` (and
   `EDITOR`, `VISUAL` if unset) to `"<path to magit-lsp binary> --editor-helper"`,
   plus `MAGIT_LSP_SOCKET=<unix socket or named pipe path>` and
   `MAGIT_LSP_EDIT_ID=<opaque run>` and private one-shot
   `MAGIT_LSP_HELPER_TOKEN=<credential>` (never sent to the client or logged).
2. Git runs the helper with the file path as argument. The helper connects to
   the socket, sends `{editId, token, path, kind}` and **blocks** waiting for a
   reply (`ok` → exit 0, `abort` → exit 1; Git aborts the operation on 1).
3. The server maps the edit to the session that spawned Git and sends an
   `editDocument` effect to that session's client. `title`, `mode` and `scope`
   are server-owned metadata; `scope` names the command/keymap scope used by
   the edit:

   ```
    { type: "editDocument", runId, uri: "magit-commit://<opaque-run>/message",
      text: string, version: 1, kind: "commit-message",
      title: "Commit message", mode: "managedDraft", scope: "commit-message" }
   ```
    The production registry allocates the draft before Git and binds its owner
    to trusted connection state, not client-supplied identity or repository.
    It accepts only `commit-message`, the one-shot credential, and the expected
    canonical actual-gitdir `COMMIT_EDITMSG`. No editable filesystem path or
    helper credential is announced. `text` is Git's exact bounded UTF-8 template.
    The dynamic `view/keymap` uses `runId` as its `viewId`.
4. **The client opens it in its own editor, in the same window/instance**:
    - Neovim: managed acwrite/scratch document with standard full-sync LSP
      attachment (implemented, R-EDIT-4).
    - VSCode: writable managed provider with the same full-sync/version contract
      (pending, R-EDIT-5).
   - TUI: may spawn `$EDITOR` itself (detail deferred).
 5. **Server cutover:** `draft/save` checkpoints only; `draft/finish` writes the
    selected saved version before helper `ok`. `common.confirm` cannot release
    a draft or bypass the versioned barriers. Explicit `common.cancel` carries
    `args: {runId}` and aborts. LSP `didClose` only detaches.
 6. Only managed commit-message editing is implemented. Merge/tag messages and
    `rebase-todo` are not emitted; there is no realFile fallback or capability
    opt-in. The old native adapters are intentionally not compatible: the
    Neovim adapter is migrated by R-EDIT-4, R-EDIT-5 migrates VSCode and
    R-EDIT-6 owns the final cross-client cutover.
 7. Client EOF, exit, shutdown, progress cancellation and helper socket closure
    terminate pending runs and reject late input. The registry lock serializes
    helper joining, validated writeback and terminal cleanup. R-EDIT-2 owns
    Linux process cleanup; its historical realFile native evidence is not
    managed-draft adapter evidence.

**Managed commit drafts (server cutover and Neovim adapter implemented; VSCode
adapter pending, no compatibility mode).** Replace only real-file commit-message
editing with a mutable server-managed per-run draft. Git still launches helper,
supplies the template/path, runs hooks, performs cleanup/comment stripping, and
owns commit;
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

**Stable R-EDIT-3 wire contract (for R-EDIT-4/5/8).**
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

The server advertises `textDocumentSync: {openClose:true, change:1}`. Send
standard `textDocument/didOpen` with `{textDocument:{uri, languageId:"gitcommit",
version:1, text:<exact announced template>}}`. A `didChange` contains
`{textDocument:{uri,version}, contentChanges:[{text:<complete text>}]}`: exactly
one change, no range/rangeLength. `didClose {textDocument:{uri}}` detaches;
reattachment must use the retained live text/version, not reset to version 1.
Draft synchronization and barriers execute in transport receive order, not
unordered request tasks. Other/status document notifications remain unchanged.

After flushing full sync, send a **JSON-RPC request** `draft/save` with params
`{runId, uri, version}`. Its result is
`{type:"draft/checkpointed", runId, uri, version}` only after copying that exact
applied live snapshot. Finish is a **JSON-RPC request** `draft/finish` with
`{runId, uri, version, reason:"confirm"|"normal-close"}`; its result is
`{type:"draft/finished", runId, uri, version}` after validated checkpoint write
and successful helper reply delivery. Git's eventual outcome/progress is separate.
Confirm requires saved == live version. Normal-close selects the saved version
even when detached or newer unsaved live text exists. Initially saved == template
at version 1. Save does not write Git's file or release the helper.

Correlation is the ordinary JSON-RPC `id`, **not** a payload `requestId` or an
extra acknowledgement notification. Both requests reject unknown fields (in
particular text, path, owner or repository). Request errors have readable
`error.message` and `error.data: {runId,uri,version,recoverable:true}`; invalid
sync emits `draft/error {uri,version,method,message,recoverable:true}` (`version`
may be null for close). Failed validation/write retains the previous snapshots
and pending helper; terminal/missing runs reject late input. No body or token
is included in errors. These methods are barriers, never a second text channel.
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

**Current-versus-target note:** The managed-draft protocol above is the current
recorded contract, not a completed implementation across both clients. The
real-`COMMIT_EDITMSG` flow in linked R-EDIT-REAL is a proposed target behind its
mandatory step-0 spike; its detailed rewrite of this section is subsequent
documentation work. Magit message-ring parity is unverified and its source-check
remains pending in R-EDIT-REAL acceptance.

R-EDIT-8 ring limits: default 50 entries and 1 MiB aggregate UTF-8 payload per
canonical repository, in memory for server lifetime. Reject retention of an
entry larger than the cap with a visible warning; never truncate or silently
drop it. Otherwise evict oldest until both limits hold; exact consecutive dedup.
Existing eligibility/recall rules above remain canonical.

**Stable R-EDIT-8 history contract (both native adapters).** Route the existing
keymap commands `commit-message.history-prev` / `commit-message.history-next`
through a JSON-RPC request `draft/history` with
`{runId, uri, version, command:<exact command ID>}`. `version` names the exact
applied live base after flushing full sync. No text, owner, path, repository or
other fields are accepted. Like save/finish, history runs on the ordered receive
lane and validates the authenticated owner, run, exact URI, attached state and
base version before changing either live text or navigation. Errors use the
same readable/recoverable correlated error contract as save/finish.

The result is `{type:"draft/replaced", runId, uri, baseVersion, version, text}`.
It is an editable server-owned replacement, **already applied** to live state;
`version = baseVersion + 1` for every accepted request, including empty-history
and clamped recalls. Integer exhaustion rejects atomically. Saved text/version,
Git's file and helper state are unchanged: history never saves or finishes.
The first previous captures the unsent live text plus a bounded browsing snapshot
of that repository's ring. Subsequent edits to recalled text do not replace this
unsent snapshot. Next at newest restores it and ends browsing; another next is a
no-op replacement. A new previous starts a fresh browsing snapshot, so eviction
or submissions during browsing cannot shift its order.

Adapters must serialize history with native edits and save/finish: flush pending
didChange, pin owner/run/URI/base, and gate native mutation while awaiting the
response. Apply the exact text only to that still-matching buffer/base, suppress
the replacement's native didChange echo, and adopt the returned version in the
draft sync tracker. Later user edits use full sync strictly above that version
(native editor version counters may be mapped by the adapter). There is no
custom text acknowledgement or second client-to-server text channel. Never apply
a response to a closed/reused buffer or overwrite newer local edits. If native
mutation nevertheless raced the response, preserve local text, do not apply the
replacement, and reconcile it through full-sync didChange at a version greater
than both the local and returned version before allowing another barrier. A
terminal/closed run is not resurrected. Adapters perform no ring/eligibility or
navigation policy; direct unversioned `view/invoke` cannot recall history.

Only successfully written, selected finish checkpoints feed retention (including
normal-close and Git failures after helper release), never live/autosaved or
aborted text. Empty means zero bytes; template-only means exact initial equality,
not comment stripping or semantic parsing. A retention overflow warning is a
user-facing warning effect, separate from successful submission. The draft's
independent 1 MiB input limit normally rejects such overflow earlier.

**Server acceptance verified (2026-10-28).** The focused R-EDIT-8 command
`cargo test --test editor_drafts --test editor_bridge --test commands --test manifest --test parity && git diff --check`
passed (72 tests). Evidence: `/tmp/opencode/r-edit-8-verification.log`.
Core and production stdio coverage verifies repository isolation, ordering/exact
deduplication, count/UTF-8 byte eviction and overflow, eligibility exclusions,
failed-commit recall, unsent restoration/clamping and exact identity/version
rejection without implicit save/finish. Native history acceptance remains pending
under R-EDIT-4/5/6; this verifies the server seam only.

**Neovim adapter acceptance verified (2026-10-28).** `make test-lua`, then
`scripts/ui run` for `commit-cancel-progress.scn`, `commit-draft-history.scn`,
`commit-draft-close.scn` and `commit-owned-cancel.scn`, plus `git diff --check`
passed. The managed acwrite buffer is not auto-attached; it sends one explicit
full-sync stream (`didOpen` version 1, complete `didChange` snapshots, monotone
versions) and uses versioned `draft/save`, `draft/finish` and `draft/history`
barriers. Native `:write` checkpoints and stays editing; explicit confirm flushes,
checkpoints and awaits finish; typed `:q`/`:close`/`:bd`/`:wq`/`:xit` classify
intent before teardown, clean close confirms the saved checkpoint and dirty close
offers Save/Discard/Cancel with acknowledged semantics. Ambiguous `BufUnload`/API
deletion and disconnect cancel without confirming; failed checkpoint/finish keeps
the recoverable buffer. Server-owned history recall, return-to-current-draft and
the version/adoption rules were driven through the real managed server. Status
readonly, A29, text mode and askpass were untouched; each owned UI run was
inspected and stopped.

**Process feasibility gate (proposed R-EDIT-2 mechanism).** One async owner task
owns `Child`, cancel receiver, exit observation, final wait/reap, and reader
completion channels; handlers only enqueue idempotent cancellation. A valid
progress-cancel request promptly receives one LSP `report` (`Cancelling…`, no
longer cancellable) and readable feedback. This acknowledges intent, not Git
termination or absence of a commit. Serialize report/end against token removal;
duplicate/unknown/ended tokens do not report, and cleanup retains the repository
reservation until the owner finishes. Exit-before-cancel still wins and completed
commits still report success. Start Git in
a new session/process group before exec (child-side spawn setup, not racy
post-spawn `setpgid`). No controlling tty: piped stdout/stderr, null stdin unless
the invocation requires otherwise. Concurrent readers retain at most 4 MiB per
stream, continue draining/discarding after cap, and flag truncation. Reader drain
deadline is 2 seconds after leader exit/terminal cancellation. When the owner
dequeues cancel, it first observes leader status with
`waitid(P_PID, pid, WEXITED|WNOHANG|WNOWAIT)`. If already exited, exit wins and
no signal is sent. Otherwise cancellation linearizes: abort a connected helper,
send TERM to the group, and send CONT to the group so stopped members can
process termination. Retain the unreaped leader for at most the 2-second TERM
grace, then send exactly one group KILL even if the leader exited during that
grace. Linux may finish grace early **only after WNOWAIT observes leader exit**:
inspect `/proc` for live nonleader members of its pinned process group, validating
stat identity/state/pgrp outside the potentially arbitrary `comm` field. No live
members observed is merely a latency hint: still send the final group KILL before
reap. A fork/scan race can miss a member and end its grace early; inspection is
neither a graceful-exit guarantee nor whole-descendant containment. Never signal
individual scan PIDs. Unavailable, malformed, slow or uncertain inspection keeps
full grace, as do observed live/stopped/TERM-resistant members. Other Unix targets
retain the existing full-grace fallback, without platform validation claims.
Reap only after KILL, using bounded observation; if KILL is not reaped within a further 2 seconds,
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
initial WNOWAIT observation → cancel linearization → TERM/CONT → full grace
(or the conservative Linux hint after observed leader exit) → KILL → final reap,
with no signal after reap. Cooperative real-hook cancellation must finish
comfortably below two seconds with readiness established before timing; prompt
intent acknowledgement is tested independently using a resistant hook. These are required tests, not
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

Stance: **keep Git operation semantics intact while making Plain presentation
and standard-LSP transport VCS-neutral.** Git-specific discovery, mutation
validation and workflow details stay behind providers; reusable interactions
take concrete views/data and stable identities. This enables future jj support
with effectively zero Plain-client changes, without requiring a jj backend,
generic VCS trait hierarchy, or speculative wire protocol. This does not
promise a low-cost backend fork; keep Git-specific CLI/domain details where
they are meaningful. See §0 and the Plain phase task list.

---

## 4. Protocol reference

No backward compatibility: delete the old `vcs/*` methods as their
replacements land.

### 4.1 Client → server (current contract; Plain presentation selection is intended, not yet implemented)

| Method | Params | Returns |
| --- | --- | --- |
| `initialize` (current wire) | `clientInfo`, current preferences `keymapDelivery` and `keyModifiers`; `profile?`, `viewport`. Adopted target selector is solely `capabilities.experimental.magit` (absent Plain, present rich); `initializationOptions.presentation` is rejected/obsolete current intent, not the selector. | server capabilities |
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

Plain mode uses standard document synchronization and code actions/commands for
semantic action dispatch; concrete views/data carry stable identities and are
never reconstructed by parsing rendered text. Git-specific discovery,
mutation validation, commit-file paths, helper lifecycle and completion
candidates belong to Git backend/workflow providers. Reusable server
interaction primitives must support serving/stacking views, single and
multi-item in-buffer selection with stable identities and caller restoration,
message-edit sessions using a provider-supplied real file and finish/cancel
callbacks, and semantic action dispatch. Git wrappers retain operation-specific
semantics. Require a minimal fake-provider independence test, not a production
jj backend or new wire protocol. R-EDIT-REAL owns the detailed real-file/ring
contract and cutover; the managed-draft paragraphs in §3.10 describe current
implementation being superseded by that gated target, not the desired contract.

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
  (`0`–`9` as counts), `j k`, `gg`, `G`, `ctrl+d/u/f/b`, `/`, `H`, `L`.
  Structured views may bind `h`/`l`; text mode retains native horizontal motions.
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

The four R-NAV bindings are Implemented. `shift+Tab` cycles the global detail
level 4 → 1 → 2 → 3 → 4. Sibling motion stays within the immediate parent
(root sections are siblings), never wraps, and treats a diff body as its hunk.
Parent motion goes to the immediate containing header; roots stay put.

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

The final column references stable parent task IDs in §7; completed status/process
work is summarized in the archive, managed editor work is R-EDIT, and todo is R7.3.

Status common hierarchy navigation and four-level visibility cycling are
implemented and verified by R-NAV (§7); these are server-tree operations.

| View | Magit analog | Contents | Key commands | §7 parent task |
| --- | --- | --- | --- | --- |
| **Status** | `magit-status` | Header lines (Head, Merge, Push, Tag); sections: in-progress operation, Untracked, Unstaged, Staged, Unmerged, Stashes, Unpulled, Unpushed, Recent commits, Submodules | stage/unstage/discard/apply/reverse, visit, toggle, refresh, all menus | Completed (§10/archive); R-3.6; R-6 |
| **Log** | `magit-log` | Commit list with graph, decorations, author, date; options (`-n`, `--graph`, `--decorate`, `--author`, `--since`, `--grep`, `-S`, `-G`, path limits); lazy "load more"; **select mode** | show commit, checkout/reset/branch/rebase/cherry-pick/revert from the commit at point, confirm/cancel | R-5.1 (completed; archive) |
| **Log variants** | `l` menu | current, other, all, local branches, reflog, file log, line-range log (`-L`), `HEAD..upstream`, `upstream..HEAD` | same | R-5.3 |
| **Revision** | `magit-revision` | Header (hash, parents, author, committer, dates, refs, message), diffstat, full diff with hunks; parent/child navigation; combined diff for merges | visit file, apply/reverse hunk, copy hash | R-5.2 |
| **Diff** | `magit-diff` | Any range `a..b`, `a...b`, worktree vs index vs revision, path-limited, stash vs parent, stat-only; context/whitespace/rename/word-diff options | visit, stage/unstage when applicable | R-5.2 |
| **Refs** | `magit-show-refs` | Local branches, remotes, tags with ahead/behind vs a base; also select mode | checkout, create, delete, rename, reset, merge, rebase onto, log | R-5.3 |
| **Cherry** | `magit-cherry` | `git cherry` between two refs | cherry-pick, show | R-7.2 |
| **Stash list / show** | `magit-stash-list` | Stashes; show = diff view of a stash | apply, pop, drop, branch | R-6.5 |
| **Rebase todo** | `git-rebase-mode` | Planned structured interactive todo: pick/reword/edit/squash/fixup/drop/exec/label/merge lines with commit summaries; reorder | move up/down, set action, show commit, confirm, abort | R-TODO / R-7.3 |
| **Rebase in progress** | status section | Todo position, stopped commit, conflicts | continue, skip, abort, edit todo | R-OP-STATE / R-7.3 |
| **Merge / conflicts** | `magit-merge` + unmerged | Unmerged section; per-conflict actions | take ours/theirs, mark resolved, open merge tool (client effect `openMergeTool`), continue, abort | R-7.1 |
| **Sequencer** | `A`, `V` | In-progress cherry-pick/revert | continue, skip, abort | 7 |
| **Commit message** | with-editor buffer | Message editor (managed draft; native adapters pending) + companion read-only staged diff, server-owned history ring (R-EDIT-8), trailers, lint diagnostics | confirm, cancel, history prev/next, add trailer | R-EDIT |
| **Process log** | `magit-process-buffer` | Every Git invocation: args, exit code, duration, output, hook output | visit, copy command, clear | Completed (archive) |
| **Blame** | `magit-blame` | Annotated file (hash, author, date, summary); also inlay hints/code lens in real file buffers | show commit, blame at parent, copy hash | R-8.1 |
| **File at revision** | `magit-find-file` | Virtual doc `magit://blob/<rev>/<path>` | diff against worktree | R-5.3 |
| **Bisect** | `magit-bisect` | In-progress section: good/bad/skip log, candidate | good, bad, skip, run, reset | R-8.2 |
| **Worktrees** | `magit-worktree` | List with branch/path/state | add, checkout, move, delete, visit | R-8.2f |
| **Submodules** | `magit-submodule` | Modules section and list | add, register, update, sync, fetch, populate, unpopulate | R-8.2 |
| **Remotes / config** | `magit-remote`, `magit-gitconfig` | Remote list; per-branch/remote config | add, rename, remove, prune, set upstream/pushRemote | R-6.3 |
| **Patches** | `w`, `W` | `git am`/`apply`, `format-patch`; in-progress am | apply, continue, skip, abort | R-8.2 |
| **Notes / ignore / run** | `T`, `i`, `!` | Notes; gitignore targets; run arbitrary git command | per command | R-8.2a |
| **Repository list** | `magit-list-repositories` | Multi-repo overview | open status | R-9.3 |
| **Dispatch / help** | `magit-dispatch` | Generated from the command registry and keymap manifest | invoke by key | Completed (§10/archive) |
| **Menus** | transients | commit, branch, fetch, pull, push, merge, rebase, stash, tag, reset, cherry-pick, revert, bisect, remote, submodule, worktree, diff, log, ignore, patch | §3.11 | R-5.1 / R-5.2 |

---

## 7. Dependency-ordered implementation queue

Checkboxes are for tracking. IDs (`R-n.m`) are used in `Planned` messages.
Each task ends with **Done when**. Use relevant, risk-scoped checks per task;
run full `make test` about every three completed implementation tasks and at
phase/handoff boundaries. Full UI suites are reserved for broad UI/shared
lifecycle changes, phase completion, or suspected regressions. Preserve explicit
platform/security acceptance gates and report skipped or deferred checks
truthfully. Historical task Verify command lists and recorded evidence describe
what was required or run at that time; do not rewrite them to imply current
policy or erase that evidence.

### Review

Task specifications and open checkboxes remain below; this index summarizes
review status without duplicating the report ledger. A01–A10, A12, and actionable
A13–A21/A24–A34 are complete; A22/A23 were withdrawn. A11 is owned by
R-EDIT-6. [`REVIEW_REPORT.md`](./REVIEW_REPORT.md) is evidence, not scheduling
authority. Completed history is in the
[completed-roadmap archive](./docs/archive/completed-roadmap-2026-10-28.md).

#### R-COPY-SOURCE — Raw source for structured diff yanks

- [x] **[P1] Return raw source for structured diff yanks** (`src/commands.rs`; `src/tree.rs`; public copy/invoke seam)
  - **Problem:** Diff copy must yield code rather than rendered patch decoration, with exact structural identity even for literal marker characters.
  - **Why it matters:** Copying whole hunks, lines and visual selections should produce usable source without parsing display text or changing repository state.
  - **Change:** Implement whole-hunk, single-line and visual-selection raw-source yanks as server-owned structured-tree/content operations returning clipboard effects. Strip exactly one structural leading diff marker (`+`, `-`, or context space); preserve remaining code, indentation, literal `+`/`-` characters and line order; exclude file/hunk headers and no-newline annotations. Do not reconstruct old/new file semantics or add bindings beyond existing profile bindings.
  - **Done when:** Rust tests exercise the public command/request seam for added, removed and context lines, literal marker characters, indentation, multiple lines, and excluded metadata; none mutates Git. A scripted real-UI scenario verifies whole-hunk, single-line and visual-selection copy output.
  - **Constraints:** No client parsing of rendered text; use structured nodes/content. Never mutate Git. No new keybindings.

  - **Verified 2026-10-28:** `cargo test --test text_mode && cargo build && scripts/ui run tests/ui/scenarios/copy-source.scn && git diff --check` passed (6 Rust tests; native Vim at 100x30 and 80x24). Built `cargo build --release` first because client discovery prefers an existing release executable over debug. Inspected `build/ui/copy-source/vim-{100x30,80x24}/copied-{hunk,line,source}.screen`; scenario asserts exact JSON-encoded register contents and unchanged worktree/index. Public-seam tests cover leading blanks, literal markers, indentation, inclusive/reversed/single-line selections, metadata exclusion and unchanged HEAD/index/diffs/status. All owned UI/server processes stopped.
  - **Sprint scheduling:** Prioritize remaining views/log work and editor-independent rebase ahead of the blocked release; preserve the separate release work in progress.

#### R-NAV — Common navigation commands ownership

- [x] **Verify and finish the four still-Planned common navigation commands**
  (`common.cycle-visibility`, `common.next-sibling`, `common.prev-sibling`,
  `common.parent-section`).
  - **Problem:** R-2.3 is recorded complete, yet §5.1 still lists these four
    bindings as Planned. Either the implementation exists and the milestone
    label is wrong, or work remains; the roadmap must not silently delete the
    bindings or mark them implemented without evidence.
  - **Done when:** each command's tree-owned resolution is confirmed or
    implemented (no rendered-text parsing; Node-ID targets survive refresh),
    covered by in-process tests and focused real-UI evidence, and the §6/§10
    status labels match verified behaviour.
  - **Verify:** relevant Rust key/menu tests plus a focused `.scn` scenario.
  - **Verified 2026-10-28:** `cargo test --test navigation --test visibility --test commands --test keymap --test menu --test menus && cargo build --release && scripts/ui run tests/ui/scenarios/common-navigation.scn && git diff --check` passed (74 Rust tests; native Neovim Vim profile at 100x30 and 80x24). Inspected `build/ui/common-navigation/vim-80x24/{hierarchy,sections-only,folded-siblings,hunk-headers,expanded}.screen`: cursor assertions cover next/previous sibling, boundaries and body-to-parent; snapshots show all four detail levels. Rust coverage includes stable-ID targets after reordered refresh, retained/stale render rejection, hunk siblings, folded headers, roots, and empty-tree cycling. `Tree::sibling` is reusable by BUG-STAGE-NEXT. Owned UI/server processes stopped; pre-existing release work preserved separately.

### Active bugfix / UX

- [x] **[P1] BUG-STATUS-FOLD — Fold a hunk from its body**
  - **Requirement:** Invoking fold from the middle/body of a hunk folds only that hunk, not the whole file; folding from the hunk title continues to work.
  - **Done when:** Tree-owned cursor resolution folds the containing hunk from body positions without text parsing or positional hunk indices; focused Rust regression tests and a scripted native-UI scenario cover body and title folding.
  - **Implementation/evidence 2026-10-28 (full gate blocked):** Scoped source/test commit `bf8e73c` resolves body lines to their nearest collapsible hunk by tree ancestry, retaining both hunk titles, the file, and the actual sibling hunk body. `cargo test --test visibility` passed all 10 tests, including body/title folding and stable identities after refresh. A separate clean worktree at that commit built its own release binary and passed `scripts/ui run tests/ui/scenarios/hunk-body-fold.scn` (1 scenario PASS, 0 FAIL; Vim 100x30 and 80x24). Inspected `body-folded`, `refreshed-fold`, `title-unfolded`, and `title-folded` screens; retained clean-native evidence under `/tmp/opencode/fold-ticket/isolated-native/`.
  - **Gate closure 2026-10-29:** Repaired inherited R-NAV manifest snapshots with `UPDATE_EXPECT=1 cargo test --test manifest --test parity --test menu --test menus`; generated only `tests/snapshots/manifest-standard.json` and `tests/snapshots/manifest-vim.json` (the keybinding/menu snapshots did not change). Focused regeneration checks passed (51 tests); `cargo test --test visibility` passed all 10 tests; `git diff --check` passed. Existing folding native evidence above was inspected and remains sufficient. Committed the snapshot repair as `87e6a94`.
  - **Full committed-tree gate:** On a clean isolated worktree at `87e6a94`, `make test` passed build and Rust tests. First attempt timed out after 120s during Rust tests; retry with the isolated build target completed Rust and Neovim execution but failed one unrelated existing clipboard case: `tests/test_magit_lsp.lua:1249`, “yy copies the value at point to the clipboard” (expected 1, got nil). VSCode tests were not reached. Thus snapshot/visibility/folding evidence is verified; full suite has this remaining unrelated failure. Inherited release changes were preserved; temporary worktree removed.

- [x] **[P1] BUG-STAGE-NEXT — Advance to the next hunk after staging**
  - **Requirement:** Staging a hunk advances to the next hunk rather than following the staged hunk into its new location. Schedule after R-NAV `common.next-sibling` is complete and reuse its navigation logic.
  - **Done when:** Successor navigation uses stable tree identity and the shared `common.next-sibling` logic; focused Rust regression tests and a scripted native-UI scenario verify behavior across the staged relocation, with no text parsing or hunk indices.
  - **Evidence (2026-10-29):** Captures `Tree::sibling` successor and source file/section identities before mutation; after successful staging, resolves the first surviving target rather than following staged relocation. Rust regressions cover header/body advancement, final-hunk source file/section fallback, Git failure and no-op cursor preservation. `cargo test --test navigation --test commands` passed all 20 tests; `cargo build --release`, `scripts/ui run tests/ui/scenarios/stage-next-hunk.scn` (Neovim Vim 100x30 and 80x24), and `git diff --check` passed. Inspected before/header/body/final screens and scenario cursor assertions at both sizes: `build/ui/stage-next-hunk/vim-{100x30,80x24}/{before-stage,header-next-hunk,body-next-hunk,final-source-section}.screen`. The scenario folds the staged section before later steps to keep cursor assertions on-screen; owned UI processes were stopped by the runner (no server/Neovim processes remained).

- [x] **[P1] BUG-STAGED-STAGE — Ignore staging an already-staged hunk**
  - **Requirement:** Staging a hunk in the staged section is an ignored/no-op action, not a misleading “no diffs” error.
  - **Done when:** A focused Rust regression verifies the staged-hunk command is a no-op without an error, and a scripted native-UI scenario confirms the user-visible behavior.
  - **Evidence (2026-10-29):** `cargo test --test commands --test navigation`, `cargo build --release`, `scripts/ui run tests/ui/scenarios/staged-stage-noop.scn` (Neovim Vim 100x30 and 80x24), and `git diff --check` passed. The focused command regression checks staged body/header requests leave HEAD, index, worktree, effects and cursor unchanged. Native snapshots verified the staged-section no-op at both sizes; the runner stopped its owned processes.

- [x] **[P2] BUG-EDIT-WINDOW — Edit in the full-size current window**
  - **Requirement:** The client edit window replaces the current buffer in the full-size window instead of opening a split or smaller edit window.
  - **Done when:** A focused native-UI scenario verifies the edit replaces the current buffer in the full-size window without creating a split, and confirm/cancel restores the caller with unchanged dimensions.
  - **Verified follow-up (2026-10-29):** Retains the completed checkbox after repairing caller restoration (including ambiguous native deletion) and creating the previously missing `commit-edit-window.scn`. `nvim --headless --noplugin -u ./scripts/minimal_init.lua -c "lua MiniTest.run_file('tests/test_magit_lsp.lua', {collect={filter_cases=function(case) return table.concat(case.desc, ' '):find('managed', 1, true) ~= nil end}})"` passed both focused cases: window identity/count/dimensions and caller restoration, full-sync/version/history/error/disconnect policies, and real saved-checkpoint close. `scripts/ui run tests/ui/scenarios/commit-edit-window.scn tests/ui/scenarios/commit-draft-close.scn && git diff --check` exited 0 (2 PASS; each scenario under both profiles at 100x30 and 80x24). Inspected retained `build/ui/commit-edit-window/{vim-100x30,vim-80x24}/{full-size-draft,confirmed-caller,cancelled-caller}.screen` and draft-close choice/cancel/ambiguous-delete/recovered-close screens; no owned UI/server processes remained.
  - **Full-gate limits:** The earlier whole-file Lua attempt failed at the unrelated clipboard assertion (`tests/test_magit_lsp.lua:1249`); the earlier full `make test` failed in `tests/errors.rs` on a recommended-Git-version mismatch during concurrent release edits. Neither full gate is claimed green or repeated here; focused evidence above supersedes the earlier unverified command-only evidence. Release edits remain untouched.

- [x] **[P2] UX-VIM-HORIZONTAL — Allow h/l overrides in structured vim views**
  - **Requirement:** The vim profile may override native `h`/`l` horizontal motion in structured/read-only mode, where it is useful only in text mode; keep log on `l`, not `g l`, and preserve native text-mode editing.
  - **Done when:** Reconcile the contradictory §5 R4/keymap documentation and tests with the intended bindings; focused keymap regression tests and scripted native-UI evidence verify structured/read-only motion, log `l`, and unchanged native text-mode editing.
  - **Verified:** `cargo test --test keymap --test keys_protocol --test manifest --test parity && cargo build --release && scripts/ui run tests/ui/scenarios/vim-horizontal.scn && git diff --check` passed. Rust keymap coverage proves user `h`/`l` overrides resolve in structured status; Neovim Vim scenario verified `l` opens log and exact native text-mode `l` then `h` cursor-column movement at 100x30 and 80x24. Inspected `build/ui/vim-horizontal/vim-{100x30,80x24}/{structured-h,text-l,text-h,restored}.screen`; UI runner stopped owned processes.

### Deferred bugfix — minimal intake slice scheduled; larger design not scheduled

- [x] **[P1] BUG-READ-1 — Reads must stay responsive while a mutation is
  blocked inside real Git** (unsolved; moved out of the completed
  mutation-ordering review item, archived evidence). The minimal intake slice
  is scheduled in R-PLAIN-PREREQ; the larger design below remains deferred and
  is not scheduled until the user asks.
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
  approved; flagged for design/user review, not a decided shortcut; the larger
  design is not scheduled until the user asks):**
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

### Completed phases 0/1/2/3/3U/3N and Phase 3 continued

Completed milestones are summarized in the
[completed-roadmap archive](./docs/archive/completed-roadmap-2026-10-28.md);
these are not active acceptance queues. Canonical contracts remain in §§0–6
and 8–9. R-3.4b, R-UI-7, R-VS-1, R-NV-1…R-NV-4, and R-3.5…R-3.7 are
completed. Historical evidence and caveats are archived.

### Feature — Plain-client phase (target; pending implementation, before release prerequisites)

Canonical acceptance and verification bodies are in the linked [Plain-client
working list](./magit-lsp-plain-client-tasks.md); this is the scanner-visible
active queue. Intended Plain contracts are not current behavior. R-EDIT-REAL and
its step-0 spike remain pending; R-DIFF-SAFETY ownership/scope are unchanged.
The minimal BUG-READ-1 intake slice is scheduled; its larger design remains
deferred. Release requires R-4.1a, R-7.3a, R-CLIENT-SPEC and R-PLAIN-GATE.

- [x] **R-PLAIN-DOC — Record decisions and phase queue.** Documentation only;
  this does not complete R-EDIT-REAL or its spike.
- [x] **R-EDIT-REAL STEP0 lifecycle observer slice (2026-10-29).** Native isolated Codium clean close-window, reload-window and same-window folder-change observed with external supervisor and persistent API/wire logs: zero qualifying draft `event.closed` events. Actual deactivation/restart and pre-action UI evidence inspected; see `docs/evidence/real-file-client-spike.md`. This scoped evidence supersedes the unverified-emission note below only for these exact cases. No production changes, overall step-0 acceptance or parent checkoff; cross-build/dirty/multi-root lifecycle behavior remains unverified.
- [x] **R-EDIT-REAL Neovim native buffer-close slice (2026-10-29).** Removed the dirty-close picker and command-line interception; real files retain current-window placement and use `bufhidden=wipe`. `common.quit` uses native `:bdelete`, with native dirty refusal, not a custom picker. Explicit terminal confirm/abort restores status even when the draft was modified. Retain the closed run/snapshot until terminal progress; a rejected saved-file close recreates an editable recovery buffer with its keymaps/full-sync observer, leaves the helper pending, and permits retry. History removal remains separate.
  - **Fresh verification:** `make test-lua && scripts/ui run tests/ui/scenarios/commit-draft-close.scn && scripts/ui run tests/ui/scenarios/commit-confirm.scn && git diff --check` passed (56 Lua cases; both scenarios pass standard/vim at 100x30 and 80x24). Inspected `build/ui/commit-draft-close/{standard,vim}-{100x30,80x24}/{dirty-native-refusal,native-save-and-close,unsaved-after-checkpoint,native-discard-to-saved,explicitly-aborted,failed-close-retained,recovery-text,recovered-close-committed}.screen` and `build/ui/commit-confirm/{standard,vim}-{100x30,80x24}/{editor,staged}.screen`. Git assertions prove `:wq` saves/submits latest text, `:q!` submits only the prior saved checkpoint, explicit abort leaves HEAD/index/worktree unchanged, and a symlink-rejected close preserves text and commits once after repair/retry. Native external-file-change protection requires `:write!` after that deliberate replacement. Lua additionally checks recovery of unsaved text and absence of command-line `<CR>` mapping.
  - **Limits:** the native scenario creates an actual additional window before `:q`/`:wq`; it proves closing a file while Neovim stays alive, not last-window editor exit. Last-window native `:q`/`:wq` still exits Neovim; teardown while `v:exiting` is set does not send confirmation, and existing VimLeave/disconnect cleanup cancels the helper. No native quit override or fresh shutdown/reload/platform acceptance is claimed. Parent R-EDIT-REAL remains unchecked. Failed attempts exposed native hit-enter handling, dirty terminal restoration and external-file retry protection; no timeout was increased. All owned UI sessions were stopped; no owned server remained after verification.
- [x] **R-EDIT-REAL VSCode native-close slice (2026-10-29).** `magit.closeEdit` now delegates to the actual workbench close workflow, not a fabricated QuickPick or finish-before-close. Only an actual owned last-tab closure across all groups sends run-aware saved-file confirmation; document close/language change never confirms. Track owned tabs on open/change events too. The integration timeout was a fake-workbench identity bug: its tab getter/close event fabricated different objects, so the production ownership guard correctly rejected the event. The integration fake now retains stable native tab identities; no production guard was weakened.
  - **Verification:** `make vscode-test && npm --prefix clients/vscode run test:integration && MAGIT_VSCODE_BIN=/home/william/.nix-profile/bin/codium MAGIT_VSCODE_EDIT_ONLY=1 npm --prefix clients/vscode run test:e2e && git diff --check` passed. Focused mode uses a normal production extension host (extensionTestsPath suppresses native dirty dialogs), shares first-open setup, fails on missing prerequisites, and drives actual Save/Don't Save/Cancel with CDP keys. Removed obsolete custom-picker expectations from the late REDIT5 block; the native helper owns this coverage instead.
  - **Inspected native evidence:** `/tmp/opencode/vscode-review-uDwXKm/real-file-native-evidence.json` and `real-file-native-trace.json`; screenshots `real-file-native-{save-alone,two-groups,nonlast-close,cancel-retained}.png`, `real-file-native-2-{cancel,save}-dialog.png`, `real-file-native-3-discard-dialog.png`, and `real-file-native-3-committed.png`. Save alone, switching/focus, language change and nonlast two-group close send no confirmation/change no HEAD. Cancel retains the dirty tab/run. Saved last-close commits exact subject/body `native saved message\n\nexact body` (HEAD `14b8cf0d72162ded52943209cc9e841138dc7b3a`); native Save commits `native Save new bytes\n\nnew body` (`01f0e865e4080264e53f64b04177d8167a929e07`); Don't Save commits `native PREVIOUS saved bytes\n\nold body` (`48eb44fbaf3427d68ab696a7c53b5382dec120c4`) despite verified unsaved full-sync version 3. Each run issues exactly one saved-file confirmation/commit, and subsequent runs have fresh Git text and distinct run IDs. Owned native host/server processes were absent after cleanup. No synthetic didClose, observer-spike substitution or shutdown confirmation; history removal is a separate pending slice and parent R-EDIT-REAL remains unchecked.
  - [ ] **R-EDIT-REAL — Proposed real `COMMIT_EDITMSG` workflow/cutover.** Current managed drafts remain authoritative until acceptance. Default `didClose` confirms saved-file text; this is an explicit accepted assumption despite standard LSP lacking run identity and not proving reliability/late-event behavior. VS Code is the known exception: verified tabGroups last-matching-tab run-aware executeCommand fallback, ignoring language-change didClose as confirmation. Core fallback, dirty choices and scoped clean lifecycle observations satisfy their respective step-0 checks; historical Neovim evidence is accepted without claiming rerun. No additional negotiation prerequisite. Parent remains unchecked: production migration, idempotency and acceptance remain pending. See §3.10 and linked ticket for shared session/owner/run-aware confirmation; saved-file close versus exact-version synchronized confirmation/writeback; atomic terminal/abort arbitration; and no duplicate helper release/commit. Preserve reusable document store/bounds and explicit-confirm barriers; saved-file close cannot use unsaved synced text. Test Git effective-empty/comments/configured-template semantics; distinguish native Discard-unsaved, explicit Abort and VS Code Git Discard-clear/save. See `docs/evidence/real-file-client-spike.md`.
- [ ] **R-PLAIN-HARNESS — Fake-editor conformance harness.** Scripted stdio LSP, document model, versioned applyEdit/showDocument/progress and quirk profiles. **Acceptance warning:** baseline commit `2b15cad` is partial scaffolding, not conformance: decomposition produces/asserts one rangeless change rather than many ranged changes; reorder test fabricates a local response rather than testing actual emit order; no real-binary FakeEditor pump services outbound requests; progress method is `workDoneProgress/create` instead of `window/workDoneProgress/create`; `respond` lacks applyEdit dispatch; positional edit ordering/overlap checks are inadequate. Keep unchecked until these gaps are fixed and accepted. Verify: `make test-rust FILTER="--test plain_harness"` (test alone is not full conformance acceptance).
- [ ] **R-PLAIN-PREREQ — Outbound requests, progress gating, minimal BUG-READ-1.** Gate progress on capability, avoid holding sessions over Git, remove test-delay hooks, and correlate outbound requests without blocking intake. Larger BUG-READ-1 design remains deferred. Verify: `make test-rust FILTER="--test server_async"`; `make test-rust`.
- [ ] **R-PLAIN-SYNC — Document store and sync mode.** URI-keyed sequential changes with negotiated encoding and bounds; incremental sync; cover multibyte, CRLF, invalid ranges and bundled clients. Verify: `make test-rust FILTER="--test plain_sync"`; `make test-lua`; `npm --prefix clients/vscode run test:integration`.
- [ ] **R-PLAIN-CORE — Presentation seam and unified dispatcher.** Preserve rich behavior; absent presentation selects Plain. One semantic dispatcher serves custom key/invoke and Plain executeCommand. Acceptance includes concrete-view serving/stacking, parameterized stable-ID single/multi picker with caller restoration, provider-file message edits with finish/cancel callbacks, and fake-provider independence without jj implementation. Verify: `make test`; `make ui-test`.
- [ ] **R-DIFF-SAFETY — Displayed-patch identity and filtered-mutation prerequisite.** Existing roadmap ownership/scope unchanged; R-PLAIN-TARGET depends on it. Verify identity and stale/filtered patch tests.
- [ ] **R-PLAIN-TARGET — Content-addressed action targets.** Revalidate node identity plus mutation hash against fresh backend state; stale means refresh/no mutation; cover rich and Plain paths. Verify: `make test-rust FILTER="--test commands"` plus patch proptests.
- [ ] **R-PLAIN-REPO — Repository/document resolution.** Resolve explicit/workspace/document context without guessing among ambiguous candidates; exclude presentation docs from untracked/watcher noise. Verify: `make test-rust FILTER="--test plain_repo"`.
- [ ] **R-PLAIN-RENDER — Bootstrap and versioned applyEdit pipeline.** Authoritative tree text, minimal encoding-correct edits, serialized/coalesced apply, echo reconciliation, divergence repair and stale retry. Verify: `make test-rust FILTER="--test plain_render"`.
- [ ] **R-PLAIN-ACTIONS — Code actions and executeCommand.** Side-effect-free snapshot-matched actions, dispatcher revalidation, range conversion and stale/latency coverage. Verify: `make test-rust FILTER="--test plain_actions"`.
- [ ] **R-PLAIN-STACK — View stack and menus in one document.** Concrete logical views are served/stacked in one document with identities and caller restoration; Back and any-line actions. Verify: `make test-rust FILTER="--test plain_stack"`.
- [ ] **R-PLAIN-VIS — Visibility, folding fallback and providers.** Expanded default, bounded large sections, visibility code actions and standard providers from matching snapshots. Verify: `make test-rust FILTER="--test plain_vis"`.
- [ ] **R-PLAIN-PROMPT — Document-backed prompts and support table.** Field-backed prompt kinds/completion, command effect support and safe unsupported behavior. Verify: `make test-rust FILTER="--test plain_prompt --test manifest --test parity"`.
- [ ] **R-PLAIN-SELECT — Parameterized stable-ID selection.** Single/multi selection, stable identity and caller restoration on confirm/cancel; no client picker or text parsing. Verify: `make test-rust FILTER="--test plain_select"`.
- [ ] **R-PLAIN-DRAFT — Provider-file commit editing.** Follow §3.10/R-EDIT-REAL lifecycle: canonical Git COMMIT_EDITMSG, saved-file close versus exact-version synchronized confirmation, planned `magit.command` executeCommand routing and terminal/abort idempotence; refresh open file and reject unrelated documents. Verify: `make test-rust FILTER="--test plain_draft --test editor_bridge"`.
- [ ] **R-PLAIN-E2E — Vertical slice/native evidence.** Plain harness and plugin-free Neovim cover status/stage/stale/menu/commit exact message and abort. Verify focused tests and linked UI scenarios.
- [ ] **R-PLAIN-HELIX — Helix manual smoke.** Record version/date and generic-configuration workflow results. Verify reviewed evidence.
- [ ] **R-PLAIN-SPEC — Client spec, README and release wording.** Document capabilities, sync/echo/actions/commit/limits/example and harness checklist; cross-check protocol/tests. Verify links and `git diff --check`.
- [ ] **R-PLAIN-GATE — Release-slice acceptance through Plain.** Run R-4.1a/R-7.3a through harness with exact repository assertions, regenerate support table and run full gate. Verify: `make test`; `make test-rust FILTER="--test plain_e2e"`.

### Feature — release prerequisites

The 0.2 release queue is strictly ordered below. Deferred platform work and
BUG-READ-1's larger design are not release gates; its minimal R-PLAIN-PREREQ
slice is scheduled and required before the Plain gate.

- [ ] **R-ARGS — Typed operation and argument propagation**
  - **Size:** S.
  - **Depends on:** R-5.1.
  - **Done when:** menu arguments survive command dispatch to Git as typed operation arguments; do not model them as a commit boolean. Invalid/missing arguments fail readably.
  - **Verify:** focused command/menu Rust tests and exact Git argv/state assertions.
- [ ] **R-SELECT-OPS — Selected commit operation continuation**
  - **Size:** M.
  - **Depends on:** R-ARGS and R-5.1.
  - **Done when:** selected full OID, captured operation arguments and caller restoration survive select-mode continuation; confirm/cancel and stale selection are covered without parsing rendered text.
  - **Verify:** focused session/command Rust tests and native select confirm/cancel scenarios in both clients.

- [ ] **R-4.1a — Minimal release commit-selection integration** (stable parent
      R-4.1; release slice only). Add a real target-dependent commit action
      (fixup) using R-5.1 server-owned log/select infrastructure, not a client
      picker. Broad menus and unrelated actions do not gate release.
      **Depends on:** R-5.1, R-ARGS and R-SELECT-OPS.
       **Plain acceptance:** also accepted through the Plain harness at R-PLAIN-GATE.
       **Done when:** exact repository-state tests prove the selected commit is
      the target; cancel causes no mutation and restores caller; confirm returns
      to caller; failures are readable. Both clients have focused native
      selection/confirm/cancel evidence.
      **Verify:** focused Rust state tests and native scenarios in both clients;
      full regression gate at milestone boundary.
- [x] **R-EDIT-8 — Server-owned commit-message history ring**
  Bounded server retention remains implemented, including saved finish/abort retention. Rich history browsing is superseded by R-EDIT-REAL; native recall is deferred to R-EDIT-HISTORY, not an R-EDIT-4/5/6 acceptance obligation. Private unit tests cover eligibility, exact deduplication, caps and isolation without a public browsing API.
- [x] **R-EDIT-4 — Neovim draft adapter** (historical; superseded by R-EDIT-REAL)
  - **Candidate paths:** `lua/magit-lsp/edit.lua`, `lua/magit-lsp/init.lua`, `lua/magit-lsp/view.lua`, `lua/magit-lsp/buffer.lua` (only where sync/attachment/capabilities require), `tests/test_magit_lsp.lua`, `tests/ui/scenarios/commit-cancel-progress.scn`, `ui-test-plan.md`; not an exhaustive rewrite list.
  - **Depends on:** R-EDIT-3 and R-EDIT-8.
  - **Done when:** native acwrite/scratch syncs versions; save checkpoints only, explicit confirm flushes, normal user close confirms the saved checkpoint, and explicit abort/disconnect cancel (§3.10 defines dirty Save/Discard/Cancel and non-user-close/shutdown guards); status/text-mode unchanged. Native history-prev/next recall and return-to-current-draft behavior is covered.
  - **Completed (2026-10-28):** managed URI-owned acwrite buffer with one explicit full-sync stream and monotone versions; `:write` checkpoint stays editing; explicit confirm awaits save/finish; typed native close (`:q`/`:close`/`:bd`/`:wq`/`:xit`) is classified before teardown, clean close confirms the checkpoint, dirty close prompts Save/Discard/Cancel, ambiguous unload/API deletion and disconnect cancel, and failed barriers retain a recoverable buffer. Server-owned history prev/next restores the unsent draft without echo or dirty save. Exact version/identity/owner checks and history policy remain server-owned. Status readonly/A29/text mode unchanged.
  - **Evidence:** `make test-lua`; real native `commit-cancel-progress.scn`, `commit-draft-history.scn`, `commit-draft-close.scn` and `commit-owned-cancel.scn` screens/state/history/close inspected, owned sessions stopped; `git diff --check` clean (see §3.10).
  - **Verify:** `make test-lua`; `scripts/ui run tests/ui/scenarios/commit-cancel-progress.scn`; `scripts/ui run tests/ui/scenarios/commit-draft-history.scn`; `scripts/ui run tests/ui/scenarios/commit-draft-close.scn`; `scripts/ui run tests/ui/scenarios/commit-owned-cancel.scn`; inspect screens/state and stop owned UI, including native history recall/return-to-current-draft behavior.
- [x] **R-EDIT-5 — VSCode draft adapter** (historical; superseded by R-EDIT-REAL)
  - **Candidate paths:** `clients/vscode/lib/edits.js`, `clients/vscode/lib/edit.js`, proposed `clients/vscode/lib/drafts.js` (writable provider if needed), `clients/vscode/package.json`, `clients/vscode/test/test.js`, `clients/vscode/test/integration.js`, `clients/vscode/test/e2e/suite.js`; not an exhaustive rewrite list.
  - **Depends on:** R-EDIT-3 and R-EDIT-8.
  - **Done when:** thin writable provider syncs exact versions; save never confirms; confirm barrier flushes; normal user close confirms the saved checkpoint, explicit abort/disconnect cancel (§3.10 defines dirty Save/Discard/Cancel and non-user-close/shutdown guards); close/recovery safe and status readonly. Native history-prev/next recall and return-to-current-draft behavior is covered.
  - **Completed (2026-10-28):** managed URI-owned writable provider with one explicit full-sync stream and monotone versions; `Ctrl+S` checkpoints and stays editing; explicit confirm flushes/saves/finishes; explicit close classifies intent and offers a native Save/Discard/Cancel picker when dirty; raw tab X/Ctrl+W, shutdown and API disposal are ambiguous and abort — VSCode retains virtual `magit-commit` documents after a tab close (so `onDidCloseTextDocument` never fires), detected through tab-group teardown. History prev/next restores the unsent draft without echo or dirty save and adopts the returned version; API-edit races keep local text and resync above the returned version; stale-sync/version errors stay recoverable; disconnect cancels pending runs while retaining text. Status readonly/A29/text mode unchanged.
  - **Evidence:** `npm --prefix clients/vscode run test:integration` and `npm --prefix clients/vscode run test:e2e` pass against the real server; real native `r-edit-5-*.png` screenshots and `r-edit-5-evidence.json` / `r-edit-5-disconnect.json` inspected; save-stays-editing, confirm, abort, dirty Save/Discard/Cancel, ambiguous tab close, progress cancel, version-error recovery, history recall and return-to-current-draft covered; owned UI stopped; `git diff --check` clean (see §3.10).
  - **Verify:** `make vscode-test`; extend `clients/vscode/test/integration.js` for real-server/scripted-widget integration (`npm --prefix clients/vscode run test:integration`), and `clients/vscode/test/e2e/suite.js` for actual native VSCode confirm/cancel/progress/close/version-error and history recall/return-to-current-draft scenarios (`npm --prefix clients/vscode run test:e2e`). The former is not native-client evidence.
- [x] **A11 / R-EDIT-6 — Migrate A11 and adversarial cutover**
  - **Candidate paths:** `src/editor.rs`, `src/runner.rs`, `src/git.rs`, `src/server.rs`, `src/session.rs` (dispatch), `src/protocol.rs`, `lua/magit-lsp/edit.lua` and relevant init/view/buffer attachment/capabilities from R-EDIT-4, `clients/vscode/lib/edits.js`, `clients/vscode/lib/edit.js`, proposed `clients/vscode/lib/drafts.js` if needed, `tests/editor_bridge.rs`, `tests/test_magit_lsp.lua`, `clients/vscode/test/test.js`, `clients/vscode/test/integration.js`, `clients/vscode/test/e2e/suite.js`, `tests/ui/scenarios/commit-cancel-progress.scn`, `ui-test-plan.md`; not an exhaustive rewrite list.
  - **Depends on:** Linux R-EDIT-2 acceptance, R-EDIT-3…R-EDIT-5 and R-EDIT-8; sole A11 implementation owner. R-EDIT-9/macOS execution is not a Linux client-cutover dependency.
  - **Done when:** remove realFile commit-message compatibility at cutover; Git keeps hooks/cleanup. Native acceptance covers save-stays-editing, normal close commits exactly saved message, dirty Save/Discard/Cancel, no hidden changes submitted, explicit abort versus close, disconnect across phases, stale/terminal cleanup without commits, and failed close recovery. Also cover pre-hook/editor-active/commit-msg/finalizing and finish-v-cancel (cancel before commit aborts; completed commit reports reality), stale/late/version/multi-root, exact HEAD/index/worktree and neighboring content, inherited pipes/bounded drain, TERM resistance, subsequent commit/no lock cleanup, lock warning/no foreign kills; preserve askpass/A29/text mode and document limits. Extend integration.js and native e2e/suite.js at the paths above; scripted widgets alone do not meet native acceptance. R-EDIT-8 verifies two-root ring ordering/dedup/eviction and current-draft restoration.
  - **Completed (2026-10-28):** realFile commit-message compatibility is removed at cutover: `EditDocumentMode` is ManagedDraft-only with no realFile path in `src/`; the sole remaining `realFile` reference is a negative VSCode test case asserting such an effect is rejected. Git owns hooks/cleanup/comment-stripping; the server never runs `git commit -F` (drafts write through the editor-bridge helper, §3.10). One adversarial gap found during final acceptance was closed in `tests/server_async.rs`: disconnect during the commit-msg hook phase — `disconnect_during_commit_msg_hook_cancels_owned_hook_group_without_committing` proves EOF cancels the owned resistant hook group, leaves exact HEAD/index/write-tree/worktree and neighboring content, and allows a subsequent commit. macOS/Windows remain deferred (R-EDIT-9/R-EDIT-7); BUG-READ-1 probe remains ignored; minimal intake slice now scheduled under R-PLAIN-PREREQ.
  - **Evidence:** Full managed-draft gate passed on 2026-10-28 (details in §3.10). It is not evidence for R-EDIT-REAL's spike, real-file behavior or Magit ring parity; these remain pending.
  - **Verify:** `cargo test --test editor_bridge --test server_async && make test-lua && make vscode-test && npm --prefix clients/vscode run test:integration && npm --prefix clients/vscode run test:e2e && make test && scripts/ui run tests/ui/scenarios/commit-cancel-progress.scn && make ui-tour` on Linux; inspect relevant tour excerpts. Linux real-Git and both clients' native evidence are required; macOS remains unverified under later R-EDIT-9 and does not block Linux acceptance/cutover. No timeout inflation.
- [ ] **R-EDIT-KINDS — Sequential editor-helper kinds**
  - **Size:** S.
  - **Depends on:** R-EDIT-3.
  - **Done when:** sequential commit-message and todo helper kinds are validated against the expected target, with per-kind policy; todo does not inherit commit history or close rules.
  - **Verify:** focused helper/editor Rust tests for allowed kinds, target rejection and policy isolation; no UI change.
- [ ] **R-OP-STATE — Rebase operation state**
  - **Size:** M.
  - **Depends on:** R-ARGS and R-EDIT-KINDS.
  - **Done when:** discover rebase state and in-progress operation; continue, skip, abort and conflict outcomes are represented and reported safely.
  - **Verify:** focused real-repository Rust tests for discovery and each outcome; native status/rebase-state check in both clients.
- [ ] **R-TODO — Structured linear rebase todo**
  - **Size:** M.
  - **Depends on:** R-EDIT-KINDS and R-OP-STATE.
  - **Done when:** server parses, edits, reorders, validates and writes back structured linear todo; unsupported grammar fails safely. Full rebase-merges support remains later.
  - **Verify:** focused parser/writeback Rust tests, malformed/unsupported grammar cases, and native todo edit/reorder scenarios in both clients.
- [ ] **R-7.3a — Interactive rebase and editable todo** (stable parent R-7.3;
      release slice only). Implement interactive rebase using server-owned
      selection and structured editable todo; unrelated history/branch features
      do not gate release.
      **Depends on:** R-ARGS, R-SELECT-OPS, R-EDIT-6, R-EDIT-KINDS,
      R-OP-STATE and R-TODO; plus Phase 3E Linux lifecycle/editor bridge acceptance.
      **Done when:** exact repository-state tests cover selected base, edited
      todo execution, continue, abort and conflict state; cancel before execution
      does not mutate and failures are readable. Both clients have focused native
       evidence for base selection and todo editing/execution. Also accepted
       **Plain acceptance:** also accepted through the Plain harness at R-PLAIN-GATE.
      **Verify:** focused real-repository Rust tests and both clients' native
      scenarios; full regression/phase evidence at milestone boundary.

### R-CLIENT-SPEC — External client implementation spec (release prerequisite)

- [x] Investigate current `src/protocol.rs` wire contracts and both clients;
  develop `docs/CLIENT_SPEC.md` as a practical implementation plan for external
  clients. Inventory standard LSP/custom methods, capabilities and lifecycle,
  version/ack rules; view/display/navigation/selection abilities; keyboard
  capture and input-forwarding alternatives; editable bridge behavior; and
  whether split/companion views are needed. Separate REQUIRED core-flow
  capabilities from RECOMMENDED capabilities useful/necessary only for named
  non-core workflows. Map features to capabilities, explain rationale, and give
  unsupported/degraded behavior or alternatives, an implementation checklist
  and example, and unresolved design decisions/follow-ups. Preserve server-owned
  text/structure/actions and thin-client invariants; ground claims in current
  contracts and clients without silently broadening core requirements.
  **Done when:** the spec is reviewed against protocol authority and both
  clients, with explicit core/non-core mapping and unresolved decisions.
  **Verify:** documentation review and cross-check every protocol claim against
   `src/protocol.rs` and both client implementations; no implementation/UI tests.
  **Review evidence (2026-10-28):** expanded `docs/CLIENT_SPEC.md` cross-checked
  against protocol, server/session/view contracts and both bundled client source
  inventories. Includes method/lifecycle inventory, render/ack/resync sequencing,
  input/navigation semantics, draft barriers and cancellation boundaries, and
  core/non-core capability matrix. `git diff --check` passed.
  Draft/evidence pointer: `docs/CLIENT_SPEC.md`.
  **Re-review (2026-10-28):** reopened: the draft still leaves implemented
  providers unresolved, omits active error/progress messages, and misstates
  render/text-mode behavior. The source-only documentation corrections below
  must be reviewed before this parent is complete; earlier evidence is retained.
  **Second source-only re-review (2026-10-28):** parent remains unchecked.
  Provider/capability inventory, render/edit/version rules, text-mode/client
  differences, progress handling, and reduced-client workflow exclusion now
  match the inspected sources. Exact history command IDs are now correct.
  Reopened the draft-reference and core-payload/example tasks below: current
  payload facts are still missing or contradicted by the new quick reference.
  Future negotiation/layout/native-close design questions may remain explicitly
  unresolved; they do not block this task. No implementation/UI tests or commits.
  **Third source-only re-review (2026-10-28):** parent remains unchecked;
  one P1 remains in the existing core-reference/example task. The draft task's
  remaining acceptance criteria are now met: announcement/result fields,
  history IDs, checkpoint/finish/history version semantics, nullable draft-error
  version and both clients' reconciliation/close distinctions match source.
  The core reference's invented fields and edit shape are corrected, but its
  replacement example regresses initialization placement and contradicts actual
  progress ordering, omits the create response, and contains non-JSON syntax.
  These are current-contract/example blockers only, not new design/release work.
  **Final source-review evidence (2026-10-28):** no remaining documentation
  blockers against the recorded acceptance criteria. Verified the final exchange
  and ordering prose at `docs/CLIENT_SPEC.md:115-138`: custom preferences are in
  `params.capabilities` (`src/server.rs:2001-2015`); capability/keymap excerpts
  are explicitly labelled; all 16 example messages parse as JSON; progress
  create/begin, correlated client create response, end, key result, render and
  render acknowledgement match the permitted sequence
  (`src/server.rs:294-343,1012-1026,1054-1070`). The prose distinguishes early
  progress from deferred view notifications and does not make the create
  response a server barrier. Prior draft/source acceptance remains verified.
  R-CLIENT-SPEC is ready for worker checkbox completion; reviewer policy leaves
  checkbox ownership with workers. Only this section was updated; no commits,
  implementation tests, UI tests, release work or future-design expansion.

   - [x] **[P1] Inventory the implemented standard providers explicitly** (`docs/CLIENT_SPEC.md:21-43,105-113`)
    - **Problem:** The spec delegates its actual standard-provider inventory to a future source check and groups implemented definition with unimplemented hover/references/formatting. An external implementer cannot tell which navigation/display APIs are available from the document.
    - **Why it matters:** Correctness and cohesion: this is existing contract information, not an unresolved design decision.
    - **Change:** Replace the source-dependent paragraph and matching follow-up with the exact advertised/handled set: `textDocument/documentSymbol`, `textDocument/definition`, `textDocument/documentLink` (no resolve), `textDocument/foldingRange`, and `textDocument/semanticTokens/full` (no range/delta). Describe their purposes, URI-based requests, acknowledged-tree lookup with current-tree fallback before the first ack, and `null` results in text mode. Document full draft sync `{openClose:true,change:1}` and the semantic legend's custom `tokenTypeGroups`. Cite `src/server.rs:1371-1431,1883-1926` and `src/session.rs:1581-1590`. Keep correct initialization placement: `params.capabilities.keymapDelivery/keyModifiers`, not initialization options (`src/server.rs:2001-2015`); profile is in initialization options in BOTH clients (`lua/magit-lsp/init.lua:354-365`, `clients/vscode/lib/session.js:26-46`). State that modifiers are currently parsed/logged, not a negotiated filtering guarantee (`src/server.rs:1313-1321`). Clarify that `workspace/didChangeConfiguration` triggers configuration reload, rather than promising ingestion of its settings payload (`src/server.rs:902-913`).
    - **Done when:** The standard provider set and initialization fields are concrete, no current-provider verification remains unresolved, and optional presentation providers are not made prerequisites for basic status/staging.
     - **Constraints:** Documentation only; do not add providers, move capability parsing, or invent negotiation.
     - **Evidence (2026-10-28):** Replaced provider uncertainty with exact advertised/handled set and positional lookup/text-mode behavior; corrected initialization capability/profile placement, modifier semantics and configuration reload description. Cross-checked against cited server/session/client source; `git diff --check` passed.

   - [x] **[P1] Correct render variants and edit/version application rules** (`docs/CLIENT_SPEC.md:31-36,49-56`)
    - **Problem:** The spec describes only full/edit renders, requires array-order edits unlike both clients, and calls keymap/render versions independent counters. Entering text mode actually emits a metadata-only render at the unchanged version; keymap versions currently reuse the view version.
    - **Why it matters:** Correctness: an adapter following the current instructions can reject a real render or mishandle keymap updates and line offsets.
    - **Change:** Document the third render form: neither `text` nor `edits`, preserving content/version while applying mode/pending/menu/cursor metadata and acknowledging application. Cite `src/session.rs:1306-1320` and `lua/magit-lsp/view.lua:368-401`. Explain full-text precedence and exclusive line endpoints. Replace the unsupported general array-order mandate with the observed contract: the current producer returns zero or one prefix/suffix replacement (`src/diff_text.rs:9-37`); both clients apply supplied edits bottom-up by `startLine` (`lua/magit-lsp/buffer.lua:117-127`, `clients/vscode/lib/render.js:44-54`). Do not assert sequential-coordinate or arbitrary overlapping-edit semantics that have not been defined. Replace 'independent counters' with separate render/keymap bookkeeping: normal keymap notices use the view's render version (`src/session.rs:418-440`), draft keymaps use 0 (`src/server.rs:1723-1737`), and a keymap notification is a complete replacement, not a render ack or a guaranteed newly incremented counter.
    - **Done when:** The method table/application algorithm handles full, edit and metadata-only payloads, matches actual client edit application, and does not impose a fictitious keymap counter.
     - **Constraints:** Do not change the server/client diff algorithms or define a new multi-edit protocol in documentation; identify genuinely unspecified general cases as such.
     - **Evidence (2026-10-28):** Documented metadata-only renders, full-text precedence, half-open edits and bottom-up client application with the current zero/one edit producer; clarified view-version reuse and draft keymap version 0. Cross-checked cited session/server/diff and both client sources; `git diff --check` passed.

   - [x] **[P1] Specify text mode as local plain-text editing with explicit limits** (`docs/CLIENT_SPEC.md:13,54,60-64,110`)
    - **Problem:** 'Not permission to reinterpret server text as editable source' obscures the intentional editable text-mode contract, and the draft omits command refusal, refresh deferral and replacement-on-exit. Marking setMode support optional also fails to explain that ordinary forwarded keys can still invoke the text-mode toggle.
    - **Why it matters:** Correctness and readability: clients must distinguish disposable local editing from Git edits and managed draft synchronization, and must not claim editable parity where VS Code remains read-only.
    - **Change:** Explain interactive read-only display versus text mode's local scratch editing/copying: edits are not synchronized to Git or to the server tree and are discarded by a full server replacement on exit. In text mode capture is off, only the toggle remains bound, other invokes are refused with a readable hint, refresh requests are queued, and positional standard providers return null (`src/session.rs:137-154,414-440,1210-1244,1505-1519`; `src/server.rs:1371-1431`). Describe Neovim's modifiable buffer/restored undo and undo clearing on exit (`lua/magit-lsp/buffer.lua:130-144`) separately from VS Code's read-only content provider (`clients/vscode/lib/session.js:65-72`). Distinguish not exposing a native `view/setMode` control from still needing to honor mode changes produced by enabled server keys. Cross-reference the metadata-only entry render correction.
    - **Done when:** An external adapter can implement entry/editing/refresh/exit without sending view edits as managed draft changes, and the two clients' editable/read-only difference is explicit.
     - **Constraints:** Documentation only; do not broaden core requirements into a mandatory native editable editor or claim VS Code currently implements editable text mode.
     - **Evidence (2026-10-28):** Clarified metadata-only entry/ack, local disposable Neovim editing and undo behavior versus VS Code's read-only provider, toggle-only capture, command refusal, refresh deferral, null positional providers, full-replacement exit, and mode handling without native setMode. Cross-checked against cited session/server/client sources; `git diff --check` passed.

   - [x] **[P1] Document managed draft payloads, rejection recovery and abort routing** (`docs/CLIENT_SPEC.md:40-41,84-88,95,127`)
    - **Problem:** A single prose paragraph omits the `draft/error` notification, actual barrier result/error shapes and the wire operation for abort. The vague instruction to verify VS Code's status leaves already-observable close behavior unresolved.
    - **Why it matters:** Correctness: ignoring rejected full sync or mistaking didClose/finish for successful commit can lose recoverability or leave Git waiting for an editor.
    - **Change:** Add a compact managed-draft reference for `editDocument` fields and `mode:"managedDraft"`; save/history `{runId,uri,version}` plus history `command`; finish additionally `reason`; results `draft/checkpointed`, `draft/finished`, and `draft/replaced` with `baseVersion/version/text`. Name the two accepted history command IDs, no-text barrier requirement, exact first-open text/version, one full-sync change event per notification, increasing i32 versions and 1 MiB UTF-8 limit (`src/protocol.rs:60-159`, `src/server.rs:1169-1213`, `src/editor/draft.rs:155-251,310-316`). Add `draft/error` to the method inventory with `{uri,version,method,message,recoverable}`; separately describe JSON-RPC InvalidParams barrier errors with `data.{runId,uri,version,recoverable}` (`src/server.rs:858-874,919-926`). Require readable errors, retained local text, invalidated in-flight barriers and no finish after a rejected sync until recovered; `recoverable:true` is not permission to revive a terminal run. Document explicit abort as `view/invoke` on the owning ordinary view, `command:"common.cancel",args:{runId}`, not `draft/abort` or didClose (`src/commands.rs:1105-1126`; both clients' edit cancellation paths). State that `draft/finished` means writeback/helper release, not Git commit success; later effects/progress report the Git result (`src/server.rs:1149-1158,1808-1863`). Replace the VS Code verification placeholder with the source-observed distinction: explicit close command arbitrates Save/Discard/Cancel; ambiguous tab/document teardown cancels (`clients/vscode/lib/drafts.js:529-598,819-887`); Neovim classifies intercepted close commands and cancels ambiguous unload/hidden/API-close paths (`lua/magit-lsp/edit.lua:135-188,340-368`). Note the current Unix-only server bridge, without claiming platform acceptance or tested parity.
    - **Done when:** A draft implementer can construct each request, correlate successes and both error channels, abort without leaking a helper, and distinguish finish acknowledgement from commit outcome using this section.
    - **Evidence (2026-10-28):** Added construction-ready managed draft payloads/barrier results, full-sync/version/size boundaries, rejected-sync recovery, owning-view abort routing, client-specific close-intent behavior and Unix bridge limitation; cross-checked against cited protocol, server, draft and client sources. `git diff --check` passed.
    - **Constraints:** Keep this workflow optional; no new draft methods, implementation changes, crash-durability promises or UI-test claims.
    - **Remaining problem (second review):** The history IDs and error-channel distinction are corrected, but the spec still never supplies the serialized barrier result objects or complete `editDocument` payload. It also omits the live/saved version rules needed to implement confirm versus discard-close, and line 95 still defers the current VS Code status to future verification.
    - **Remaining change:** In `docs/CLIENT_SPEC.md:84-88`, explicitly specify the ordinary JSON-RPC `result` objects `{type:"draft/checkpointed",runId,uri,version}`, `{type:"draft/finished",runId,uri,version}`, and `{type:"draft/replaced",runId,uri,baseVersion,version,text}` (`src/protocol.rs:145-159`); these names are `type` values, not notification methods. List all required `editDocument` fields (`type,runId,uri,text,version,kind,title,mode,scope`), with current `kind:"commit-message"`/`mode:"managedDraft"` (`src/protocol.rs:60-80`; `lua/magit-lsp/edit.lua:311-329`). Explain that save checkpoints exactly the live version without writing/releasing Git; initial text/version is also the initial saved checkpoint; confirm must name a version equal to both live and saved; normal-close names saved and may discard newer live text (`src/editor/draft.rs:138-147,198-253`). Every accepted history request returns a replacement at `baseVersion+1`, even at a history clamp, leaves the saved checkpoint unchanged, and requires subsequent full sync strictly above the returned version (`src/editor/draft.rs:214-231`; `src/editor.rs:165-175`; `src/protocol.rs:155-159`). If native text changed despite the editing gate, retain it and reconcile above that returned version instead of merely ignoring the replacement (`lua/magit-lsp/edit.lua:201-219`; `clients/vscode/lib/drafts.js:645-674`). Add `draft/error` as a server-to-client notification in the method inventory; its `version` can be null, notably for didClose, unlike the non-null version in successful barrier results (`src/server.rs:919-926`). Retain the already-correct InvalidParams error data, readable-error recovery and abort/close distinctions. Replace line 95's verification placeholder with the source-observed behavior already described at line 88, without claiming native-UI/platform validation.
    - **Remaining done when:** The draft section alone is sufficient to construct and validate each announcement/request/result, select confirm versus discard-close checkpoints, and advance safely after history replacement; no already-implemented draft behavior is deferred to a future source check. Verify by source comparison only, including the exact `commit-message.history-prev`/`commit-message.history-next` IDs; no tests or implementation changes.
    - **Third-review verification:** `docs/CLIENT_SPEC.md:42,85-89,96` now meets those remaining criteria against `src/protocol.rs:60-159`, `src/editor/draft.rs:138-253`, `src/editor.rs:165-175`, `src/server.rs:858-874,919-926`, `lua/magit-lsp/edit.lua:194-219`, and `clients/vscode/lib/drafts.js:627-674`. The current-client verification placeholder is removed. No further draft corrections requested; retain the worker's completed checkbox.

  - [x] **[P1] Add progress and standard message handling to the lifecycle inventory** (`docs/CLIENT_SPEC.md:9,21-43,70-72,105-111`)
    - **Problem:** The method inventory omits `window/workDoneProgress/create`, `$/progress`, `window/workDoneProgress/cancel` and `window/showMessage`, despite core key/stage operations emitting progress and startup/configuration errors using standard messages. Cancellation is mentioned without explaining how to send it.
    - **Why it matters:** Correctness: a bare external JSON-RPC adapter needs to service incoming server requests while its own requests are pending, and cannot infer operation completion or cancellation outcome from its initiating response.
    - **Change:** Add directions/kinds/payloads: server-to-client create request `{token}` answered by matching JSON-RPC ID; server-to-client progress notification `{token,value}` with begin/report/end; client-to-server cancel notification `{token}`; server-to-client showMessage `{type,message}`. Explain standard LSP library handling versus work needed in a manual transport. Record the current quirk accurately: the server inserts `capabilities.window.workDoneProgress:true` and sends create then begin without awaiting its response or negotiating support (`src/server.rs:294-343,1903-1914`). Include cancellation's 'Cancelling…' report, retained running reservation until terminal end, and that end alone is not a success result (`src/server.rs:370-398,1741-1753`); results/errors arrive separately through effects. Cite actual core key/stage progress (`src/server.rs:1012-1026,1969-1985`) and `show_message` (`src/server.rs:1875-1880`). Explain that prompt responses/progress cancellation must remain serviceable while an operation is outstanding, not queued behind its completion.
    - **Done when:** The inventory covers all active progress/message traffic and includes a concrete cancel example; no new capability negotiation or success-on-progress-end guarantee is implied.
    - **Constraints:** Do not make a rich progress UI a core prerequisite; distinguish servicing the existing transport protocol from optional visual presentation. No implementation/UI tests.
    - **Evidence:** Source review of `src/server.rs:294-343,370-398,824,1012-1026,1741-1753,1875-1880,1903-1914,1969-1985` and `src/protocol.rs`; documented create/progress/cancel/showMessage traffic, response servicing, cancellation lifetime and outcome distinction.

   - [x] **[P1] Make optional-workflow exclusion safe without negotiation** (`docs/CLIENT_SPEC.md:7-15,51,66,84,88`)
    - **Problem:** 'Do not expose dependent operations'/'do not enable that workflow' are not an executable fallback when dynamic/forwardAll input still reaches the full server command/menu set. There is no advertised mechanism that removes commit or other effect-producing operations for an incapable client; a readable error after an ignored editDocument does not release the helper.
    - **Why it matters:** Correctness: optional support must not accidentally start unsupported workflows and leave pending prompts, drafts or repository mutations unattended.
    - **Change:** State explicitly that delivery/modifier preferences and binding statuses do not negotiate client feature support or suppress server menu entries. Give a bounded reduced-client option: an explicit, documented allowlist of directly invoked supported commands and no unrestricted key/menu forwarding; ordinary hiding of buttons is insufficient if other input can still invoke them. Clients that forward unrestricted menus/keys must implement the dependencies of reachable operations or disclose that the reduced mode is not yet safely supported; do not promise automatic disabling. Document safe cancellation of unsupported prompts and, for an unexpected managed draft, explicit owning-run abort rather than silently dropping its effect (cross-reference the draft task). Keep single-window placement distinct from destroying prior view identities; optional splits do not eliminate required push/pop/focus lifetime bookkeeping (`src/protocol.rs:36-60`; `src/server.rs:1667-1716,1777-1800`; `lua/magit-lsp/keymap.lua:108-159`; `clients/vscode/lib/menus.js:37-61`). Leave future feature negotiation as a genuine design follow-up, not existing behavior.
    - **Done when:** Every unsupported/degraded claim names a currently achievable client-side restriction or an explicit unresolved limitation; unrestricted forwarding is never claimed safe merely because UI entries are hidden.
     - **Constraints:** Documentation only; do not implement a command policy, broaden the required core to all workflows, or move command/key resolution into clients.
     - **Evidence (2026-10-28):** Replaced vague exclusion promises with an explicit direct-invoke allowlist/no-unrestricted-key-or-menu-forwarding reduced-client option; clarified that delivery/modifier preferences and binding status do not suppress server operations. Documented cancellation for unexpected prompts, owning-run abort for unexpected drafts, and retained view identities/focus across single-window push/pop independently of optional splits. Cross-checked cited protocol/server/keymap/menu sources; `git diff --check` passed.

     - [x] **[P1] Supply a self-contained core payload reference and exchange** (`docs/CLIENT_SPEC.md:117-137`)
    - **Problem:** The external implementation guide still requires Rust source reading for basic payload field names/enums and keymap bindings; its example omits initialized, a descriptor response, the actual acknowledgement and any complete input request. Source pointers alone are not an externally usable wire specification.
    - **Why it matters:** Readability and cohesion: an external adapter should not guess casing, distinguish editor/view modes by inference, or treat a transport response as completion of its follow-up renders.
    - **Change:** Add concise schemas/tables (or a checked-in-reference link within this same document) for core descriptor, invoke/key, keymap binding, prompt and effect payloads, marking required/optional fields and wire enum values. In particular name `editorMode:"normal"|"visual"`, view mode `interactive|text`, placement `main|side|bottom|float`, binding `keys` as alternative space-separated canonical chords with command/modes/label/status, and `liveKeys`' actual camelCase field (`src/view.rs:79-159`, `src/keymap.rs:16-57`, `src/protocol.rs:175-537`). Explain that dynamic draft keymaps identify `runId` even though the draft is not an ordinary `view/open` view, with confirm/history routed through draft barriers (`src/server.rs:739-756,1719-1738`; `lua/magit-lsp/edit.lua:293-329`). Extend the JSON-RPC example through initialize response/initialized, open descriptor/render/ack, a complete versioned `view/key`, and its result plus subsequent render/ack; include standard initialize fields rather than suggesting open is pipelined before initialization. Explain response-before-follow-up-notifications and buffering renders/keymaps until their descriptor/effect is registered (`src/server.rs:1054-1074`; `lua/magit-lsp/view.lua:332-353,435-450`; `clients/vscode/lib/views.js:42-91`). Record concrete client presentation differences where relevant: Neovim rendered-buffer menus versus VS Code structured-menu QuickPicks, limited VS Code passThrough replay and placement handling, rather than a blanket parity claim (`clients/vscode/lib/menus.js:11-103`, `effects.js:15-37`, `views.js:95-135`).
    - **Done when:** A reader can construct the minimal status/input/ack loop and consume a dynamic keymap without opening Rust/client sources; examples agree with the corrected render/progress/draft sections and optional native menus never require parsing rendered text.
     - **Constraints:** Keep the reference compact and linked to authoritative sources; do not invent a separate structure notification, impose a particular native layout, claim runtime/UI verification, or duplicate the whole roadmap.
     - **Evidence (2026-10-28):** Added core payload fields/enums, corrected history IDs, and supplied initialized/open/descriptor/render/keymap/ack/versioned-key/result/render/ack exchange with buffering and concrete client presentation notes. Documentation-only; no runtime/UI testing claimed. `git diff --check` passed.
      - **Remaining problem (second review; raised from P2 to P1):** The new reference invents a `progress` effect, prompt `error` and visit-file `preview` fields; marks mandatory visit-file coordinates optional; and the incremental example uses `newText` instead of required `lines`. Those are incorrect wire instructions, not future design questions. The purported complete exchange also shows a capture-enabled status view and singleton/live-key status map unlike the actual server.
     - **Remaining change:** Correct `docs/CLIENT_SPEC.md:127` directly against `src/protocol.rs:16-80,470-503`: no `progress` effect (progress uses the separately documented LSP messages); no prompt `error`; `visitFile` requires `uri,line,column` and has no `preview`; prompt `destructive/default/secret/allowCustom/preselected` are optional fields, not guaranteed members. Add the explicit `LineEdit` shape `{startLine,endLine,lines:string[]}` and keep `mode` required for all render forms (`src/protocol.rs:190-220`). Fix line 144's `newText` to `lines`; a replacement of a line with itself is not emitted by the actual producer (`src/diff_text.rs:13-37`), so show either `edits:[]` for unchanged content or a genuinely changed example. Make the exchange internally consistent: status capture is false (`src/status_view.rs:249-264`; default `src/view.rs:207-210`), ordinary status keymaps omit empty `liveKeys` and contain the complete resolved binding set (`src/session.rs:418-440`; `src/protocol.rs:393-395`). Either include actual complete initialize/keymap payloads or explicitly label those messages as abridged excerpts, never a singleton map to install as the actual full status map. Do not imply staging the displayed branch heading succeeds with no effects; an uncomplicated example can send the standard profile's refresh key `g`/`F5` (`profiles/standard.toml:13-18`) and receive an unchanged-content incremental render. If showing a complete `view/key` exchange, include or explicitly account for its progress-create/begin/end traffic rather than 'may instead return effects/progress': progress is separate traffic, emitted for every ordinary view/key request (`src/server.rs:1012-1026,1969-1972`) and is never a result effect. Consolidate/remove the old incomplete example at lines 104-117 so it cannot still suggest opening before initialization has completed. Keep genuinely illustrative IDs/text/opaque URIs labelled as such.
      - **Remaining done when:** Every named field/variant in the reference exists with the correct optionality in Rust; both clients could consume the example edit using `edit.lines` (`lua/magit-lsp/buffer.lua:121-127`; `clients/vscode/lib/render.js:4-12`); example bookkeeping/command outcomes and any explicitly abridged messages agree with the current server. Verify by source inspection only; no implementation/UI tests or commits.
       - **Third-review remaining problem:** The quick reference now fixes the incorrect effect/prompt/visit-file/edit fields, and the refresh/capture/empty-edits examples are corrected. However, line 120 now puts `keymapDelivery`/`keyModifiers` in `initializationOptions`, contradicting line 23 and the actual parser. Lines 128-133 put the key result before progress creation/begin/end, whereas this handler emits progress first. The server create request has no matching client response. Line 126 includes a JavaScript comment in a `json` payload; line 121's reduced initialize result is still not explicitly labelled abridged.
       - **Third-review change:** Confine the remaining edit to the exchange and its explanatory prose (`docs/CLIENT_SPEC.md:117-137`). Move `keymapDelivery:"dynamic"` and `keyModifiers:"full"` into `params.capabilities`; leave `profile` in `initializationOptions` (`src/server.rs:2001-2015`). For the illustrated ordinary `view/key` refresh, show server progress create, then begin, then end BEFORE its key response, with the render after that response (`src/server.rs:1012-1026,1054-1070`; create/begin/end construction at `294-343`). Include a client-to-server `{"jsonrpc":"2.0","id":<same create ID>,"result":null}` to service the create request; explain that the server does not wait for this response, so its placement relative to begin/end is not a server barrier. A clear sketch is `client key → server create → server begin → client create response → server end → server key result → server render → client render ack`. Replace the inline comment with syntactically valid JSON; put omission/excerpt labels in surrounding prose. Either provide full capability/keymap payloads or explicitly label BOTH as abridged, non-installable illustrations of server messages (the initialize provider set is at `src/server.rs:1883-1926`; keymaps remain complete replacements in production). Do not reintroduce `newText`, fabricated effects, incorrect status capture or the discarded duplicate opening example.
       - **Fourth-review:** Completed. The client capability preferences are top-level capabilities; initialize result/keymap are explicitly abridged, keymap JSON is valid illustrative data and disclaimed as non-installable, and progress/create response/key result/render/ack order is documented and shown. Each JSONL example line parses; `git diff --check` passes.
      - **Third-review done when:** Each example message is valid JSON; the example and line 23 agree on capability placement; progress ordering and the correlated create response agree with the actual handler; every shortened capability/keymap message is clearly identified outside the JSON as an excerpt rather than a complete wire payload. Source-only/document-payload verification suffices; no UI/implementation tests, commits, or changes outside this documentation task.

### R-REL — Release sprint (after R-4.1a, R-7.3a, R-CLIENT-SPEC and R-PLAIN-GATE)

These linked, bounded tasks are the release checklist, not a competing ledger.
Release sprint begins only after **R-4.1a, R-7.3a, R-CLIENT-SPEC and R-PLAIN-GATE**; no other
feature work is scheduled in between. Each item stays unchecked until its own
evidence is produced. Release readiness explicitly assesses and discloses
BUG-READ-1's known limitation; the minimal prerequisite slice is scheduled, but
its larger design is not a release gate.

- [ ] **R-REL-1 — Reproducible artifacts and safe publication**
  **Depends on:** release-slice binaries/package metadata being buildable.
  Tagged GitHub binary releases and crates.io publication; VS Code Marketplace
  and Open VSX publication (the requested “open vsix” means Open VSX). Align tag,
  Cargo and extension versions; packaging dry runs; scoped credentials and
  protected publishing; document safe reruns and partial-publication recovery.
  Clean-machine install/launch smoke tests use actual binary, crate and VSIX
  deliverables. Review license/package metadata and dependencies/licenses;
  generate checksums. State tested versus build-only platform/architecture
  support explicitly. Document bundled/downloaded/external server strategy so
  VS Code installation does not unexpectedly require Rust.
  **Done when:** dry runs, credential-safety review, actual-artifact smoke tests,
  metadata/license review and checksums are recorded; no unsupported platform is
  described as tested.
  **Verify:** packaging dry runs and clean-machine installation/launch on each
  claimed tested target; CI workflow review and publication rehearsal, without
  publishing from unprotected PR credentials.

  - [x] **R-REL-1a — Build-only binary distribution foundation (2026-10-28).**
    Added read-only, pinned-action Linux x86_64 GNU artifact CI, aligned-version
    checks, locked builds, versioned archives and SHA-256 files; standalone
    latest/pinned GitHub Releases installer and conditional README instructions.
    `docs/RELEASING.md` records artifact/install contracts, proposed Ubuntu 22.04
    baseline, local-only evidence, safe reruns and publication blockers.
    **Evidence:** `cargo build --release --locked &&
    MAGIT_LSP_TEST_BINARY=target/release/magit-lsp python3 -m unittest discover
    -s tests/release -p 'test_*.py' && shellcheck scripts/install.sh
    scripts/package-release.sh && git diff --check` passed (12 tests, including
    real package/install/launch smoke). Hosted CI, clean-machine baseline and
    actual GitHub Release downloads remain unverified; nothing was published.
    **Remaining blockers:** prerequisite feature gates, registry dry runs,
    dependency/upstream-license and corresponding-source review, publication
    approval and unverified hosted evidence remain open.

  - [x] **R-REL-1b — Explicit component licensing and archive notices.** Preserve
    the root GPL-3.0-only server license; explicitly license the named client,
    protocol and specification components under Apache-2.0 with standalone notices.
    Align Cargo metadata, README/spec acknowledgements and release archive
    contents; verify license bytes and installer rejection/preservation when
    either required notice is missing. This resolves the owner-approved split,
    not dependency/source review or publication approval.
    **Done when:** scoped licensing tests and the release tooling verification
    pass with the real release binary, shellcheck and diff checks; parent R-REL-1
    and remaining release gates stay open.
    **Evidence (2026-10-28):** 16 release tests passed, 0 skips, including
    licensing boundaries and real-binary package/install/launch; shellcheck and
    `git diff --check` passed. Hosted CI and publication gates remain unverified.

- [ ] **R-REL-2 — Client install and screenshots**
  **Depends on:** R-REL-1 artifact/server strategy.
  Document tested Neovim native-pack and lazy.nvim setup and server install/
  discovery. Add cheap deterministic CI screenshot capture for both real
  Neovim and VS Code using existing harnesses where feasible; reuse resulting
  README assets, with documented manual fallback if capture is expensive or
  flaky (not a costly per-PR requirement). Validate consumption/linkage of the
  separately completed `docs/CLIENT_SPEC.md`. Welcome Helix, Zed and JetBrains
   as external community clients; do not promise first-party implementations.
    General LSP clients are a target for Plain mode per `docs/CLIENT_SPEC.md`;
   make no first-party promise beyond bundled clients.
  **Done when:** both client setup paths are tested and documented; both-client
  screenshot assets or manual fallback are usable; client spec is linked and
  consumable.
  **Verify:** clean setup smoke tests for Neovim and VS Code; deterministic
  screenshot job checked for both harnesses; spec link/consumption review.

  - [x] **R-REL-2a — Unblocked client setup documentation.** Document native-pack
    and lazy.nvim setup with explicit on-demand status opening, external-server
    discovery for both clients, and usable screenshot capture/manual-fallback
    instructions using the existing harnesses. Retain isolated setup smoke
    evidence where available; distinguish it from pending published-VSIX,
    clean-machine and screenshot evidence. Link `docs/CLIENT_SPEC.md` and welcome
    community clients without first-party promises. Do not close R-REL-2 yet.
    **Evidence (2026-10-28):** `docs/CLIENT_INSTALL.md` and
    `docs/SCREENSHOTS.md`, linked from README, document both clients' external
    server setup and existing native screenshot/manual-capture paths. All 13
    release-tool tests passed with `MAGIT_LSP_TEST_BINARY` set, including the
    isolated Neovim native-pack and lazy-spec callback smoke; shellcheck and
    `git diff --check` passed. Follow-up native capture passed all four Neovim
    commit-menu variants and VSCode A06 root lifecycle; exact commands and local
    artifact paths are in `docs/RELEASING.md` (final bounded verification).
    VSCode's captured Commit QuickPick has no action rows, so it is not a
    release-ready menu screenshot. The lazy.nvim manager, desktop-terminal PNG
    fallback and clean-machine/published VSIX installation remain unverified;
    parent gates remain open. No core code was changed to address capture gaps.

- [ ] **R-REL-3 — Release-facing README and honest project signals**
  **Depends on:** R-REL-1/2 evidence and R-CLIENT-SPEC.
  Redo README for actual features versus Planned, project purpose/differentiation,
  modest evidenced responsiveness claims, both supported clients, install and
  quickstart, screenshots, links and concise feature-focused roadmap. Publish
  a brief fair comparison, researched at writing time, against Neogit and
  Edamagit. Evaluate as candidate advantages over Edamagit faster/more responsive
  key handling, a scriptable command interface, and contextual
  auto-selection/preselection of commit-at-point for log/fixup/rebase; research
  whether these are implemented or follow-up opportunities rather than asserting
  them as established. Note Neogit's greater maturity/completeness/
  integration and this project's possible easier key rebinding, thin multi-editor
  architecture and possible performance differences; allow the conclusion
  that no compelling Neovim-specific advantage currently exists. Support speed
  claims with evidence or qualify/omit them; accurately state current feature and
  integration limitations, respectfully, without unsupported superiority claims.
  Add GitHub Releases, crates.io, VS Code Marketplace and Open VSX badges, plus
  measured, clearly scoped coverage badges (distinguish Rust and client scopes);
  link `docs/CLIENT_SPEC.md`. At the very end of the future README, disclose the user's
  exact intent: most code generated using combinations around Sol 6.1 and
  Deepseek 4.1 flash tiers, occasional Opus 5.5 guidance, very little code
  reviewed by eye but overall goals human-reviewed, robust tests mostly Neovim
  (mini.test, tmux snapshots, multiple viewport sizes) plus some VS Code
  coverage, creator daily use mostly VS Code, and intent for sustained
  maintenance/no vaporware at least for these two clients. Do not imply tests
  equal human review or promise guarantees.
  **Done when:** README claims match evidence; configured publication badge
  targets are validated before tag (live publication checks belong to R-REL-4),
  scoped coverage badges have measured evidence, and disclosure is last.
  **Verify:** render/read README links and assets; independently verify badge
  sources and every product/platform claim.

- [ ] **R-REL-4 — v0.2.0 go/no-go and publication**
  **Depends on:** R-REL-1…R-REL-3 and both feature gates.
  Record bounded changelog and release checklist, history-rewrite safety
  warnings, truthful known limitations/platform status, full regression and
  phase/native evidence, install/publication dry-run gates, and explicit
  **pre-tag human go/no-go** for v0.2.0. Before approval, validate configured
  badge targets and publication destinations without requiring live URLs. After
  approval, tagged CI publishes the binary release and all configured channels
  (crates.io, VS Code Marketplace, Open VSX), then verifies publication and live
  badges. Explicitly disclose BUG-READ-1's known failing
  read-responsiveness probe and keep claims bounded; it is not silently fixed or
  converted into a release prerequisite. Do not make deferred Windows/macOS
  acceptance, unrelated menus/views/branches/history, or build-only targets
  release gates.
  **Done when:** all scoped evidence is linked, limitations and platform matrix
  are truthful, pre-tag go/no-go is recorded, and post-approval tagged CI verifies
  all publication channels and live badges; no release success is presumed.
  **Verify:** full `make test`, phase and focused native evidence for both
  clients, clean-install smoke tests and publication dry runs; human review of
  changelog, safety warnings, known limitations, configured targets before tag,
  and all live publication channels/badges after tagged CI.


### Post-release feature queue (after release gates, including R-PLAIN-GATE)

Three queues below order the remaining post-release work. S/M/I are relative
size labels (small/medium/infrastructure), not time promises. Parent IDs remain
stable references; slices are independently checkable. Shared infrastructure
is bounded by its named consumers. For UI work, use focused real-UI scenarios
for the affected behavior; tests are named with each slice or shared rule.

#### Cheap post-release batches

- [ ] R-4.1-basic — S. Depends on: R-ARGS. Plain commit, extend, amend, and reword,
      including commit-menu `-a` toggle and `c c` UI acceptance. Preserve
      `--all`, `--allow-empty`, `--verbose`, `--no-verify`, `--signoff`,
      `--reset-author`, and `--author=`. *Done when:* these actions and switches work;
      `c c` lists Commit, Extend, Amend, Reword and `-a` flips `--all`.
      *Verify:* extend `commit-menu-shows-actions` and run commit-menu tests.
- [ ] R-4.1-signing — M. Depends on: R-4.1-basic. Signing (`--gpg-sign`),
      pinentry and error verification. *Done when:* signing is independently
      selectable and pinentry/errors are handled and verified. *Verify:* signing
      action tests and focused native commit-menu scenario.
- [ ] R-5.3a — S. Depends on: existing log. Basic existing-log variants and
      filters. *Done when:* supported variants/filters are selectable without
      changing log identity/navigation. *Verify:* log tests and focused UI.
- [ ] R-6.3a — S. Depends on: none. Remote configuration and lightweight tags.
      *Done when:* remote configuration and lightweight tag actions work.
      *Verify:* focused menu/action tests.
- [ ] R-6.1a — S. Depends on: none. Branch configuration, create, rename, and
      delete. *Done when:* these operations work independently of checkout and
      spin workflows. *Verify:* focused branch action tests.
- [ ] R-8.2a — S. Depends on: none. Cheap batch: notes and ignore. *Done when:*
      both workflows are available without expanding the Git boundary. *Verify:*
      focused notes/ignore tests and native UI.
- [ ] R-9.1a — S. Depends on: none. Theming documentation. *Done when:*
      supported theme customization is documented. *Verify:* documentation review.
- [ ] R-9.5a — S. Depends on: none. `docs/FORKING.md`. *Done when:* fork
      boundaries and guidance are documented. *Verify:* documentation review.

#### Structured read infrastructure and views

- [ ] R-VIEW-DATA — I. Depends on: none. Shared structured revision/diff data
      for the read views below. *Done when:* revision and diff content is
      represented as server-owned structured data, including stable identities
      and ranges; no client parses rendered text. *Verify:* data-model tests.
- [ ] R-5.2a — M. Depends on: R-VIEW-DATA. Revision view for a confirmed
      structured commit: in a plain log `common.confirm`/Enter opens the commit;
      in select mode Enter selects as usual. *Done when:* confirmed commits open
      the revision view. *Verify:* structured revision/session tests.
- [ ] R-5.2b — M. Depends on: R-5.2a. Diff view with ranges and arguments.
      *Done when:* `d` on a commit opens its diff and `q` returns to the log.
      *Verify:* focused `revision-view` UI scenario.
- [ ] R-5.2c — M. Depends on: R-5.2a. Picker inspection and caller restoration:
      proposed `o` opens a read-only commit view without selecting/mutating;
      `q` restores originating picker/rebase view, cursor, marks, and pending
      operation. Keep proposed keys terminal-portable and independent of
      `ctrl+Enter`; `d` remains the revision-view scenario. These keys/behaviour
      are proposals, not approved/implemented. *Done when:* inspection preserves
      selection and complete caller state. *Verify:* session tests and focused
      UI inspection/restoration scenario.
- [ ] R-5.3b — M. Depends on: R-VIEW-DATA. Refs view. *Done when:* `y` opens
      refs listing branch and tag names and `q` returns to status. *Verify:*
      focused `refs-view` UI scenario.
- [ ] R-5.3c — M. Depends on: R-VIEW-DATA. Immutable file-at-revision virtual
      docs. *Done when:* a file at a revision opens read-only and is immutable.
      *Verify:* focused virtual-document tests.
- [ ] R-5.3d — M. Depends on: R-5.3b. Reflog view. *Done when:* reflog entries
      have structured identities and navigable display. *Verify:* focused view
      tests.
- [ ] R-5.3e — M. Depends on: R-5.3b. Line-history view. *Done when:* line
      history is a structured read view. *Verify:* focused view tests.
- [ ] R-5.4 — S. Depends on: log view. `documentLink` and `hover` for hashes
      and refs, independently of semantic styling. *Done when:* targets resolve.
      *Verify:* extend `log-view`, asserting links/hover and style in `.screen`.
- [ ] R-5.6 — M. Depends on: log view. Server-owned semantic styling for
      hashes, branch/tag decorations, headings, author/date, and graph elements;
      independent of R-5.4. *Done when:* clients use semantic metadata and do
      not parse rendered text. *Verify:* spans in UTF-8 byte and UTF-16 code-unit
      coordinates including non-ASCII; assert colors in real-UI log scenario.

#### Shared Git and workflow extensions

- [ ] R-DIFF-SAFETY — I. Depends on: none. Displayed-patch identity and
      filtered-mutation prerequisite for status-diff actions. *Done when:*
      mutations target the displayed patch identity, not a fresh reconstructed
      selection. *Verify:* identity and stale/filtered patch tests.
- [ ] R-5.5 — M. Depends on: R-DIFF-SAFETY. Diff argument plumbing across
      views, including context/whitespace/rename/word arguments; wire context
      configuration rather than hardcoded `-U3`. *Done when:* options propagate
      consistently to rendering and mutation. *Verify:* diff-argument tests and
      native `-w` scenario: status diff has no whitespace-only hunks.
- [ ] R-BASELINE — I. Depends on: none. Resolve push target when configured,
      otherwise upstream, as the common remote-tracking baseline. *Done when:*
      resolver does not fetch and exposes absence rather than asserting push
      state. *Verify:* baseline resolution tests.
- [ ] R-4.3 — I. Depends on: R-BASELINE, R-VIEW-DATA, R-5.2a. Status history
      sections: **Unpushed** and **Pushed**. Keep history as server-owned
      structured commit nodes with full OIDs; reuse section folding/navigation
      and integrate Enter-to-view with R-5.2. Clients display rendered history
      verbatim and never reconstruct or parse Git history. This task owns both
      history sections; R-6.4 owns its independent Unpulled section and status
      header lines, not Unpushed.

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
- [ ] R-6.4 — M. Depends on: R-BASELINE, status view; coordinate with R-4.3 without
      duplicating Unpushed. Add Unpulled section and Head, Merge, Push, Tag
      header lines. *Done when:* these status elements render independently of
      R-4.3 history ownership. *Verify:* status tests and focused native UI.
- [ ] R-4.2a — M. Own history-view integration (reuse completed EDIT8).
      Depends on: R-EDIT-6 (Linux managed-draft cutover, which in turn requires
      R-EDIT-2/4/5); macOS CI R-EDIT-9 and Windows feasibility R-EDIT-7 are
      deferred, not Linux blockers. Staged-diff companion view.
      *Done when:* commit-message view displays staged diff. *Verify:* focused
      view tests and `commit-message-view` UI scenario.
- [ ] R-4.2b — M. Depends on: R-4.2a, R-EDIT-8 (reuse message-ring ownership).
      Trailers and lint diagnostics. *Done when:* trailers and diagnostics are
      available in the editor bridge. *Verify:* focused editor bridge tests.
- [ ] R-4.2c — S. Depends on: R-4.2a, R-4.2b. Confirm/cancel/close integration.
      *Done when:* confirm closes the view and status is clean; cancel/close
      restore the caller. *Verify:* `commit-message-view` native scenario.
- [ ] R-4.1c — M. Depends on: R-SELECT-OPS, R-ARGS. Targeted fixup, squash,
      augment, and alter workflows. *Done when:* each targets the selected history commits
      without changing plain commit flow. *Verify:* focused history action tests.
- [ ] R-4.1d — M. Depends on: R-4.1c, rebase and history-edit workflows. Instant
      fixup/squash rewriting. No generic new framework. *Done when:* instant
      rewriting uses established history-edit workflows. *Verify:* focused
      rewrite tests.
- [ ] R-6.1b — M. Depends on: R-6.1a. Checkout, create-and-checkout, spin-off,
      spin-out. *Done when:* operations explicitly protect worktree/branch safety
      and report unsafe operations without losing changes. *Verify:* branch
      safety tests and focused UI.
- [ ] R-NETWORK — I. Depends on: none. Owned progress/askpass/cancel seam for
      network operations. *Done when:* progress, credential prompting, and
      cancellation are represented through owned effects. *Verify:* seam tests.
- [ ] R-6.2 — M. Depends on: R-NETWORK; pull also depends on R-OP-STATE. Fetch and
      push flags (`--force-with-lease`, `--set-upstream`, `--dry-run`, `--prune`),
      pull `--rebase`/`--ff-only`, and conflict handling. *Done when:* operations
      use the shared seam and pull respects operation state. *Verify:* network
      action tests, including progress/askpass/cancel and pull conflict cases.
- [ ] R-6.3b — M. Depends on: R-DIFF-SAFETY. Own branch reset, delegated by R-6.1, separately from checkout of
      a file at a revision. *Done when:* reset and file checkout have distinct
      targets/confirmation. *Verify:* focused mutation tests.
- [ ] R-6.3c — S. Depends on: R-6.3a. Annotated and signed tags, separate from
      lightweight tags. *Done when:* annotation and signing are independently
      handled. *Verify:* focused tag tests.
- [ ] R-6.5a — M. Depends on: R-VIEW-DATA. Stash read views (list/show), capture
      (stash/snapshot), Stashes section, and stable stash identity. *Done when:*
      displayed stash identity survives refresh/reordering. *Verify:* stash view
      and identity tests.
- [ ] R-6.5b — M. Depends on: R-6.5a. Conflictful apply/pop, branch, and drop.
      *Done when:* operations use stable stash identity and surface conflicts
      safely. *Verify:* stash operation/conflict tests and focused UI.

Parent-to-slices: R-4.1 → R-4.1-basic, R-4.1-signing, R-4.1c–d; R-4.2 → R-4.2a–c; R-4.3 → R-4.3;
R-5.2 → R-5.2a–c; R-5.3 → R-5.3a–e; R-5.4 → R-5.4; R-5.5 → R-5.5;
R-5.6 → R-5.6; R-6.1 → R-6.1a–b; R-6.2 → R-6.2; R-6.3 → R-6.3a–c;
R-6.4 → R-6.4; R-6.5 → R-6.5a–b. Common cycle-visibility, next-sibling,
prev-sibling and parent-section are Implemented; R-NAV records verification.

### Advanced workflows and integrations



- [ ] R-7.1a — M. Depends on: R-OP-STATE. Merge menu, Unmerged section, conflict
      actions, continue and abort. *Done when:* merge operation state and
      conflict resolution are represented safely. *Verify:* merge/conflict tests
      and focused native UI.
- [ ] R-7.1b — M. Depends on: R-7.1a. Native mergetool effect. *Done when:*
      conflict nodes can request the native tool through an owned effect.
      *Verify:* effect tests and focused native UI.
- [ ] R-7.2a — M. Depends on: R-OP-STATE. Sequencer state for cherry-pick and
      revert. *Done when:* in-progress, continue and abort states are handled.
      *Verify:* sequencer tests and focused native UI.
- [ ] R-7.2b — M. Depends on: R-7.2a. Read-only Cherry view. *Done when:*
      cherry-pick history is shown in a structured read-only view. *Verify:*
      view tests and focused native UI.
- [ ] R-7.3-options — M. Depends on: R-OP-STATE, R-5.1. Linear rebase options for
      upstream, pushRemote or elsewhere, autosquash and update-refs; reuse the
      release todo. *Done when:* linear rebase options and todo flow work.
      *Verify:* rebase tests and focused native UI.
- [ ] R-7.3b — L. Depends on: R-7.3-options. Full `--rebase-merges` todo grammar and
      graph rewrite. *Done when:* merge topology is represented and rewritten
      correctly. *Verify:* graph rewrite tests and focused native UI.
- [ ] R-7.3c — M. Depends on: R-7.3-options. Scripted `GIT_SEQUENCE_EDITOR` edits:
      reword, drop, fixup and modify. *Done when:* scripted sequence-todo choices are applied; reword may invoke Git commit-message
      editing and does not claim to avoid an editor bridge. *Verify:* sequence-editor tests and focused
      native UI.
- [ ] R-7.4 — M. Depends on: R-SELECT-OPS. Integrate remaining
      commit-picker workflows, including commit selection across rebase,
      cherry-pick and reset; no new core picker. *Done when:* integrations use
      shared selection infrastructure. *Verify:* picker action tests and focused
      native UI.
- [ ] R-8.1a — M. Depends on: R-VIEW-DATA. Server-rendered blame view. *Done
      when:* blame is navigable through structured server nodes. *Verify:* blame
      view tests and focused native UI.
- [ ] R-8.1b — M. Depends on: R-8.1a. Real-file inlay hints/code lens. *Done
      when:* blame annotations are provided in real files. *Verify:* client tests
      and focused native UI.
- [ ] R-8.2b — M. Depends on: none. Validated run-command workflow. *Done when:*
      arguments and execution boundary are validated. *Verify:* command tests and
      focused native UI.
- [ ] R-8.2c — M. Depends on: R-DIFF-SAFETY. Apply and format patch workflows.
      *Done when:* patch input is safely applied and formatted. *Verify:* patch
      tests and focused native UI.
- [ ] R-8.2d — M. Depends on: R-OP-STATE. Am-state workflow. *Done when:* am
      progress, continue and abort are represented. *Verify:* am tests and
      focused native UI.
- [ ] R-8.2e — M. Depends on: R-OP-STATE. Bisect-state workflow. *Done when:*
      bisect lifecycle and state are available. *Verify:* bisect tests and
      focused native UI.
- [ ] R-9.3a — L. Depends on: none. Bounded multi-repository/session and
      worktree identity, before worktree consumers. *Done when:* repository
      sessions and worktrees have bounded distinct identities. *Verify:*
      session identity tests and focused native UI.
- [ ] R-8.2f — M. Depends on: R-9.3a. Worktree operations. *Done when:*
      worktrees can be managed without conflating repository/session identity.
      *Verify:* worktree tests and focused native UI.
- [ ] R-8.2g — M. Depends on: none. Submodule workflows. *Done when:*
      submodule state and supported actions are available. *Verify:* submodule
      tests and focused native UI.
- [ ] R-8.3a — M. Depends on: R-9.3a. Repository/path context for editor
      buffers. *Done when:* context is resolved before any-buffer dispatch.
      *Verify:* context tests and focused native UI.
- [ ] R-8.3b — M. Depends on: R-8.3a, R-8.1a. Any-buffer file log, diff, stage
      and blame dispatch. *Done when:* dispatch uses resolved repo/path context.
      *Verify:* dispatch tests and focused native UI.
- [ ] R-9.1b — M. Depends on: none. Server-owned section layout policy.
      *Done when:* layout customization is expressed as server policy. *Verify:*
      layout tests and focused native UI.
- [ ] R-9.1c — M. Depends on: R-9.1b. Native full-line tint. *Done when:*
      clients render optional full-line background tint from server metadata.
      *Verify:* client tests and focused native UI.
- [ ] R-9.2a — M. Depends on: none. Profiling, budgets and size limits. *Done
      when:* measured budgets and limits are documented and enforced. *Verify:*
      profiling/limit tests.
- [ ] R-9.2b — M. Depends on: R-9.2a. Conditional node-level incremental
      rendering only if profiling justifies it. *Done when:* evidence supports
      the change and focused implementation meets budgets. *Verify:* benchmark
      evidence and rendering tests.
- [ ] R-9.4 — M. Depends on: none. Windows paths and CRLF; R-EDIT-7 remains
      deferred. Not a release/package gate. *Done when:* path and line-ending
      behavior is validated without claiming platform feasibility prematurely.
      *Verify:* Windows-targeted tests when available; no release gate.

- [ ] R-9.5b — L. Depends on: R-9.5a. Optional semi-hard jj fork; do not
      compromise the Git boundary. *Done when:* separate fork demonstrates the
      boundary without reducing Git features. *Verify:* fork review. Deferred,
      not a post-release commitment.

Parent-to-slices: R-7.1 → R-7.1a–b; R-7.2 → R-7.2a–b; R-7.3 → R-7.3-options–c;
R-7.4 → R-7.4; R-8.1 → R-8.1a–b; R-8.2 → R-8.2a–g; R-8.3 → R-8.3a–b;
R-9.1 → R-9.1a–c; R-9.2 → R-9.2a–b; R-9.3 → R-9.3a; R-9.4 → R-9.4;
R-9.5 → R-9.5a–b.

### R-SELECT-AUDIT — Cross-hunk selection investigation (not a release prerequisite)

- [ ] Inspect
  actual stage/unstage/discard paths and both clients' selection forwarding to
  determine whether a selection spanning hunks acts only on selected changed
  lines or expands to whole hunks. Document current versus intended IMM-2/IMM-6
  behavior with concrete partial-endpoint, header/context, intervening-hunk and
  preferably cross-file cases. Retain exact expected Git state and focused
  native evidence. Create a separately scoped fix only if a demonstrated
  discrepancy exists; do not assume a behavior change.
  **Done when:** evidence-backed behavior and any discrepancy/follow-up are
  recorded without converting investigation into an automatic release gate.
  **Verify:** focused real-repository state checks and native selection evidence
  in both clients, if a behavior discrepancy is found; otherwise document the
  traced paths and cases.

### Deferred work (not release gates)

- **R-EDIT-HISTORY:** History prev/next as a server code action issuing a versioned `applyEdit`, identical for every client. Not scheduled until separately requested; depends on R-EDIT-REAL and R-PLAIN-ACTIONS.

### Deferred platform work (not release gates)

- [ ] **R-EDIT-7 — Windows feasibility (deferred, not Unix release blocker)**
  - **Paths:** `src/editor.rs`, `src/runner.rs`, `src/protocol.rs`, `TASKS.md`.
  - **Depends on:** R-EDIT-1 design; separate from Unix cutover.
  - **Done when:** named pipe, restrictive ACL and atomic suspended Job assignment feasibility/limitations documented; no guarantee claimed without evidence.
  - **Verify:** explicit platform design review/go-no-go; no test command claimed.
- [ ] **R-EDIT-9 — macOS CI/platform acceptance (deferred, not Linux release blocker)**
  - [x] Documentation-only acceptance split (2026-10-28): Linux gates and native guidance defer macOS execution here; R-EDIT-2 has Linux-only acceptance and no macOS validation is claimed.
  - **Candidate paths:** CI workflow configuration, `src/runner.rs`, `src/runner/owned.rs`, `tests/runner.rs`, `tests/runner_hooks.rs`, `tests/editor_bridge.rs`, `tests/server_async.rs`, `TASKS.md`, `ui-test-plan.md`; choose concrete CI paths when this later task starts.
  - **Depends on:** R-EDIT-1 design and Linux R-EDIT-2 acceptance; R-EDIT-6 for managed-draft native cutover acceptance on macOS. No reverse dependency from Linux R-EDIT-2/R-EDIT-6. Intended for a later macOS CI runner; unavailable local macOS execution is not a Linux blocker. Windows remains R-EDIT-7.
  - **Done when:** retained macOS CI artifacts identify runner image, macOS/architecture, Rust/Git versions and exact commands/results; real-Git tests prove pre-exec session/group ownership, non-reaping WNOWAIT exit observation with PID/PGID pinned through full TERM/CONT grace and final group KILL, sole final reap and no signals after reap. Cover leader-exits/TERM-resistant descendant, stopped member, exit-before-cancel/no signal, normal-exit daemon survival, inherited-pipe bounded drain, exactly-once release, unchanged HEAD/index/worktree on abort, subsequent commit without lock cleanup, lock warning and no foreign kills. Trace observation → cancellation → TERM/CONT → full grace → KILL → reap; retain native evidence for both clients before claiming macOS cutover acceptance. Full-grace fallback stays; Linux `/proc` latency evidence is not macOS evidence. Until these gates pass, macOS remains explicitly unverified.
  - **Verify:** on the macOS CI runner, `cargo test --test runner --test runner_hooks --test editor_bridge --test server_async`, plus the R-EDIT-6 native acceptance commands for macOS client cutover; retain process traces, runner metadata and native artifacts. This is a future verification requirement, not an execution claim.

---

## 8. Testing strategy

**Target distribution:** ~80% Rust in-process, ~15% protocol-level (spawned
binary over stdio), ~5% Neovim. The Neovim tests that exist stay as
integration tests; their *intent* for logic moves to Rust.

### 8.1 In-process `TestSession` harness (main tool)

The harness is implemented in `tests/common/session.rs` (IMM-7; completed
history in the archive); extend it there rather than adding another session
type, with visibility/keymap/effects inspection as needed. Drive the real key/invoke path,
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

## 10. Implementation status and deviations

Current implementation status is determined from source and tests, not this
historical section. Relevant caveats: BUG-READ-1's minimal intake slice is
scheduled in R-PLAIN-PREREQ, while its larger design remains deferred
and its probe ignored; Phase 3E is partially complete (R-EDIT-2 Linux acceptance
only; R-EDIT-3 server integration and R-EDIT-8 history are complete, native
adapters pending; A11 cutover is owned by R-EDIT-6; macOS/Windows remain
unverified/deferred). See §§7 and 3.10, and the
[completed-roadmap archive](./docs/archive/completed-roadmap-2026-10-28.md)
for historical deviations and evidence.

R-NAV is verified complete: common visibility cycling and sibling/parent
navigation are Implemented, with focused Rust and native Neovim evidence in §7.

### Historical limitations audit (collection-time; not an open bug list)

The rows below record where earlier reviews expected each limitation to be
addressed. Most cited tasks are now complete; treat the archive and current
source/tests as authoritative. The binary whole-file-only limitation and future
layout customization remain genuinely open.

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

## 11. Client reviews

Completed Neovim, VSCode and UI harness review history is retained in the
[completed-roadmap archive](./docs/archive/completed-roadmap-2026-10-28.md).
Current review status: A01–A10/A12 and actionable A13–A21/A24–A34 complete;
A22/A23 withdrawn; A11 owned by R-EDIT-6. See `REVIEW_REPORT.md` for evidence,
not scheduling authority. Preserve BUG-READ-1's larger-design deferral (§7).

## 12. Review backlog

### Review and test task summary

The canonical review queue lives in §7; R-COPY-SOURCE and R-NAV are complete.

Completed R-5.1 review, VSCode resolution, client/harness reviews and checked
review/test tasks are archived at
[`docs/archive/completed-roadmap-2026-10-28.md`](./docs/archive/completed-roadmap-2026-10-28.md).
Detailed historical evidence remains at baseline commit `49efd16`; it is not
current behavior or active scheduling. BUG-READ-1's probe remains ignored;
the minimal intake slice is scheduled and the larger design remains deferred (§7).

### Test-review summary

Open acceptance gates are retained above; recheck review-time findings against
source before implementation. §0 holds decisions; §7 holds priority/status.
