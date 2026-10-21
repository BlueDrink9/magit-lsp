# magit-lsp — Roadmap

Standalone roadmap (split out of `AGENTS.md`). Agent-facing. Written so that a
less capable model can implement each item without guessing: every term is
defined in the glossary (§1), every protocol shape is spelled out (§3), and
every task states what "done" means.

**Project status:** early stage. **Backward compatibility is not a goal.**
Rename, delete, and reshape protocol methods, config keys and Lua APIs freely.
Delete the old thing; do not leave shims.

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

---

## 0. Decisions already made

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
| **Editor bridge** | The mechanism that lets Git's "open an editor" calls (commit message, rebase todo) open inside the user's own editor window (§3.10) |
| **Process log** | A view listing every Git invocation with arguments, exit code, duration and output |
| **Text mode** | A view mode in which the buffer is plain editable text with no server key handling, so the user can select/copy freely (§3.14) |
| **Semantic token** | LSP's standard way for a server to label ranges of text with a *type* (and modifiers) so the client colours them using the user's theme |
| **Parity** | Having the same operation available as Magit |

---

## 2. Immediate tasks (do these first)

These come from hands-on testing. Do them before starting any phase. Each has
acceptance criteria. Write the failing test first. Where a test is needed
before the new in-process harness exists, write it against `GitRepo` (the
existing Rust integration tests) and move it later.

### IMM-1 — Unstaging a file header fails in a repo with no commits

**Symptom:** pressing unstage on a staged file's header in a repository with no
commits gives `git restore --staged -- tests/git_backend.rs failed: fatal: could not resolve 'HEAD'`.
Unstaging on a hunk works.

**Cause:** `unstage_file` uses `git restore --staged`, which needs `HEAD`.
`unstage_all` already handles the unborn case; `unstage_file` does not.

**Verified behaviour (git 2.43):**

| Command on unborn HEAD | Result |
| --- | --- |
| `git restore --staged -- f` | Fails: `could not resolve HEAD` |
| `git reset -q -- f` | Works |
| `git reset -q HEAD -- f` | Works |
| `git rm --cached -- f` | **Fails** if the working copy differs from the index ("staged content different from both the file and the HEAD") |
| `git rm --cached -f -- f` / `git update-index --force-remove -- f` | Work |

Magit itself uses `git rm --cached` when there are no commits and
`git reset HEAD --` otherwise (confirmed in `magit-apply.el`).

**Do:**
1. Add one function `unstage_paths(&[&str])` in `backend::git` used by *every*
   caller (file, all, rename pairs). It runs `git reset -q -- <paths>`. If that
   fails, fall back to `git rm --cached -q -f --ignore-unmatch -- <paths>`.
2. Renames: unstage both old and new path.
3. Delete the separate unborn-HEAD branch in `unstage_all` if the helper covers it.
4. Audit every other mutation (`stage_hunk`, `unstage_hunk`, `stage_lines`,
   `unstage_lines`, `stage_file`, `stage_all`, `discard*`) with an unborn-HEAD
   test each.

**Done when:** a test with an unborn repo covers unstage-file, unstage-hunk,
unstage-lines, unstage-all and a modified-after-staging file (index ≠ worktree),
all passing, and the same tests pass on the oldest git version we claim to
support (record it in `README.md`).

- [x] Done.

### IMM-2 — Visual stage/unstage with non-change lines corrupts the patch

**Symptom:** a visual selection that includes context lines (` `), hunk
headers (`@@`) or file headers makes `git apply` fail with `corrupt patch at <stdin>:8`.

**Required behaviour:** *only lines that are actual changes (`+` or `-`) inside
the selection are staged; everything else in the selection is ignored.*

**Selection normalisation (implement as one pure function in `patch.rs`, with
unit tests):**

1. Input: the view's nodes and a line range `[start, end]` (inclusive, from the
   cursor context).
2. For each file node the range touches:
   - If the range covers the **file header line**: select the whole file
     (use whole-file stage/unstage).
   - Otherwise, for each hunk node the range touches:
     - If the range covers the **hunk header line**: select the whole hunk.
     - Otherwise: collect only the `+`/`-` diff lines inside the range.
3. Drop empty results. If nothing remains, do nothing and emit
   `Message{ level: info, text: "No changed lines in the selection" }`.
4. A range spanning several hunks and/or files is allowed; each is handled by
   step 2 and applied as a separate patch in order (one `git apply` per file).
5. The same rule applies to unstage and discard (reverse apply).

**Patch construction (see §3.13):** for each hunk with selected change lines,
- keep selected `+`/`-` lines unchanged,
- turn an **unselected `-` line into a context line** (prefix ` `),
- **drop an unselected `+` line**,
- recompute the hunk header counts (or pass `--recount` to `git apply`),
- reuse the original file header lines verbatim.

**Done when:** Rust tests cover: selection of context only (no-op + message),
context+change (change staged), header+change, a range crossing two hunks, a
range crossing two files, and selection inside a staged hunk for unstage. Add
a proptest that random selections over random diffs always produce a patch
that passes `git apply --cached --check`.

- [x] Done.

### IMM-3 — Error messages must be readable

**Symptom:** the Neovim client prints raw Lua tables
(`magit-lsp vcs/stageLines failed: { code = -32603, message = "git apply failed: …", <metatable> … }`).

**Required behaviour:**
- A Git command failing is **not** a JSON-RPC error. The command's response
  succeeds and carries an `Effect::Message{level:"error"}`. JSON-RPC errors are
  reserved for protocol faults (bad params, unknown method, internal panic).
- The server converts Git failures into a `UserError`:

  ```
  UserError { summary: string      // one sentence, plain language, no jargon
              hint?: string        // what to do next
              detail?: string      // raw stderr, multi-line
              command?: string     // "git apply --cached"
              processLogId?: number }
  ```
- A mapping table in one place (`errors.rs`) recognises common failures by
  exit code + stderr pattern and fills `summary`/`hint`. Initial entries:

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
- Clients show `summary` (+ `hint`) as a notification at the right level and
  offer to open `detail`/process log. They never print raw tables or JSON.

**Done when:** an in-process test triggers each mapped failure and asserts the
exact `Message` effect; the Neovim client has a test asserting a failed command
produces one `vim.notify` with the summary text and no table dump.

- [x] Done.

### IMM-4 — Neovim status buffer must be read-only

**Symptom:** lines can be deleted or changed and undo is clobbered, so the
buffer cannot be recovered except by refresh.

**Do (in the client; text mode in §3.14 later toggles these back):**
- Buffer options: `buftype=nofile`, `bufhidden=hide`, `swapfile=false`,
  `modifiable=false`, `readonly=true`, `undolevels=-1`, `modified=false`.
- Render helper: `set_lines(buf, lines)` sets `modifiable=true`, replaces
  lines (range edits when the server sent edits), resets `modified=false`,
  sets `modifiable=false`. It is the **only** place that writes to the buffer.
- Block `:w`/`:write` on the buffer (`BufWriteCmd` autocmd that does nothing).
- Unbound edit keys fail natively with `E21`; on the first such failure in a
  session show an info message "Read-only view. Press <text-mode key> to edit/copy as text."

**Done when:** a Neovim test types `dd`, `x`, `ciw`, `u`, `:s/a/b/` in the
buffer and asserts the lines are unchanged and no error escapes; a second test
checks that a refresh still works afterwards.

- [x] Done.

### IMM-5 — Two separate line stagings appear as one staged hunk; audit the diff/patch layer

**Symptom:** staging lines in two different parts of a file seems to produce a
single staged hunk.

**First determine whether it is correct.** Git merges two changes into one
hunk when the unchanged gap between them is at most `2 × context` lines (default
context 3, so ≤ 6 unchanged lines). If the two parts are that close, a single
hunk is exactly what `git diff --cached` shows and what Magit shows.

**Investigate in this order:**
1. Reproduce with a fixture: a 40-line file, change line 5 and line 35, stage
   each separately. Expected: `git diff --cached` shows **two hunks**, and the
   staged section must show two hunks.
2. Compare the staged section's hunks with `git diff --cached -U3` output.
   They must be identical in count and content.
3. Read `git.rs`/`patch.rs`: find any place that *computes* hunks itself
   (merging, splitting, line numbering) rather than parsing `git diff` output.
   Remove it. The status view's hunks are always exactly what Git printed.
4. Reproduce with changes 4 lines apart: expect one hunk.

**Answer to "are we reinventing the wheel?"** Magit does **not** implement its
own diff engine. It runs `git diff`, parses the text, and for staging builds a
patch from the file header plus the hunk text (for a partial selection, a
"region patch") and pipes it to `git apply` (`--cached`, `--reverse`). Our
`patch.rs` is the equivalent of Magit's region-patch builder and nothing more.
Anything beyond *parse git's output* and *build a patch for git apply* should be
deleted. Details in §3.13.

**Done when:** the two-part fixture test and the 4-lines-apart test pass,
`git.rs` contains no diff computation, and `patch.rs` either keeps its
renumbering algorithm with a written reason or is simplified using
`git apply --recount` (see §3.13) with the proptests passing.

- [x] Done.

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

**Done when:** every row has an in-process test.

- [x] Done. (node-kind dispatch + executor for stage/unstage in
  `src/dispatch.rs`; discard execution and server/client wiring deferred.)

### IMM-7 — Extract the test harness

Move `init_repo`/`git`/`write`/`make_standard_changes` into
`tests/common/mod.rs`; add the determinism environment from §8.2; add a
`RepoFixture` builder. Needed before the tests for IMM-1…6 are written.

- [x] Done.

### IMM-8 — Spike: colour verification with `:TOhtml`

Prototype only. See §8.4. Produce one script that opens the status buffer in
headless Neovim, runs `:TOhtml`, writes `build/test-output/status-<scheme>.html`,
and prints a table of semantic class → resolved fg colour. Record findings
(does `:TOhtml` include LSP semantic-token extmarks? what is the exact API in
the Neovim version in use?) in this file.

- [x] Done. `scripts/color_spike.lua` (`make color-spike`) writes
  `build/test-output/status-<scheme>-<background>.html` for `default`
  dark/light, `vim`, `habamax`, `delek`, `lunaperche` and prints the
  class → link → resolved fg table. Findings in §8.4.

### IMM-9 — Update `AGENTS.md`

Replace its roadmap section with a link to this file; record the decisions in
§0 and the conventions: no backward compatibility, never address hunks by
index, never parse rendered text, never print raw JSON/Lua tables to the user.

---

## 3. Design decisions (expanded)

### 3.1 The server owns the document

- The server builds a **tree of nodes** per view and renders text from it.
  Nothing parses rendered text back (delete `src/document.rs`'s parsing and
  `vcs/didRender`).
- Every render produces, from the same tree: the text, semantic tokens, a
  symbol tree, link ranges and a `line → node` map.
- **Node IDs** are derived from identity (path + hunk content hash, commit
  hash, ref name, menu item key), never from position. They preserve cursor,
  visibility and selection across refreshes.
- **Versioning:** each render carries `version`. Clients ack with
  `view/rendered`. Position-based requests (symbols, definition, key presses
  with a cursor) are resolved against the last *acked* version. Actions carry
  the version and node ID they were issued against; if the node no longer
  exists (e.g. the hunk changed), the server re-resolves by content hash or
  replies with a `Message` — it never applies the action to "whatever is at
  index 0 now".
- **Visibility is server-owned.** Collapsed sections are not rendered. Their
  bodies are not even computed until expanded (Magit does the same: its
  `magit-insert-section-body` does not evaluate a hidden section's body until
  the section is expanded).

### 3.2 Sessions, views and the view stack

- One `Session` per repository root per client. It holds a `ViewStack`.
- A `View` has: `id`, `kind`, `uri`, `title`, `mode` (`interactive`/`text`),
  `placement`, `scope`, `capture`, `viewport {cols, rows}` (sent by the client;
  needed to lay out multi-column menus), its node tree and visibility state.
- Trait: `render(&Ctx) -> Tree`, `scope() -> Scope`, `on_refresh()`, optional
  `load_more()` for lazy lists.
- The server gives a `placement` hint; the **client** decides actual windows.

### 3.3 Rendering and incremental updates (how Magit does it)

Findings from Magit's source and manual:
- Magit refreshes by **erasing the buffer and re-creating its contents from
  scratch**; the manual states this "can be slow in large repositories".
- It restores the cursor by remembering the **identity** of the section at
  point (and line/column offset within it) and finding the "successor"
  section after the refresh; it also restores window scroll position.
- Hidden sections' bodies are lazy (not computed until expanded).

Our plan:
1. **Now:** full re-render on the server (cheap, simple), but send the client
   only **line-range edits** computed by diffing old text vs new text
   (common prefix/suffix, or a line diff such as the `similar` crate).
   This keeps the client's cursor, scroll and marks stable without the client
   having to do identity matching. The server also computes where the cursor
   should go (successor node by Node ID) and returns it.
2. **Lazy bodies from the start:** collapsed sections skip body computation.
3. **Later, only if profiling demands:** true incremental rendering —
   re-render only dirty subtrees (possible because Node IDs are stable).
4. Protocol: `view/render` carries `{version, baseVersion, edits}` or
   `{version, text}` for a full replace. If the client's applied version ≠
   `baseVersion` it requests `view/resync`.

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
filter `items` however their picker does.

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
fixup target, reset to, …).

- Server opens a view (usually `log`) with
  `select: {purpose: "rebase onto", multi: false, prompt: "Rebase onto which commit?", allowFreeText: true}`.
  `purpose`/`prompt` are shown in the view's title area.
- Extra bindings active (scope `log.select`): `common.confirm` (returns the
  commit under the cursor, or all marked commits if `multi`), `common.cancel`,
  `select.toggle-mark` (multi only).
- `allowFreeText`: a key (`select.enter-rev`) opens a `text` prompt so the user
  can type `HEAD~3`, a branch, or a tag; the server resolves it with
  `git rev-parse --verify`.
- The same pattern selects a branch (refs view), a stash, or files.

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
   - Neovim: the client keeps `applied[bufnr] = {lhs…}`. To apply a keymap it
     deletes exactly the maps in `applied[bufnr]`, sets the new buffer-local
     maps with `vim.keymap.set(modes, lhs, fn, {buffer = bufnr, nowait = true})`,
     and stores the new list — all in one synchronous Lua function, so no key
     can be pressed in between. Neovim has no *named* keymaps, so this tracked
     table is the "named map". Maps call `view/key` with the canonical key.
   - In **capture mode** the client also maps every other printable key to a
     forwarding function.
2. **`static`** (VSCode): VSCode cannot add keybindings at runtime, but it
   can change `when` clauses via context keys. The build generates
   `contributes.keybindings` from `magit-lsp keymap --format vscode`. Each
   entry is `{ "key": "s", "command": "magit.key", "args": {"key": "s"},
   "when": "magit.active && magit.scope == 'status' && !magit.textMode" }`.
   The extension sets context keys `magit.active`, `magit.scope`,
   `magit.textMode`, `magit.capture` on every `view/keymap` and on every active
   editor change. Users customise with VSCode's native `keybindings.json` by
   pointing a key at `magit.invoke` with `{"command": "status.stage"}` (runs a
   command by ID) or `magit.key` with a key to emulate. Both end up in
   `view/invoke` / `view/key`.
3. **`forwardAll`** (fallback): the client sends every key and the server
   answers `passThrough` for anything unbound.

**Is capturing every key in VSCode "just for magit-lsp buffers" safe?**
Mostly yes, with four caveats:
- *Scope:* keybindings with a `when` clause (`magit.active`, set from the
  active editor) cannot affect other editors. This part is safe.
- *"Every key" is not literally possible with static keybindings.* They
  enumerate specific keys, and printable characters depend on the keyboard
  layout (VSCode resolves by layout or, with `"keyboard.dispatch": "keyCode"`,
  by physical code; this setting is known to fix some layouts). IME and dead
  keys do not trigger keybindings.
- *The `type` command* is the usual way to capture every printable character
  (as VSCodeVim does) but: it is global to the window, only one extension can
  own it, overriding it routes **all** typing in **all** editors through the
  extension (and forwarding to `default:type` has been reported ~10× slower per
  call), and it is incompatible with VSCodeVim. Therefore: do **not** register
  `type` permanently. Register it only while a capture-mode view is focused,
  dispose it as soon as focus leaves or the view closes, forward to
  `default:type` when the active editor is not a magit-lsp view, and fall back
  to static bindings/QuickPick if registration fails (another extension owns it).
- *Never shadow workbench-level shortcuts* (`ctrl+p`, `ctrl+shift+p`, `ctrl+w`,
  `ctrl+tab`, `F1`, `alt+tab`, …). The generated key list contains only keys
  the server actually binds. A test fails if the manifest contains a key from a
  deny-list of workbench shortcuts without an explicit allow entry.
- *Copy:* `ctrl+c` is `common.cancel` in the `standard` profile. To keep
  normal copying working in read-only views, `common.cancel` replies
  `passThrough` when there is nothing to cancel (no menu, prompt or select mode
  open) and the editor has a non-empty text selection.

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
- The server validates the merged keymap and reports conflicts (same key bound
  to two commands in one scope chain) as a `Message` and in
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
   `editDocument` effect to that session's client:

   ```
   { type: "editDocument", editId, path, kind: "commit-message"|"rebase-todo"|"merge-message"|"tag-message"|"other",
     title, mode: "realFile"|"virtual", scope }
   ```
4. **The client opens it in its own editor, in the same window/instance**:
   - Neovim: open the file in a window per client config (split, tab, or float;
     default: bottom split), set `filetype` (`gitcommit`, `gitrebase`), attach
     the LSP client to the buffer, show the `commit-message` keymap.
   - VSCode: `showTextDocument(Uri.file(path))` in the active window; set
     context key `magit.editing`; restrict the LSP documentSelector to these
     filenames so diagnostics/completion apply.
   - TUI: may spawn `$EDITOR` itself (detail deferred).
5. **Confirm** (`common.confirm` in the editing scope): the client saves the
   file (if `realFile`) and sends `view/invoke common.confirm`; the server
   replies `ok` to the helper; Git continues. **Cancel** (`common.cancel`):
   server replies `abort`. **Closing the buffer/tab without confirming = cancel.**
   If the message is non-empty and modified, cancel first asks a `confirm` prompt
   ("Discard commit message?") unless disabled in config.
6. `mode: "realFile"` (commit/merge/tag message): the real file is edited; saving
   writes it. `mode: "virtual"` (rebase todo): the server parses the todo file
   into a structured view (move up/down, change action, show commit) and writes
   the file itself on confirm.
7. A client disconnecting, or the helper's socket closing, aborts the edit so
   Git never hangs.

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
  default for that menu (global or per-repo, per config); `menu.reset-args`
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
`menu.disabled`, `menu.preview`. Each maps to a standard theme group (§3.15).

**Keys inside a menu:** the view has `capture: true`. The client forwards every
key; the server does the two-step matching (`-` then letter) and answers
`passThrough`-free (unbound keys are swallowed with a one-line hint). `Escape`,
`ctrl+c` and `q` close the menu.

**Can LSP hover/floating UI be used? Can it sit at the bottom?** No. LSP hover
is anchored to a document position, delivers Markdown only, and the server
cannot choose its placement; `window/showDocument` can open a document but not
choose a window position. So menus are **not** LSP hovers. The server states
`placement` (`bottom` for menus, `float` optional) and each client uses its
own native windowing:

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
  the toggle binding; the client makes the buffer modifiable, restores undo
  (`undolevels` back to the user's value), and stops forwarding keys. The
  server stops answering position-based requests for the view and **queues**
  refreshes (does not render them).
- **Edits** in text mode are never parsed and never synced to the server model.
- **Exit:** the server re-renders the current tree (with queued refreshes
  applied) as a full replace; the client replaces the buffer, resets
  `modified=false` and sets `undolevels=-1` before replacing so the user
  cannot undo into stale content.
- Complementary first-class commands avoid needing text mode for common
  copies: `common.copy-value` (hash for a commit, path for a file, diff text
  for a hunk/selection) returning a `clipboard` effect.

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

Modifiers: `staged`, `selected`, `planned`, `disabled`, `bold`.

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
- **Keys:**
  ```toml
  profile = "vim"                 # "standard" | "vim"

  [keys.common]
  confirm = ["Enter"]             # replaces the profile's list for common.confirm
  cancel  = ["Escape", "ctrl+c"]

  [keys.status]
  stage   = ["s"]                 # command id is "status.stage"
  discard = ["x", "Delete"]
  unbind  = ["K"]                 # remove a profile binding

  [keys."menu.commit"]
  amend = ["a"]

  [keys.log]
  confirm = ["Space"]             # overrides common.confirm only in log
  ```
- Other settings (non-exhaustive): `[diff] context, whitespace`, `[sections]
  order, default_visibility`, `[log] default_args, margin`, `[menus]
  save_scope = "global"|"repo"`, `[ui] confirm_discard = true`.
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
| `view/open` | `{kind, args?, placement?, viewport}` | `ViewDescriptor` |
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
| `view/render` (notification) | `{viewId, version, baseVersion?, text?, edits?: [{startLine, endLine, lines}], cursor?: {line, column}, menu?: Menu}` |
| `view/keymap` (notification) | `{viewId, scope, version, capture, bindings: [{command, keys: [string], modes: ["normal","visual"], label, doc, group, status}]}` |
| `view/effects` (notification) | `{effects: [...]}` for asynchronous effects |
| `view/prompt` (request) | See §3.6; returns `{cancelled, value?}` |
| standard | `window/showDocument`, `window/showMessage`, `$/progress`, semantic tokens, symbols, definition, links, hover, code actions, diagnostics, completion |

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
| `common.show-all` / `hide-all` | `alt+4` / `alt+1` | `z R` / `z M` |
| `common.more-detail` / `less-detail` | `alt+3`, `alt+2` set levels (see note) | `z r` / `z m` |
| `common.next-section` / `prev-section` | `n` / `p` | `] ]` / `[ [` |
| `common.next-sibling` / `prev-sibling` | `alt+ArrowDown` / `alt+ArrowUp` | `g j` / `g k` |
| `common.parent-section` | `alt+ArrowLeft` | `g u` |
| `common.toggle-text-mode` | `ctrl+t` | `ctrl+t` |
| `common.copy-value` | `ctrl+shift+c` | `y y` |
| `common.visit` | `Enter` | `Enter` |
| `open.dispatch` (help / menu of menus) | `?` | `g ?` |

Visibility level note: levels 1–4 (titles only → sections → files → hunks).
`standard` binds `alt+1` … `alt+4` to set level N; `vim` uses `z m`/`z r` to
step the level down/up.

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
| `select.enter-rev` | `/` | `i` |
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

### 5.5 Editable scopes (`commit-message`, merge/tag message, `rebase-todo` text part)

| Command | `standard` | `vim` |
| --- | --- | --- |
| `common.confirm` | `ctrl+Enter` | `Z Z` (and `ctrl+Enter` where the terminal supports it) |
| `common.cancel` | `ctrl+shift+x` | `Z Q` |
| `commit-message.history-prev` / `next` | `alt+ArrowUp` / `alt+ArrowDown` | `[ m` / `] m` (normal mode) |
| `commit-message.add-signoff` | `ctrl+alt+s` | `g s` |

Closing the buffer is always cancel (§3.10). `ctrl+c` is **not** cancel here
because it is copy in non-modal editors.

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
| **Log** | `magit-log` | Commit list with graph, decorations, author, date; options (`-n`, `--graph`, `--decorate`, `--author`, `--since`, `--grep`, `-S`, `-G`, path limits); lazy "load more"; **select mode** | show commit, checkout/reset/branch/rebase/cherry-pick/revert from the commit at point, confirm/cancel | 5 |
| **Log variants** | `l` menu | current, other, all, local branches, reflog, file log, line-range log (`-L`), `HEAD..upstream`, `upstream..HEAD` | same | 5 |
| **Revision** | `magit-revision` | Header (hash, parents, author, committer, dates, refs, message), diffstat, full diff with hunks; parent/child navigation; combined diff for merges | visit file, apply/reverse hunk, copy hash | 5 |
| **Diff** | `magit-diff` | Any range `a..b`, `a...b`, worktree vs index vs revision, path-limited, stash vs parent, stat-only; context/whitespace/rename/word-diff options | visit, stage/unstage when applicable | 5 |
| **Refs** | `magit-show-refs` | Local branches, remotes, tags with ahead/behind vs a base; also select mode | checkout, create, delete, rename, reset, merge, rebase onto, log | 5 |
| **Cherry** | `magit-cherry` | `git cherry` between two refs | cherry-pick, show | 7 |
| **Stash list / show** | `magit-stash-list` | Stashes; show = diff view of a stash | apply, pop, drop, branch | 6 |
| **Rebase todo** | `git-rebase-mode` | Structured interactive todo: pick/reword/edit/squash/fixup/drop/exec/label/merge lines with commit summaries; reorder | move up/down, set action, show commit, confirm, abort (editor bridge, `mode: virtual`) | 7 |
| **Rebase in progress** | status section | Todo position, stopped commit, conflicts | continue, skip, abort, edit todo | 7 |
| **Merge / conflicts** | `magit-merge` + unmerged | Unmerged section; per-conflict actions | take ours/theirs, mark resolved, open merge tool (client effect `openMergeTool`), continue, abort | 7 |
| **Sequencer** | `A`, `V` | In-progress cherry-pick/revert | continue, skip, abort | 7 |
| **Commit message** | with-editor buffer | Message editor (real file) + companion read-only staged diff, history ring, trailers, lint diagnostics | confirm, cancel, history prev/next, add trailer | 4 |
| **Process log** | `magit-process-buffer` | Every Git invocation: args, exit code, duration, output, hook output | visit, copy command, clear | 3 |
| **Blame** | `magit-blame` | Annotated file (hash, author, date, summary); also inlay hints/code lens in real file buffers | show commit, blame at parent, copy hash | 8 |
| **File at revision** | `magit-find-file` | Virtual doc `magit://blob/<rev>/<path>` | diff against worktree | 5 |
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
Each task ends with **Done when**. Each phase leaves `make test` green.

### Phase 0 — Groundwork

- [ ] R-0.1 IMM-1…IMM-9 complete.
- [ ] R-0.2 `git status --porcelain=v2 -z --branch` parsing; paths with spaces/
      quotes/unicode; rename old/new paths. *Done when:* fixtures with each of
      those pass.
- [ ] R-0.3 Separate **Untracked files** section (currently lumped in Unstaged
      with status `?`); stage/unstage them via UI; `stage_all` also stages
      untracked when `status.stage-all` is invoked on the Untracked header.
      *Done when:* snapshot of a repo with untracked files shows the section.
- [ ] R-0.4 `patch.rs`: original headers preserved; renames, mode changes,
      symlinks, binary (whole-file only), no-trailing-newline, CRLF. *Done when:*
      one fixture and test per case.
- [ ] R-0.5 Hunk identity by content hash everywhere (no hunk indexes in any
      API). *Done when:* a test mutates the file between render and action and
      the action targets the right hunk or is rejected with a message.
- [ ] R-0.6 `tracing` to stderr/file; child stdout never reaches the LSP pipe.
- [ ] R-0.7 Move the stdio loop to `tokio`.

### Phase 1 — Server-owned document and sessions

- [ ] R-1.1 Node tree, Node IDs, renderer for Status in Rust producing text,
      tokens, symbols, links, line→node map. *Done when:* a snapshot test
      shows the full status text; deleting `src/document.rs` parsing breaks
      nothing.
- [ ] R-1.2 `Session`, `View` trait, `ViewStack`, `view/open`, `view/render`
      (full + line edits), `view/rendered` ack, `view/resync`, cursor
      successor by Node ID. *Done when:* a test refreshes after a mutation and
      the returned cursor is on the same node.
- [ ] R-1.3 Semantic tokens from the tree (§3.15 mapping); Neovim defines
      `@lsp.type.magit.*` links.
- [ ] R-1.4 `documentSymbol` replaces `vcs/targets`; `definition`/`showDocument`
      for visit.
- [ ] R-1.5 Server-owned visibility with lazy bodies; `common.toggle-section`,
      level commands.
- [ ] R-1.6 Delete `vcs/didRender`, `vcs/status` and all old `vcs/*` methods;
      the Lua client keeps only buffer replacement, key forwarding and
      window management.

### Phase 2 — Commands, keymaps, config, text mode, menu framework, second client

- [ ] R-2.1 Command registry (`id`, `scope`, `doc`, `status`, `run`).
- [ ] R-2.2 Keymap resolution, profiles `standard` and `vim` (§5), TOML config
      with `include`/`include_optional`, XDG location, `--config`,
      `MAGIT_LSP_CONFIG`, hot reload, validation messages.
- [ ] R-2.3 `view/key`, `view/invoke`, `view/keymap`, `passThrough`, capture mode.
- [ ] R-2.4 Neovim: atomic keymap apply (tracked buffer-local set), visual
      selection forwarding, capture mode, fold-key mirroring (§3.9, incl.
      scanning user mappings), default profile `vim`.
- [ ] R-2.5 `magit-lsp keymap --format json|vscode|neovim|markdown --check`;
      CI snapshot; workbench deny-list test.
- [ ] R-2.6 Text mode end to end (§3.14); `common.copy-value`.
- [ ] R-2.7 **Menu framework** (§3.11): model, text rendering with columns from
      `viewport`, switches/options/actions, argument preview, structured model
      in `view/render`, `bottom` placement in Neovim (float and split).
- [ ] R-2.8 **Planned-feature menus** (§3.12): create every dispatch menu
      with its full Magit action list as `Planned`; `open.dispatch` view
      generated from the registry; `cargo xtask parity`.
- [ ] R-2.9 **Second client to prove decoupling:** a minimal VSCode extension
      (static keybindings + context keys + QuickPick menu + fold-key mirroring
      command) or a ratatui TUI. Fix protocol gaps found here before Phase 3.

### Phase 3 — Effects, prompts, runner, process log, editor bridge

- [ ] R-3.1 `Ui` handle and `view/prompt` with the four kinds (§3.6); Neovim
      and VSCode mappings; `ScriptedUi` for tests. Menu option prompts now work.
- [ ] R-3.2 Process runner (queue, progress, cancel, ring buffer).
- [ ] R-3.3 **Process log view.**
- [ ] R-3.4 Editor bridge: helper subcommand, socket, `editDocument` effect,
      Neovim and VSCode handling, confirm/cancel/close semantics, askpass
      (§3.10). *Done when:* a Rust test runs `git commit` with the fake client
      answering the `editDocument` effect and the commit has the typed message.
- [ ] R-3.5 File watcher auto-refresh; `index.lock` handling.
- [ ] R-3.6 Discard (`status.discard`) for files, hunks, selections, with
      `confirm` prompt (`destructive: true`); consider saving discarded content
      under a private ref for undo.
- [ ] R-3.7 `UserError` mapping table for all remaining Git failures as they
      are met.

### Phase 4 — Commit flow

- [ ] R-4.1 Commit menu: commit, extend, amend, reword, fixup, squash, augment,
      instant fixup/squash, alter; switches `--all`, `--allow-empty`,
      `--verbose`, `--no-verify`, `--signoff`, `--gpg-sign`, `--reset-author`,
      option `--author=`. Items flip from `Planned` to `Implemented`.
- [ ] R-4.2 Commit-message view over the editor bridge: staged-diff companion
      view, history ring, trailers, lint diagnostics, confirm/cancel/close.
- [ ] R-4.3 Status "Recent commits" section.

### Phase 5 — Read views

- [ ] R-5.1 Log (graph, decorations, filters, lazy load) and select mode (§3.7).
- [ ] R-5.2 Revision view; Diff view (ranges, args).
- [ ] R-5.3 Refs view; file-at-revision virtual docs; log variants.
- [ ] R-5.4 `documentLink` and `hover` for hashes/refs.
- [ ] R-5.5 Diff-argument menus applied across views.

### Phase 6 — Branches, remotes, stash

- [ ] R-6.1 Branch menu: checkout, create, create-and-checkout, spin-off,
      spin-out, rename, reset, delete, configure upstream/pushRemote.
- [ ] R-6.2 Fetch/pull/push menus (`--force-with-lease`, `--set-upstream`,
      `--dry-run`, `--prune`, pull `--rebase`/`--ff-only`) with progress and askpass.
- [ ] R-6.3 Remote and tag menus; reset menu; checkout file from revision.
- [ ] R-6.4 Status: Unpulled/Unpushed sections; header lines.
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
- [ ] R-7.4 Commit pickers everywhere via select mode.

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

```rust
let repo = RepoFixture::new().commit("init", [("f.txt", TEN_LINES)]).modify_standard();
let mut t = TestSession::open_status(&repo);
t.goto("+NEW line");          // find a node by visible text
t.press("s");                 // same code path as view/key
t.assert_buffer(expect_file!["snapshots/stage_single_line.txt"]);
assert!(repo.git("diff --cached").contains("+NEW line"));
```
- API: `press`, `press_visual(startLine, endLine, keys)`, `invoke`, `goto`,
  `text()`, `focused_node()`, `visibility()`, `keymap()`, `effects()`.
- `ScriptedUi` answers prompts and `editDocument` effects from a queue
  (`reply_text`, `reply_confirm`, `reply_choice`, `edit_document(|text| …)`).
- Snapshot buffers with `insta` or `expect-test`; assert repo state with `git`.

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

Neovim's `:TOhtml` was rewritten in Lua in 0.10 to support Nvim-specific
decorations (extmarks). The findings below were confirmed empirically by
`scripts/color_spike.lua` (`make color-spike`).

**Findings (IMM-8)** — Neovim 0.12.5:

1. **Client extmark highlights are included.** Ranges from any non-LSP
   namespace (the client's `magit-lsp` namespace) become
   `<span class="…">`. The class name is the *resolved* group
   (`synIDtrans`), so an extmark using `MagitHeader` emits class `Title`, not
   `MagitHeader`.
2. **LSP semantic-token extmarks are NOT included.** `tohtml` explicitly skips
   any extmark whose namespace name contains `nvim.lsp.semantic_tokens` and
   emits the warning `lsp semantic tokens are not supported, HTML may be
   incorrect`. Tested with a dummy namespace named
   `nvim.lsp.semantic_tokens` (the server does not implement
   `semanticTokens` yet); the real namespaces are
   `nvim.lsp.semantic_tokens` and `nvim.lsp.semantic_tokens:<client_id>`, both
   matched by that substring check.
3. **API:** in 0.12.5 `tohtml` moved to the dist-opt package `nvim.tohtml`, so
   `:packadd nvim.tohtml` is required first (in 0.10/0.11 it is built in).
   Then `require('tohtml').tohtml(winid?, opt?) -> string[]`, with
   `opt = { title?, number_lines?, font?, width?, range? }`. `:TOhtml [file]`
   is a thin wrapper that writes the lines and splits the HTML buffer.
4. **`hl_eol` is ignored.** Only the explicit extmark range is styled; the rest
   of the line gets no class. Verified with `hl_eol = true` and `end_col = 4`:
   only the first four characters are wrapped.
5. **Pitfalls:**
   - Set `termguicolors` *before* `tohtml`. Otherwise it emits no
     `guifg`/`guibg` and every rule is empty (`{}`).
   - Closed folds hide their content from the HTML. The client sets
     `foldmethod=expr` and the default `foldlevel` is 0, so set
     `foldenable=false` (or `zR`) on the status window first.
   - `:colorscheme` runs `hi clear`, deleting the client's `default = true`
     `Magit*` groups. Re-apply the links after every scheme change (the spike
     captures the links once and re-applies them).
   - Use `nvim_get_hl(0, {name = g, link = true})` for the link and
     `link = false` for the resolved `fg`/`bg` (numbers; format `#%06x`).
   - `tohtml` warnings are batched and delivered from a scheduled
     `vim.notify`; only the first message text is named. Warm its
     terminal-colour caches and drain the loop before asserting on a specific
     warning.
   - Headless has no terminal colours; with `termguicolors` the scheme's
     `Normal` `guibg` is used, so background is reported reliably.
   - `width` only affects fillers/repeat characters, not wrapping; pass a
     generous value.
6. **Is the plan-step-5 fallback needed?** For the *current* client
   (extmark-based `Magit*` groups) `:TOhtml` is sufficient and the plan can be
   implemented as written. It is **not** sufficient once highlighting moves to
   LSP semantic tokens (§3.15's target), because those are skipped; then the
   `nvim_buf_get_extmarks(buf, ns, 0, -1, {details = true})` + `nvim_get_hl`
   dump becomes mandatory. The dump is also the only way to recover the
   semantic class (token type / `Magit*` name), since HTML classes carry only
   the resolved group name.

**Additional finding (mapping bug):** `MagitDiffAdded` / `MagitDiffRemoved`
link to `diffAdded` / `diffRemoved`, which do **not** exist in Neovim's
built-in schemes (`hlexists` = 0; resolved fg is nil in all six tested
cases). Added/removed lines therefore render uncoloured, and the HTML carries
an empty `.diffAdded {}` rule. §3.15 already maps `@diff.plus` → `Added` and
`@diff.minus` → `Removed`; the client should link to those (or
`DiagnosticOk` / `DiagnosticError`). Note also that `Title` in `habamax` and
`lunaperche` has no `guifg` (bold only), so "no foreground" is not by itself
a failure — the §8.4 assertions must inspect all resolved attributes.

Spike output: `build/test-output/status-<scheme>-<background>.html` plus the
class → link → resolved fg table printed by `make color-spike`.

Plan:
1. Helper `render_status_html(scheme, background)`: start headless Neovim with
   `scripts/minimal_init.lua`, `:set background=…`, `:colorscheme …`, open the
   status view on a fixture repo, **wait for semantic tokens** (poll for
   extmarks in the `nvim.lsp.semantic_tokens` namespace), run `:TOhtml`, write
   `build/test-output/colors/<scheme>-<background>.html`.
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
     the colourscheme changes (i.e. nothing is hard-coded).
4. Emit a legend `build/test-output/colors/legend.md`: token → link target →
   resolved fg per scheme, so a human can review at a glance. Optionally render
   the HTML to PNG with a headless browser if available (not required).
5. If `:TOhtml` turns out not to include the tokens, fall back to a custom dump
   using `nvim_buf_get_extmarks(buf, ns, 0, -1, {details = true})` and
   `nvim_get_hl`, producing the same cell structure.

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
on the oldest supported git version.

---

## 9. Open questions

1. **macOS config dir:** `~/.config` via `etcetera` (proposed) or
   `~/Library/Application Support` via `dirs`.
2. **Editable-scope cancel key** in `standard` (`ctrl+shift+x` proposed).
   Closing the buffer already cancels, so this is a convenience.
3. **VSCode menu default:** QuickPick (proposed) vs. text document in a lower
   editor group. Revisit after the Phase 2 spike.
4. **Per-repo config trust:** is `.magit-lsp.toml` worth the risk surface?
   Currently limited to keys and display, behind `trust_repo_config`.
5. **Minimum supported Git version** (decide by running IMM-1 tests on old
   versions).
6. **Full-line background tinting** for diff lines (optional client feature).
7. **LSP 3.18 `workspace/textDocumentContent`** adoption when clients support it.
8. **Fold keys in VSCode** for users with customised `editor.toggleFold`:
   the "mirror my shortcuts" command (proposed) vs. any better mechanism found
   during the spike.
